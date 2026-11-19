# STATE — rewritten at the end of every stage. Last update: S11 (real-Xray E2E rig + stream-bonding field fix, v0.6.2)

## Current position
- Stage: S11 CLOSED — v0.6.2 fixes the "per-connection works, stream bonding
  does not" field report. ROOT CAUSE (server-side): Xray 26.x freedom
  outbound's default safety policy BLACKHOLES loopback targets dialed from
  VLESS ("target blocked by a finalrule") — the merge daemon lives on
  127.0.0.1:<merge port>, so every carrier was black-holed. Client-side
  aggravation: Xray's SOCKS inbound answers "connected" before the upstream
  exists → phantom-live carriers. FIXES: (A) setup-xray-reality.sh writes a
  narrow freedom finalRules allow (127.0.0.1:<MERGE_PORT>, tcp) and, without
  FORCE, IDEMPOTENTLY PATCHES existing installs (backup + run -test +
  restart, credentials kept) — the user's one-command VM fix; (B) mt-bond
  honest carriers: a carrier is schedulable only after a PING/PONG proof
  through the daemon (VERIFY_PONG_TIMEOUT=5s; ensure_carriers + respawn
  demand it; fallback WARN names the fix). Daemon binary UNCHANGED.
- NEW RIG: tests/e2e_real_xray.rs (gated MT_E2E_XRAY=1 + XRAY_BIN=...):
  real VLESS+REALITY engines → real Xray server → in-process daemon → echo.
  It caught the field bug that all mock-link tests could not. Dest must be a
  REAL external site (openssl s_server breaks every REALITY handshake — and
  s_server exits on stdin EOF; both documented in the rig).
- Gates at close: fmt clean; clippy --workspace --all-targets -D warnings
  clean; 26/26 workspace suites green (381 tests) + vendored iced 6;
  E2E 2/2 green (survives + honest fallback).
- USER ACTION REQUIRED for stream bonding: on the VM run
  `sudo bash server/setup-xray-reality.sh` (patches finalRules, keeps
  credentials) and confirm `sudo bash server/setup-merge-daemon.sh` is
  installed/running; then reconnect with bonding mode "stream".

## Next actions
1. USER VERIFICATION of v0.6.2 stream bonding: patch the VM
   (setup-xray-reality.sh without FORCE), confirm the daemon, connect in
   stream mode — expect "carrier <name>: live" lines and BOTH NICs active
   on one download (Status per-adapter graphs).
2. Watch the v0.6.0 release run (tag push) — verify assets like v0.5.0/v0.5.1.
3. Backlog unchanged: weighted carrier scheduling (D17 §4), subscription
   auto-update, vmess validation, profiles.json locking, engine log
   rotation, listener max_conns, upstream iced migration (re-base or
   drop the vendor patches when moving past 0.13).
4. S7 (native REALITY core) still parked.

## Blockers / open items
- None blocking. Watch item: disk headroom (release zigbuild needs ~5G;
  /tmp/mt-target/debug was cleaned before the release build this stage).
  After a sandbox recycle the whole toolchain needs reinstalling (recipe
  below) — budget ~10 minutes.

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
     ../tools/pypkgs EXTRACTION is INCOMPLETE: its ziglang/lib/ holds
     only tsan/ubsan/zig.h, so the cached `zig` binary aborts with
     "unable to find zig installation directory" on any real compile;
     `python3 -m ziglang version` alone still works and LIES);
     `cargo install cargo-zigbuild --locked`.
  3. Vendored-iced builds just work ([patch.crates-io] in the root
     Cargo.toml; vendor/ is committed).
- Cross build: CARGO_TARGET_DIR=/tmp/mt-target cargo zigbuild --release
  --target x86_64-pc-windows-gnu --workspace (clean /tmp/mt-target/debug
  first if disk-tight).
- GUI visual rig: python3 ../scripts/gui_drive.py --app
  /tmp/mt-target/debug/mt-app --script "..." (verbs: wait|shot|click|
  type|key|scroll|resize; scroll:X,Y[,up][,N] = XTEST wheel).
  HiDPI repro: --env WINIT_X11_SCALE_FACTOR=1.5 + --size 2080x1750
  (an sf=1.5 window defaults to 1536x1152 PHYSICAL — a smaller screen
  clamps it and the Status graph card collapses to zero height).
  sf=1.5 control coordinates (mock, resized 2000x1700): connect toggle
  ≈(285,350), Check-exit-IP ≈(324,552), adapter name rows ≈(293,1333)+
  — RE-DERIVE per run (scroll drift); pixel-scan the purple name text
  (primary #6750A4-family bands) or the tonal button (232,222,248)
  instead of trusting VLM coordinates. Needs: Xvfb (apt, preinstalled),
  python-xlib (`pip3 install python-xlib`), ffmpeg (preinstalled), and
  libxkbcommon-x11 + libxcb-xkb extracted into /tmp/xkblibs (Debian
  debs, `apt-get download libxkbcommon-x11-0 libxcb-xkb1` +
  `dpkg-deb -x`) with LD_LIBRARY_PATH — the driver wires all of this
  itself. No WM runs under Xvfb: the driver calls SetInputFocus itself;
  typed paths should be cwd-relative (XTEST Shift-layer chars are
  unreliable). Verify claims with PIXEL scans — VLM coordinates are
  approximate at best.
