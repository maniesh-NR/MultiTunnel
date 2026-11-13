# S2 Reviewer-B Report — BUILD / PORTABILITY / DEPENDENCIES / LICENSING / TEST QUALITY

- Stage: S2 adversarial review (round 1), Reviewer-B
- Repo: /home/z/my-project/MultiTunnel, branch main, HEAD 59d7bba (working tree clean — verified `git status --short` empty, `git rev-parse HEAD` = 59d7bba751e99707bb08bcb32288808a3d4a69be)
- Date: 2026-11-13
- Scope reviewed: src/core_proc.rs, src/supervisor.rs, src/sysproxy.rs, src/elevate.rs, src/main.rs, src/bin/tray.rs, Cargo.toml, Cargo.lock, .github/workflows/ci.yml, .github/workflows/release.yml (+ S1 modules read as context for cross-module issues)
- No files modified except this report.

## 1. Verification log (all commands run by me, nothing trusted)

| Command | Result |
|---|---|
| `. tools/env.sh; cargo test` | **52/52 ok, 0 failed, finished 5.64 s** (lib 52; bin/tray/doc-test harnesses 0) |
| `cargo test` ×3 more (back-to-back) | 52/52 ×3, no flakes |
| `cargo test` under full 2-core CPU load (2 busy-loops) | 52/52 ok, 6.17 s |
| `cargo test --lib -- supervisor:: core_proc:: --test-threads=1` ×5 | 13/13 ×5 (~6.2 s each) — the timing-sensitive tests repeated single-threaded |
| `touch src/**/*.rs; cargo clippy --all-targets` | clean, no warnings ("Checking multitunnel v0.1.0 … Finished") — rebuild forced, not a cached pass |
| `cargo fmt --check` | clean (exit 0, no diff) |
| `touch src/**/*.rs; cargo zigbuild --target x86_64-pc-windows-gnu` | green, crate genuinely recompiled for the target ("Compiling multitunnel v0.1.0") |
| `file /tmp/mt-target/x86_64-pc-windows-gnu/debug/*.exe` | `multitunnel.exe`: **PE32+ executable for MS Windows 6.00 (console), x86-64**; `multitunnel-tray.exe`: **PE32+ … (GUI), x86-64** — both exes link, subsystems correct |
| `objdump -p multitunnel.exe` (import table) | imports **ShellExecuteW** (shell32), **InternetSetOptionW** (wininet), **OpenProcessToken / GetTokenInformation** (advapi32), **RegOpenKeyExW / RegQueryValueExW / RegSetValueExW** (advapi32, winreg) → definitive proof the cfg(windows) code in elevate.rs/sysproxy.rs compiled **and linked** for x86_64-pc-windows-gnu (not a skipped rebuild) |
| Test inventory (`cargo test -- --list`) | config 9, core_proc 5, elevate 3, netmerge 16, supervisor 8, sysproxy 6, xrayconf 5 = **52** |

## 2. Findings by severity

### P0 — none

Cross-build is green and proven (imports above); windows-sys feature list covers every referenced API; no license contamination; both CI workflows are functional.

### P1 — 3

**P1-1. A `v*` tag releases without any test run.**
`.github/workflows/ci.yml:2-4` triggers only on `push: branches: [main]` and PRs — a tag push does **not** trigger ci.yml. `.github/workflows/release.yml:9-14` goes straight to `cargo build --release` with no test/lint step. So the official release artifact can be built from code that never ran the suite (e.g. a tag pushed ahead of a green main commit). Fix: add a `cargo test` step to build-windows (and ideally ubuntu too) in release.yml, or make the release job depend on the ci workflow.

**P1-2. `THIRD_PARTY_NOTICES.md` is referenced by LICENSE and release.yml but does not exist in the tree.**
`LICENSE:23-25` states "Third-party components … (see THIRD_PARTY_NOTICES.md in releases)"; `release.yml:21-23` lists it in the copy loop behind a `Test-Path` guard. The file is absent, so the guard silently drops it and every release zip ships without the notices the project's own LICENSE promises (MIT attribution for winreg, serde/regex/windows-sys dual-license choice, MPL-2.0 statement for the non-redistributed Xray-core). Not contamination (see §3.5), but it should be authored before the first `v*` tag.

