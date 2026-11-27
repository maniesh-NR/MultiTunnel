# STATE — rewritten at the end of every stage. Last update: S23 (open-log-folder os-error-50 fix + reset-to-defaults + changelog-research pass; PR flow)

## Current position
- Stage: S23 IMPLEMENTED, all local gates green, delivered as the PR
  `s23-open-folder-reset-perf` requesting @copilot review (the user's
  explicit delivery instruction this session — NO tag/release until the
  review lands).
- The user's three asks shipped on the branch: (1) OPEN LOG FOLDER
  FIXED FOR REAL — the v0.15.0 explorer.exe spawn hit
  ERROR_NOT_SUPPORTED (os error 50) because a GUI-subsystem binary
  passes NULL stdio handles into CreateProcessW; the fix is
  crates/mt-app/src/shell_open.rs: ShellExecuteW (shell's open-verb
  path, no handle inheritance) with an explorer.exe fallback that now
  uses Stdio::null() on all three streams; failures name every
  mechanism tried + the folder. (2) RESET ALL SETTINGS — a Settings
  "Reset" card (ErrorText, two-click confirm, scope caption: servers
  and routing rules untouched; bonded_adapters cleared through
  set_bonded_adapters([]); mt_service::SettingsView::default() is the
  single stock source — five diverging literals collapsed; reset note
  in SettingsSaved). (3) CHANGELOG RESEARCH APPLIED —
  docs/research/2026-11-27-stability-changelog-survey.md: adopted the
  v2rayN 7.25.6 7-day stale-log sweep (diagnostics::sweep_old_logs at
  every RealService::with), the desktop-client remembered window size
  (window_w/h serde-default, debounced save, restored on load), and
  1 MiB SO_SNDBUF/SO_RCVBUF on the merge daemon's WAN streams via
  socket2 (transitive dep, zero new lockfile entries).
- Gates: fmt; clippy host + windows-gnu -D warnings; workspace tests
  520/0 (+10 new); vendored iced 6/0; mt-app --smoke 0; zigbuild
  PE32+ ×3 + ELF ×2; rig sf=1.0 + 1.5 (reset two-click flow verified
  end-to-end, seed reverted, log-folder honest failure, 0 panics).

## Prior stages (compressed)
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
