# STATE — rewritten at the end of every stage. Last update: v0.3.1 HOTFIX (2026-11-18)

## Current position
- **v0.3.0 (S6, all-in-one) released; first real-Windows user run hit the
  bare-launch UX bug → v0.3.1 hotfix implemented, gated, and shipped.**
- v0.3.1 changes (D15): no-args launch = the Material You GUI (was the
  `tray` pointer error — a double-click looked like "no launch activity");
  Explorer/autostart launches free their orphan console (FreeConsole when
  sole owner); GUI errors and panics can no longer die invisibly
  (MessageBoxW when no shared console); `gui_args` empty-argv panic fixed
  (caught by running the host binary bare); README/QUICKSTART updated
  (QUICKSTART step 2 leads with double-click).
- Workspace: 8 crates, 323 tests, all gates green (fmt, clippy host +
  windows-gnu via zig shims, -D warnings, release zigbuild PE32+ x3,
  gui --smoke exit 0, bare host launch reaches iced).

## Next actions
1. USER MANUAL STEP (retry on the v0.3.1 zip — now double-click friendly):
   double-click `multitunnel.exe`; import the live xray-gcp vless link on
   the Servers screen; connect; verify LAN bypass, close-window proxy
   restore, tray coexistence. If a MessageBox ever appears, its text is the
   bug report.
2. Optional v0.3.2 backlog (parked; see S6-consensus + reports): same-tag
   engine file races, subscription/download body-size caps + staging GC,
   engine log rotation, blocking I/O notes, Stopped-event race, vmess
   pbk/sid validation, listener max_conns, profiles.json cross-process
   locking, mt-app.exe packaging decision. (C-P2-10 "autostart registers
   the console exe" is largely resolved by the D15 console policy.)
3. S7 (parked): native pure-Rust REALITY core.

## Blockers / open items
- None blocking. GitHub pushes MUST go through harness/backup.sh (token in
  upload/, never printed).
- RESUME RULE (violated in incidents #9/#10): `git fetch origin && git
  log --oneline -5 origin/main` FIRST on every session start, BEFORE any
  work. The local snapshot can be days behind the remote.
- CI lesson (v0.3.0 tag retraction): ALWAYS check the frontier's latest CI
  runs (actions API) before cutting a tag — a green local sandbox is not
  proof of green runners (Windows RST semantics + timing races differ).
- v0.3.1 lesson (D15): after changing a dispatch default, run the binary
  BARE on the host — unit tests and `gui --smoke` never exercise the
  empty-argv path.

## Environment map (sandbox)
- Repo: /home/z/my-project/MultiTunnel — branch main, remote
  github.com/maniesh-NR/MultiTunnel (origin embeds the token; prefer
  harness/backup.sh for pushes).
- Toolchains: /home/z/my-project/tools/ — source tools/env.sh first
  (RUSTUP_HOME/CARGO_HOME/PATH). Rust 1.100.0, rustfmt + clippy,
  x86_64-pc-windows-gnu target, cargo-zigbuild 0.24.1, ziglang 0.17.1 pip
  wheel (this pair WORKS — the 9th-incident note was a corrupted wheel).
- Cross build: CARGO_TARGET_DIR=/tmp/mt-target cargo zigbuild --release
  --target x86_64-pc-windows-gnu --workspace (3 exes). Debug artifacts eat
  ~5G — clean /tmp/mt-target between phases.
- Windows-gnu clippy: sh scripts/mingw-zig-shims.sh /tmp/shims + export
  CC/CXX/AR per the script header.
- Stale registry cache after a sandbox recycle breaks cargo with "path
  source" internal errors → move tools/cargo/registry aside, re-download.
- Notices regeneration: python3 scripts/gen_notices.py (repo copy).
