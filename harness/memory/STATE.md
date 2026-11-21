# STATE — rewritten at the end of every stage. Last update: S19 (server editing + offline probes + speed test + diagnostics + kill switch, v0.13.0)

## Current position
- Stage: S19 IMPLEMENTED, all local gates green, review fixes in,
  releasing v0.13.0.
- The user's six asks shipped: (1) SERVER EDITING — Servers tab per-card
  Edit button → full-window modal (name/remark/address/port/UUID +
  REALITY fields for reality profiles); mt-service ProfileDetails
  round-trip (profile_details → update_profile); ProfileStore::update
  replaces BY ID in place (id/position/active preserved, no dedupe on
  explicit edit); RealService validates primitives + a
  capability-regression check (an edit that would break a convertible
  profile is rejected with the reason). (2) EXIT IP WHILE OFFLINE —
  exit_probe prefers the live session, falls back to
  temp_link_for() (the same factory path test_latency uses);
  ExitInfo.via_temp_tunnel drives a "temporary probe tunnel" chip.
  (3) SPEED TEST — Status card; speed_test(None) rides the live session
  for the active profile (else the temp tunnel); downloads
  settings.speed_test_url (default
  http://speed.cloudflare.com/__down?bytes=25000000, plain-HTTP verified);
  clock starts after connect (engine spawn excluded); 5 s stall / 30 s
  budget / 64 MiB caps; ≥400 rejected. (4) DIAGNOSTICS MODE —
  src/diagnostics.rs: one redacted paste-ready
  <log_dir>/diagnostics-YYYYMMDD-HHMMSS.log (header + GUI LogLine tee +
  ALL tracing events via a hand-rolled global subscriber; engine level
  forced debug; UUID/pbk never written — test-pinned); Settings card
  with the path + "Open log folder". (5) KILL SWITCH — give_up()
  fail-CLOSED: system proxy re-pointed at 127.0.0.1:9 (review fix: the
  app BINDS the port itself while engaged — spawn_blackhole accept loop
  swallows connections — so a local process cannot pre-bind it); every
  restore path releases it; explicit disconnect restores even with no
  session; Status banner "KILL SWITCH ACTIVE" when Failed + armed.
  (6) CODE REVIEW — Copilot channels unavailable from the sandbox (no
  account entitlement → reviewer requests silently dropped;
  models.github.ai proxy-intercepted): a 3-round discussion-based review
  ran on the strongest reachable model instead (z-ai chat), 4 findings
  fixed (above), the rest assessed + documented in the transcript;
  review PR #1 left open (base v0.12-line) for a Copilot pass from an
  entitled machine.
- 466/0 workspace tests (+19 vs v0.12); fmt/clippy host+windows-gnu;
  vendored iced 6/0; smoke 0; rig sf=1.0+1.5 VLM-verified (52.0 Mbps
  chip row, offline exit-IP chip, modal fields + rename→save→card
  updated, Cancel + backdrop dismissal, kill switch + diagnostics
  toggles); PE32+ ×4 + ELF ×2; version 0.13.0.

## Next actions
1. Tag v0.13.0 → release.yml; watch the run; verify assets by
   download+unzip (PE32+ in zip, ELFs, README/QUICKSTART carried).
2. User-side Windows verification: Edit dialog against a real profile
   (save + reconnect-apply), speed test + offline exit IP against the
   real server, diagnostics file paste-readiness (turn on, connect,
   reproduce an issue, paste), kill switch behavior after killing the
   VM side.
3. Review PR #1: request Copilot from an entitled machine
   (`gh pr edit 1 --add-reviewer Copilot`) if the account gains a
   Copilot seat; otherwise close it after the release.

## Watch items (unchanged unless noted)
- Disk: 9.9G rootfs — the release zigbuild + debug trees TOGETHER do not
  fit; clean /tmp/mt-target/debug before release builds (hit ENOSPC +
  ld signal 7 twice this stage). Stage release binaries in
  ../download/v0.13.0-staging/ before wiping target trees.
- Sandbox recycles cost: registry src/ wiped (rm -rf
  $CARGO_HOME/registry/src re-extracts), ziglang lib/std wiped
  (pip reinstall ziglang==0.14.1), zig symlink missing (recreate
  tools/zig/zig → pypkgs/ziglang/zig), /tmp/xkblibs re-extract +
  libxkbcommon-x11.so + libxcb-xkb1 symlinks, python-xlib pip install.
- `z-ai chat --thinking` is BROKEN in the current CLI (instant exit 1)
  — plain mode works; VLM `z-ai vision` works.
- rig coordinate discipline: derive control coords from a screenshot of
  the SAME scroll position before clicking (scroll-then-locate-then-
  click in separate runs); the app window is 1080x720 — clicks beyond
  x≈1080 land outside it.
