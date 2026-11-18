# STATE — rewritten at the end of every stage. Last update: S6 review round closed + P1 fixes (2026-11-18)

## Current position
- **ROADMAP S0–S5 CLOSED; S6 (v0.3 All-in-One) IMPLEMENTED, REVIEWED AND
  FIXED — ready to release.**
- S6 review round 2026-11-18: 2× APPROVE under D6 (REV-C 0P0/5P1/15P2,
  REV-B 0P0/3P1/8P2). All 8 P1s fixed + 4 cheap P2s folded (details in
  harness/memory/consensus/S6-consensus.md and ROADMAP).
- Gates re-verified on the fixed tree: fmt --all; clippy host +
  windows-gnu (zig shims) -D warnings; cargo test --workspace 321/0;
  release zigbuild PE32+ x3 (multitunnel.exe console, multitunnel-tray.exe
  GUI, mt-app.exe); `gui --smoke` exit 0 ("6 ticks, 0 events, 6 views").
- Docs rewritten for v0.3: README (all-in-one architecture, workspace
  table, gui/profiles/import CLI, MSRV 1.97), QUICKSTART (GUI-first path,
  legacy v0.2 as appendix), THIRD_PARTY_NOTICES regenerated from
  Cargo.lock (391 crates, scripts/gen_notices.py in-repo, OFL fonts +
  license elections recorded).

## Next actions (v0.3.0 release, in order)
1. `harness/backup.sh s6-review-fixes` (commit + push of the fix set).
2. Tag `v0.3.0` + push → release.yml (workspace gates + MSVC build + zip
   + GitHub Release). Verify the workflow run and the attached asset
   (`fail_on_unmatched_files` set); restore the zip to sandbox download/.
3. User manual steps (needs a real Windows box): run `multitunnel gui`
   (first REAL display smoke of the Material You window — window open,
   theme, import of the live xray-gcp vless link, connect, LAN bypass,
   close-window proxy restore), tray coexistence check, autostart check
   (note C-P2-10: autostart registers the console exe — verify it errors
   sanely; fix queued for v0.3.1).
4. Optional v0.3.1 backlog (parked, see consensus): GUI autostart target,
   same-tag engine file races, body-size caps, engine log rotation,
   profiles.json locking, vmess pbk/sid validation, max_conns.

## Blockers / open items
- None blocking. GitHub pushes MUST go through harness/backup.sh (token in
  upload/, never printed).
- RESUME RULE (violated twice, incidents #9/#10): `git fetch origin && git
  log --oneline -5 origin/main` FIRST on every session start, BEFORE any
  work. The local snapshot can be days behind the remote.

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
