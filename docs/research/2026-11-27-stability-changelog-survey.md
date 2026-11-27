# Stability & changelog survey (S23, 2026-11-27)

Scope: the user's ask — "check for any improvements that can be made to
the connection in terms of speed and stability, and to the UI by
checking the changelogs of similar applications and implementing
anything that has worked for them." This is the second changelog pass
(first: 2026-11-21, `2026-11-21-ui-changelog-survey.md`; the bonding
literature lives in `2026-11-21-bonding-performance-survey.md`). Sources
below were read on 2026-11-27 from the projects' own release pages.

## Sources reviewed

- **v2rayN** 7.25.6 (2026-10-11) — the import-compatibility reference:
  https://github.com/2dust/v2rayN/releases/tag/7.25.6
  plus the release-history summaries at v2rayn.org mirrors ("computer
  base rebuilt to launch faster and run more steadily", "global
  Fragment control", "a rare crash during server switching repaired",
  "real delay test").
- **Clash Verge Rev** v2.5.10 (2026-11-03):
  https://github.com/clash-verge-rev/clash-verge-rev/releases/tag/v2.5.10
- **Hiddify** app changelog + core feature pages (Balancer semantics,
  connection-info home screen).
- **Speedify** release-notes pages (17: redundant-mode streaming
  enhancements; bonding-mode overview; the socket/buffer-oriented
  engineering posts behind them).
- **Windows engineering references** for the os-error-50 root cause:
  learn.microsoft.com System Error Codes (ERROR_NOT_SUPPORTED = 50),
  the CreateProcess-with-explorer error-code-50 Stack Overflow case,
  and the subprocess handle-inheritance writeups that pin it to
  GUI-subsystem parents (null STD_INPUT_HANDLE → DuplicateHandle fails).

## Adopted (shipped in this branch)

| Finding (source, verbatim where it matters) | What shipped |
|---|---|
| v2rayN 7.25.6: "日志和临时文件自动清理时间默认为 7 天" — automatic log/temp-file cleanup, 7-day default | `diagnostics::sweep_old_logs` — 7-day retention, newest-40 floor, live diagnostics file protected, best-effort, runs at every `RealService::with` construction with an INFO line |
| Window geometry persistence — a standing convention in every desktop client (v2rayN guiNConfig, Clash Verge, VS Code) | `SettingsView.window_w/window_h` (serde-default 1080×720, D8); debounced save after `Resized` (2 quiet ticks), clamped to the 880×580 window minimum; restored via `iced::window::resize` on load; reset-to-defaults restores stock |
| Speedify-style socket-buffer engineering on WAN legs (their bonding-mode posts discuss per-path window sizing; the MPTCP literature says the same: window-limited paths cap aggregate throughput) | `daemon::tune_wan_stream` — 1 MiB SO_SNDBUF/SO_RCVBUF + existing TCP_NODELAY on the merge daemon's carrier-accept and target-dial legs (the WAN sockets MultiTunnel owns; loopback engine hops intentionally left on OS defaults). Rationale: BDP of a 150 Mbps × 50 ms carrier ≈ 0.94 MiB |
| The os-error-50 report itself (user, this session) + the documented Windows cause | `mt-app::shell_open` — ShellExecuteW primary + NUL-stdio explorer fallback; the regression write-up lives in the module docs |
| Hiddify / v2rayN settings-reset affordances ("How to Reset HiddifyManager to default" is one of their most-linked docs) | The Settings "Reset" card (two-click confirm; scope caption; profiles/rules untouched) |

## Confirmed parity (checked, nothing to do)

- v2rayN "real delay" latency tests: `test_latency` already builds the
  real links and probes through them (S20's first-link fix).
- Hiddify Balancer (round-robin / lowest-delay / member stickiness):
  MultiTunnel's five merge policies (D23 + S18 BLEST/ECF) are a superset.
- Hiddify "connection info when connected — active node, speed, IP":
  Status screen + titlebar rates + tray tooltip (S17/S18/S22).
- v2rayN 7.25.6 "lock the maximum core version": the engine is
  self-provisioned and pinned (D14).
- Clash Verge 2.5.10's headline fixes (GLOBAL policy group, SSH
  private-key service mode, GEO DB sync, DNS-overwrite toggle showing
  stale state): all are Clash-specific machinery; the "toggle shows a
  state that is not real" class was already fixed here in S22
  (diagnostics restore-at-construction).

## Deliberately NOT adopted (with reasons)

- **Mux settings in the core UI** (v2rayN #10055): the client→engine leg
  already carries every proxied connection over one local hop per
  engine; adding mux knobs would expose Xray-internal knobs this
  project deliberately hides behind the profile model.
- **MASQUE / new core protocols** (v2rayN 7.25.6): protocol surface
  growth is a maintenance stream; the VLESS+REALITY + stream-bonding
  path is this project's scope.
- **Custom HTTP headers for subscription updates** (v2rayN #10118):
  MultiTunnel's subscription support is vless-link paste/import today;
  headers belong to that future feature, not now.
- **Redundant mode** (Speedify 17's streaming duplicate-ship): sending
  every frame on ALL carriers doubles bandwidth — the right behavior
  for video calls, wrong default for a general tunnel. MultiTunnel's
  failover + quarantine + BLEST already cover failure-driven switching
  without the constant duplicate cost. Worth revisiting only as an
  opt-in "redundant streaming" policy (roadmap).
- **TUN mode** (Clash Verge / FlClash): rejected in the 2026-11-21
  survey and unchanged — driver-level risk class.

## Standing perf findings (unchanged from the 2026-11-21 surveys)

The remaining big-ticket perf items are the roadmap's scheduling and
HoL-blocking work (RTT-trend score, per-carrier active window
probing), not connection-setup knobs: the S21 work already removed
Nagle stalls and send-path copies, and the client's own legs are
loopback (the WAN legs belong to the Xray engines and now the tuned
merge daemon).
