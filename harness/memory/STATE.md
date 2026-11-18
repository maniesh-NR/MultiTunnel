# STATE — rewritten at the end of every stage. Last update: S9 (stream bonding, v0.5.0)

## Current position
- Stage: S9 CLOSED — stream bonding (true per-stream split + server-side
  merge), Status-tab live dashboard (bandwidth graph, server picker, exit
  identity probe) and Advanced settings (v2rayN/net-merge parity) shipped
  as v0.5.0. All gates green: 356 tests / 0 failed, fmt clean, clippy
  host + windows-gnu -D warnings clean, zigbuild x86_64-pc-windows-gnu
  release PE32+ x4 (console, tray, gui, mt-merge-server.exe), linux
  mt-merge-server ELF verified listening, `gui --smoke` 6/6 on the real
  backend.
- Shipped surface: crates/mt-bond (D17 protocol: 24-byte framing,
  per-session reorder, cumulative-ACK replay-on-carrier-death, PING/PONG
  keepalives; client BondOutbound as a single mt-net Outbound; daemon in
  server/setup-merge-daemon.sh); bond_mode setting with NEVER-BLACK-HOLE
  fallback; exit probe (D18) answering the user's Singapore-exit question
  in-product; Status rows = carriers in stream mode; engine loglevel,
  ports, allow-LAN, probe/exit targets in Advanced settings.

## Next actions
1. Watch the v0.5.0 release run (windows zip + linux daemon assets) and
   verify the assets (PE32+ x2 in zip, 2 linux ELFs).
2. USER VERIFICATION on real Windows + the GCP VM: run
   `sudo ./server/setup-merge-daemon.sh` on the VM, pick "stream" mode,
   tick two adapters, confirm (a) Status shows two carrier rows, (b) a
   big download keeps flowing when one adapter is disabled, (c) exit
   probe explains the Singapore IP (expected: Host Universal Pty Ltd =
   the VM's VPN egress, NOT a hidden hop).
3. Backlog: weighted carrier scheduling by RTT/throughput (D17 §4 future),
   subscription auto-update, vmess validation, profiles.json locking,
   engine log rotation, listener max_conns (carried from v0.3.1).
4. S7 (native REALITY core) still parked.

## Blockers / open items
- None blocking. Watch item: disk headroom (release zigbuild needs ~5G;
   debug dirs were cleaned pre-build). The cargo registry needed a REPAIR
   this stage (sandbox recycle dropped .cargo-checksum.json from 390
   extracted crates — removed them; cargo re-extracted from the .crate
   cache; note for next recycle).

## Environment map (sandbox)
- Repo: /home/z/my-project/MultiTunnel — branch main, remote
  github.com/maniesh-NR/MultiTunnel (origin URL embeds the token; push via
  harness/backup.sh or the existing remote).
- Toolchains: /home/z/my-project/tools/ — source tools/env.sh first
  (RUSTUP_HOME/CARGO_HOME/PATH; CARGO_TARGET_DIR=/tmp/mt-target).
  Rust 1.100.0, zig 0.14.1 via the pip wheel (`python3 -m ziglang`,
  reinstalled with `pip3 install ziglang==0.14.1` after this recycle),
  cargo-zigbuild, target x86_64-pc-windows-gnu. Windows-target clippy:
  `sh scripts/mingw-zig-shims.sh /tmp/mt-zig-shims` then export the
  CC/CXX/AR vars (see the script header).
- Cross build: cargo zigbuild --target x86_64-pc-windows-gnu --release
  --workspace
