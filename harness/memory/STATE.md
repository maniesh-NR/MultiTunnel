# STATE — rewritten at the end of every stage. Last update: S6 CLOSED, v0.3.0 RELEASED (2026-11-18)

## Current position
- **ROADMAP S0–S6 ALL CLOSED. v0.3.0 (All-in-One: Material You GUI, Rust
  core, v2rayN+net-merge functions, zero user-installed deps) IS RELEASED.**
- S6 review round: 2× APPROVE (REV-C 0P0/5P1/15P2, REV-B 0P0/3P1/8P2); all
  8 P1s + 4 cheap P2s fixed; docs rewritten (README/QUICKSTART/NOTICES).
- Release story: first v0.3.0 tag retracted — the frontier CI had failed
  on both runners (mt-net Windows RST error-close class; mt-engine
  respawn race). Both fixed properly (graceful_close everywhere, dedicated
  NetError::ProxyUnreachable, forced respawn + synchronous reap);
  re-tagged; **CI green on ubuntu + windows; release workflow green;
  v0.3.0 zip published** (multitunnel-v0.3.0-windows-x64.zip, 5,561,966
  bytes, both exes PE32+; restored to sandbox download/).
- Workspace at v0.3.0: 8 crates, 321 tests, all gates green (fmt, clippy
  host + windows-gnu via zig shims, -D warnings, release zigbuild PE32+ x3,
  gui --smoke exit 0).

## Next actions
1. USER MANUAL STEP (needs a real Windows box, per QUICKSTART step 2-3):
   run `multitunnel gui` — first REAL display of the Material You window;
   import the live xray-gcp vless link; connect; verify LAN bypass,
   close-window proxy restore, tray coexistence. Note: GUI autostart
   registers the console exe (C-P2-10, v0.3.1 backlog).
2. Optional v0.3.1 backlog (parked; see S6-consensus + reports): GUI
   autostart target, same-tag engine file races, subscription/download
   body-size caps + staging GC, engine log rotation, blocking I/O notes,
   Stopped-event race, vmess pbk/sid validation, listener max_conns,
   profiles.json cross-process locking, mt-app.exe packaging decision.
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

## Environment map (sandbox)
- Repo: /home/z/my-project/MultiTunnel — branch main, remote
  github.com/maniesh-NR/MultiTunnel (origin embeds the token; prefer
  harness/backup.sh for pushes).
- Toolchains: /home/z/my-project/tools/ — source tools/env.sh first
  (RUSTUP_HOME/CARGO_HOME/PATH). Rust 1.100.0, rustfmt + clippy,
  x86_64-pc-windows-gnu target, cargo-zigbuild 0.24.1, ziglang 0.17.1 pip
  wheel (this pair WORKS — the 9th-incident note was a corrupted wheel).
- Cross build: cargo zigbuild --release --target x86_64-pc-windows-gnu
  (workspace) and -p mt-app (third exe). Debug artifacts eat ~5G — clean
  /tmp/mt-target between phases.
- Windows-gnu clippy: sh scripts/mingw-zig-shims.sh /tmp/shims + export
  CC/CXX/AR per the script header.
- Stale registry cache after a sandbox recycle breaks cargo with "path
  source" internal errors → move tools/cargo/registry aside, re-download.
- Notices regeneration: python3 scripts/gen_notices.py (repo copy).
