# STATE — rewritten at the end of every stage. Last update: S18 (tray tooltip/icon rework + color wheel + theme modes + research-driven scheduler refinements, v0.12.0)

## Current position
- Stage: S18 IMPLEMENTED + all local gates green, releasing v0.12.0.
- The user's six asks shipped (D28): (1) 3-line tray tooltip —
  `MultiTunnel v0.12.0` / `Total ↓x ↑y` / `↓x/s ↑y/s` (CRLF, ≤70 NIF_TIP
  units worst case); (2) ALL runtime icons (tray ICOs, window/taskbar
  logo_64, titlebar logo_24) transparent-background + hue-preserving
  contrast boost + tighter margin (0.08→0.04); (3) Settings accent COLOR
  WHEEL (256px HSV disc asset + inverse hit-test/marker math in theme.rs,
  live re-theme during drag, persist on release); (4) System/Light/Dark
  mode with OS detection (env seam → Windows AppsUseLightTheme →
  gsettings/gtk) re-polled every 5 s in System mode, legacy dark_mode
  kept as the forward-compat shadow; (5) docs/research/ bonding survey —
  BLEST tie-guard + ECF completion scheduling IMPLEMENTED in mt-bond
  (reconciled with S14: the guard only narrows the min-score tied set),
  redundant-mode/FEC/reinjection/delay-signal roadmap documented; (6)
  docs/research/ UI changelog survey (CVR/v2rayN/FlClash/Hiddify/
  Speedify) with a prioritized adopt-next list.
- 447/0 workspace tests; fmt/clippy host+windows-gnu; vendored iced 6/0;
  zigbuild PE32+ ×4 + ELF ×2; rig verified sf=1.0+1.5 (wheel pick
  purple→green end-to-end, mode buttons light↔dark, system env boot,
  emblem legibility both modes).
- Sandbox recycled AGAIN before the stage (registry/rustup/zig/xkblibs
  gone, tools/zig tarball truncated): full D2 bootstrap re-run, env.sh
  rewritten; disk hit 100% twice — stale /tmp/my-project snapshots +
  mt-target/debug cleaned.

## Next actions
1. Tag v0.12.0 → release.yml; watch the run; verify assets by
   download+unzip (PE32+ ×2 in zip, ELFs, README/QUICKSTART carried).
2. User-side Windows verification: 3-line hover tooltip, icon size/
   transparency on light AND dark taskbars, wheel + System mode against
   the real OS theme flip (Settings → Personalization → Color).
3. Backlog candidates from the research docs when asked: redundant bond
   mode (calls), FEC parity frames, reinjection, per-link live status
   badges, exit-IP auto-refresh, empty states.

## Watch items (unchanged unless noted)
- Disk: /tmp/mt-target release trees ~4.5G; clean debug trees after gate
  runs; the 100%-full rootfs manifests as linker "signal 7" or ENOSPC
  temp-dir panics (hit BOTH this stage).
- Background processes DO NOT survive between sandbox tool calls — long
  builds must run in the foreground (setsid/nohup both reaped).
