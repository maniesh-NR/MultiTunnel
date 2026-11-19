# S6-REV-C verdict: APPROVE

Reviewed `git diff d721cba..6df8767` (S6 waves 1–3) on top of full-file reads
of every new module (`crates/mt-{links,net,engine,m3,platform,service,app}`,
`src/real_service.rs`, `src/profiles.rs`, `src/main.rs` diffs, workspace
`Cargo.toml`), the frozen spec (harness/CONTRACTS-S6.md), the design laws
(DECISIONS.md D5/D9/D10/D14), and the S2 supervisor/sysproxy contracts the
new paths had to preserve. Sandbox rules obeyed: the leftover-debug-print
finding below was byte-verified with `od -An -c` (not filtered output), and
no credential-like string from any config file was copied into this report
(test vectors referenced are the synthetic CONTRACTS-S6 set).

No P0s. Five P1s (one debug leftover, two regressions of deliberately-fixed
prior guarantees, one fail-open law regression, one routing-policy
regression), plus P2 notes. All 6 gates green.

## Gates

| gate | command | result |
|------|---------|--------|
| 1 | `cargo fmt --all --check` | **PASS** (no output, exit 0) |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** (exit 0, 1m09s) |
| 3 | `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings` | **PASS** (exit 0, 1m02s). Note: the brief's literal `--target x86_64_pc_windows_gnu` (underscores) is rejected by cargo — dashes required. One non-failing cargo-manifest lint appears: `mt-m3` unused dependency `thiserror` (P2-12); it does not trip `-D warnings` (rustc flag). |
| 4 | `cargo test --workspace` | **PASS** — 321 tests, 0 failed, 0 ignored (18+4+2+3+64+13+44+25+140+8 across suites, ~16s slowest suite). |
| 5 | `cargo zigbuild --release --target x86_64-pc-windows-gnu` | **PASS** (exit 0, 3m03s) |
| 6 | `cargo zigbuild --release --target x86_64-pc-windows-gnu -p mt-app` | **PASS** (exit 0, 37s) |

## Findings

### P0 (blocks approval)

None.

### P1 (should fix before release)

1. **`crates/mt-engine/src/engine.rs:336` — leftover debug `eprintln!` in the production liveness path.**
   `child_addr_if_alive()` — called on EVERY engine connect/probe (i.e. per proxied connection) — contains `eprintln!("[dbg-eng] probe pid={pid:?} tw={tw:?}")` in non-test code. Byte-verified with `od -An -c` (sandbox output-filter rule). This violates global contract rule 5 ("tracing for diagnostics, no println! in libs") and will spam stderr of `multitunnel gui` with one line per tunnel connection. It leaks no credentials (pid + try_wait result only) but is exactly the class of leftover that must not ship. One-line deletion.