**P1-3. `connect()` and `disconnect()` are not serialized — concurrent call can leave a half-up tunnel with `State::Failed`/`Connected` mismatch (S3 landmine).**
`supervisor.rs:238-260` (connect), `:264-341` (connect_sequence), `:396-433` (disconnect). If `disconnect()` runs while `connect_sequence()` is in flight (state `Connecting`), the loop handle is still `None` (spawn_loop runs at the *end* of connect), so disconnect joins nothing and immediately restores the proxy / stops the core; connect_sequence then continues through steps 4–5 **after** the disconnect: `net-merge on` and `sysproxy::set` re-run, and either the state ends `Connected` with a live loop that undoes the user's disconnect (spawn_loop resets the stop flag at `:386`), or (restartMax=0) `Failed` with net-merge left on and the proxy restored by the campaign but no owner. Also, the loop's `wait_for_port`/`socks_probe` (up to 15 s each) are not stop-flag-aware, so disconnect can block a calling thread ~15 s. Unreachable from the S2 CLI (main.rs: `connect()?` then sleep-forever, single thread), but the module contract (`supervisor.rs:29-31`) explicitly hands the S3 tray an `Arc<Supervisor>` callable "from menu handlers on any thread" — a quick connect→disconnect click sequence is exactly that. Fix before S3: one cycle mutex / in-flight guard around connect+disconnect, and make the loop's port-wait/probe stoppable.

### P2 — 7

**P2-1. Unused windows-sys feature.** `Cargo.toml:27` enables `Win32_System_LibraryLoader`, but no code references `GetModuleHandle`/`LoadLibrary` (grep over src: zero hits). Harmless (declaration-only crate) but it is dead dependency surface — drop it.

**P2-2. CI never enforces the project's own fmt/clippy gates.** `ci.yml` runs only `cargo test` + zigbuild. The harness treats "clippy clean / fmt clean" as gate criteria; a regression in either lands silently. Add a lint job (`cargo fmt --check && cargo clippy --all-targets -- -D warnings`).

**P2-3. `core_proc.rs` test stub still uses the blocking-accept pattern that caused the supervisor hang.** `core_proc.rs:421-444` (`stub_socks`: blocking `listener.accept().expect(...)`, `:454` join). It is provably unhangable here — the probe always connects before anything can fail (addr comes from the bound listener) and the stub thread ends right after writing its reply, independent of the probe — but supervisor's stub was hardened (nonblocking accept + quit flag, `supervisor.rs:705-754`) precisely because this *class* of bug hung the suite once. Apply the same pattern for defense-in-depth.

**P2-4. Narrow `spawn_loop` race after a give-up.** `supervisor.rs:377-392` vs `:497`. If `connect()` reaches `spawn_loop` in the few-instruction window between the old loop's `break` (GaveUp, `:468`) and `loop_running.store(false)` (`:497`), the `loop_running` guard early-returns and the new cycle gets **no poll loop** (Connected, unsupervised). Practically unreachable (connect_sequence takes ≥ms; the window is instructions), but the guard could join-wait instead of returning.

**P2-5. Tray stub `eprintln!` from a GUI-subsystem process.** `src/bin/tray.rs:5-6`: with `windows_subsystem = "windows"` and no console (double-click launch), the stderr handle is invalid and `eprintln!` panics ("failed printing to stderr") → exit 101 instead of the intended 2. Stub-only (real tray is S3); trivial to make robust (ignore the write error / drop the line).

**P2-6. Cross-module note for S3: net-merge spawns lack CREATE_NO_WINDOW.** `netmerge.rs:134-138` (`run_full` — used by the supervisor poll loop every 5 s, `supervisor.rs:474`) sets no creation flags, unlike `core_proc.rs:149-154` for xray. Invisible from the S2 console binary (children share its console), but the S3 tray is GUI-subsystem: spawning `net-merge status` every 5 s will flash a console window. Either add the flag in netmerge or have the tray spawn through a helper that does.

**P2-7. Metadata/hygiene nits.** `Cargo.toml` has no `repository` and no `rust-version` (lock is v4 → needs cargo ≥1.78; `dtolnay/rust-toolchain@stable` is a moving target — pinning `rust-version` documents the floor). `release.yml:29-32` could set `fail_on_unmatched_files: true` so a path typo can't silently produce an asset-less release. No CI caching (optional; suite is fast).

## 3. Scrutiny items — detailed results

