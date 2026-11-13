# STATE — rewritten at the end of every stage. Last update: S2 CLOSED (2026-11-13T06:45Z)

## Current position
- Stage: **S2 CLOSED** with consensus (2×APPROVE round 2; see
  harness/memory/consensus/S2-consensus.md). All four roadmap acceptance
  criteria met: 57/57 tests, host clippy clean, windows-target clippy
  clean, zigbuild green (both bins PE32+, console + GUI).
- Session recovered from RECOVERY-3 via repo memory, merged the diverged
  LEDGER, pushed the never-committed S2 superset, root-caused the suite
  hang (test-rig stub owned a try_clone'd listener fd), survived two full
  adversarial review rounds and a premature-push incident.

## Next actions
1. `harness/backup.sh s2-close` (first action of the next session if this
   one dies before pushing).
2. S3 — Interface: tray module per D4 (raw win32 via windows-sys:
   Shell_NotifyIconW, popup menu Connect/Disconnect/links/raw-status/logs/
   config/Exit, tooltip state, embedded icon asset). Contract notes:
   - MUST run connect()/disconnect() on a worker thread (ops can block
     ~15 s behind op_lock; blocking a menu handler wedges the tray loop).
   - Keep non-UI logic in pure helper fns with unit tests; unsafe FFI will
     get a dedicated reviewer pass per ROADMAP acceptance.
   - tray binary already links GUI-subsystem PE; make src/bin/tray.rs real.
3. S3 review round (Reviewer-C must audit the unsafe FFI specifically),
   then S4 (server script, README/QUICKSTART, v0.1.0 tag → GitHub Release).

## Blockers / open items
- None blocking. Push token available via upload/ (one-shot use only).
- P2 backlog carried from S2 consensus: config::validate loopback check
  (generate/doctor bypass the supervisor guard), sysproxy snapshot .ok()
  typing, temp+rename config writes, IPv6 probe literals, doctor xray
  version timeout, pump line interleaving, --config= equals form, CI
  caching, required status checks.

## Environment map (sandbox)
- Repo: /home/z/my-project/MultiTunnel — branch main, remote
  github.com/maniesh-NR/MultiTunnel (backup.sh pushes, one-shot token URL).
- Toolchains: `. /home/z/my-project/tools/env.sh` FIRST. Rust 1.100.0 +
  zig 0.17.1 + cargo-zigbuild, target x86_64-pc-windows-gnu,
  CARGO_TARGET_DIR=/tmp/mt-target.
- Gates: cargo fmt --check; cargo clippy --all-targets (host);
  cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings;
  cargo test (~6 s); cargo zigbuild --target x86_64-pc-windows-gnu.
- Gate rule (from the 06:19Z incident): the gate command must gate the
  push (exit-code chaining), never `rg || echo` shortcuts.