2. **`crates/mt-engine/src/engine.rs:450-461` — the v0.2 kill-on-close Job Object was not ported; `xray.exe` is orphaned on abnormal GUI death.**
   The module doc claims "the exact `core_proc` discipline", but `src/core_proc.rs:583-618` (added by the 2026-11-17 v0.2-audit REV-C#2 backport, commit ff0abb4) puts the child in a Windows Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so the *kernel* guarantees "the core cannot outlive this process". `ManagedEngine` relies solely on `Drop::kill_current()`, which does not run on `TerminateProcess` (Task Manager kill), console Ctrl+C (the C-runtime exits without unwinding), power loss, or panic-abort — the precise scenarios the v0.2 audit fixed. Normal window close is safe (runtime drop cancels tasks → Arc chain → Drop → kill), so the orphan trigger is abnormal death; the orphan then runs an open REALITY tunnel with a loopback listener indefinitely. Restore the job-object wrapper (cfg(windows)) around the spawned child, or route spawns through `core_proc`.

3. **`src/real_service.rs:919-949,688-714` + `crates/mt-app/src/app.rs:831-842` + `src/main.rs:605-625` — the GUI path loses the S2 supervisor's fail-open/self-heal contract for the system proxy (D5).**
   Three facets, one root cause (RealService has no supervision loop and no lifecycle hook): **(a)** closing the GUI window while Connected (or Ctrl+C on the console host) never calls `disconnect()`/`sysproxy::restore` — no `window::close` subscription, no exit hook in `cmd_gui`; the registry keeps pointing at the now-dead `127.0.0.1:10809`. **(b)** Nothing heals a stale snapshot at startup: v0.2 `connect` step 0 is "sysproxy::restore() best-effort" (supervisor.rs:25, contract line); `RealService::connect_flow` has no equivalent, and `disconnect()` in a fresh instance is a no-op (`sysproxy_set=false`), so a crashed run leaves browsing blackholed until the user manually fixes Windows settings or completes a full connect→disconnect cycle. **(c)** Engine give-up does not restore: after `ManagedEngine` exhausts its respawn budget the outbound errors permanently while `RealService` state stays `Connected` and the system proxy stays set — the v0.2 supervisor restored the proxy on restart exhaustion (supervisor.rs:20-22, D5). Keep-first `save_snapshot` (sysproxy.rs:219-223) does protect the user's original values from being overwritten, so this is recoverable-but-stuck, not data loss. Distinct from the parked "no-snapshot-disable UX" backlog item: v0.2 had all three recovery paths; v0.3's GUI has none.

4. **`src/real_service.rs:696-699,587-588` — LAN/private traffic no longer bypasses the tunnel when the GUI system proxy is on (regression of a stated top-3 design principle).**
   `lib.rs:22` lists "LAN/private traffic bypasses the tunnel" as a project invariant, implemented in v0.2 twice over: the proxy override default is the full LAN bypass list `localhost;127.*;10.*;192.168.*;…;<local>` (config.rs:134-139) AND the generated Xray config routes `geoip:private → direct` (xrayconf.rs:66). The S6 RealService path writes a hard-coded `ProxyOverride="localhost"` only, and the `Router` starts with an empty rule list whose default action is `Proxy` (SettingsFile default rules = `Vec::new()`). Net effect with `system_proxy=true`: requests to 192.168.x/10.x/172.16-31.x (router admin pages, printers, NAS) are sent through the VLESS tunnel to the remote server — breaking local access and leaking LAN hostnames/IPs to the tunnel operator. Fix: seed the default rules with a private-CIDR → direct rule (or route private IPs direct in `ProxyServer::establish`) and write the v0.2 LAN bypass override.

5. **`crates/mt-platform/src/engine.rs:129-133` (via `crates/mt-engine/src/engine.rs:412`) — engine-download errors echo the full request URL, which can carry a credential (D9).**
   `ReqwestFetch::fetch` propagates `reqwest::Error` via `?`; its `Display` includes the full URL ("error sending request for url (…)"). The chain `ensure() → "engine \`{tag}\`: binary download failed: {e}" → connect error string → GUI LogLine + tracing` therefore prints whatever `$MULTITUNNEL_ENGINE_URL` contains — and user-set download-mirror URLs with embedded tokens (`https://host/xray.zip?token=…`) are a common pattern; the token then persists in GUI logs and any pasted bug report. mt-links solved exactly this with `reqwest::Error::without_url()` (subscription.rs:100-108) — mt-platform needs the same treatment in `fetch` (and `error_for_status` on the same line).

### P2 (notes)

1. **`src/real_service.rs:661,932-934` — stale `disconnect_requested` flag can spuriously abort one later connect.**
   A `disconnect()` landing in the window between the post-probe flag check (line 661) and session installation sets the flag but leaves the session Connected; the flag is consumed only by a later `connect_flow`, which then aborts itself right after its probe (one lost connect click, self-healing thereafter). Clear the flag at `begin_connect` or after session install. NEW.
2. **`crates/mt-app/src/app.rs:524-531` — the GUI cannot cancel a Connecting session.**
   `ConnectToggle(false)` only acts when `cur == Connected`; while Connecting the switch silently bounces back. The service already implements cancellation (`disconnect_requested` honored after the probe) — the GUI just never calls it in that state. NEW.
3. **`crates/mt-engine/src/engine.rs:316-324,428-430` + `src/real_service.rs:1015-1020` — two engines with the same profile tag race on shared files.**
   `test_latency` on the active profile while connected builds a second `ManagedEngine` with the same tag; both write `engine-<tag>.tmp` → `engine-<tag>.json` (one can truncate the other's in-flight temp; the fsutil single-writer assumption from S5-REV-C P2-6 is now reachable) and append to the same log handle. Worst case is a corrupt config → one failed spawn → retry; the misdirected port mostly self-corrects. Include the port or a nonce in the config/log filename, or serialize per-tag. NEW.
4. **`crates/mt-links/src/subscription.rs:77-98` — subscription fetch has no body-size cap and accepts plaintext http.**
   A hostile server can stream an unbounded body into `resp.text()` within the 20 s timeout (memory exhaustion), and an `http://` subscription URL leaks the token-in-URL and all returned credentials on the wire with no warning. Latent today (`fetch_subscription` has no GUI/CLI caller yet), but it is a public API that wave 4 will wire. NEW.
5. **`crates/mt-platform/src/engine.rs:129-133,323-327` — engine download has no size cap, and crashed staging/backup dirs are never GC'd.**
   Same unbounded `resp.bytes()` concern for the zip download; and `.engine-old-*`/`.engine-new-*` leftovers from a crash between renames (each with a fresh token) accumulate forever — only the current token's dirs are cleaned. NEW.
6. **`crates/mt-engine/src/engine.rs:321-324,675-682` — `engine-<tag>.log` grows unbounded** (append-only, no rotation), consistent with v0.2 core logs but now multiplied per profile. NEW.
7. **`src/real_service.rs:385-404,696-712` — blocking std-fs and registry I/O inside async methods** (settings/profiles loads, `sysproxy::set/restore` on the tokio worker). Files are tiny and the runtime is multi-threaded so this is a hygiene note, not a hang; mt-platform documents the same choice deliberately. NEW.
8. **`crates/mt-net/src/server.rs:44-49,270` — after `disconnect()`, in-flight tunnels keep the engine child alive** until the connections close (documented drain semantics: listeners close, state flips to Disconnected, but long-lived tunnels keep the server+engine Arcs, so xray runs on after "disconnected"). Surprising-but-documented; consider a hard kill after a grace period. NEW (by-design).
9. **`src/real_service.rs:747-759` vs `crates/mt-net/src/server.rs:270-272` — the `Stopped` event almost never reaches the GUI:** the pump exits on the shutdown flip immediately, while `run()` emits `ProxyEvent::Stopped` up to `DRAIN_GRACE` (3 s) later with no receivers left. Cosmetic ("proxy server stopped" log line lost). NEW.
10. **`src/real_service.rs:1065-1073` + `src/main.rs:52-55` — GUI autostart registers the wrong launch target.** `current_exe()` is the console binary whose default command is `tray`, which errors out ("the tray UI ships as the separate multitunnel-tray.exe binary"); login would flash an error instead of starting anything. Should register `multitunnel-tray.exe` or `… gui`. NEW.
11. **`crates/mt-links/src/link.rs:626-634` — vmess reality `pbk`/`sid` are stored unvalidated** (the 43-char/hex checks exist only in the vless query path and `migrate`). Contained: vmess profiles cannot drive the engine (`endpoint_from_profile` rejects non-VLESS) and cannot be re-rendered. NEW.
12. **`crates/mt-m3/Cargo.toml` — unused `thiserror` dependency** (cargo manifest lint; clippy stays green). Housekeeping.
13. **`crates/mt-net/src/server.rs:554-579` — no connection-count limit on the loopback listeners** (a local same-user process can open unbounded idle connections; handshakes are time-bounded but tunnels are not). Loopback-only mitigates; note for a future `max_conns`. NEW.
14. **`crates/mt-links/src/store.rs` + `src/real_service.rs:967-980` — profiles.json has no cross-process/cross-call locking**; concurrent writers (CLI `profiles add` racing a GUI import, or two GUI imports) are last-writer-wins. Atomic saves prevent corruption, but one profile can be lost. NEW.
15. **Backlog confirmations (parked, unchanged status):** localhost-listen inconsistency (v0.2 path untouched; RealService sidesteps it by binding the `127.0.0.1` literal, real_service.rs:589-590) · join_reader grace loss (v0.2 main.rs untouched) · restore-error surfacing — still not surfaced in the NEW GUI path either (real_service.rs:941-944 logs restore failures only via `tracing::warn`, no `ServiceEvent::LogLine`) · snapshot downgrade doc · CI cache keying · no-snapshot-disable UX · disconnect-latency tray note. All still parked as accepted.

### Verified-clean highlights (no findings)

- **D10/D9 redaction**: every link.rs/migrate.rs error carries lengths only; ignored query params log NAMES only (link.rs:246-248); subscription failures are numbered-line + redacted; `profiles` CLI never prints identity; engine errors carry tag/port/binary-path at most; no `tracing` site in the new code interpolates `identity`/`public_key`/`uuid` (grepped). The one gap is P1-5. The mt-links percent-decode policy (decode query values/userinfo, never authority; pbk/sid alphabet-validated after decode) deviates from v0.2's stricter "no percent-decoding of stored fields" but is the frozen S6 contract's "parse liberally", keeps `parse(render(p)) == p` a fixed point, and D10 strictness still gates the engine path (`endpoint_from_profile`: VLESS×Reality×Tcp×vision-or-absent, polite credential-free errors).
- **mt-platform zip install**: zip-slip is closed by basename allowlist + `..`/`:`/leading-`/` rejection + flat extraction into a private staging dir; SHA256SUMS is verified at locate-time exactly per D14 (advisory warn by default, `MULTITUNNEL_ENGINE_STRICT=1` refuses — by design, not a bug); the staging→rename swap with backup+restore makes partial installs crash-safe; reqwest is `default-features = false` + `rustls-tls` (validation on, no `danger_accept_*` anywhere in the workspace).
- **mt-net data plane**: socks5/http parsers are total-budget (10 s) and size-bounded (16 KiB head, wire-length-bounded ATYP reads), reject non-UTF-8/zero-length domains, answer BIND/UDP with REP 07 and unknown ATYP with REP 08; CONNECT authority forms incl. `[v6]:port` correct, bare IPv6 rejected; `Proxy-*` hop-by-hop headers stripped before forwarding; router label-boundary suffix matching and first-match ordering are exactly per contract (tests pin `evil-example.com`); merge quarantine (3 fails/30 s, lazy expiry, optimistic re-admission, tier fallback) and the single transparent retry on a *different* link match the contract; `pick_link` lock discipline (async health mutex, then std counter mutex, never across await) is sound; graceful shutdown closes listeners promptly and never unwraps an accept.
- **Async/panic hygiene**: no `unwrap`/`expect`/`panic!`/`unreachable!` in non-test code of any new crate (scan; the two `expect("constant parses")` sites are on compile-time constants); b64/percent decoders are library-`Result`-based and lossy; no std lock held across an await in RealService/mt-net/mt-engine; all spawns are either joined, abort-on-shutdown, or bounded by connection lifetime.
- **RealService connect flow**: every failure path before `Connected` (store error, no profile, factory error/empty, bind error, bind timeout, probe error/timeout, mid-connect disconnect) tears down the spawned run/pump tasks and never touches the registry; sysproxy is applied only AFTER `Connected`; `save_snapshot` keep-first protects user originals across crash cycles; cancellation by future drop is safe (sender drop ⇒ `wait_for_shutdown` treats it as stop).

## Verdict rationale

The S6 expert crates are individually solid: parsers are panic-free and credential-redacted, the data plane matches its frozen contract in every detail I could pin down, the zip installer's zip-slip and atomicity story is sound, and all six gates pass with a 321-test suite. What keeps this at five P1s rather than a BLOCK is that none of them is an exploitable hole, credential *value* leak to a remote party, data loss, or a reachable panic/deadlock — the closest to the P0 bar are the D5 fail-open regression (P1-3) and the LAN-bypass regression (P1-4), both of which require the user to enable the system-proxy feature and both recoverable by manual action. But they are real regressions against deliberately-fixed guarantees (the v0.2-audit Job Object, the S2 stale-heal/give-up contract, the "LAN bypasses the tunnel" invariant), and P1-1/P1-5 are trivial fixes, so all five should land before the v0.3 release ships. APPROVE per D6 (zero P0s), with the P1 list as the release blocker checklist.
