# STATE — rewritten at the end of every stage. Last update: S10 (vendored iced rendering fixes + exclusions import, v0.6.0)

## Current position
- Stage: S10 CLOSED — v0.6.0 on the first v0.5.1 field report: (a) graphs
  "occasionally out of frame / don't load" and (b) the "Check exit IP"
  non-total-comparator panic were BOTH root-caused in the terminal iced
  0.13 releases and fixed as vendored patches (vendor/ +
  [patch.crates-io]; DECISIONS D21): damage/clip coordinate spaces in
  iced_tiny_skia, honest MEASURED canvas-text bounds (the INFINITY
  markers produced the NaN damage rectangles), total_cmp in
  iced_graphics::damage::group. Reproduced and verified end-to-end in a
  new Xvfb+XTEST rig (scripts/gui_drive.py): series paint immediately
  after connect (were blank until a resize), right-aligned labels stay
  inside the card, the exit-IP click no longer panics.
- Features: exclusions import (mt-service::parse_exclusion_list: plain
  list + v2rayN routing JSON; Routing-tab import card feeding the DRAFT;
  CLI `multitunnel routing list|import`). Two latent v0.5.x bugs fixed on
  the way: rule-kind vocabulary was kebab in the GUI vs camelCase in the
  backend (every real-backend Routing save failed, smoke tests blind —
  MockService is permissive), and hyphenated merge-policy names never
  parsed ("lowest-latency" silently meant round-robin).
- Gates: fmt clean; clippy host + windows-gnu -D warnings clean;
  371 tests / 0 failed (368 workspace + 3 vendored); zigbuild release
  PE32+ x4; gui --smoke exit 0.
- Protocol/daemon UNCHANGED from v0.5.0 (D17 framing intact; the VM's
  mt-merge-server needs no upgrade).

## Next actions
1. USER VERIFICATION of v0.6.0: graphs render without needing a resize;
   "Check exit IP" is safe; Routing tab import works (plain .txt or
   v2rayN JSON); Settings → Network → merge policy "lowest-latency" now
   actually engages (check per-link spread on Status).
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
