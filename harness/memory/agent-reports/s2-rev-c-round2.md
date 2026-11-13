# S2 Review — Reviewer-C Round 2 (Correctness & Security)

- **Reviewer:** Reviewer-C (adversarial round 2; round-1 verdict was BLOCK: P0-1, P0-2, P1-1, P1-2, P1-3)
- **Date:** 2026-11-13 (repo HEAD `eb1235a`, branch main, clean tree — reviewed as-is, no files modified)
- **Scope:** verification of the claimed fixes on top of `59d7bba`, adversarial re-review of all changed code (`core_proc.rs`, `supervisor.rs`, `elevate.rs`, `netmerge.rs`, `bin/tray.rs`, `Cargo.toml`, workflows, `THIRD_PARTY_NOTICES.md`), plus regression thinking around the new designs.

## Verification log (run myself, trusted nothing)

| Command | Result |
|---|---|
| `git log --oneline -6` | `eb1235a` ← `9fafc6c` (s2-fix-round-1) ← `59d7bba` (round-1 tree) — history as claimed |
| `git diff 59d7bba..HEAD --stat` | 12 files: src/{core_proc,supervisor,elevate,netmerge,bin/tray}.rs, Cargo.toml, 2 workflows, THIRD_PARTY_NOTICES.md, LEDGER, 2 agent reports. config/sysproxy/main/xrayconf/error/lib **untouched** (consistent with S1-closed + my P1-3 being fixed at the supervisor) |
| `cargo test` | **57/57 passed, 0 failed** in 6.04 s — run **4× total**, all green (6.04/5.99/6.05 s), incl. all 5 new tests |
| `cargo clippy --all-targets` | **clean** — 0 warnings / 0 errors |
| `cargo fmt --check` | **clean** (exit 0) |
| `cargo zigbuild --target x86_64-pc-windows-gnu` | **success** (both binaries, current sources) |

**Round-1 adversarial demos re-run against the fixed tree** (same `/tmp/mt-revc-demo` crate, path-dep rebuilt at `eb1235a`; repo untouched):

- `start-hang` (was: >2 s hang, then silent double-start): second `start()` now returns **`Err("core already running: …")` promptly** → **P0-1 fixed, empirically**.
- `disc-race` (was: disconnect Ok in 44 ms, tunnel left Connected + loop alive): disconnect now **blocks on op_lock for the in-flight connect (3.66 s), then completes the teardown** — final `state == Disconnected`, `loop_running == false` → **P0-2 fixed, empirically** (semantics correctly changed from "lost teardown" to "serialized teardown").

---

## Fix-by-fix verification (my round-1 IDs)

### P0-1 — `CoreProc::start()` waiter-join hang — **FIXED, CONFIRMED**
`src/core_proc.rs:105-124`: the child-slot check is now FIRST (`:109-111`) and returns the documented `Err`; the leftover-waiter join (`:115-117`) runs only when the slot is empty; a re-check under the slot lock (`:119-124`) covers the join window. Verified:

- **Soundness of the join:** with the slot empty, the old waiter has either exited or will observe `None` within one 50 ms poll (`waiter_loop`, :346-380) — the join is bounded. The re-check under the slot lock closes the TOCTOU between first check and join completion.
- **Concurrent `start()`×2:** the slot lock is held across the whole spawn+publish region (`:119-198`), so a loser blocks at its first check and then sees `Some` → Err. No double-spawn.
- **The orchestrator's question (child dies between check and join):** if a child is alive at the first check → immediate Err (no join). If the slot is empty, the join cannot wait on a live child — the waiter's exit conditions don't depend on any child. ✓
- Empirical demo + new test `start_while_running_errors_promptly` (`<1 s` bound on an instant Err — µs in practice, huge margin). ✓
- **Residual (new P1-1 below):** a theoretical hang remains for *concurrent external* `start()` calls hitting the ~50 ms window where the old waiter hasn't exited yet and another `start()` publishes a fresh child the old waiter then adopts. Not reachable through `Supervisor` (see op_lock analysis), so P1 hardening, not P0.

### P0-2 — connect/disconnect not serialized — **FIXED, CONFIRMED**
`src/supervisor.rs:178,209,247-248,413-414`: `Shared::op_lock: Mutex<()>` acquired for the **whole body** of `connect()` and `disconnect()`; poison-tolerant `lock()`; guard released by RAII on every path. Verified by demo (above) and by `connect_waits_for_op_lock_of_in_flight_operation` (a blocked connect cannot even set `Connecting`). **Deadlock analysis (the specific question):**