### 3.1 cfg gating (both host and windows-gnu compile)
Audited every `#[cfg(...)]` in src (39 occurrences, grep log in session):
- Windows-only API code all behind `#[cfg(windows)]`: `core_proc.rs:149-154` (CREATE_NO_WINDOW via `std::os::windows::process::CommandExt`), `sysproxy.rs:141-221` + `:225-243`/`:253-274` (winreg + WinInet, with `#[cfg(not(windows))]` no-op bodies), `elevate.rs:38-72`/`:79-110` (with non-windows stubs at `:74-77`/`:112-116`), `main.rs:311-319` (UAC relaunch), `netmerge.rs:238-244` (DETACHED_PROCESS tray launch). No POSIX-only traits inside windows blocks anywhere.
- Unix-only code all behind `#[cfg(unix)]`: `OpenOptionsExt` 0600 writes (`sysproxy.rs:94-98`, `supervisor.rs:621-633`, `main.rs:107-118` — each with a `cfg(not(unix))` fallback), fake-xray shell script + all process tests (`supervisor.rs:681-1065`, `netmerge.rs:433-510`).
- The `#[cfg(any(windows, test))]` trick (sysproxy/elevate pure helpers) is correct: pure helpers compile on the host for tests, real registry/UAC code only on windows; the two windows-only mapping tests (`sysproxy.rs:416-449`) are `#[cfg(windows)]`-gated and run on the Windows CI leg.
- Verified BOTH sides compile: host via cargo test/clippy (all `any(windows,test)` items exercised), target via zigbuild forced rebuild + PE import table proof (§1).

### 3.2 windows-sys 0.61 features vs referenced APIs
APIs referenced (grep): `Foundation::{CloseHandle, HANDLE}`, `Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY}`, `System::Threading::{GetCurrentProcess, OpenProcessToken}`, `UI::Shell::ShellExecuteW`, `UI::WindowsAndMessaging::SW_SHOWNORMAL`, `Networking::WinInet::{InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED}`. Cargo.toml features: Win32_Foundation, Win32_Security, Win32_System_Threading, Win32_System_LibraryLoader (unused — P2-1), Win32_UI_Shell, Win32_UI_WindowsAndMessaging, Win32_Networking_WinInet. **Complete**: every referenced module is covered; a missing feature would have failed the target compile, and the zigbuild rebuild + objdump imports (§1) prove the windows paths genuinely compiled and linked, not skipped.

### 3.3 Tray / console subsystems
`file` output (§1): `multitunnel-tray.exe` = PE32+ **GUI** (`#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]` at `tray.rs:2`), `multitunnel.exe` = PE32+ **console**. Both link. Stub behavior nit = P2-5.

### 3.4 Dependency hygiene
Lock: 18 packages, **all** `registry+https://github.com/rust-lang/crates.io-index` — no git deps, no path deps, no wildcard requirements (`serde = "1"`, `regex = "1"` are caret reqs; lock pins exact: serde 1.0.229, serde_json 1.0.154, regex 1.13.1, windows-sys 0.61.2, winreg 0.56.0). Transitive set is the minimum expected: regex → aho-corasick/memchr/regex-automata/regex-syntax; serde_derive → syn/quote/proc-macro2/unicode-ident; serde_json → itoa/memchr/zmij; windows-sys → windows-link; winreg → cfg-if/windows-sys. No baggage. winreg/windows-sys correctly under `[target."cfg(windows)".dependencies]` so the Linux host build pulls neither. Lock committed → reproducible CI. Only nit: unused LibraryLoader feature (P2-1), metadata (P2-7).

### 3.5 Licensing
Per-package: aho-corasick / memchr — MIT OR Unlicense; cfg-if, itoa, proc-macro2, quote, syn, unicode-ident, regex*, serde*, serde_json, zmij, windows-link, windows-sys — MIT OR Apache-2.0; winreg — MIT. **No copyleft anywhere in the build graph.** Xray-core (MPL-2.0) and net-merge run as separate processes and are not redistributed (LICENSE:23-25) — no contamination. Cargo.toml: `license = "MIT"`, `publish = false` ✓, LICENSE file is proper MIT ✓. Open obligation: the promised THIRD_PARTY_NOTICES.md doesn't exist yet (P1-2).

