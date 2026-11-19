# S2 Review — Reviewer-C (Correctness & Security)

- **Reviewer:** Reviewer-C (adversarial round, stage S2)
- **Date:** 2026-11-13 (repo HEAD 59d7bba, branch main, clean tree — reviewed as-is)
- **Scope:** `src/core_proc.rs`, `src/supervisor.rs`, `src/sysproxy.rs`, `src/elevate.rs`, `src/main.rs` (incl. embedded `#[cfg(test)]` modules); S1 modules (config/xrayconf/netmerge/error/lib) treated as stable context, cross-module contract violations flagged where seen.
- **Lens:** races, deadlocks, panics, error paths, unsafe FFI, process lifecycle, contract conformance.

## Verification log (run myself, trusted nothing)

| Command | Result |
|---|---|
| `. tools/env.sh; cargo test` | **52/52 passed, 0 failed, 5.64 s** (run twice, both green) |
| `cargo clippy --all-targets` | **clean** — 0 warnings / 0 errors |
| `cargo fmt --check` | **clean** (exit 0) |
| `cargo zigbuild --target x86_64-pc-windows-gnu` | **success** — `multitunnel.exe` (26.7 MB) + `multitunnel-tray.exe` (4.9 MB), both PE32+ |

**Adversarial demos** (standalone crate in `/tmp/mt-revc-demo` against the repo as a path dependency; the repo itself was not modified — verified `git status` clean after):

- `start-hang`: `CoreProc::start()` called while a child is running — **blocked past 2 s** until the child was killed externally; on unblock it returned `Ok(())` and spawned a *second* core generation. The documented `Err("core already running")` never materialized. → finding **P0-1**.
- `disc-race`: `disconnect()` issued while a `connect()` sequence was in flight — **returned `Ok` in 44 ms**; the connect then completed and left `state == Connected` with `loop_running() == true`. → finding **P0-2**.

---

## P0 — must fix before close

### P0-1 — `CoreProc::start()` hangs (and then double-starts) when a child is already running
**`src/core_proc.rs:108-116`** (doc claim at `:104`, module doc `:10-11`)

`start()` joins the previous waiter thread **before** checking the child slot:

```rust
let leftover = lock(&self.waiter).take();
if let Some(h) = leftover { let _ = h.join(); }   // blocks while the child lives
let mut slot = lock(&self.child);
if slot.is_some() { return Err(...core already running...); }  // unreachable while child alive
```

The waiter (`waiter_loop`, :333-367) only exits when the child dies, the slot empties, or the stop flag is set. So while a child is live, `join()` blocks **indefinitely**; the `is_some()` error branch is dead code on that path. When the child eventually dies, the join returns, the (now empty) slot passes the check, and `start()` spawns a *new* child and returns `Ok(())` — a silent double-generation restart the caller never asked for. This violates the method's own contract ("Returns an error if a child is already running") and is an unbounded block in a public lib API that stage S3 (tray) is specified to call from menu threads.

Reachability inside the supervisor today is via P0-2's corrupted states (e.g. `State::Failed` with a live core → next `connect()` → `connect_sequence` step 2 → `core.start()` → wedged forever); it is also directly reachable by any external API user. **Fix (trivial):** check `lock(&self.child).is_some()` and return the error *before* joining the leftover waiter (the leftover join is only safe when the previous cycle's child is gone).

### P0-2 — `connect()` and `disconnect()` are not serialized as whole operations; disconnect can be silently ineffective
**`src/supervisor.rs:238-260` (connect), `:264-341` (connect_sequence), `:396-433` (disconnect)**