- `op_lock` is taken **only** by `connect()`/`disconnect()` (grep: :248, :414 + test) — the poll loop, restart campaign, and all helpers never take it. `disconnect()` joining the loop while holding op_lock therefore cannot deadlock; worst case it waits out one bounded blocking call (see below).
- Lock nesting is strictly one-way: op_lock → {inner, loop_thread, core locks}, never the reverse; every inner/loop_thread acquisition is a standalone short statement. No ABBA cycle.
- `spawn_loop` (`:391-408`) drops the loop_thread guard **before** joining and re-acquires after — and both `spawn_loop` and `disconnect` (the only slot writers) run under op_lock, so the re-acquire window is race-free. The old `loop_running` early-return (REV-B P2-4) is gone; the join target is provably exited: the only paths leaving a handle in the slot are give-up (loop already returned — `loop_running=false` is its final statement) or a just-exiting give-up thread (ns window). A *live* loop implies state Connecting/Connected, for which `connect()` no-ops before `spawn_loop` — so joining a live loop is unreachable.
- Loop-join blocking is bounded: ticks sleep in 100 ms stoppable slices, campaigns re-check the stop flag every iteration, `wait_for_port` ≤ 15 s, `socks_probe` now has a **total** deadline (P1-1 fix), netmerge calls have hard timeouts. So `disconnect()` holds op_lock at most ~15-30 s; a concurrent connect waits, then sees the settled state. Serialization latency, not a hang — and it is exactly what makes the round-1 race impossible.
- The previously-compounding "connect from Failed with a live core → `start()` wedges" path is dead: op_lock guarantees state ∈ {Failed, Disconnected} ⇒ no in-flight sequence and the core stopped.

### P1-1 — `socks_probe` total deadline — **FIXED, CONFIRMED**
`src/core_proc.rs:271-277, 402-440`. The design is correct this time and the reasoning in the fix note is right: socket timeouts are per-underlying-syscall, so the budget must be re-armed before **every** `read` call, not per logical read. Verified in detail:

- `refresh_socket_budget` (:404-420): `checked_duration_since` (None → Err) + explicit `is_zero` guard, then sets BOTH read and write timeouts to the remaining budget. No panic; errors are `socks5:`-prefixed.
- `socks_read_exact` (:427-…): manual fill loop — re-arms the budget before each `s.read`, `Ok(0)` → clean "connection closed" error, io errors (incl. `TimedOut`/`WouldBlock` from the shrunken window) propagate with context, `filled` arithmetic cannot exceed `buf.len()` (read contract), empty buffer → no-op Ok. **No panic surface.**
- Every reply phase (greeting, head, ATYP length, drain) passes the same deadline; the CONNECT write is preceded by its own refresh (:304). Total probe time ≤ `timeout` + ε for any drip schedule: each read's timeout is the *remaining* budget, so the deadline is a hard ceiling.
- The new test drips 12 valid SOCKS5 reply bytes at 80 ms intervals against a 500 ms budget and asserts Err in **[350, 900) ms**. Timing analysis on a loaded 2-core box: in *every* delivery schedule the probe errors at ~500 ms (a delayed drip merely moves the failing read to the deadline; an on-time drip completes the head at ~480 ms and the drain read then has ~20 ms left), so the window has ≥150 ms lower / ≥400 ms upper margin — the tightest test in the suite but sound; observed stable across my 4 runs. Without the fix the probe returns Ok at ~880 ms and the test fails on `expect_err` — it genuinely pins the bug.
- Minor residuals (noted as P2): the two `write_all`s re-arm once per call, so in *theory* a write leg could take up to ~2× budget across partial syscalls — physically irrelevant here (writes are 3 / ≤262 bytes, always kernel-buffered on loopback); and the `socks_probe` method doc still says only "read/write timeouts are set" — the total-deadline guarantee lives in the helper docs.

