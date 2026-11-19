# STATE — rewritten at the end of every stage. Last update: S13 (Status tab: one connection frame + one page scrollable, v0.7.1)

## Current position
- Stage: S13 CLOSED — v0.7.1 on the user's report "the Status
  scrollbars only scroll the adapter list; the wheel should scroll
  everything below the connection frame; wide gap between the connect
  toggle and server dropdown; the graph should use that free space".
  D24: ONE fixed connection frame (state text + toggler + picker
  stacked in a 296px left column, STATUS_FRAME_LEFT_WIDTH; combined
  graph in a fixed-240px panel — STATUS_GRAPH_HEIGHT, D22 rule intact —
  Fill-filling the remaining frame width) + ONE page scrollable below
  it owning exit card + Links/Adapters card. Mock seeds EIGHT adapters
  (3-NIC bond + Bluetooth/vEthernet-WSL/Tailscale/Loopback/VirtualBox);
  the richer seed exposed a stale test idiom (apply used all row ids;
  real handler filters to selected — fixed to mirror production).
- Rig-verified (Xvfb+XTEST, pixel-cluster): frame anchors byte-stable
  across scrolls at sf=1.0 AND sf=1.5; all 8 adapter rows render in
  priority order, last row visible at max scroll; expansion, exit-probe
  verdict card, resize round-trip all work; 0 panics. Row-ID LESSON:
  identify by x-cluster widths (gap>12) — chips + AA grays contaminate
  raw extents; link rows are DARK text, purple bands = adapter names.
- Gates: fmt clean; clippy host + windows-gnu -D warnings clean; 398
  workspace tests / 0 failed; vendored iced 6/0; gui --smoke exit 0;
  zigbuild PE32+ x4 + linux ELF; release 44574748448 SUCCESS, CI
  44574711227 ALL GREEN; assets verified by download (zip 6,003,079 B,
  PE32+ x2, docs carry v0.7.1 wording; mt-merge-server ELFs x2).
  Staged in ../download/: multitunnel-v0.7.1-windows-x64.zip (the
  VERIFIED CI zip) + mt-merge-server-v0.7.1-x86_64-unknown-linux-gnu.

## Next actions
1. USER VERIFICATION of v0.7.1: wheel scrolls everything below the
   connection frame; graph fills the frame; long adapter lists fully
   reachable; expansion + exit probe unchanged.
2. If stream bonding still fails on the VM: sudo bash
   server/setup-xray-reality.sh (idempotent finalRules patch) and
   server/setup-merge-daemon.sh.
3. Backlog: subscription auto-update, vmess validation, profiles.json
   locking, engine log rotation, listener max_conns, upstream iced
   migration, S7 native REALITY core (parked).

## Blockers / open items
- None blocking. Watch item: disk headroom (release zigbuild needs ~5G;
  /tmp/mt-target/debug was cleaned before this release build and the
  rig rebuilt it). After a sandbox recycle the toolchain needs
  reinstalling (recipe below) AND the clone may be STALE: fetch
  origin/main FIRST and rebase before building.

## Environment map (sandbox, rebuilt this stage)
- Repo: /home/z/my-project/MultiTunnel — branch main, remote
  github.com/maniesh-NR/MultiTunnel. Push via harness/backup.sh (reads
  the token from ../upload/, never printed). GitHub REST works with the
  token (unauthenticated api.github.com is 403 in this sandbox).
- Toolchain (rebuild recipe after a recycle):
  1. rustup: `curl -sSfL static.rust-lang.org/.../rustup-init` →
     `./rustup-init -y --profile minimal --default-toolchain stable
     --component rustfmt,clippy`; `rustup target add
     x86_64-pc-windows-gnu`. (Rust 1.100.0.)
  2. zig: `pip3 install ziglang==0.14.1` (NETWORK install — the
     ../tools/pypkgs EXTRACTION is INCOMPLETE); `cargo install
     cargo-zigbuild --locked`.
  3. Vendored-iced builds just work ([patch.crates-io] in the root
     Cargo.toml; vendor/ is committed).
- Cross build: CARGO_TARGET_DIR=/tmp/mt-target cargo zigbuild --release
  --target x86_64-pc-windows-gnu --workspace (clean /tmp/mt-target/debug
  first if disk-tight). Zip manifest: multitunnel.exe,
  multitunnel-tray.exe, config.example.json, LICENSE,
  THIRD_PARTY_NOTICES.md, README.md, QUICKSTART.md.
- GUI visual rig: python3 ../scripts/gui_drive.py --app
  /tmp/mt-target/debug/mt-app --script "verb|verb|..." (verbs:
  wait|shot|click|type|key|scroll|resize; script verbs are PIPE-separated,
  scroll args comma-separated). Needs Xvfb (preinstalled), python-xlib
  (`pip3 install python-xlib`), ffmpeg (preinstalled), libxkbcommon-x11 +
  libxcb-xkb extracted into /tmp/xkblibs (Debian debs, `apt-get download
  libxkbcommon-x11-0 libxcb-xkb1` + `dpkg-deb -x`) — the driver wires
  LD_LIBRARY_PATH itself. No WM under Xvfb: the driver calls
  SetInputFocus itself; typed paths should be cwd-relative.
- RIG COORDINATES (mock, default 1280x720 window): rail buttons ≈
  (58, 18+i·47) — Status 18, Servers 65, Adapters 112, Routing 159,
  Settings 206, Logs 253. Adapters tab: row 1 chip ≈ y 254, ↑ ≈ (988,
  254), ↓ ≈ (1015, 254) (rows every ~61px; the rightmost card edge is
  ≈ x 1055; the "up" chip + its text sit at x 940-970). Settings: the
  merge-policy pick_list ≈ (347, 359), menu rows ≈ every 33px below
  y 376. The iced default window is 1280x720 — the Status adapters
  section is below the fold at that height: `resize:1280x1150` (needs
  --size ≥ that) makes the bottom card show; chip color for pixel
  scans: primary container #EADDFF (234,221,255) tol 5, >40 px per
  row band. RE-DERIVE per run (scroll drift); verify with PIXEL scans,
  not VLM coordinates. A quick pixel-scan helper lives at
  ../scripts/scan_priority_chips.py.