Only the tiny state-check windows hold `inner`. `connect_sequence` performs seconds of work (config write, core spawn, up-to-15 s port wait, up-to-20 s probe, up-to-15 s bonding wait) holding **no lock and no joinable loop handle** — `spawn_loop` runs only *after* it succeeds. A concurrent `disconnect()` sees `state != Disconnected`, sets the stop flag, joins a **`None`** loop handle (`:402` — the loop doesn't exist yet), restores proxy, stops whatever core exists, sets `Disconnected`, and returns `Ok` — while `connect_sequence` keeps running. Demonstrated (44 ms disconnect during an in-flight probe): final state `Connected`, poll loop alive; with `restart_max > 0` the loop even resurrects the core the disconnect killed, and step 5 sets the **system proxy after the user disconnected**, which in the kill-window black-holes all WinINET traffic through a dead proxy.

This is exactly the usage the module docs advertise for S3 ("wrap the `Supervisor` in an `Arc` and call from menu handlers on any thread", :29-31). It also compounds with P0-1: the interleaving `connect A → disconnect B (completes early) → connect C` can leave `State::Failed`/`Disconnected` with a live core, after which the next `connect()` blocks forever in `start()`'s waiter join. **Fix:** one operation-level `Mutex<()>` (or making the state transition a token) held for the whole of `connect()`/`disconnect()`; all methods are already `&self`, so this is local and cheap.

---

## P1 — should fix

### P1-1 — `socks_probe` has per-read timeouts but no total deadline
**`src/core_proc.rs:259-268, 390-393`**

`set_read_timeout(timeout)` bounds each `read_exact`, but a proxy that drips one byte just inside every timeout window keeps each read alive forever — the probe never returns. The contract ("a probe against a malicious/half-open proxy must never hang", module doc :14-16) is satisfied per-read, not per-call. Consequences: `connect()` blocks unboundedly at step 3; the poll loop stalls mid-probe (no tick, no stop-flag observation), and since `disconnect()` joins the loop thread, a tray disconnect wedges with it. Practical exposure is low (the probed proxy is our own spawned xray on loopback), hence P1 not P0. **Fix:** compute a call deadline and re-set `read_timeout` to the remaining budget before each read.

### P1-2 — `elevate::join_args` does not escape quotes / backslash-before-quote (argument injection into the elevated relaunch)
**`src/elevate.rs:17-28`** (used at `src/main.rs:314` via `relaunch_elevated`)

Arguments containing whitespace are wrapped in quotes but embedded `"` is never escaped. An argument like `a" --evil` becomes `"a" --evil"`, which `CommandLineToArgvW` parses as **two** arguments (`a`, `--evil"`) — a single crafted argv entry splits into multiple arguments in the UAC-elevated child. Similarly a path argument ending in `\` (e.g. `-config C:\`) corrupts: `"C:\"` parses as `C:"`. Today's exposure is same-user self-injection (the user launches the CLI), but `main.rs` forwards arbitrary `std::env::args()` verbatim, so any future caller that passes semi-trusted arguments turns this into a real elevation-boundary injection. **Fix:** proper `CommandLineToArgvW`-compatible escaping (`"` → `\"`, doubling trailing backslashes before a closing quote).

### P1-3 — cross-module: `local.listen` is never validated to be loopback (contract #2)
**`src/config.rs:234-263` (validate), `src/xrayconf.rs:20,27` (binds verbatim), `src/supervisor.rs:609-616` (socks_addr/http_addr)**

`Config::validate` checks server fields and port collision only. A config with `"listen": "0.0.0.0"` passes validation, is bound verbatim by **both** xray inbounds — the SOCKS inbound is `"auth": "noauth"` (`xrayconf.rs:23`) — exposing an unauthenticated SOCKS+HTTP proxy to the LAN, and becomes the WinINET `ProxyServer` value. This directly violates the lib.rs architecture contract #2 ("the core exposes loopback-only SOCKS/HTTP inbounds"). Defaults are safe (`127.0.0.1`) and `doctor` only checks parseability (`main.rs:237`), so it is a config-reachable footgun rather than an exploit — but the S2 supervisor is the natural enforcement point (fail `connect_sequence` if `socks_addr`/`http_addr` is not loopback), or add the check to `Config::validate` in a follow-up. Flagged as a cross-module contract violation per stage instructions (config.rs is S1-closed).

---

## P2 — nits / robustness

1. **`src/supervisor.rs:35-36`** — module doc claims "a disconnect is honored within ~100 ms"; in practice `disconnect()` joins a loop that may be inside `wait_for_port` (≤15 s) or `socks_probe` (≤15 s per read) → up to ~30 s stall, and unbounded with P1-1. Doc should scope the claim to sleep slices.
2. **`src/core_proc.rs:371-387`** — `pump_to_log`: two detached threads append to `xray.log` through separate handles; `write_all` can split, interleaving partial lines (log-only corruption). Silent output drop if the open fails is documented — fine.
3. **`src/sysproxy.rs:228-233`** — registry snapshot reads use `.ok()`; a type-mismatched value (e.g. `REG_EXPAND_SZ`/`REG_BINARY` `ProxyServer`) snapshots as *absent* and `restore` then **deletes** the user's value. Also a mid-`set` failure (after `ProxyEnable=1`, before `ProxyServer`) leaves a half-applied proxy until restore — self-heals via the snapshot on disconnect, but worth a comment.
4. **`src/supervisor.rs:620-639` / `src/main.rs:106-123`** — `write_private` truncates in place (no temp+rename): a crash mid-write leaves a partial `xray-client.json`.
5. **`src/supervisor.rs:599-606`** — `parse_probe_target` has no IPv6-literal support (`rsplit_once(':')` keeps the brackets; `[::1]:443` → host `[::1]`).
6. **`src/main.rs:168`** — `doctor` runs `xray version` with no timeout (a hung binary hangs doctor) and without `CREATE_NO_WINDOW` (cosmetic flash on Windows).
7. **test-only, `src/core_proc.rs:412-417` / `src/supervisor.rs:663-666`** — `free_loopback_port` released-port reuse race (standard pattern, accepted risk).
8. **`src/config.rs:200-202` + `src/sysproxy.rs:239-241`** — an empty `proxyOverride` falls back to the default on load, so the override can never be disabled via config; `set()` also skips writing it when empty, leaving a previous override in force.
9. **`src/main.rs:23`** — `--config=path` (equals form) unsupported; only space-separated `-config/--config` (usage-documented, nit).

---

## Scrutiny checklist — verified clean (per stage instructions)

1. **Rig deadlock fix (nonblocking stub + quit flag)** — correct: `set_nonblocking` on the try_clone'd fd (shared OFD, so the rig's copy is nonblocking too — irrelevant since it never accepts), quit checked every ~10 ms, `WouldBlock` sleeps, `Rig::drop` flips quit **first** then joins (bounded ~10 ms). **No other join-on-immortal-thread patterns found:** netmerge `join_reader` is grace-bounded (1 s) and detaches readers on timeout; `ensure_running`'s reaper is detached; `spawn_loop` only joins handles proven finished (`loop_running=false` is `poll_loop`'s final store); `disconnect` joins only a loop that observes the stop flag every 100 ms slice, every campaign iteration and every status-call boundary. The single remaining instance is P0-1's leftover-waiter join.
2. **`Rig::build(restart_max)` call sites** — 4/4 consistent: tests 4/5/6 pass `3` (campaigns never exercised), test 7 passes `0` (needs the immediate give-up). Assertions match each value.
3. **Lock discipline** — no lock is held across I/O or long operations: `netmerge::status`/`ensure_running` run outside `inner`; the core child mutex is held only around non-blocking `try_wait` (and briefly around spawn); `loop_thread`/`waiter` handles are taken and the guard dropped *before* joining. No two-lock cycles (every lock acquisition is a standalone statement). The blocking hazards are the P0/P1 items above, not lock ordering.
4. **Panic surface (non-test)** — none reachable: poison-tolerant `lock()` in both modules; saturating shift/add (`backoff_delay`, `restarts`); `Instant` subtraction guarded by prior comparisons; SOCKS buffers are fixed-size or bounded (`hb.len() ≤ 255` checked before `as u8`); `caps[1]/caps[2]` groups unconditional in the regex; `main.rs` arg indexing guarded by `i+1 >= len`; static `Regex::new(...).expect(...)` compile-time-constant patterns. Hostile net-merge output cannot panic (`parse_status` is total, tested on garbage).
5. **Process lifecycle** — kill-then-wait ordering correct in `stop()` and netmerge `run_full`; waiter reaps via `try_wait` (no zombies); pump threads EOF-terminate after reap; log header written pre-spawn (failed spawns diagnosable); give-up path stops core, restores proxy, `State::Failed` ✓; `restart_max=0` = never ✓ (test 7).
6. **socks_probe wire format** — byte-correct vs RFC 1928: greeting `05 01 00` → `05 00`; CONNECT `05 01 00 03 len host port`; reply head VER/REP/RSV/ATYP; BND drain 6/`1+len+2`/18 bytes for ATYP 01/03/04; unknown ATYP → error naming the byte; REF≠0 → error naming the status. Errors contain no secrets. Only gap: total deadline (P1-1).
7. **sysproxy (read-review, host has stubs; windows code cross-compiles)** — snapshot save is `create_new` (never clobbers a previous run's snapshot ✓); restore writes values then removes the snapshot file (crash-safe retry ✓) and refreshes via `INTERNET_OPTION_SETTINGS_CHANGED(39)` + `INTERNET_OPTION_REFRESH(37)` with NULL handle — correct, returns checked; corrupt snapshot → `Err` (never silently guesses); missing snapshot → `ProxyEnable=0` per contract; snapshot 0600 on unix; contains only proxy addresses, no secrets; **tests are registry-free** (pure helpers + temp state dirs; `#[cfg(windows)]` tests use in-memory values only).
8. **elevate** — `to_wide` UTF-16 with surrogate pairs ✓ (tested); `ShellExecuteW` SAFETY comments accurate; `<= 32` return semantics ✓; `is_elevated` handle lifecycle correct (close on all paths after open). Gap: quoting (P1-2).
9. **Supervisor contract conformance** — CONNECT order (step-0 restore self-heal → config 0600 → core start + 15 s port wait → probe fail-fast(stop core, restore proxy, Failed) → bonding non-fatal → sysproxy non-fatal) ✓; DISCONNECT reverse with loop stop-first ✓; backoff 1,2,4,8,16→30 cap, saturating at `u32::MAX` ✓; snapshot serde round-trip incl. `State` naming ✓; `connect` no-op while Connecting/Connected ✓; `disconnect` no-op from Disconnected ✓; restarts counter reset at the connect reset point ✓; consumed-once exit flag swallowed at reset point + `!running()` backstop ✓.
10. **Flake watch** — all timing assertions have ≥2× margins (test 7: 15 s deadline vs ~5.3 s expected; `joined_disconnect` 20 s vs ~100 ms; core_proc 5 s bounds vs 2-3 s budgets; dead-port probes return in µs). No sleep-based assertion found that should flake on a loaded 2-core box; the historical flake did not reproduce in either of my runs.
11. **Secrets** — `xray-client.json` written 0600 (unix) in both `supervisor::write_xray_config` and `main::generate`; the VLESS uuid/publicKey appear only in that file; no credential is logged (core log header records command + paths only), embedded in error strings, or stored in the sysproxy snapshot; Windows ACL caveat is documented (S1 consensus).

## Verdict

**BLOCK** — two P0s, both empirically reproduced against the current tree:

1. `core_proc::start()` blocks indefinitely (then silently double-starts) when a child is already running — contract violation with a trivial fix (check the slot before joining the leftover waiter).
2. `connect()`/`disconnect()` lack whole-operation serialization — a disconnect during an in-flight connect returns `Ok` while the tunnel (and system proxy, and poll loop) subsequently comes up; through the corrupted states it wedges the next `connect()` forever via P0-1.

Both fixes are small and local (reorder two statements; add one operation mutex); no architectural change is required. Everything else in the stage — process lifecycle, SOCKS5 probe bytes, sysproxy snapshot/restore semantics, backoff math, panic surface, secrets handling, test rig — verified sound.

*Repo left untouched (clean tree at 59d7bba); demo crate lives in `/tmp/mt-revc-demo` outside the repo.*