### P1-2 — argv quoting — **FIXED, CONFIRMED**
`src/elevate.rs:35-77`: `quote_windows_arg` implements the full `CommandLineToArgvW` rules. Verified against the rules case by case: n backslashes before a quote → `2n+1` (parses back as n literal backslashes + non-toggling quote) ✓; backslashes before any other char → literal ✓; trailing backslashes doubled before the closing quote ✓; bare args (no whitespace/quote, non-empty) untouched — so bare `C:\` round-trips without the trailing-backslash trap ✓; empty → `""` ✓; quote-only arg `"` → `"\""` ✓. The round-1 injection case `a" --evil` now produces `"a\" --evil"` → re-parses to the **single** original argument (tested). `join_args` keeps the space separator. Tests cover `a" --evil`, `C:\my dir\`, bare `C:\`, `a\b"c`, plain `a\b`. Correct.

### P1-3 — loopback enforcement — **FIXED, CONFIRMED**
`src/supervisor.rs:279-281, 616-634`: `ensure_loopback_listen` runs at connect step 0.5 — after the stale-proxy self-heal, **before** any config write or core spawn, so a bad `listen` fails fast into `State::Failed` with a clear message. Correctness: `"localhost"` allowed (ASCII case-insensitive), IP parse → `IpAddr::is_loopback()` — which covers the whole `127.0.0.0/8` (`127.9.9.9` ✓) and `::1` ✓; `0.0.0.0`, `::`, private/link-local IPs, garbage and empty strings are all rejected with `local.listen`-named errors (tested exhaustively). Non-IP strings like `192.168.1.5:1080` (bad config shapes) are rejected rather than guessed ✓. Note: `restart_campaign` doesn't re-check, but the config is immutable per Supervisor and connect_sequence validated it before the loop can exist — sound. Config.rs untouched as agreed (S1-closed).

### P2-1 — misleading "~100 ms disconnect" doc — **FIXED**
`src/supervisor.rs:32-37`: now says "honored within ~100 ms **while the loop is between blocking calls**; worst case one wait_for_port/probe window (~15 s) finishes first" — accurate.

---

## Other diffs — quick adversarial pass (nothing violating my lens)

- **`src/bin/tray.rs`** (REV-B P2-5): GUI-subsystem stderr writes via `let _ = writeln!(…)` instead of `eprintln!` — correct; `eprintln!` panics on a failed stderr write (exit 101); now best-effort + intended exit 2. ✓
- **`src/netmerge.rs:139-145`** (REV-B P2-6): `CREATE_NO_WINDOW (0x0800_0000)` on `run_full` spawns — right constant, right place (the 5 s status polls from the future S3 GUI tray); `ensure_running`'s tray spawn keeps its own DETACHED_PROCESS flags. ✓
- **`.github/workflows/ci.yml`**: fmt + clippy `-D warnings` gate — good. **`release.yml`**: tests before release build, `fail_on_unmatched_files: true`, ships THIRD_PARTY_NOTICES.md — good. **`Cargo.toml`**: dropped unused `Win32_System_LibraryLoader` (zigbuild green confirms), +repository, +`rust-version = "1.78"` (all APIs used are ≤1.78: let-else 1.65, OnceLock 1.70, checked_duration_since 1.39, Duration::is_zero 1.53). ✓
- **`THIRD_PARTY_NOTICES.md`**: cross-checked against Cargo.lock — the serde_json deps listed (itoa, zmij) match the lock file in this environment; license table is accurate; the "user runs separately" section correctly separates Xray-core (MPL-2.0, not redistributed) from shipped crates. ✓
- **`LEDGER.md` / agent reports**: bookkeeping only. ✓

## New findings this round

### P1
1. **`src/core_proc.rs:112-117`** — residual hang in `start()`'s leftover-waiter join under *concurrent external* API use: if two threads call `CoreProc::start()` in the ~50 ms window after a child died but before its waiter exited, thread A can take and join the old waiter while thread B spawns and publishes a **new** child — the old waiter then sees `Some(new child)`, adopts it, and polls as long as it lives, so A's join blocks indefinitely. **Not reachable through `Supervisor`** (op_lock serializes every `core.start()` call site: connect_sequence under op_lock; restart_campaign only runs from a live poll loop, which implies state Connecting/Connected, for which connect() no-ops — I verified there is no state in which both can run), so this is pub-API hardening, not a shipped-path deadlock. Suggested one-liner: only join `is_finished()` handles, else drop (detach) them — the exiting waiter needs no join.

### P2 (new this round)
1. `src/core_proc.rs:283-284, 304-306` — the two `write_all` legs re-arm the budget once per call, so a *theoretical* adversarial partial-write schedule could stretch a write leg past the deadline (multiple full `remaining` windows inside one `write_all`). Physically irrelevant here (3 / ≤262-byte writes, kernel-buffered), but a per-syscall write helper would make the bound airtight like the read side.
2. `src/core_proc.rs:262-266` — `socks_probe` method doc still describes only per-socket timeouts ("nothing blocks forever"); the actual guarantee is now the total deadline — one sentence would align doc and mechanism.
3. `src/supervisor.rs:246-256` — `connect()`'s Connecting/Connected no-op check happens *after* acquiring op_lock, so a redundant connect during a long in-flight operation (worst case ~50 s: 15 s port wait + 20 s probe + 15 s bond wait) blocks the caller instead of returning the no-op promptly. Correctness-first tradeoff is fine (and the simplest race-free form), but a fast-path pre-check would spare S3 tray threads the wait.

### P2 (carried from round 1, unaddressed, non-blocking)
4. `core_proc.rs:384-400` — two detached pump threads append to `xray.log` concurrently; `write_all` may split → interleaved partial lines (log-only).
5. `sysproxy.rs:228-233` — registry snapshot reads use `.ok()`: type-mismatched values (REG_EXPAND_SZ/REG_BINARY `ProxyServer`) snapshot as absent and restore then deletes them; mid-`set` failure leaves `ProxyEnable=1` half-applied until restore.
6. `supervisor.rs:634-653` / `main.rs:106-123` — `write_private` truncates in place (no temp+rename); crash mid-write leaves a partial `xray-client.json`.
7. `supervisor.rs:613-620` — `parse_probe_target` has no IPv6-literal support (brackets not stripped).
8. `main.rs:168` — `doctor` runs `xray version` with no timeout (hung binary hangs doctor).
9. `core_proc.rs:472-477` / `supervisor.rs:722-725` (test-only) — `free_loopback_port` released-port reuse race.
10. `config.rs:200-202` + `sysproxy.rs:239-241` — an empty `proxyOverride` silently becomes the default (can't be disabled via config); `set()` skips writing it when empty, leaving a previous override in force.
11. `main.rs:23` — `--config=path` equals-form unsupported (space-separated only, usage-documented).

## Answers to the orchestrator's specific questions

- **op_lock design sound? New deadlock potential?** Sound. Taken only by connect/disconnect whole-body; the loop/campaign never take it; lock nesting is strictly op_lock → short inner/loop_thread/core acquisitions, never reversed; `spawn_loop` drops the loop_thread guard before joining and both slot writers hold op_lock. `disconnect()` joining the loop under op_lock blocks at most one bounded window (~15-30 s, all constituent timeouts now hard-bounded). No deadlock, no livelock; the round-1 race is provably closed (demo + test).
- **New `socks_read_exact` correct?** Yes — EOF (`Ok(0)`) → clean error, timeout propagation via re-armed per-syscall budget, no panic (fill arithmetic bounded), `socks5:`-context error strings, empty-buffer no-op. Only the write-leg and doc nits above.
- **start() new ordering race (child dies between check and join)?** None in supervisor-reachable flows: a live child at the first check → immediate Err; an empty slot means the waiter's exit doesn't depend on any child; the post-join re-check under the slot lock covers the window. The one residual is the external-concurrent-start case (new P1-1).
- **`ensure_loopback_listen` correct?** Yes — 127.0.0.0/8 via `IpAddr::is_loopback`, `::1`, ASCII-case-insensitive `localhost`; rejects 0.0.0.0/::/private/garbage/empty with clear errors; enforced before any side effect at step 0.5.
- **New tests flaky?** No. Margins: `start_while_running…` 1 s bound on an instant Err; drip test [350,900) ms around a hard 500 ms ceiling (every delivery schedule lands ~500 ms ± wakeup); op_lock test's blocking assertion is deterministic while the guard is held and its 10 s `recv_timeout` faces a sub-second refused-probe path; loopback and quoting tests are pure functions. Suite green 4/4 runs at ~6 s each on this 2-core box.
- **Other diffs** — tray stderr fix, netmerge CREATE_NO_WINDOW, CI/release hardening, Cargo cleanup, notices file: all verified sound; nothing violating correctness/security.

## Verdict

**APPROVE.** All five of my round-1 blocking/should-fix findings are fixed and verified — the two P0s both empirically (round-1 exploit demos now behave correctly against `eb1235a`) and by code analysis; the three P1s verified by construction plus targeted tests. The new op_lock design introduces no deadlock; the reworked probe deadline is genuinely airtight on the read path. One new P1 (pub-API-only residual join hazard in `CoreProc::start`, unreachable via `Supervisor`) and a set of P2 nits remain — none block closing S2; the P1 is a one-line hardening worth folding into S3.

*Repo left untouched (clean tree at `eb1235a`; only this report file written). Demo crate remains in `/tmp/mt-revc-demo`, outside the repo.*
