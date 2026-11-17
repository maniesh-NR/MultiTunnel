# STATE — rewritten at the end of every stage. Last update: S2 (implementers verified)

## Current position
- Stage: S2 implementation COMPLETE and verified — supervisor (S2-A), core_proc
  (S2-A1), sysproxy+elevate (S2-B) all in tree; full suite 52/52 green;
  windows-gnu zigbuild green (both PE32+ binaries). Two test-harness bugs
  found and fixed by the orchestrator after the S2-A session crashed (see
  LEDGER 2026-11-17 06:30–06:50Z): Rig stub-join deadlock + hardcoded
  restart_max.
- Adversarial review round for S2 is PENDING (this is the next action).

## Next actions
1. Run adversarial review round IN PARALLEL: Reviewer-C (correctness/security)
   + Reviewer-B (build/portability) — both must run `cargo test` and
   `cargo zigbuild --target x86_64-pc-windows-gnu` independently.
2. Resolve findings; record consensus in harness/memory/consensus/S2-consensus.md.
3. harness/backup.sh s2-close; ROADMAP S2 -> [x].
4. Proceed to S3 (tray + icon + release binaries) per ROADMAP.

## Blockers / open items
- None blocking. Toolchain now lives under tools/ (survives recycling).
- Note: 3rd sandbox recycle happened 2026-11-17 ~06:05Z; env recovered per
  HARNESS.md bootstrap contract.

## Environment map (sandbox)
- Repo: /home/z/my-project/MultiTunnel — branch main, remote
  github.com/maniesh-NR/MultiTunnel.
- Toolchains: UNDER /home/z/my-project/tools/ (rustup home + cargo home) —
  `. /home/z/my-project/tools/env.sh` FIRST. Rust 1.100.0, zig (pip wheel),
  cargo-zigbuild 0.24.1, target x86_64-pc-windows-gnu installed.
- Cross build: `cargo zigbuild --target x86_64-pc-windows-gnu` (artifacts in
  /tmp/mt-target/x86_64-pc-windows-gnu/debug/).
- Background processes do NOT survive between commands; run long jobs in the
  foreground with generous timeouts.
