# STATE — rewritten at the end of every stage. Last update: S9.1 (D20 visibility fixes, v0.5.1)

## Current position
- Stage: S9.1 CLOSED — v0.5.1 hotfix on the first real two-adapter run
  report: (a) Status graph drew in widget space (iced canvas frames are
  LOCAL; every point was double-offset out of the card — "graph does not
  load"), fixed + regression test; (b) merge policy default failover →
  round-robin (failover left every connection on the FIRST adapter — read
  as broken bonding) with a settings_version=1 stock-value migration
  (explicit failover respected from v1 files); (c) stream-bond connect now
  logs per-carrier verdicts (CarrierReport) + skipped-adapter WARNs;
  (d) Adapters tab rows expand (▸) into per-interface mini graphs (1 s
  deltas of cumulative link/carrier wire bytes, engine-tag → adapter
  matcher), combined Status graph got a color-coded legend. All gates
  green: 371 tests / 0 failed, fmt clean, clippy host + windows-gnu -D
  warnings clean, zigbuild x86_64-pc-windows-gnu release PE32+ x4, linux
  mt-merge-server ELF, gui --smoke 6/6 real backend.
- Protocol/daemon UNCHANGED from v0.5.0 (D17 framing intact — the VM's
  v0.5.0 mt-merge-server needs no upgrade; v0.5.1 binaries interop).

## Next actions
1. USER VERIFICATION of v0.5.1: install the zip, connect with both
   adapters ticked — expect round-robin connection spreading by default
   (both NICs carry traffic), the Status graph rendering with legend, and
   ▸-expandable per-adapter graphs. In stream mode the log must name each
   carrier live/down — if Wi-Fi still carries nothing, the log line now
   says WHY (dial failure / skipped / daemon down).
2. Watch the v0.5.1 release run (tag push) — verify assets like v0.5.0.
3. Backlog unchanged: weighted carrier scheduling (D17 §4), subscription
   auto-update, vmess validation, profiles.json locking, engine log
   rotation, listener max_conns.
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