### 3.6 Test quality
- Determinism: no unbounded sleeps anywhere in tests. Bounds: stub socket reads 5 s timeouts; probe timeouts 2–3 s; `joined_disconnect` uses `mpsc` + `recv_timeout(20 s)` (supervisor.rs:829-841); test-7 deadline 15 s with 200 ms polling. All timing assertions are **upper** bounds only (e.g. `<5 s` around a 500 ms/2 s timeout) — nothing that flakes slow; the only exact assertions are pure math (backoff table).
- The just-fixed rig stub (`supervisor.rs:705-754`, Drop at `:844-857`): pattern is **sound** — listener set nonblocking (`:706-708`), loop re-checks `quit` every iteration, `WouldBlock` → 10 ms sleep, `Err(_)` (listener truly closed) → break; `Rig::drop` flips `quit` **first**, then disconnect (bounded), then drops the rig-owned listener, then joins the stub (≤ ~10 ms) — the try_clone'd-fd hang is structurally impossible now. `refuse` mode drops the accepted socket immediately (fast probe failure, no 20 s wait). Cleanup: fake xray killed+reaped via disconnect→core.stop; temp dirs removed in Drop and in every non-rig test; core_proc test (e) removes its log dir; sysproxy tests remove their dirs.
- Process tests: all `#[cfg(unix)]` (supervisor rig tests `:951,:990,:1019,:1038`; netmerge POSIX-tool tests). Windows-leg tests (43 of 52 run on windows-latest) are pure/loopback/spawn-failure only.
- Network: loopback only (`127.0.0.1:0` listeners everywhere; probe hostnames go through the in-test stub, never DNS or real egress).
- Registry: never touched from tests — all sysproxy tests exercise serde/file helpers against throwaway temp state dirs; HKCU code is `#[cfg(windows)]` and untested by design.
- Residual nit: core_proc's older stub still blocks in accept (P2-3, provably safe here).

### 3.7 CI workflows
- ci.yml: matrix `ubuntu-latest` + `windows-latest` running `cargo test` (right — Windows leg compiles the MSVC side of all cfg(windows) code and runs the 2 windows-only sysproxy tests), plus an ubuntu `cross-check` job installing cargo-zigbuild via `taiki-e/install-action` and building `--target x86_64-pc-windows-gnu` (mirrors the sandbox gate). Gaps: no fmt/clippy (P2-2); tag pushes bypass it (P1-1).
- release.yml: triggers on tags `v*`, `permissions: contents: write` ✓, windows-latest + stable toolchain (MSVC) ✓, `cargo build --release` builds **both** bins ✓, copies `target/release/multitunnel.exe`, `multitunnel-tray.exe`, `config.example.json`, `LICENSE` (paths correct for default CARGO_TARGET_DIR) ✓, `Test-Path` guards for optional docs ✓, zips via Compress-Archive and attaches to a GitHub Release via `softprops/action-gh-release@v2` with `files: dist/*.zip` ✓ — this works on GitHub runners. Gaps: no tests (P1-1), missing notices file (P1-2), no `fail_on_unmatched_files` (P2-7). Nothing here fails on GitHub but not locally.

### 3.8 Watch item — first-compile-load flake, 2-core machines
This sandbox is itself a 2-core box (nproc = 2). Suite green **8 suite-equivalent runs this session**: 1 baseline (5.64 s) + 3 back-to-back + 1 under full CPU contention from two busy-loops (6.17 s) + 5 single-threaded focused runs of the 13 supervisor/core_proc tests. Margins are ≥3× everywhere (give-up path needs ~5.3 s of its 15 s deadline — dominated by the fixed 5 s TICK, not scheduler noise; worst-case disconnect join ≈ 15.2 s < 20 s recv_timeout; 0.5 s/2 s timeouts asserted < 5 s). The historical one-off flake predates the rig-hang fix and did not reproduce; the only load-sensitive step (fake-xray spawn) is off the critical path (the stub owns the port, so wait_for_port never depends on the child). Residual flakiness risk: **low**.

## 4. Verdict

**APPROVE** — 0 P0. The mandatory gates all pass on my own runs (52/52 in ~6 s, clippy clean, fmt clean, both PEs link with correct subsystems, windows code proven linked via import table). The three P1s are real but none blocks S2: P1-1/P1-2 are release-pipeline hygiene fixable before the first `v*` tag; P1-3 is a concurrency contract gap that is unreachable from the S2 CLI and must be fixed before the S3 tray starts calling the supervisor from menu threads.
