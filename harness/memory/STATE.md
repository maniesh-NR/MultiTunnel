# STATE — rewritten at the end of every stage. Last update: S12 (balanced+priority scheduling + adapter priority order, v0.7.0, rebased onto v0.6.2)

## Current position
- Stage: S12 CLOSED — v0.7.0 on the user's report "with 3+ adapters most
  traffic rides one adapter, the second less, the third rarely any; want
  even spreading AND a fill-in-order mode with a user-settable adapter
  order + UI refinement". Two new merge policies (DECISIONS D23):
  `balanced` (new stock default — live byte-load spreading with a
  per-connection reserve; kills the one-fat-flow-hogs-one-NIC shape) and
  `priority` (user order, fill #1 until its measured bandwidth saturates
  — a ramp/plateau detector, not a naive threshold — then spill to #2…;
  recovers when load drops). Stream-bond frame scheduling is
  policy-aware too (least writer backpressure / queue high-water), and
  the Adapters tab is the ordering UI (selected rows first with #n chips
  + ↑/↓; the saved bonded_adapters ORDER is the priority).
- S11 (remote session, v0.6.2) fixed STREAM BONDING: Xray 26.x freedom
  default-policy blackholes loopback merge targets → the setup script
  now writes a narrow finalRules allow; client carriers are VERIFIED
  with a PING/PONG round trip before becoming schedulable (usable());
  tests/e2e_real_xray.rs is the real-Xray E2E rig (MT_E2E_XRAY=1).
  S12 was rebased onto it — pick_carrier is policy-aware AND
  verified-only. THE USER'S VM may still need `sudo bash
  server/setup-xray-reality.sh` (idempotent finalRules patch) for
  stream mode to work.
- Gates (re-run post-rebase): fmt clean; clippy host + windows-gnu -D
  warnings clean; 396+ workspace tests / 0 failed (381 on the v0.6.2
  base + the S12 scheduling/ordering tests); vendored iced 6/0; gui
  --smoke exit 0; Xvfb rig pixel-verified the chips, the ↓ swap (chip
  x-centers 283↔305), the 5-row policy menu and the Status chips.
- Protocol/daemon UNCHANGED (D17 framing intact; no VM daemon upgrade).

## Next actions
1. USER VERIFICATION of v0.7.0: Settings shows merge policy "balanced"
   (upgraded files migrate from round-robin automatically); watch the
   Status per-adapter graphs spread more evenly under load; pick
   "priority" and reorder adapters on the Adapters tab (↑/↓) to fill
   #1 first; in stream mode the policy drives frame spreading too. If
   stream bonding still fails: re-run server/setup-xray-reality.sh on
   the VM (finalRules) and server/setup-merge-daemon.sh.
2. Watch the v0.7.0 release run (tag push) — verify assets like
   v0.6.x. Local zip + linux daemon staged in ../download/ (NOTE: they
   were built BEFORE the rebase — rebuild + restage if any size differs
   from the released asset).
3. Backlog: subscription auto-update, vmess validation, profiles.json
   locking, engine log rotation, listener max_conns, upstream iced
   migration, S7 native REALITY core (parked).

## Blockers / open items
- None blocking. Watch item: disk headroom (release zigbuild needs ~5G;
  /tmp/mt-target/debug was cleaned before the release build; the rig
  rebuilds it). After a sandbox recycle the whole toolchain needs
  reinstalling (recipe below) — budget ~10 minutes — AND the clone may
  be a STALE snapshot: fetch origin/main FIRST and rebase before
  building on last session's tree (this stage's push was rejected for
  exactly that reason).

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
