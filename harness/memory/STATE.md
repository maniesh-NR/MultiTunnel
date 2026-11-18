# STATE — rewritten at the end of every stage. Last update: S8 (adapter bonding, v0.4.0)

## Current position
- Stage: S8 CLOSED — the adapter-bonding menu the user asked for shipped as
  v0.4.0. All gates green: 335 tests / 0 failed, clippy clean, fmt clean,
  zigbuild x86_64-pc-windows-gnu green (multitunnel.exe, multitunnel-tray.exe,
  mt-app.exe), `gui --smoke` 6/6 views on the real backend.
- Shipped surface: GUI "Adapters" screen (checkbox rows, IPv4 + up/down chips,
  Apply/Revert/Refresh); one ManagedEngine per selected adapter bound via Xray
  `sendThrough`; merge policies spread connections across the per-adapter
  links; `multitunnel adapters` CLI; settings.json carries
  `bondedAdapters` (v0.3 files load unchanged via serde default).
- Review mode this stage: single-implementation with orchestrator audit (the
  MoE split was one parallel expert for mt-platform + orchestrator for the
  contract/engine/service/GUI); no adversarial consensus round was run —
  FLAGGED as process debt if S9 needs a reference precedent.

## Next actions
1. Watch the v0.4.0 release run (release.yml on windows-latest) and verify
   the zip publishes (PE32+ x2 in dist).
2. User verification on real Windows: Adapters screen lists NICs, selection
   persists, Status screen shows N links (one per adapter) while connected.
3. Backlog from v0.3.1 notes still open (engine log rotation, profiles.json
   locking, vmess validation, listener max_conns, body-size caps).
4. S7 (native REALITY core) remains parked; pick it up or queue S9 from the
   v0.3.1 backlog + user feedback.

## Blockers / open items
- None blocking. Watch item: disk headroom in the sandbox (the 9.9G rootfs
  filled twice during S8; clean /tmp/mt-target/debug before big builds).

## Environment map (sandbox)
- Repo: /home/z/my-project/MultiTunnel — branch main, remote
  github.com/maniesh-NR/MultiTunnel (origin URL embeds the token; push via
  harness/backup.sh or the existing remote).
- Toolchains: /home/z/my-project/tools/ — source tools/env.sh first
  (RUSTUP_HOME/CARGO_HOME/PATH incl. zig; CARGO_TARGET_DIR=/tmp/mt-target).
  Rust 1.99.1, zig 0.17.1, cargo-zigbuild, target x86_64-pc-windows-gnu.
- Cross build: cargo zigbuild --target x86_64-pc-windows-gnu --workspace
