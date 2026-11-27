# PROJECT_MEMORY.md — MultiTunnel single source of truth

> Protocol (pstack): persistent external memory, zero-noise compaction, deep-first
> investigation, playbook execution. The durable sections below are maintained
> BY HAND; the "Recent Progress & Git Activity" section at the bottom is
> maintained BY SCRIPT (`python3 update_memory.py` — replaces it in place with
> the last 3 commit diffs). Run the script at session start and after every
> backup push. Long-form history stays in `harness/memory/` (LEDGER.md,
> STATE.md, DECISIONS.md); this file is the compressed, always-current view.
> NOTE: never write the exact section header string anywhere above the section
> itself — the script splits on its first occurrence.
> Since S14 the memory is ALSO a typed graph store + RAG (all-agent
> accessible): query it with `python3 query_memory.py "<topic>"` from the
> repo root (--prompt = paste-ready context pack, --json for programs,
> -k/--expand tune it); agents WRITE via `python3
> harness/memory/write_memory.py --node/--edge` (lands in
> harness/memory/curated/, evidence-gated, auto-rebuild with rollback). The
> store harness/memory/store/memory_graph.json is rebuilt by update_memory.py
> and build_graph.py — deterministic (byte-identical while HEAD and sources
> are unchanged). Tests: python3 harness/memory/test_graph.py.

## Tech Stack & Architecture Rules

- **What**: MultiTunnel — Rust rewrite of BondTunnel; zero-dependency all-in-one
  (v2rayN import + net-merge), Material You GUI. Repo: github.com/maniesh-NR/MultiTunnel.
- **Data path**: Windows client → local SOCKS/HTTP → ProxyServer → BondOutbound
  → N carriers (one per bonded NIC, VLESS+REALITY via Xray `sendThrough`,
  engine self-provisioned D14) → VM Xray (public 8443) → freedom → mt-merge-server
  (127.0.0.1:7443) → real target. Reverse symmetric. Auth at the Xray edge.
- **Two bond modes**: per-connection (default) and stream bonding (D17 framing,
  needs the VM merge daemon; carriers verified by PING/PONG before schedulable).
- **Scheduling (D23, v0.7.0)**: five merge policies in mt-net — round-robin,
  failover, lowest-latency (EWMA), **balanced** (stock default; pressure =
  rate_bps + active_conns × 128 KiB/s reserve), **priority** (user order; #1
  fills until bandwidth plateaus (ramp detector, ≥3 flat ticks + rate ≥ 0.85 ×
  capacity ≥ 256 KiB/s), then spills). Stream-mode frame scheduler is
  policy-aware too (least writer backpressure / 512 KiB queue high-water).
  S18 adds: BLEST tie-guard (250 ms drain cap among min-score-tied
  carriers; the score leader is always eligible — queue depth alone is
  never a saturation signal) and ECF completion for lowest-latency
  (rtt + queued/deliver when reports exist; sampled-RTT otherwise).
  Adapter priority == saved `bonded_adapters` ORDER; Adapters tab = ordering UI
  (selected rows first, #n chips, ↑/↓).
- **Crates**: mt-net (proxy servers, merge, policy), mt-bond (stream framing,
  carrier scheduling, daemon), mt-engine (ManagedEngine lifecycle),
  mt-links (vless:// + subscriptions), mt-platform (Win32 adapter FFI),
  mt-service (service trait + real_service), mt-m3 + mt-app (Material You GUI,
  iced). Root crate = CLI binaries (multitunnel, tray, mt-app).
- **Hard rules**: config schema camelCase + versioned migrations (D8); no
  secrets in repo (D9) — token lives in ../upload/, backup.sh reads it;
  fail-open routing (D5); GUI never dies invisibly (D15); vendored iced under
  `vendor/` + `[patch.crates-io]` (D21) — upstream 0.13.0 has 4 patched bugs
  (damage NaN total-order, clip mapping, fill_text bounds, HiDPI transform
  order); CI timestamps UTC; rustc 1.100.0 + zig 0.14.1 + cargo-zigbuild
  (D2), windows target x86_64-pc-windows-gnu.

## Active Playbook & Todo List

- **Stage**: S23 IMPLEMENTED, delivered as PR `s23-open-folder-reset-perf`
  with @copilot requested as reviewer (the user's explicit delivery
  instruction — PR + review instead of direct-to-main; NO tag/release
  until the review lands). The user's three asks shipped on the branch:
  (1) OPEN LOG FOLDER, OS ERROR 50 FIXED — root cause: v0.15.0 spawned
  explorer.exe with inherited stdio; mt-app is a Windows GUI-subsystem
  binary (NULL standard handles) and Rust's spawn passes them with
  STARTF_USESTDHANDLES → CreateProcessW fails ERROR_NOT_SUPPORTED ("os
  error 50") — console parents never hit it, which is why every manual
  probe passed while the user's GUI failed. FIX: mt-app/src/shell_open.rs
  — Windows ShellExecuteW (shell open-verb, no handle inheritance;
  windows-sys Win32_UI_Shell was already the tray dep; documented
  SE_* ≤32 code table) + explorer.exe fallback with Stdio::null() on
  ALL three streams; macOS open(1)/other xdg-open(1), NUL-stdio;
  failures name every mechanism tried + the folder (never silent).
  (2) RESET ALL SETTINGS — Settings "Reset" card (ErrorText,
  TWO-CLICK confirm; first press arms: relabel + scope status; second
  executes; any other settings mutation disarms). Stock source is now
  ONE: mt_service::SettingsView::default() (five diverging literals
  collapsed; serde-defaults == Default test-pinned; theme_mode's
  absent-key legacy semantics documented). Scope: every Settings field;
  servers + routing rules untouched (card caption says so);
  bonded_adapters cleared via set_bonded_adapters([]) (set_settings
  deliberately preserves server-side selection); always-on-top
  UN-applies at stock(false); SettingsSaved carries the reset note.
  (3) CHANGELOG RESEARCH APPLIED —
  docs/research/2026-11-27-stability-changelog-survey.md (v2rayN
  7.25.6 / Clash Verge 2.5.10 / Hiddify / Speedify): adopted v2rayN's
  7-day stale-log sweep (diagnostics::sweep_old_logs — newest-40 floor,
  live diag file protected, best-effort, at every RealService::with),
  the desktop-client remembered window size (window_w/h serde-default
  1080x720; debounced 2-quiet-tick save; clamped 880x580; restored via
  iced::window::resize on load; reset restores stock), and 1 MiB
  SO_SNDBUF/SO_RCVBUF + existing NODELAY on the merge daemon's WAN
  streams (socket2, transitive dep — zero new lockfile entries). Rig
  sf=1.0+1.5 verified the reset two-click flow end-to-end + the
  log-folder honest failure; TESTS 520/0 (+10); all gates green (fmt,
  clippy host+windows-gnu, workspace, vendored iced 6/0, smoke,
  zigbuild PE32+ ×3 + ELF ×2). NEXT: await the Copilot review on the
  PR; merge + tag v0.16.0 only after it (the release notes already
  have their material).

- **Stage**: S21 CLOSED — v0.14.0 released and verified (tag on e510e43,
  release run 45477702090 SUCCESS on attempt 3, same-commit ci green on
  both OS legs, assets download-verified). The user's four asks shipped:
  (1) P0 SECOND-CONNECTION BUG — mid-traffic reconnect re-binds the fixed
  listener ports while untracked in-flight connection tasks pin their
  sockets (no SO_REUSEADDR); on Windows the re-bind lands WSAEADDRINUSE
  → connect Failed → dead tunnel until restart. Linux tolerates the
  re-bind (std rebind probe), so the regression test asserts the
  platform-independent observable: the lingering client connection sees
  EOF after the picker switch (pre-fix FAILS, post-fix passes). Fix:
  Shared.conns tracks every connection task's AbortHandle; run()
  force-closes whatever the 3 s drain could not reap; AbortPumpsOnDrop
  aborts the DETACHED pump children (dropping JoinHandles leaves them
  running — found when the first EOF assertion failed); bind_with_retry
  5 s (BIND_WAIT 2→8 s). H2: engine config/log stems carry a
  deterministic FNV-1a hash of the full endpoint identity
  (engine_file_stem) — same-tag different-endpoint profiles can no
  longer overwrite each other's Xray config (temp probe tunnels
  corrupting the LIVE engine → phantom respawn → DEGRADED while GUI
  showed Connected). H3: Core.store_mtx serializes profiles.json
  read-modify-write. (2) P1 PERF PARITY — TCP_NODELAY on all four legs
  (Nagle+delayed-ACK stalled each 16 KiB frame ~40 ms on Linux peers);
  quarantine can no longer blackhole the sole link (pick_excluding
  last-resort fallback; dial decides, fail-open); production prober uses
  the user probe_target; cumulative ACKs coalesced (8 frames / 20 ms,
  flushed by CLOSE + the 1 s PING heartbeat); PONG echoes on the
  RECEIVING carrier (rtt_ms was cross-contaminated across carriers);
  send-path copies 5→3 (encode_session_borrowed; wire-identity tested);
  carrier writer batches ≤16 frames per write_vectored. (3) O — QoS
  FAIR SHARE toggle (Settings → Network, qos_fair): mt-net/src/qos.rs
  QosScheduler groups flows by TARGET; 1 s tick refills
  aggregate/groups ONLY under contention (≥2 groups AND ≥2 MiB/s
  aggregate; single group/idle unlimited); pump read gate; ALWAYS
  attached so the toggle flips LIVE (no reconnect). (4) P — merged
  Exit & speed panel: one Status card, both triggers side-by-side,
  result rows stacked (D24-safe; rig-verified sf=1.0 + pixel-diff of the
  toggle knob 39→55 + sf=1.5). TESTS 490/0 (+16). GATES all green
  (fresh ziglang wheel — the tools/pypkgs wheel was incomplete AGAIN,
  no lib/libc/include → ring stddef.h failure; reinstalled 0.14.1 from
  PyPI). RELEASE SAGA: attempt-1 ubuntu carrier_death 'no live bond
  carrier' (the PRE-EXISTING S11/S18/S19 runner race, signature exactly
  as backlogged), attempt-2 windows domain_targets 60 s deadline,
  attempt-3 SUCCESS; logs archived s21-ci-*.log. SANDBOX was recycled:
  stale clone at s13 → remote authoritative, snapshot+fast-forward;
  rustup/zigbuild/xkblibs/python-xlib rebuilt per the ritual.

- **Stage**: S20 CLOSED — the S19 leftover (Copilot code review) COMPLETED
  and shipped as v0.13.1 (tag on 4923566, release run 44983522874 SUCCESS,
  ci ×2 green, assets download-verified). The cloud Copilot reviewer ran
  via `POST /repos/maniesh-NR/MultiTunnel/pulls/1/copilot-code-review`
  (needs only pull-requests:write, which the PAT has): effort ladder
  per the fallback instruction — max → quota-rejected, balanced →
  quota-rejected, **lite → ran**. Round 1 (on 52f1d3c): 12 findings
  (5 High: step-0 self-heal never cleared sysproxy_set → no-snapshot
  restore disables a user's pre-existing proxy; give_up unserialized
  with disconnect → black-hole re-installed AFTER an explicit
  disconnect; set-BEFORE-bind → a foreign port-9 owner receives the
  blocked traffic; listener stopped even when restore failed → stranded
  block, no retry, false "restored" log; diag header wrote user URLs
  verbatim → credential leak. 7 Medium: live diag toggle misses engine
  debug, failed enable leaves the toggle on, probes ride a session that
  is not the target profile, unbounded head buffer, port-0 edit,
  unvalidated REALITY edits, pop() = lowest-priority adapter). All 12
  fixed (Core::proxy_mtx serializes every proxy hand-off with the
  bind hoisted out of the lock; bind-before-set with no-set-on-bind-
  failure; restore-first release + retry; redact_url; diag-on-live
  reconnect; failed-enable revert; Session.profile_id pins both probes;
  64 KiB head cap; port-0 + REALITY structural validation; first-link
  selection). Round 2 (lite, on the fixes): CLEAN PASS, zero comments.
  PR #1: 12 maintainer replies + summary, CLOSED. Transcript:
  harness/memory/agent-reports/s20-copilot-review-transcript.md.
  474/0 (+8); gates fmt/clippy host+windows-gnu/iced 6/0/smoke 0/
  zigbuild PE32+ ×4 + ELF ×2; rig launch+render-healthy (no GUI code
  changed). The standalone @github/copilot CLI remains blocked by the
  PAT (401 "Personal Access Token does not have Copilot Requests
  permission" from api.individual.githubcopilot.com — network fine;
  ONLY the user can add that permission when editing the PAT in the
  GitHub web UI, then `COPILOT_GITHUB_TOKEN=<PAT> copilot -p "/review"`
  would work). S19's "left open for an entitled machine" debt is paid.
- **S20 sandbox-restore notes** (the sandbox was recycled before this
  stage; toolchain rebuilt): rustup minimal + rustfmt + clippy
  reinstalled; tools/pypkgs ziglang wheel was INCOMPLETE (only
  musl/wasi under lib/libc — stddef.h compile failure; reinstalled
  ziglang==0.14.1 from PyPI, ZIG="python3 -m ziglang" +
  PYTHONPATH=tools/pypkgs); python-xlib reinstalled to
  tools/pypkgs-xlib; /tmp/xkblibs re-extracted from the debs in the
  my-project root; cargo-zigbuild 0.24.1 via cargo install. DISK:
  /tmp/mt-target debug trees wiped TWICE (ENOSPC at 94% — always stage
  release binaries before wiping target trees).

- **Stage**: S19 CLOSED — v0.13.0 released and verified (tag run
  44910934439 SUCCESS ~7 min; CI all-green on the rerun after the
  documented mt-bond runner-flake round — LEDGER 15:10Z). The six asks
  shipped: (1) server EDITING (Servers → per-card Edit button →
  full-window modal; mt-service ProfileDetails round-trip via
  profile_details/update_profile; ProfileStore::update replaces BY ID in
  place — id/position/active preserved, no dedupe on explicit edits;
  primitive + capability-regression validation; editing the active
  server logs "reconnect to apply"); (2) exit-IP while OFFLINE
  (exit_probe falls back to temp_link_for — a one-shot factory link
  through the active profile, same engine path as test_latency;
  ExitInfo.via_temp_tunnel → "temporary probe tunnel" chip); (3) SPEED
  TEST (Status card; rides the live session for the active profile else
  the temp tunnel; settings.speed_test_url default
  http://speed.cloudflare.com/__down?bytes=25000000; clock starts AFTER
  connect so engine spawn never pollutes the rate; 5 s stall / 30 s
  budget / 64 MiB caps; HTTP ≥400 rejected); (4) DIAGNOSTICS MODE
  (SettingsView.diagnostics; src/diagnostics.rs writes one redacted
  paste-ready <log_dir>/diagnostics-*.log — header + GUI LogLine tee +
  ALL tracing events via a hand-rolled process-global subscriber (the
  e2e PrintSubscriber pattern, no tracing-subscriber dep); engine log
  level forced debug while ON; redaction test-pinned; Settings card +
  Open log folder); (5) KILL SWITCH (SettingsView.kill_switch; give_up()
  is fail-CLOSED: system proxy → 127.0.0.1:9 via sysproxy::set
  (keep-first snapshot survives for the true restore); REVIEW FIX: the
  app BINDS port 9 itself while engaged (spawn_blackhole accept loop
  swallows connections — a local process cannot pre-bind the black
  hole; dead-port fallback documented for hosts that cannot bind);
  stop_blackhole on every restore path (both disconnect branches,
  toggle-off escape hatch, connect step-0); explicit disconnect now
  restores proxy state even with NO session; Status "KILL SWITCH
  ACTIVE" banner when Failed+armed); (6) CODE REVIEW — Copilot
  unavailable from the sandbox (no account entitlement → reviewer
  requests silently dropped; models.github.ai proxy-intercepted): a
  3-round discussion-based review ran on the strongest reachable model
  (z-ai chat; transcript archived at
  harness/memory/agent-reports/s19-review-transcript.md); 4 findings
  fixed (port ownership, banner, edit-failure logging, chained
  diag-path refresh), 7 assessed non-issues documented; review PR #1
  (base v0.12-line) left open for a Copilot pass from an entitled
  machine. 466/0 tests (+19); rig verified sf=1.0+1.5 (VLM: 52.0 Mbps
  chips, offline exit-IP chip, modal fields, rename→save, Cancel +
  backdrop dismissal, both toggles).
- **S19 CI lessons**: mt-bond carrier_death + domain_targets are
  runner-only timing-flaky (GitHub I/O scheduling; local 2-core taskset
  stress 0/8); BOTH now carry hard 60-90 s deadlines (a wedge fails in
  a minute with a LEDGER pointer instead of hanging an hour — log
  captured in harness/memory/agent-reports/s19-ci-wedge-*.log).
  BACKLOG (top): root-cause the carrier-lifecycle race — 'no live bond
  carrier' at half2 means usable() went EMPTY (Wi-Fi dead + LTE
  momentarily !alive mid-death-replay) in PRE-EXISTING S11/S18
  mt-bond code; investigate the death handler's replay path + the
  alive-flag transitions on a runner-like environment.
- **Prior stages**: S18 (v0.12.0, D28: 3-line tray tooltip; transparent
  contrast-boosted larger icons everywhere; Settings accent COLOR WHEEL;
  System/Light/Dark mode with OS detection; mt-bond BLEST tie-guard +
  ECF scheduling per the docs/research/ papers survey; UI changelog
  survey), S17 (v0.11.0, titlebar version + live speeds, logs floating
  jump buttons + autoscroll).
  The three asks shipped: release version next to the app name in the
  titlebar (workspace version inheritance — one-line release bumps;
  mt-app.exe PE metadata now carries the release version), live global
  ↓/↑ rates on the strip's right (the tray tooltip's same 1 s sample),
  Logs tab = fixed toolbar + dedicated viewport with floating top/bottom
  jump buttons (26px right stand-off — the rig caught the scrollbar
  grab-zone swallowing flush-corner clicks) + autoscroll toggle
  (default ON, follows the tail; re-enabling re-anchors). All 8 gates
  green; rig verified sf=1.0+1.5 (VLM content reads + pixel diffs);
  433/0 tests. Release: attempt-1 windows leg cancelled at 68 min on a
  cold rust-cache (Cargo.lock changed), attempt-2 SUCCESS in 6 min
  warm (s9 pattern proven); CI green on attempt 3 after a two-test
  ubuntu runner-flake round (carrier_death + socks_probe deadline,
  different tests per attempt, all other legs/local green — LEDGER
  16:15Z).
- **Prior stages**: S16 (v0.10.0, About+topics+attribution+banner,
  transparent emblem, mono tray icon, hover speeds+totals, titlebar
  menu + always-on-top), S15 (v0.9.0, logo + blended titlebar +
  close-to-tray + persistent tray + Quick Settings), S13 (v0.7.1, status
  tab: one connection frame + one page scrollable, graph fills frame),
  S12 (v0.7.0, balanced+priority scheduling + adapter ordering), S11
  (v0.6.2, stream bonding finalRules + verified carriers), S10/10.1
  (v0.6.0/01, rendering fixes + per-adapter graphs).
- **Token note**: the upload token is a fine-grained PAT
  (`github_pat_...`) — extract with pattern `github_pat_[A-Za-z0-9_]+`;
  the old `gh[pousr]_` pattern silently matches nothing ("github"
  contains no literal "gh" substring).
- **Pending — user side**: verify v0.9/v0.10 on the real Windows box —
  tray mono↔color transition, hover speeds, right-click titlebar →
  always-on-top actually floating, transparent emblem legible on light
  AND dark titlebars. If stream bonding still fails on the VM: `sudo
  bash server/setup-xray-reality.sh` (idempotent finalRules patch) and
  `sudo bash server/setup-merge-daemon.sh`.
- **Backlog (parked)**: subscription auto-update, vmess validation,
  profiles.json locking, engine log rotation, listener max_conns, upstream iced
  migration, S7 native REALITY core.
- **Memory graph (S14)**: store at harness/memory/store/memory_graph.json;
  READ `python3 query_memory.py "<topic>"` (--prompt for agent context
  packs); WRITE `python3 harness/memory/write_memory.py --node/--edge`;
  rebuilt by update_memory.py; deterministic; 18/18 tests. When delegating
  to subagents, include the query one-liner so they can pull their own
  context.
- **Memory-file rule (S18 lesson)**: update_memory.py NO LONGER embeds
  PROJECT_MEMORY.md's own diff (the old self-embedding doubled the file
  every ritual run → 134 MB → GitHub 100 MB per-file push rejection;
  never re-add it; the file must stay < 1 MB).
- **Watch items**: sandbox recycles (stale clone → fetch + rebase before
  building; registry src/ wiped → rm -rf $CARGO_HOME/registry/src lets
  cargo re-extract from cache; cargo-fmt shims missing → symlink from the
  toolchain bin; ziglang wheel + aarch64 rust target need reinstall);
  disk headroom (release zigbuild needs ~5G; clean /tmp/mt-target/debug
  first; a 99%-full rootfs manifests as linker "signal 7 Bus error");
  GitHub runner wedges (>45 min in cargo test while a sibling passed =
  cancel + rerun; mt-bond carrier_death test flaked twice on runners
  2026-11-20 while CI + 408 loaded local runs were green — if it recurs,
  capture the hung job's logs BEFORE rerunning).

## Verification Contracts

Every change closes only after ALL of these (CI runs them too):

1. `cargo fmt --all -- --check` — clean.
2. `cargo clippy --workspace --all-targets -- -D warnings` — clean, host AND
   windows-gnu (use cargo/zig-cc shims in `cargo/`).
3. `cargo test --workspace` — 0 failed (current baseline: 396 tests; E2E rig
   `tests/e2e_real_xray.rs` gated by `MT_E2E_XRAY=1`).
4. Vendored iced tests: 6/0 green.
5. `cargo zigbuild --release --target x86_64-pc-windows-gnu --workspace` —
   PE32+ ×4 (multitunnel, tray, mt-app, mt-merge-server) + linux ELF.
6. GUI: `mt-app --smoke` exit 0; visual changes verified in the Xvfb rig
   (`scripts/gui_drive.py`, pixel-verified, sf=1.0 AND sf=1.5).
7. GUI-affecting bugs: reproduce FIRST in the rig, fix, re-verify (no symptom
   patches); user-reported crashes get a regression test.
8. Release: tag `v*` → release.yml; verify assets by download+unzip; record in
   LEDGER.md; push via `./harness/backup.sh <label>`.

## Recent Progress & Git Activity

```diff
### Commit: 978da2b - backup(s23-pr-ready): 2026-11-27 memory + gates + rig evidence (Z User, 2026-11-27)
diff --git a/PROJECT_MEMORY.md b/PROJECT_MEMORY.md
index 685fda3..a45181f 100755
--- a/PROJECT_MEMORY.md
+++ b/PROJECT_MEMORY.md
@@ -56,51 +56,47 @@
 
 ## Active Playbook & Todo List
 
-- **Stage**: S22 CLOSED — v0.15.0 released (tag on 21fd59f). The user's
-  five asks shipped: (1) Exit & speed panel split into TWO columns —
-  exit-IP outputs LEFT (heading, IP, location/ISP chips, verdict card),
-  speed-test outputs RIGHT (heading, Mbps, volume chips, download
-  note), quiet placeholders while idle; pure view change, D24-safe.
-  (2) UPLOAD SPEED TEST: "Test upload speed" button appears ONLY after
-  the download test concluded, result in the right column;
-  speed_test_upload_url setting (serde default cloudflare __up, D8),
-  RealService::speed_test_upload POSTs 25 MiB through the same tunnel
-  selection (S20 profile_id rule; clock after connect; 30 s/5 s stall/
-  64 MiB caps; partial stands; status guard only on clean completion);
-  Settings → Advanced "upload test URL" field; diag header redacts it.
-  (3) PER-LINK ADAPTER: `via <adapter>` dim line UNDER each link row —
-  mt_service::adapter_of_link_name parses "<tag> [adapter]" (dedup
-  " #N" stripped; "Wi-Fi Direct"/"Ethernet 2" verbatim), populated in
-  link_status_of + carrier_snapshot map + mock; survives util::short
-  truncation. (4) OPEN LOG FOLDER FIXED: root cause = the diag-file
-  gate (diag_path None after restart → "turn diagnostics on first");
-  new MtService::log_dir() (always resolvable) + handler
-  create_dir_all + opens the folder itself via open_folder_command
-  (explorer/open/xdg-open; spawn-result reported; Settings status +
-  Logs-tab WARN, never silent); fetched at boot in BOTH init paths;
-  ADJACENT: diagnostics persisted ON re-enables the sink AT
-  CONSTRUCTION (RealService::with) — the toggle no longer lies after a
-  restart. (5) ICON FILL: make_logo_assets.py re-crops emblem/lockup to
-  their ALPHA bboxes (dead ~71px left band dropped), margins
-  0.04→0.02/0.10→0.03; measured fills 93.8–96.9%w × 75.0–76.6%h (was
-  75–81%w × 61–64%h — tray glyph ~24% larger); ICO invariants held
-  (4 entries 16/32/48/256, biWidth=32/biHeight=64, logo_64 16384 B,
-  logo_24 2304 B, deterministic, banner/logo/color_wheel untouched).
-  RIG sf=1.0+1.5 (mock): split layout, download→upload button→result,
-  exit verdict left, via-lines, log-folder click honest-failure
-  surfaced (no xdg-open under Xvfb; Windows spawns explorer — spawn
-  command unit-pinned). LESSONS: gui_drive clicks on buttons that
-  APPEARED after a reflow need a pointer jiggle (no motion event →
-  stale hover); content appended BELOW a scrollable's initial viewport
-  mimics a stale frame — damage-instrumented the vendored compositor
-  (reverted byte-identical) to prove the rows were just below the
-  fold; scroll y=400 is ABOVE the viewport (403.2), y=500 works. TESTS
-  497/0 (+7). GATES all green (ziglang wheel incomplete AGAIN →
-  reinstalled 0.14.1 mid-gates; disk hit 99% → rm -rf target/debug +
-  windows target between phases). CI: ubuntu+windows test legs hit the
-  DOCUMENTED mt-bond runner flake (domain_targets 60 s deadline, both
-  OSes, same commit — logs archived s22-ci-*.log) → failed-jobs rerun
-  per protocol.
+- **Stage**: S23 IMPLEMENTED, delivered as PR `s23-open-folder-reset-perf`
+  with @copilot requested as reviewer (the user's explicit delivery
+  instruction — PR + review instead of direct-to-main; NO tag/release
+  until the review lands). The user's three asks shipped on the branch:
+  (1) OPEN LOG FOLDER, OS ERROR 50 FIXED — root cause: v0.15.0 spawned
+  explorer.exe with inherited stdio; mt-app is a Windows GUI-subsystem
+  binary (NULL standard handles) and Rust's spawn passes them with
+  STARTF_USESTDHANDLES → CreateProcessW fails ERROR_NOT_SUPPORTED ("os
+  error 50") — console parents never hit it, which is why every manual
+  probe passed while the user's GUI failed. FIX: mt-app/src/shell_open.rs
+  — Windows ShellExecuteW (shell open-verb, no handle inheritance;
+  windows-sys Win32_UI_Shell was already the tray dep; documented
+  SE_* ≤32 code table) + explorer.exe fallback with Stdio::null() on
+  ALL three streams; macOS open(1)/other xdg-open(1), NUL-stdio;
+  failures name every mechanism tried + the folder (never silent).
+  (2) RESET ALL SETTINGS — Settings "Reset" card (ErrorText,
+  TWO-CLICK confirm; first press arms: relabel + scope status; second
+  executes; any other settings mutation disarms). Stock source is now
+  ONE: mt_service::SettingsView::default() (five diverging literals
+  collapsed; serde-defaults == Default test-pinned; theme_mode's
+  absent-key legacy semantics documented). Scope: every Settings field;
+  servers + routing rules untouched (card caption says so);
+  bonded_adapters cleared via set_bonded_adapters([]) (set_settings
+  deliberately preserves server-side selection); always-on-top
+  UN-applies at stock(false); SettingsSaved carries the reset note.
+  (3) CHANGELOG RESEARCH APPLIED —
+  docs/research/2026-11-27-stability-changelog-survey.md (v2rayN
+  7.25.6 / Clash Verge 2.5.10 / Hiddify / Speedify): adopted v2rayN's
+  7-day stale-log sweep (diagnostics::sweep_old_logs — newest-40 floor,
+  live diag file protected, best-effort, at every RealService::with),
+  the desktop-client remembered window size (window_w/h serde-default
+  1080x720; debounced 2-quiet-tick save; clamped 880x580; restored via
+  iced::window::resize on load; reset restores stock), and 1 MiB
+  SO_SNDBUF/SO_RCVBUF + existing NODELAY on the merge daemon's WAN
+  streams (socket2, transitive dep — zero new lockfile entries). Rig
+  sf=1.0+1.5 verified the reset two-click flow end-to-end + the
+  log-folder honest failure; TESTS 520/0 (+10); all gates green (fmt,
+  clippy host+windows-gnu, workspace, vendored iced 6/0, smoke,
+  zigbuild PE32+ ×3 + ELF ×2). NEXT: await the Copilot review on the
+  PR; merge + tag v0.16.0 only after it (the release notes already
+  have their material).
 
 - **Stage**: S21 CLOSED — v0.14.0 released and verified (tag on e510e43,
   release run 45477702090 SUCCESS on attempt 3, same-commit ci green on
@@ -330,4323 +326,781 @@ Every change closes only after ALL of these (CI runs them too):
 ## Recent Progress & Git Activity
 
 ```diff
-### Commit: a5ca2f7 - backup(s22-close-v0.15.0): 2026-11-25T19:41:20Z (Z User, 2026-11-25)
-diff --git a/PROJECT_MEMORY.md b/PROJECT_MEMORY.md
-index b292b18..52e7792 100755
---- a/PROJECT_MEMORY.md
-+++ b/PROJECT_MEMORY.md
-@@ -56,6 +56,52 @@
- 
- ## Active Playbook & Todo List
- 
-+- **Stage**: S22 CLOSED — v0.15.0 released (tag on 21fd59f). The user's
-+  five asks shipped: (1) Exit & speed panel split into TWO columns —
-+  exit-IP outputs LEFT (heading, IP, location/ISP chips, verdict card),
-+  speed-test outputs RIGHT (heading, Mbps, volume chips, download
-+  note), quiet placeholders while idle; pure view change, D24-safe.
-+  (2) UPLOAD SPEED TEST: "Test upload speed" button appears ONLY after
-+  the download test concluded, result in the right column;
-+  speed_test_upload_url setting (serde default cloudflare __up, D8),
-+  RealService::speed_test_upload POSTs 25 MiB through the same tunnel
-+  selection (S20 profile_id rule; clock after connect; 30 s/5 s stall/
-+  64 MiB caps; partial stands; status guard only on clean completion);
-+  Settings → Advanced "upload test URL" field; diag header redacts it.
-+  (3) PER-LINK ADAPTER: `via <adapter>` dim line UNDER each link row —
-+  mt_service::adapter_of_link_name parses "<tag> [adapter]" (dedup
-+  " #N" stripped; "Wi-Fi Direct"/"Ethernet 2" verbatim), populated in
-+  link_status_of + carrier_snapshot map + mock; survives util::short
-+  truncation. (4) OPEN LOG FOLDER FIXED: root cause = the diag-file
-+  gate (diag_path None after restart → "turn diagnostics on first");
-+  new MtService::log_dir() (always resolvable) + handler
-+  create_dir_all + opens the folder itself via open_folder_command
-+  (explorer/open/xdg-open; spawn-result reported; Settings status +
-+  Logs-tab WARN, never silent); fetched at boot in BOTH init paths;
-+  ADJACENT: diagnostics persisted ON re-enables the sink AT
-+  CONSTRUCTION (RealService::with) — the toggle no longer lies after a
-+  restart. (5) ICON FILL: make_logo_assets.py re-crops emblem/lockup to
-+  their ALPHA bboxes (dead ~71px left band dropped), margins
-+  0.04→0.02/0.10→0.03; measured fills 93.8–96.9%w × 75.0–76.6%h (was
-+  75–81%w × 61–64%h — tray glyph ~24% larger); ICO invariants held
-+  (4 entries 16/32/48/256, biWidth=32/biHeight=64, logo_64 16384 B,
-+  logo_24 2304 B, deterministic, banner/logo/color_wheel untouched).
-+  RIG sf=1.0+1.5 (mock): split layout, download→upload button→result,
-+  exit verdict left, via-lines, log-folder click honest-failure
-+  surfaced (no xdg-open under Xvfb; Windows spawns explorer — spawn
-+  command unit-pinned). LESSONS: gui_drive clicks on buttons that
-+  APPEARED after a reflow need a pointer jiggle (no motion event →
-+  stale hover); content appended BELOW a scrollable's initial viewport
-+  mimics a stale frame — damage-instrumented the vendored compositor
-+  (reverted byte-identical) to prove the rows were just below the
-+  fold; scroll y=400 is ABOVE the viewport (403.2), y=500 works. TESTS
-+  497/0 (+7). GATES all green (ziglang wheel incomplete AGAIN →
-+  reinstalled 0.14.1 mid-gates; disk hit 99% → rm -rf target/debug +
-+  windows target between phases). CI: ubuntu+windows test legs hit the
-+  DOCUMENTED mt-bond runner flake (domain_targets 60 s deadline, both
-+  OSes, same commit — logs archived s22-ci-*.log) → failed-jobs rerun
-+  per protocol.
-+
- - **Stage**: S21 CLOSED — v0.14.0 released and verified (tag on e510e43,
-   release run 45477702090 SUCCESS on attempt 3, same-commit ci green on
-   both OS legs, assets download-verified). The user's four asks shipped:
-@@ -284,17 +330,17 @@ Every change closes only after ALL of these (CI runs them too):
- ## Recent Progress & Git Activity
+### Commit: a0e0ccb - chore: clippy/fmt pass (struct-update idioms, sort_by_key, drop dead default fns) (Z User, 2026-11-27)
+diff --git a/crates/mt-app/src/app.rs b/crates/mt-app/src/app.rs
+index 5a4db83..d145b80 100755
+--- a/crates/mt-app/src/app.rs
++++ b/crates/mt-app/src/app.rs
+@@ -1504,9 +1504,9 @@ impl App {
+                 let apply = self.apply_settings_view(stock.clone());
+                 // Reset restores the stock window size too (a live
+                 // resize to 1080×720 — the next launch reads it back).
+-                let resize = if self.window_id.is_some() {
++                let resize = if let Some(id) = self.window_id {
+                     iced::window::resize(
+-                        self.window_id.expect("checked above"),
++                        id,
+                         iced::Size::new(stock.window_w as f32, stock.window_h as f32),
+                     )
+                 } else {
+diff --git a/crates/mt-app/src/mock.rs b/crates/mt-app/src/mock.rs
+index ce6667f..7f82c28 100755
+--- a/crates/mt-app/src/mock.rs
++++ b/crates/mt-app/src/mock.rs
+@@ -77,18 +77,20 @@ impl MockService {
+         let active = profiles.first().map(|p| p.id.clone());
+         // S23: stock base (theme/system/balanced/…) with ONE demo
+         // deviation — the bonded adapters below.
+-        let mut settings = SettingsView::default();
+         // D22/D23: ALL THREE wired adapters bonded — mirrors the demo
+         // setup (balanced needs ≥2 ticked adapters to show its point)
+         // and gives every Status-tab adapter graph traffic. The mock's
+         // byte split is deliberately UNEVEN (Wi-Fi ≫ Ethernet ≫
+         // Cellular) — the "one adapter hogs everything" shape the
+         // balanced policy exists to fix.
+-        settings.bonded_adapters = vec![
+-            "mock-wifi".to_string(),
+-            "mock-ethernet".to_string(),
+-            "mock-cellular".to_string(),
+-        ];
++        let settings = SettingsView {
++            bonded_adapters: vec![
++                "mock-wifi".to_string(),
++                "mock-ethernet".to_string(),
++                "mock-cellular".to_string(),
++            ],
++            ..SettingsView::default()
++        };
+         let inner = Inner {
+             state: ServiceState::Disconnected,
+             active,
+diff --git a/crates/mt-app/src/theme.rs b/crates/mt-app/src/theme.rs
+index 8cf6715..87d723f 100755
+--- a/crates/mt-app/src/theme.rs
++++ b/crates/mt-app/src/theme.rs
+@@ -672,15 +672,16 @@ mod tests {
+     /// `from_settings` / `rederive` honor the settings view (live re-theme).
+     #[test]
+     fn theme_from_settings_and_rederive() {
+-        let mut t = MtTheme::from_settings(&{
++        let mut t = MtTheme::from_settings(&SettingsView {
+             // S23: one base-line diff off the stock view (the fields
+-            // from_settings actually reads: seed + mode).
+-            let mut s = SettingsView::default();
+-            s.theme_seed = 0x4285F4;
+-            s.theme_mode = "light".into();
+-            s.system_proxy = true;
+-            s.merge_policy = "failover".into();
+-            s
++            // from_settings actually reads: seed + mode; the system_proxy
++            // / merge_policy values keep the original test's "unrelated
++            // fields are ignored" coverage).
++            theme_seed: 0x4285F4,
++            theme_mode: "light".into(),
++            system_proxy: true,
++            merge_policy: "failover".into(),
++            ..SettingsView::default()
+         });
+         assert_eq!(t.palette.seed(), 0x4285F4);
+         assert!(!t.dark);
+diff --git a/crates/mt-service/src/lib.rs b/crates/mt-service/src/lib.rs
+index 2da54c6..d7ea35a 100755
+--- a/crates/mt-service/src/lib.rs
++++ b/crates/mt-service/src/lib.rs
+@@ -992,13 +992,14 @@ mod close_to_tray_migration_tests {
+             // S23: build off the stock view, preserving this fixture's
+             // historical deviations (seed 0, blank mode, info logs,
+             // blank probe/exit URLs).
+-            let mut v = SettingsView::default();
+-            v.theme_seed = 0;
+-            v.theme_mode = String::new();
+-            v.log_level = "info".into();
+-            v.probe_target = String::new();
+-            v.exit_probe_url = String::new();
+-            v
++            SettingsView {
++                theme_seed: 0,
++                theme_mode: String::new(),
++                log_level: "info".into(),
++                probe_target: String::new(),
++                exit_probe_url: String::new(),
++                ..SettingsView::default()
++            }
+         }
+     }
+ }
+diff --git a/src/diagnostics.rs b/src/diagnostics.rs
+index d312b1b..af70eb5 100644
+--- a/src/diagnostics.rs
++++ b/src/diagnostics.rs
+@@ -138,7 +138,7 @@ pub(crate) fn sweep_old_logs(
+             Some((e.path(), mtime))
+         })
+         .collect();
+-    files.sort_by(|a, b| b.1.cmp(&a.1));
++    files.sort_by_key(|(_, mtime)| std::cmp::Reverse(*mtime));
+     let mut pruned = Vec::new();
+     for (i, (path, mtime)) in files.into_iter().enumerate() {
+         if i < keep_newest {
+diff --git a/src/real_service.rs b/src/real_service.rs
+index 90b5627..2162f2e 100755
+--- a/src/real_service.rs
++++ b/src/real_service.rs
+@@ -607,18 +607,6 @@ impl Default for SettingsFile {
+ /// round-robin; D23 = 2: stock round-robin → balanced).
+ const SETTINGS_VERSION: u32 = 2;
  
- ```diff
--### Commit: 033c009 - backup(s21-close-v0.14.0): 2026-11-24T20:57:46Z (Z User, 2026-11-24)
-+### Commit: 21fd59f - backup(s22-release-prep): 2026-11-25T19:24:03Z (Z User, 2026-11-25)
- diff --git a/Cargo.lock b/Cargo.lock
--index 91f9783..c32d817 100755
-+index c32d817..8b3e2b9 100755
- --- a/Cargo.lock
- +++ b/Cargo.lock
- @@ -1962,7 +1962,7 @@ dependencies = [
-  
-  [[package]]
-  name = "mt-app"
---version = "0.13.1"
--+version = "0.14.0"
-+-version = "0.14.0"
-++version = "0.15.0"
-  dependencies = [
-   "async-trait",
-   "iced",
-@@ -302,2360 +348,120 @@ index 91f9783..c32d817 100755
-  
-  [[package]]
-  name = "multitunnel"
---version = "0.13.1"
--+version = "0.14.0"
-+-version = "0.14.0"
-++version = "0.15.0"
-  dependencies = [
-   "anyhow",
-   "async-trait",
--diff --git a/harness/memory/DECISIONS.md b/harness/memory/DECISIONS.md
--index a3f7c46..7949054 100755
----- a/harness/memory/DECISIONS.md
--+++ b/harness/memory/DECISIONS.md
--@@ -512,3 +512,76 @@ apps' changelogs.
--    Verge Rev / v2rayN / FlClash / Hiddify / Speedify changelogs mapped
--    to shipped vs proposed (next: per-link live status badges, exit-IP
--    auto-refresh, empty states).
--+
--+## D29 — S21: P0 reconnect fix — tracked force-close at drain end + bind retry (2026-11-24)
--+
--+User report: "The connection works for some time, but it stops working
--+when a second connection is added. The application must be restart for it
--+to work again."
--+
--+1. ROOT CAUSE (H1): connection tasks spawned per accepted proxy
--+   connection were UNTRACKED; a shutdown drained for 3 s and returned
--+   WITH LIVE CONNECTIONS. Each lingering socket pins the listener port's
--+   local endpoint; the next connect re-binds 10808/10809. Linux tolerates
--+   a listen re-bind over non-listening sockets (proven with a standalone
--+   std probe — scripts/rebind_probe.rs); WINDOWS does NOT
--+   (WSAEADDRINUSE), so every picker switch after a busy session landed
--+   Failed and only process death (RST) freed the ports. Reproduced
--+   first: the regression test fails pre-fix (EOF never arrives) and
--+   passes post-fix (bug-first contract).
--+2. FIX: track every connection task's AbortHandle in `Shared.conns`
--+   (pruned at 4096 pushes; aborting finished tasks is a no-op), and at
--+   the END of the drain grace FORCE-CLOSE whatever wait_idle could not
--+   reap. Listeners re-bind after a `bind_with_retry` (5 s budget / 250 ms
--+   interval; BIND_WAIT 2→8 s) absorbing the kernel teardown race.
--+3. PUMP DETACHMENT: `tunnel` spawns two pump tasks; aborting the outer
--+   connection task merely DROPS their JoinHandles (detach), leaving the
--+   socket halves open — the EOF assertion caught it. `AbortPumpsOnDrop`
--+   aborts the pumps whenever the tunnel future dies (normal completion
--+   too: no-op).
--+4. H2 (same report family): engine config/log filenames keyed by tag
--+   alone collided across profiles sharing a `#fragment` — a temp
--+   probe/speed-test engine REWROTE the live engine's config and its next
--+   respawn dialed the wrong server into DEGRADED while the GUI showed
--+   Connected. `engine_file_stem` appends a deterministic FNV-1a hash of
--+   the full endpoint identity (same profile → same file across
--+   reconnects; different profiles → different files). Wire files gain a
--+   hash suffix; only mt-engine's lifecycle-test helper had to follow.
--+5. H3: profiles.json load→mutate→save cycles are now serialized by
--+   `Core::store_mtx` (GUI iced tasks run concurrently; the CLI can race
--+   the GUI; shared per-PID temp name collided on Windows).
--+
--+## D30 — S21: QoS fair share by destination + live toggle; merged Status panel (2026-11-24)
--+
--+User asks (verbatim): "Include a QoS toggle that can be turned on to
--+prevent a single application from dominating all the bandwidth." / "Have
--+the 'check exit ip' and 'speed test' in the same UI block, as there is a
--+lot of blank space in their individual blocks."
--+
--+1. GROUPING: local flows all originate from 127.0.0.1 ephemeral ports —
--+   per-PROCESS attribution is impossible at the proxy layer. Flows group
--+   by TARGET (`host:port`): one site/app = one group regardless of how
--+   many connections it opens (a download manager spreading over many
--+   servers counts as several groups — documented in the Settings copy).
--+2. SHAPING: the existing 1 s ticker measures aggregate up+down and
--+   refills per-group token buckets at `aggregate/groups` ONLY under real
--+   contention (≥2 active groups AND aggregate ≥ 2 MiB/s floor). Pumps
--+   acquire a `PUMP_BUF`-sized budget before each read (20 ms retry until
--+   the next refill). One group alone / idle tunnels are UNLIMITED — no
--+   throughput tax without contention.
--+3. LIVE TOGGLE: the scheduler is ALWAYS attached at connect (initialized
--+   from settings), so `set_settings` flips an AtomicBool — NO reconnect
--+   (unlike diagnostics/kill-switch, nothing in the data plane changes).
--+   Disabling unblocks waiters and clears the table (tested).
--+4. MERGED PANEL: exit-IP + speed-test share ONE Status card (shared
--+   header, both triggers side-by-side Fill-width, result rows stacked
--+   with label prefixes "exit"/"speed"); all messages/state unchanged
--+   (pure view merge inside the D24 page scrollable).
--+5. P1 PARITY (same batch): TCP_NODELAY on all four carrier legs; ACK
--+   coalescing (8 frames / 20 ms, CLOSE+PING flush, cumulative semantics
--+   unchanged); PONG echoes on the receiving carrier (RTT was
--+   cross-contaminated); send-path copies 5→3 per frame (single-pass
--+   encoder, wire-identity test); carrier writer batches ≤16 frames per
--+   write_vectored; quarantine last-resort fallback (a probe can no
--+   longer blackhole the only link); production prober uses the
--+   user-configured probe target.
--diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
--index 62d858e..2fef1de 100755
----- a/harness/memory/LEDGER.md
--+++ b/harness/memory/LEDGER.md
--@@ -218,3 +218,4 @@
-- 2026-11-21T15:10Z | s19 | RELEASED v0.13.0 (tag run 44910934439 SUCCESS ~7 min; assets verified by download+unzip: multitunnel-v0.13.0-windows-x64.zip 6,506,263 B, 11-entry manifest integrity OK, README/QUICKSTART/server scripts carried; mt-merge-server x86_64 1,826,272 B + aarch64 1,860,808 B; zip staged in sandbox download/v0.13.0-staging/). CI SAGA: the push leg failed on mt-bond carrier_death (documented runner flake, 4th occurrence) → rerun wedged 54 min inside domain_targets_dial (log captured: harness/memory/agent-reports/s19-ci-wedge-job-136868435499.log — 5/6 tests ok, domain_targets 'over 60 seconds', cargo+integration orphans killed) → HARDENED both tests with 60-90 s deadlines (bodies untouched; a wedge now fails in a minute with a LEDGER pointer) → next push run: BOTH tests failed loudly on runners (windows carrier_death 'no live bond carrier', ubuntu domain_targets deadline) while the same-commit PR run was fully green and local 2-core taskset stress (8× integration at test-threads=6) stayed 0-fail → rerun: ALL FOUR JOBS GREEN. ROOT-CAUSE LEAD (backlog, top priority): pick_carrier's 'no live bond carrier' = usable() empty = Wi-Fi dead + LTE flagged !alive mid-death-replay — a carrier-lifecycle race in PRE-EXISTING S11/S18 mt-bond code, GitHub-runner-only (slower I/O scheduling), never reproduced locally; investigate the death handler's replay path + LTE alive-flag transitions on a runner-like environment. Review PR #1 commented + left open.
-- 2026-11-22T04:20Z | s20 | COPILOT CODE REVIEW COMPLETED (the S19 leftover): the cloud Copilot reviewer finally ran from this sandbox via POST /repos/maniesh-NR/MultiTunnel/pulls/1/copilot-code-review — the user's runbook pointed at the standalone @github/copilot CLI, which is installed and network-verified but hard-blocked by the PAT: api.individual.githubcopilot.com answers 401 "Personal Access Token does not have \"Copilot Requests\" permission" (authentic server response, NOT a proxy block; the permission can only be added by editing the PAT in the GitHub web UI — told the user exactly that). The REST code-review endpoint needs only pull-requests:write, which the PAT has. Effort ladder per the fallback instruction: max → quota-rejected ("insufficient remaining quota"), balanced → quota-rejected, LITE → ran (review 6010002263 on 52f1d3c): 12 findings (5 High: step-0 self-heal never cleared sysproxy_set → later no-snapshot restore DISABLES a user's pre-existing proxy; give_up check-and-set unserialized with disconnect → black-hole re-installed AFTER an explicit disconnect; proxy pointed at 127.0.0.1:9 BEFORE the bind → a foreign port-9 owner receives the blocked traffic; black-hole listener stopped even when restore FAILED → stranded blocked proxy, no retry, false "restored" log; diag header wrote user URLs verbatim → credential leak in the paste-ready file; 7 Medium: diag-on-while-connected misses engine debug, failed enable leaves persisted toggle on, exit probe/speed test ride a session that is not the target profile, unbounded pre-\r\n\r\n head buffer, port-0 edit accepted, edited REALITY material unvalidated, temp_link_for pop() = lowest-priority adapter). ALL 12 fixed in 4923566: step-0 clears ownership on successful restore (keeps flag+listener on failure for retry); new Core::proxy_mtx serializes every proxy hand-off (give_up/disconnect×2/toggle-off/step-0/arming; the black-hole BIND hoisted outside the lock so no std lock crosses an await — spawn_blackhole split into bind_blackhole+engage_blackhole); bind-BEFORE-set with no-set-on-bind-failure (stale refused entry keeps blocking, nothing routes to a foreign listener); restore-first release on all paths with ERROR log + retry on failure; redact_url() (scheme://host:port/path, userinfo/query/fragment stripped, invalid → placeholder); diag-on-live reconnects (engine level is spawn-time-only); failed enable reverts the persisted toggle; Session.profile_id pins the live-session match for BOTH probes; SPEED_TEST_MAX_HEAD 64 KiB independent cap; port-0 + REALITY structural validation (vless parser rules, is_base64url now crate-visible; fingerprint deliberately unchecked = import parity); first-link selection (temp_link_for AND test_latency). ROUND 2 (lite, on 4923566): CLEAN PASS — review 6010239912, zero inline comments. PR #1: 12 per-finding maintainer replies + summary, CLOSED. Transcript: harness/memory/agent-reports/s20-copilot-review-transcript.md. TESTS 474/0 (+8: connect_selfheal_clears_stale_proxy_ownership, update_profile_rejects_port_zero_and_invalid_reality, diag_header_redacts_url_credentials, speed_test_rejects_a_header_flood, probes_fall_back_when_active_profile_switched, temp_link_uses_the_top_priority_first_link, diagnostics_enable_failure_reverts_the_toggle, diagnostics_enable_while_connected_reconnects). Gates: fmt clean; clippy host + windows-gnu -D warnings clean; vendored iced 6/0; mt-app --smoke 0; rig launch+alive+render-healthy (82% non-white, 1738 colors — zero GUI code changed this round); zigbuild PE32+ ×4 + ELF ×2. SANDBOX RESTORE NOTES: rustc 1.100.0 reinstalled (rustup minimal), tools/pypkgs ziglang wheel was INCOMPLETE (libc only musl/wasi — stddef.h compile failure; reinstalled 0.14.1 from PyPI), python-xlib reinstalled, /tmp/xkblibs re-extracted from the my-project root debs; DISK: /tmp/mt-target debug trees wiped twice (ENOSPC at 94% — stage release binaries before wiping, per the S19 lesson).
-- 2026-11-22T04:16Z | s20 | v0.13.1 RELEASED: tag on 4923566 → release run 44983522874 SUCCESS (~4 min, no cold-cache stall); ci ×2 on main (44983512609/44983518575) ALL GREEN first try. Assets verified by download+unzip: multitunnel-v0.13.1-windows-x64.zip 6,511,395 B integrity OK (multitunnel.exe PE32+ console 13.8 MB + multitunnel-tray.exe PE32+ GUI 2.95 MB + README v0.13.1 section + QUICKSTART v0.13.1 note + server scripts + LICENSE + THIRD_PARTY_NOTICES + config.example) + mt-merge-server {x86_64 1,826,272 B, aarch64 1,860,808 B} ELFs. Verified copies staged in sandbox download/ (v0.13.1-verify-*). S20 CLOSED.
--+2026-11-24T21:00Z | s21 | IMPLEMENTED (four asks: P0 bug, P1 perf parity, QoS toggle, merged panel; v0.14.0). Sandbox recycled AGAIN (rustup/zigbuild/xkblibs gone; tools/pypkgs ziglang wheel INCOMPLETE again — no lib/libc/include → ring stddef.h failure mid-gates; reinstalled 0.14.1 from PyPI per the S20 note; local clone was STALE at s13 with a dirty PROJECT_MEMORY.md — remote authoritative at s20/v0.13.1, snapshot→/tmp then fast-forward). P0 (user: "stops working when a second connection is added, restart required"): 3 Explore agents mapped the paths; H1 = mid-traffic reconnect re-binds 10808/10809 while untracked in-flight connection tasks pin their sockets (no SO_REUSEADDR anywhere) → bind WSAEADDRINUSE → Failed; PROVEN platform-split: Linux tolerates a listen re-bind over non-listening sockets (std rebind probe in scripts/rebind_probe.rs) while Windows does NOT — the sandbox cannot reproduce the bind refusal, so the regression test asserts the platform-independent observable (lingering client sees EOF after the switch) and pre-fix it FAILS (8.0 s timeout) / post-fix passes. FIX: Shared.conns tracks every connection task's AbortHandle (pruned at 4096 push), run() force-closes whatever the 3 s drain could not reap; AbortPumpsOnDrop aborts the DETACHED pump children too (dropping their JoinHandles leaves them running — found when the first EOF assertion failed; pumps held the socket halves forever); bind_with_retry (5 s budget, 250 ms interval; BIND_WAIT 2→8 s). H2: engine config/log stems now carry a deterministic FNV-1a hash of the FULL endpoint identity (engine_file_stem; same profile keeps its file across reconnects, same-tag different-endpoint profiles never share engine-<tag>.json again — temp probe/speed-test tunnels could rewrite the LIVE engine's config → phantom-live respawn budget exhaustion → DEGRADED while GUI showed Connected). H3: Core.store_mtx serializes profiles.json read-modify-write (import/remove/set_active/update; no await inside the critical sections). H1+H3 tests: picker_switch_with_lingering_client_reconnects, shutdown_force_closes_lingering_connections_and_frees_the_port (mt-net level: EOF + immediate re-bind). P1 PERF (user: v2rayN faster on ONE adapter): (1) TCP_NODELAY on all four legs (client→engine loopback socks5.rs establish; daemon accepted carrier; daemon dial_target ×2 — Nagle+delayed-ACK stalled each 16 KiB frame's tail segment ~40 ms on Linux peers ≈ Mbps-per-carrier stop-and-wait); (2) quarantine can no longer BLACKHOLE the sole link (pick_excluding last-resort fallback over quarantined/down links when no healthy candidate; the dial decides, fail-open; retry-exclusion contract kept; module doc + contract test updated, new sole-link test); (3) production background prober now uses the user-configured probe_target (was hardcoded 1.1.1.1:443); (4) ACK coalescing (ACK_COALESCE_FRAMES=8 / 20 ms window, per-session AckCoalescer, flushed by CLOSE handling + the 1 s PING heartbeat → ≤1 s ACK latency; cumulative semantics unchanged; guard scoped before the await for Send); (5) PONG echoes on the RECEIVING carrier (schedule_bytes re-picked — rtt_ms was cross-contaminated across carriers feeding ECF/Balanced wrong data); (6) send-path copies 5→3 per frame (encode_session_borrowed encodes header+payload in one pass from the slice; send_data sends slices; wire-format identity test); (7) carrier writer batches up to 16 co-queued frames into ONE write_vectored (write_all_vectored with partial-write advance). O — QoS FAIR SHARE toggle (Settings → Network, qos_fair serde-default bool): new mt-net/src/qos.rs QosScheduler groups flows by TARGET, 1 s tick refills buckets at aggregate/groups ONLY under real contention (≥2 groups AND aggregate ≥ 2 MiB/s floor; single group/idle = unlimited), pump read gate blocks until budget (20 ms retry), ALWAYS attached at connect so set_settings flips it LIVE (no reconnect — scheduler flag atomic; log line emitted); 5 scheduler unit tests + live-flip persistence test + GUI draft test. P — merged Exit & speed panel: one Status card (shared header, both triggers side-by-side Fill-width, exit row + verdict + speed row + hint stacked compactly; all messages/state unchanged; D24-safe inside the page scroll), rig-verified sf=1.0 (VLM + knob-pixel-diff: 1957 px changed, knob center 39→55) + sf=1.5. TESTS 490/0 (+16: mt-engine stem ×4, merge sole-link ×1, mt-net shutdown-force-close ×1 + qos ×5, frame wire-identity ×1, root P0 ×1 + qos ×1, mt-app qos ×1, mt-service serde ×1). GATES: fmt clean; clippy host + windows-gnu -D warnings clean (zig shims + fresh ziglang wheel); cargo test --workspace 490/0; vendored iced 6/0; mt-app --smoke 0; rig sf=1.0+1.5; zigbuild PE32+ ×4 + ELF ×2. RELEASE: v0.14.0 tag → run 45477702090 — attempt-1 ubuntu leg carrier_death 'no live bond carrier' (documented S11/S18/S19 pre-existing runner race, signature exactly as backlogged), attempt-2 windows leg domain_targets 60 s deadline, attempt-3 SUCCESS; same-commit ci.yml runs green on BOTH OS legs throughout + local 2-core taskset stress 8/8; all four logs archived under harness/memory/agent-reports/s21-ci-*.log. Assets download-verified: multitunnel-v0.14.0-windows-x64.zip 6,540,352 B (11-entry manifest identical shape to v0.13.1: multitunnel.exe + tray + docs + server scripts) + mt-merge-server x86_64 1,845,824 B + aarch64 1,864,608 B. PARKED (needs user input or new scope): single-instance guard (H4 — two app instances race the sysproxy snapshot; needs a CreateMutexW design decision), BufReader-style carrier read batching (2 read_exact syscalls/frame), MAX_PAYLOAD 64 KiB protocol bump (needs a coordinated mt-merge-server update on the user's VM), carrier-lifecycle race root-cause (S19 backlog — this session hit it 2/3 release legs; reruns shipped), upstream-iced migration, subscription auto-update, vmess validation, profiles.json locking (partially addressed by store_mtx in-process; cross-process locking still open), engine log rotation, listener max_conns. Copilot CLI still blocked by PAT (S20 finding unchanged — cloud route works, needs only pull-requests:write).
--diff --git a/harness/memory/agent-reports/s21-ci-carrier-death-release-run1.log b/harness/memory/agent-reports/s21-ci-carrier-death-release-run1.log
--new file mode 100644
--index 0000000..c42ab0b
----- /dev/null
--+++ b/harness/memory/agent-reports/s21-ci-carrier-death-release-run1.log
--@@ -0,0 +1,591 @@
--+﻿2026-11-24T20:08:59.0729386Z Current runner version: '2.338.2'
--+2026-11-24T20:08:59.0755442Z ##[group]Runner Image Provisioner
--+2026-11-24T20:08:59.0756501Z Hosted Compute Agent
--+2026-11-24T20:08:59.0757129Z Version: 20261111.635
--+2026-11-24T20:08:59.0757806Z Commit: b8a594ea514ff44cadb5342986c68b1eac6e22ed
--+2026-11-24T20:08:59.0758608Z Build Date: 2026-11-11T01:49:01Z
--+2026-11-24T20:08:59.0759371Z Worker ID: {e19d26c1-8e95-4353-9497-e891587fab3c}
--+2026-11-24T20:08:59.0760131Z Region: westus
--+2026-11-24T20:08:59.0760708Z Cloud: Azure
--+2026-11-24T20:08:59.0761289Z ##[endgroup]
--+2026-11-24T20:08:59.0762874Z ##[group]Operating System
--+2026-11-24T20:08:59.0763546Z Ubuntu
--+2026-11-24T20:08:59.0764061Z 26.04.1
--+2026-11-24T20:08:59.0764858Z LTS
--+2026-11-24T20:08:59.0765436Z ##[endgroup]
--+2026-11-24T20:08:59.0766039Z ##[group]Runner Image
--+2026-11-24T20:08:59.0766813Z Image: ubuntu-26.04
--+2026-11-24T20:08:59.0767371Z Version: 20261116.202.1
--+2026-11-24T20:08:59.0768753Z Included Software: https://github.com/actions/runner-images/blob/ubuntu26/20261116.202/images/ubuntu/Ubuntu2604-Readme.md
--+2026-11-24T20:08:59.0770350Z Image Release: https://github.com/actions/runner-images/releases/tag/ubuntu26%2F20261116.202
--+2026-11-24T20:08:59.0771391Z ##[endgroup]
--+2026-11-24T20:08:59.0772861Z ##[group]GITHUB_TOKEN Permissions
--+2026-11-24T20:08:59.0775265Z Contents: write
--+2026-11-24T20:08:59.0775871Z Metadata: read
--+2026-11-24T20:08:59.0776555Z ##[endgroup]
--+2026-11-24T20:08:59.0778699Z Secret source: Actions
--+2026-11-24T20:08:59.0779791Z Cache mode: write
--+2026-11-24T20:08:59.0780491Z Prepare workflow directory
--+2026-11-24T20:08:59.1174316Z Prepare all required actions
--+2026-11-24T20:08:59.1243861Z Getting action download info
--+2026-11-24T20:08:59.4882166Z Download action repository 'actions/checkout@v4' (SHA:11d5960a326750d5838078e36cf38b85af677262)
--+2026-11-24T20:08:59.5855381Z Download action repository 'dtolnay/rust-toolchain@stable' (SHA:39008827e6b4319a349e9a42dfc90579dd25f1ca)
--+2026-11-24T20:08:59.8078102Z Download action repository 'Swatinem/rust-cache@v2' (SHA:d1527c8f67ef9af357290475d4af4b0fab8f12e0)
--+2026-11-24T20:09:00.1496465Z Download action repository 'actions/upload-artifact@v4' (SHA:ea165f8d65b6e75b540449e92b4886f43607fa02)
--+2026-11-24T20:09:00.2327639Z Download action repository 'softprops/action-gh-release@v2' (SHA:3bb12739c298aeb8a4eeaf626c5b8d85266b0e65)
--+2026-11-24T20:09:00.6818899Z Complete job name: build-linux-daemon
--+2026-11-24T20:09:00.7762510Z ##[group]Run actions/checkout@v4
--+2026-11-24T20:09:00.7763485Z with:
--+2026-11-24T20:09:00.7763970Z   repository: maniesh-NR/MultiTunnel
--+2026-11-24T20:09:00.7768172Z   token: ***
--+2026-11-24T20:09:00.7768642Z   ssh-strict: true
--+2026-11-24T20:09:00.7769105Z   ssh-user: git
--+2026-11-24T20:09:00.7769582Z   persist-credentials: true
--+2026-11-24T20:09:00.7770087Z   clean: true
--+2026-11-24T20:09:00.7770560Z   sparse-checkout-cone-mode: true
--+2026-11-24T20:09:00.7771100Z   fetch-depth: 1
--+2026-11-24T20:09:00.7771563Z   fetch-tags: false
--+2026-11-24T20:09:00.7772036Z   show-progress: true
--+2026-11-24T20:09:00.7772511Z   lfs: false
--+2026-11-24T20:09:00.7772974Z   submodules: false
--+2026-11-24T20:09:00.7773442Z   set-safe-directory: true
--+2026-11-24T20:09:00.7773965Z   allow-unsafe-pr-checkout: false
--+2026-11-24T20:09:00.7774945Z ##[endgroup]
--+2026-11-24T20:09:01.0725321Z Syncing repository: maniesh-NR/MultiTunnel
--+2026-11-24T20:09:01.0728317Z ##[group]Getting Git version info
--+2026-11-24T20:09:01.0729533Z Working directory is '/home/runner/work/MultiTunnel/MultiTunnel'
--+2026-11-24T20:09:01.0731210Z [command]/usr/bin/git version
--+2026-11-24T20:09:01.0765382Z git version 2.56.1
--+2026-11-24T20:09:01.0788788Z ##[endgroup]
--+2026-11-24T20:09:01.0805088Z Temporarily overriding HOME='/home/runner/work/_temp/0ee5723b-6558-4d98-b163-e8c4c43f2b23' before making global git config changes
--+2026-11-24T20:09:01.0807588Z Adding repository directory to the temporary git global config as a safe directory
--+2026-11-24T20:09:01.0811738Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/MultiTunnel/MultiTunnel
--+2026-11-24T20:09:01.0880888Z Deleting the contents of '/home/runner/work/MultiTunnel/MultiTunnel'
--+2026-11-24T20:09:01.0884842Z ##[group]Initializing the repository
--+2026-11-24T20:09:01.0889953Z [command]/usr/bin/git init /home/runner/work/MultiTunnel/MultiTunnel
--+2026-11-24T20:09:01.1021879Z hint: Using 'master' as the name for the initial branch. This default branch name
--+2026-11-24T20:09:01.1023932Z hint: will change to "main" in Git 3.0. To configure the initial branch name
--+2026-11-24T20:09:01.1025881Z hint: to use in all of your new repositories, which will suppress this warning,
--+2026-11-24T20:09:01.1027116Z hint: call:
--+2026-11-24T20:09:01.1027747Z hint:
--+2026-11-24T20:09:01.1028506Z hint: 	git config --global init.defaultBranch <name>
--+2026-11-24T20:09:01.1029719Z hint:
--+2026-11-24T20:09:01.1030577Z hint: Names commonly chosen instead of 'master' are 'main', 'trunk' and
--+2026-11-24T20:09:01.1032432Z hint: 'development'. The just-created branch can be renamed via this command:
--+2026-11-24T20:09:01.1034008Z hint:
--+2026-11-24T20:09:01.1034939Z hint: 	git branch -m <name>
--+2026-11-24T20:09:01.1035816Z hint:
--+2026-11-24T20:09:01.1036973Z hint: Disable this message with "git config set advice.defaultBranchName false"
--+2026-11-24T20:09:01.1038359Z Initialized empty Git repository in /home/runner/work/MultiTunnel/MultiTunnel/.git/
--+2026-11-24T20:09:01.1049924Z [command]/usr/bin/git remote add origin https://github.com/maniesh-NR/MultiTunnel
--+2026-11-24T20:09:01.1112076Z ##[endgroup]
--+2026-11-24T20:09:01.1113003Z ##[group]Disabling automatic garbage collection
--+2026-11-24T20:09:01.1117696Z [command]/usr/bin/git config --local gc.auto 0
--+2026-11-24T20:09:01.1154456Z ##[endgroup]
--+2026-11-24T20:09:01.1155601Z ##[group]Setting up auth
--+2026-11-24T20:09:01.1163253Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
--+2026-11-24T20:09:01.1200437Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
--+2026-11-24T20:09:01.1829030Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
--+2026-11-24T20:09:01.1868560Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
--+2026-11-24T20:09:01.2173357Z [command]/usr/bin/git config --local --name-only --get-regexp ^includeIf\.gitdir:
--+2026-11-24T20:09:01.2213363Z [command]/usr/bin/git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url
--+2026-11-24T20:09:01.2504832Z [command]/usr/bin/git config --local http.https://github.com/.extraheader AUTHORIZATION: basic ***
--+2026-11-24T20:09:01.2549715Z ##[endgroup]
--+2026-11-24T20:09:01.2551454Z ##[group]Fetching the repository
--+2026-11-24T20:09:01.2562088Z [command]/usr/bin/git -c protocol.version=2 fetch --no-tags --prune --no-recurse-submodules --depth=1 origin +e510e439249fe4072477c05040bd5b9d0678db6a:refs/tags/v0.14.0
--+2026-11-24T20:09:01.8095159Z From https://github.com/maniesh-NR/MultiTunnel
--+2026-11-24T20:09:01.8096932Z  * [new ref]         e510e439249fe4072477c05040bd5b9d0678db6a -> v0.14.0
--+2026-11-24T20:09:01.8101801Z ##[endgroup]
--+2026-11-24T20:09:01.8104116Z ##[group]Determining the checkout info
--+2026-11-24T20:09:01.8106792Z ##[endgroup]
--+2026-11-24T20:09:01.8111131Z [command]/usr/bin/git sparse-checkout disable
--+2026-11-24T20:09:01.8172199Z [command]/usr/bin/git config --local --unset-all extensions.worktreeConfig
--+2026-11-24T20:09:01.8205255Z ##[group]Checking out the ref
--+2026-11-24T20:09:01.8210966Z [command]/usr/bin/git checkout --progress --force refs/tags/v0.14.0
--+2026-11-24T20:09:01.8583679Z Note: switching to 'refs/tags/v0.14.0'.
--+2026-11-24T20:09:01.8584787Z 
--+2026-11-24T20:09:01.8585540Z You are in 'detached HEAD' state. You can look around, make experimental
--+2026-11-24T20:09:01.8587284Z changes and commit them, and you can discard any commits you make in this
--+2026-11-24T20:09:01.8588999Z state without impacting any branches by switching back to a branch.
--+2026-11-24T20:09:01.8590563Z 
--+2026-11-24T20:09:01.8591347Z If you want to create a new branch to retain commits you create, you may
--+2026-11-24T20:09:01.8594121Z do so (now or later) by using -c with the switch command. Example:
--+2026-11-24T20:09:01.8596063Z 
--+2026-11-24T20:09:01.8596978Z   git switch -c <new-branch-name>
--+2026-11-24T20:09:01.8598226Z 
--+2026-11-24T20:09:01.8599026Z Or undo this operation with:
--+2026-11-24T20:09:01.8600172Z 
--+2026-11-24T20:09:01.8600878Z   git switch -
--+2026-11-24T20:09:01.8601874Z 
--+2026-11-24T20:09:01.8603372Z Turn off this advice by setting config variable advice.detachedHead to false
--+2026-11-24T20:09:01.8605802Z 
--+2026-11-24T20:09:01.8607054Z HEAD is now at e510e43 backup(s21-release-prep-2): 2026-11-24T20:08:15Z
--+2026-11-24T20:09:01.8613092Z ##[endgroup]
--+2026-11-24T20:09:01.8656923Z [command]/usr/bin/git log -1 --format=%H
--+2026-11-24T20:09:01.8684695Z e510e439249fe4072477c05040bd5b9d0678db6a
--+2026-11-24T20:09:01.9431291Z ##[group]Run dtolnay/rust-toolchain@stable
--+2026-11-24T20:09:01.9432625Z with:
--+2026-11-24T20:09:01.9433555Z   components: rustfmt, clippy
--+2026-11-24T20:09:01.9434803Z   toolchain: stable
--+2026-11-24T20:09:01.9435745Z ##[endgroup]
--+2026-11-24T20:09:01.9526187Z ##[start-action display=Parse toolchain version;id=__dtolnay_rust-toolchain.parse]
--+2026-11-24T20:09:01.9608440Z ##[group]Run if [[ -z $toolchain ]]; then
--+2026-11-24T20:09:01.9609827Z [36;1mif [[ -z $toolchain ]]; then[0m
--+2026-11-24T20:09:01.9611866Z [36;1m  # GitHub does not enforce `required: true` inputs itself. https://github.com/actions/runner/issues/1070[0m
--+2026-11-24T20:09:01.9614067Z [36;1m  echo "'toolchain' is a required input" >&2[0m
--+2026-11-24T20:09:01.9615483Z [36;1m  exit 1[0m
--+2026-11-24T20:09:01.9616888Z [36;1melif [[ $toolchain =~ ^stable' '[0-9]+' '(year|month|week|day)s?' 'ago$ ]]; then[0m
--+2026-11-24T20:09:01.9618560Z [36;1m  if [[ Linux == macOS ]]; then[0m
--+2026-11-24T20:09:01.9620814Z [36;1m    echo "toolchain=1.$((($(date -v-$(sed 's/stable \([0-9]*\) \(.\).*/\1\2/' <<< $toolchain) +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:09:01.9622963Z [36;1m  else[0m
--+2026-11-24T20:09:01.9624729Z [36;1m    echo "toolchain=1.$((($(date --date "${toolchain#stable }" +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:09:01.9626670Z [36;1m  fi[0m
--+2026-11-24T20:09:01.9627926Z [36;1melif [[ $toolchain =~ ^stable' 'minus' '[0-9]+' 'releases?$ ]]; then[0m
--+2026-11-24T20:09:01.9630091Z [36;1m  echo "toolchain=1.$((($(date +%s)/60/60/24-16569)/7/6-${toolchain//[^0-9]/}))" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:09:01.9632126Z [36;1melif [[ $toolchain =~ ^1\.[0-9]+$ ]]; then[0m
--+2026-11-24T20:09:01.9634264Z [36;1m  echo "toolchain=1.$((i=${toolchain#1.}, c=($(date +%s)/60/60/24-16569)/7/6, i+9*i*(10*i<=c)+90*i*(100*i<=c)))" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:09:01.9636407Z [36;1melse[0m
--+2026-11-24T20:09:01.9637284Z [36;1m  delimiter=EOF[0m
--+2026-11-24T20:09:01.9638594Z [36;1m  while [[ $toolchain == *$delimiter* ]]; do delimiter+=_; done[0m
--+2026-11-24T20:09:01.9640672Z [36;1m  printf 'toolchain<<%s\n%s\n%s\n' "$delimiter" "$toolchain" "$delimiter" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:09:01.9642409Z [36;1mfi[0m
--+2026-11-24T20:09:01.9683303Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:09:01.9684830Z env:
--+2026-11-24T20:09:01.9685667Z   toolchain: stable
--+2026-11-24T20:09:01.9686573Z ##[endgroup]
--+2026-11-24T20:09:01.9822111Z ##[end-action id=__dtolnay_rust-toolchain.parse;outcome=success;conclusion=success;duration_ms=29]
--+2026-11-24T20:09:01.9831824Z ##[start-action display=Set $CARGO_HOME;id=__dtolnay_rust-toolchain.__run]
--+2026-11-24T20:09:01.9889017Z ##[group]Run echo CARGO_HOME=${CARGO_HOME:-"$HOME/.cargo"} >> $GITHUB_ENV
--+2026-11-24T20:09:01.9891016Z [36;1mecho CARGO_HOME=${CARGO_HOME:-"$HOME/.cargo"} >> $GITHUB_ENV[0m
--+2026-11-24T20:09:01.9928506Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:09:01.9929943Z ##[endgroup]
--+2026-11-24T20:09:02.0017022Z ##[end-action id=__dtolnay_rust-toolchain.__run;outcome=success;conclusion=success;duration_ms=18]
--+2026-11-24T20:09:02.0024024Z ##[start-action display=Install rustup if needed;id=__dtolnay_rust-toolchain.__run_2]
--+2026-11-24T20:09:02.0081548Z ##[group]Run if ! command -v rustup &>/dev/null; then
--+2026-11-24T20:09:02.0082992Z [36;1mif ! command -v rustup &>/dev/null; then[0m
--+2026-11-24T20:09:02.0086004Z [36;1m  curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused --location --silent --show-error --fail https://sh.rustup.rs | sh -s -- --default-toolchain none -y[0m
--+2026-11-24T20:09:02.0088830Z [36;1m  echo "$CARGO_HOME/bin" >> $GITHUB_PATH[0m
--+2026-11-24T20:09:02.0090053Z [36;1mfi[0m
--+2026-11-24T20:09:02.0126566Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:09:02.0127916Z env:
--+2026-11-24T20:09:02.0128777Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:02.0129929Z ##[endgroup]
--+2026-11-24T20:09:02.0216981Z ##[end-action id=__dtolnay_rust-toolchain.__run_2;outcome=success;conclusion=success;duration_ms=18]
--+2026-11-24T20:09:02.0224148Z ##[start-action display=Install rustup if needed on windows;id=__dtolnay_rust-toolchain.__run_3]
--+2026-11-24T20:09:02.0241199Z ##[end-action id=__dtolnay_rust-toolchain.__run_3;outcome=skipped;conclusion=skipped;duration_ms=0]
--+2026-11-24T20:09:02.0262344Z ##[start-action display=rustup toolchain install stable;id=__dtolnay_rust-toolchain.__run_4]
--+2026-11-24T20:09:02.0304922Z ##[group]Run set -f  # split targets and components on commas and whitespace, without glob expansion
--+2026-11-24T20:09:02.0307422Z [36;1mset -f  # split targets and components on commas and whitespace, without glob expansion[0m
--+2026-11-24T20:09:02.0309196Z [36;1mflags=()[0m
--+2026-11-24T20:09:02.0310348Z [36;1mfor t in ${targets//,/ }; do flags+=(--target "$t"); done[0m
--+2026-11-24T20:09:02.0311983Z [36;1mfor c in ${components//,/ }; do flags+=(--component "$c"); done[0m
--+2026-11-24T20:09:02.0313931Z [36;1mif [[ $toolchain == nightly && -n $components ]]; then flags+=(--allow-downgrade); fi[0m
--+2026-11-24T20:09:02.0315777Z [36;1mlog=$(mktemp)[0m
--+2026-11-24T20:09:02.0316733Z [36;1mtrap 'rm -f "$log"' EXIT[0m
--+2026-11-24T20:09:02.0317797Z [36;1mfor attempt in 1 2 3 4 5; do[0m
--+2026-11-24T20:09:02.0319985Z [36;1m  if rustup toolchain install "$toolchain" "${flags[@]}" --profile minimal --no-self-update --force-non-host 2>&1 | tee "$log"; then[0m
--+2026-11-24T20:09:02.0322235Z [36;1m    break[0m
--+2026-11-24T20:09:02.0323136Z [36;1m  else[0m
--+2026-11-24T20:09:02.0323976Z [36;1m    status=$?[0m
--+2026-11-24T20:09:02.0324993Z [36;1m  fi[0m
--+2026-11-24T20:09:02.0326149Z [36;1m  if [[ $attempt == 5 ]] || ! grep -Fq 'checksum failed for ' "$log" \[0m
--+2026-11-24T20:09:02.0328272Z [36;1m      || ! grep -Fq 'this is likely due to an ongoing update of the official release server' "$log"; then[0m
--+2026-11-24T20:09:02.0330095Z [36;1m    exit "$status"[0m
--+2026-11-24T20:09:02.0331045Z [36;1m  fi[0m
--+2026-11-24T20:09:02.0332472Z [36;1m  echo "Retrying rustup toolchain install in 30 seconds (retry $attempt of 4)" >&2[0m
--+2026-11-24T20:09:02.0334151Z [36;1m  sleep 30[0m
--+2026-11-24T20:09:02.0335107Z [36;1mdone[0m
--+2026-11-24T20:09:02.0373235Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:09:02.0374784Z env:
--+2026-11-24T20:09:02.0375640Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:02.0376681Z   toolchain: stable
--+2026-11-24T20:09:02.0377538Z   targets: 
--+2026-11-24T20:09:02.0378383Z   components: rustfmt, clippy
--+2026-11-24T20:09:02.0379424Z   RUSTUP_PERMIT_COPY_RENAME: 1
--+2026-11-24T20:09:02.0380409Z ##[endgroup]
--+2026-11-24T20:09:02.2313103Z info: syncing channel updates for stable-x86_64-unknown-linux-gnu
--+2026-11-24T20:09:02.3246052Z info: latest update on 2026-11-12 for version 1.100.0 (0ef1bc386 2026-11-09)
--+2026-11-24T20:09:02.3393971Z info: component clippy is up to date
--+2026-11-24T20:09:02.3396420Z info: component rustfmt is up to date
--+2026-11-24T20:09:02.3402647Z 
--+2026-11-24T20:09:02.3490310Z   stable-x86_64-unknown-linux-gnu unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
--+2026-11-24T20:09:02.3492270Z 
--+2026-11-24T20:09:02.3552437Z ##[end-action id=__dtolnay_rust-toolchain.__run_4;outcome=success;conclusion=success;duration_ms=328]
--+2026-11-24T20:09:02.3563566Z ##[start-action display=rustup default stable;id=__dtolnay_rust-toolchain.__run_5]
--+2026-11-24T20:09:02.3605716Z ##[group]Run rustup default "$toolchain" --force-non-host
--+2026-11-24T20:09:02.3607197Z [36;1mrustup default "$toolchain" --force-non-host[0m
--+2026-11-24T20:09:02.3646525Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:09:02.3674243Z env:
--+2026-11-24T20:09:02.3675758Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:02.3677456Z   toolchain: stable
--+2026-11-24T20:09:02.3678887Z ##[endgroup]
--+2026-11-24T20:09:02.3800027Z info: using existing install for stable-x86_64-unknown-linux-gnu
--+2026-11-24T20:09:02.3810232Z info: default toolchain set to stable-x86_64-unknown-linux-gnu
--+2026-11-24T20:09:02.3811674Z 
--+2026-11-24T20:09:02.3894417Z   stable-x86_64-unknown-linux-gnu unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
--+2026-11-24T20:09:02.3896832Z 
--+2026-11-24T20:09:02.3919415Z ##[end-action id=__dtolnay_rust-toolchain.__run_5;outcome=success;conclusion=success;duration_ms=35]
--+2026-11-24T20:09:02.3927824Z ##[start-action display=Create cachekey;id=__dtolnay_rust-toolchain.rustc-version]
--+2026-11-24T20:09:02.3969921Z ##[group]Run DATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')
--+2026-11-24T20:09:02.3972999Z [36;1mDATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')[0m
--+2026-11-24T20:09:02.3975729Z [36;1mHASH=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-hash: //p')[0m
--+2026-11-24T20:09:02.3977688Z [36;1mecho "cachekey=$(echo $DATE$HASH | head -c12)" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:09:02.4016387Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:09:02.4017680Z env:
--+2026-11-24T20:09:02.4018519Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:02.4019546Z   toolchain: stable
--+2026-11-24T20:09:02.4020398Z ##[endgroup]
--+2026-11-24T20:09:02.4415951Z ##[end-action id=__dtolnay_rust-toolchain.rustc-version;outcome=success;conclusion=success;duration_ms=48]
--+2026-11-24T20:09:02.4423124Z ##[start-action display=Disable incremental compilation;id=__dtolnay_rust-toolchain.__run_6]
--+2026-11-24T20:09:02.4466215Z ##[group]Run if [ -z "${CARGO_INCREMENTAL+set}" ]; then
--+2026-11-24T20:09:02.4467602Z [36;1mif [ -z "${CARGO_INCREMENTAL+set}" ]; then[0m
--+2026-11-24T20:09:02.4468913Z [36;1m  echo CARGO_INCREMENTAL=0 >> $GITHUB_ENV[0m
--+2026-11-24T20:09:02.4470085Z [36;1mfi[0m
--+2026-11-24T20:09:02.4508904Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:09:02.4510206Z env:
--+2026-11-24T20:09:02.4511037Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:02.4512062Z ##[endgroup]
--+2026-11-24T20:09:02.4596458Z ##[end-action id=__dtolnay_rust-toolchain.__run_6;outcome=success;conclusion=success;duration_ms=17]
--+2026-11-24T20:09:02.4602909Z ##[start-action display=Enable colors in Cargo output;id=__dtolnay_rust-toolchain.__run_7]
--+2026-11-24T20:09:02.4643502Z ##[group]Run if [ -z "${CARGO_TERM_COLOR+set}" ]; then
--+2026-11-24T20:09:02.4645090Z [36;1mif [ -z "${CARGO_TERM_COLOR+set}" ]; then[0m
--+2026-11-24T20:09:02.4646438Z [36;1m  echo CARGO_TERM_COLOR=always >> $GITHUB_ENV[0m
--+2026-11-24T20:09:02.4647651Z [36;1mfi[0m
--+2026-11-24T20:09:02.4684366Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:09:02.4685880Z env:
--+2026-11-24T20:09:02.4686722Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:02.4687755Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:09:02.4688647Z ##[endgroup]
--+2026-11-24T20:09:02.4768601Z ##[end-action id=__dtolnay_rust-toolchain.__run_7;outcome=success;conclusion=success;duration_ms=16]
--+2026-11-24T20:09:02.4776399Z ##[start-action display=Enable Cargo sparse registry;id=__dtolnay_rust-toolchain.__run_8]
--+2026-11-24T20:09:02.4818114Z ##[group]Run # implemented in 1.66, stabilized in 1.68, made default in 1.70
--+2026-11-24T20:09:02.4820146Z [36;1m# implemented in 1.66, stabilized in 1.68, made default in 1.70[0m
--+2026-11-24T20:09:02.4822722Z [36;1mif [ -z "${CARGO_REGISTRIES_CRATES_IO_PROTOCOL+set}" -o -f "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol ]; then[0m
--+2026-11-24T20:09:02.4825585Z [36;1m  if rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[89]\.'; then[0m
--+2026-11-24T20:09:02.4827808Z [36;1m    touch "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
--+2026-11-24T20:09:02.4829874Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse >> $GITHUB_ENV[0m
--+2026-11-24T20:09:02.4831868Z [36;1m  elif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[67]\.'; then[0m
--+2026-11-24T20:09:02.4834113Z [36;1m    touch "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
--+2026-11-24T20:09:02.4836480Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git >> $GITHUB_ENV[0m
--+2026-11-24T20:09:02.4837932Z [36;1m  fi[0m
--+2026-11-24T20:09:02.4838742Z [36;1mfi[0m
--+2026-11-24T20:09:02.4873824Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:09:02.4875259Z env:
--+2026-11-24T20:09:02.4876093Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:02.4877123Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:09:02.4878046Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:09:02.4879002Z   toolchain: stable
--+2026-11-24T20:09:02.4879860Z ##[endgroup]
--+2026-11-24T20:09:02.5227951Z ##[end-action id=__dtolnay_rust-toolchain.__run_8;outcome=success;conclusion=success;duration_ms=44]
--+2026-11-24T20:09:02.5236988Z ##[start-action display=Work around spurious network errors in curl 8.0;id=__dtolnay_rust-toolchain.__run_9]
--+2026-11-24T20:09:02.5279870Z ##[group]Run # https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation
--+2026-11-24T20:09:02.5282504Z [36;1m# https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation[0m
--+2026-11-24T20:09:02.5285085Z [36;1mif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.7[01]\.'; then[0m
--+2026-11-24T20:09:02.5286896Z [36;1m  echo CARGO_HTTP_MULTIPLEXING=false >> $GITHUB_ENV[0m
--+2026-11-24T20:09:02.5288139Z [36;1mfi[0m
--+2026-11-24T20:09:02.5327256Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:09:02.5328516Z env:
--+2026-11-24T20:09:02.5329393Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:02.5330403Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:09:02.5331299Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:09:02.5332204Z   toolchain: stable
--+2026-11-24T20:09:02.5333030Z ##[endgroup]
--+2026-11-24T20:09:02.5561980Z ##[end-action id=__dtolnay_rust-toolchain.__run_9;outcome=success;conclusion=success;duration_ms=32]
--+2026-11-24T20:09:02.5572379Z ##[start-action display=rustc +stable --version --verbose;id=__dtolnay_rust-toolchain.__run_10]
--+2026-11-24T20:09:02.5613985Z ##[group]Run rustc +"$toolchain" --version --verbose
--+2026-11-24T20:09:02.5615889Z [36;1mrustc +"$toolchain" --version --verbose[0m
--+2026-11-24T20:09:02.5655288Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:09:02.5656556Z env:
--+2026-11-24T20:09:02.5657384Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:02.5658410Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:09:02.5659315Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:09:02.5660233Z   toolchain: stable
--+2026-11-24T20:09:02.5661076Z ##[endgroup]
--+2026-11-24T20:09:02.5847271Z rustc 1.100.0 (0ef1bc386 2026-11-09)
--+2026-11-24T20:09:02.5849398Z binary: rustc
--+2026-11-24T20:09:02.5851520Z commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a
--+2026-11-24T20:09:02.5853668Z commit-date: 2026-11-09
--+2026-11-24T20:09:02.5855499Z host: x86_64-unknown-linux-gnu
--+2026-11-24T20:09:02.5857212Z release: 1.100.0
--+2026-11-24T20:09:02.5858712Z LLVM version: 23.1.5
--+2026-11-24T20:09:02.5881370Z ##[end-action id=__dtolnay_rust-toolchain.__run_10;outcome=success;conclusion=success;duration_ms=30]
--+2026-11-24T20:09:02.6099273Z ##[group]Run Swatinem/rust-cache@v2
--+2026-11-24T20:09:02.6100614Z with:
--+2026-11-24T20:09:02.6101398Z   prefix-key: v0-rust
--+2026-11-24T20:09:02.6102280Z   add-job-id-key: true
--+2026-11-24T20:09:02.6103226Z   add-rust-environment-hash-key: true
--+2026-11-24T20:09:02.6104289Z   cache-targets: true
--+2026-11-24T20:09:02.6105482Z   cache-all-crates: false
--+2026-11-24T20:09:02.6106456Z   cache-workspace-crates: false
--+2026-11-24T20:09:02.6107425Z   save-if: true
--+2026-11-24T20:09:02.6108266Z   cache-provider: github
--+2026-11-24T20:09:02.6109168Z   cache-bin: true
--+2026-11-24T20:09:02.6109987Z   lookup-only: false
--+2026-11-24T20:09:02.6110834Z   cmd-format: {0}
--+2026-11-24T20:09:02.6111632Z env:
--+2026-11-24T20:09:02.6112426Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:02.6113426Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:09:02.6114312Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:09:02.6115489Z ##[endgroup]
--+2026-11-24T20:09:03.0504720Z ##[group]Cache Configuration
--+2026-11-24T20:09:03.0507633Z Cache Provider:
--+2026-11-24T20:09:03.0509759Z     github
--+2026-11-24T20:09:03.0511923Z Workspaces:
--+2026-11-24T20:09:03.0514826Z     /home/runner/work/MultiTunnel/MultiTunnel
--+2026-11-24T20:09:03.0517348Z Cache Paths:
--+2026-11-24T20:09:03.0519749Z     /home/runner/.cargo/bin
--+2026-11-24T20:09:03.0522303Z     /home/runner/.cargo/.crates.toml
--+2026-11-24T20:09:03.0525194Z     /home/runner/.cargo/.crates2.json
--+2026-11-24T20:09:03.0528148Z     /home/runner/.cargo/registry
--+2026-11-24T20:09:03.0530807Z     /home/runner/.cargo/git
--+2026-11-24T20:09:03.0533519Z     /home/runner/work/MultiTunnel/MultiTunnel/target
--+2026-11-24T20:09:03.0536365Z Restore Key:
--+2026-11-24T20:09:03.0538453Z     v0-rust-build-linux-daemon-Linux-x64-2fd77104
--+2026-11-24T20:09:03.0541024Z Cache Key:
--+2026-11-24T20:09:03.0543088Z     v0-rust-build-linux-daemon-Linux-x64-2fd77104-39107a77
--+2026-11-24T20:09:03.0545811Z .. Prefix:
--+2026-11-24T20:09:03.0547455Z   - v0-rust-build-linux-daemon-Linux-x64
--+2026-11-24T20:09:03.0549496Z .. Environment considered:
--+2026-11-24T20:09:03.0551335Z   - Rust Versions:
--+2026-11-24T20:09:03.0553619Z     - 1.100.0 x86_64-unknown-linux-gnu 0ef1bc386059596dada8fb65b50425aab2ea637a
--+2026-11-24T20:09:03.0556520Z   - CARGO_HOME
--+2026-11-24T20:09:03.0558508Z   - CARGO_INCREMENTAL
--+2026-11-24T20:09:03.0638794Z   - CARGO_TERM_COLOR
--+2026-11-24T20:09:03.0640600Z .. Lockfiles considered:
--+2026-11-24T20:09:03.0642672Z   - /home/runner/work/MultiTunnel/MultiTunnel/Cargo.lock
--+2026-11-24T20:09:03.0645555Z   - /home/runner/work/MultiTunnel/MultiTunnel/Cargo.toml
--+2026-11-24T20:09:03.0648459Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-app/Cargo.toml
--+2026-11-24T20:09:03.0651701Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond/Cargo.toml
--+2026-11-24T20:09:03.0655129Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-engine/Cargo.toml
--+2026-11-24T20:09:03.0658441Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-links/Cargo.toml
--+2026-11-24T20:09:03.0661548Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-m3/Cargo.toml
--+2026-11-24T20:09:03.0664792Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net/Cargo.toml
--+2026-11-24T20:09:03.0667987Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-platform/Cargo.toml
--+2026-11-24T20:09:03.0672305Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-service/Cargo.toml
--+2026-11-24T20:09:03.0676043Z ##[endgroup]
--+2026-11-24T20:09:03.0676794Z 
--+2026-11-24T20:09:03.0677522Z ... Restoring cache ...
--+2026-11-24T20:09:03.3061004Z No cache found.
--+2026-11-24T20:09:03.3203938Z ##[group]Run cargo fmt --all --check
--+2026-11-24T20:09:03.3205302Z [36;1mcargo fmt --all --check[0m
--+2026-11-24T20:09:03.3245610Z shell: /usr/bin/bash -e {0}
--+2026-11-24T20:09:03.3246549Z env:
--+2026-11-24T20:09:03.3247344Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:03.3248343Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:09:03.3249232Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:09:03.3250157Z   CACHE_ON_FAILURE: false
--+2026-11-24T20:09:03.3251053Z ##[endgroup]
--+2026-11-24T20:09:04.1474369Z ##[group]Run cargo clippy -p mt-bond --all-targets -- -D warnings
--+2026-11-24T20:09:04.1475406Z [36;1mcargo clippy -p mt-bond --all-targets -- -D warnings[0m
--+2026-11-24T20:09:04.1507893Z shell: /usr/bin/bash -e {0}
--+2026-11-24T20:09:04.1508177Z env:
--+2026-11-24T20:09:04.1508402Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:04.1508704Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:09:04.1508956Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:09:04.1509217Z   CACHE_ON_FAILURE: false
--+2026-11-24T20:09:04.1509462Z ##[endgroup]
--+2026-11-24T20:09:04.4199487Z [1m[92m    Updating[0m crates.io index
--+2026-11-24T20:09:05.0420797Z [1m[92m Downloading[0m crates ...
--+2026-11-24T20:09:05.0949849Z [1m[92m  Downloaded[0m futures v0.3.34
--+2026-11-24T20:09:05.1051390Z [1m[92m  Downloaded[0m signal-hook-registry v1.4.8
--+2026-11-24T20:09:05.1069684Z [1m[92m  Downloaded[0m thiserror-impl v2.0.21
--+2026-11-24T20:09:05.1086486Z [1m[92m  Downloaded[0m tokio-macros v2.7.2
--+2026-11-24T20:09:05.1101083Z [1m[92m  Downloaded[0m futures-core v0.3.34
--+2026-11-24T20:09:05.1122824Z [1m[92m  Downloaded[0m futures-macro v0.3.34
--+2026-11-24T20:09:05.1136132Z [1m[92m  Downloaded[0m bytes v1.12.1
--+2026-11-24T20:09:05.1186982Z [1m[92m  Downloaded[0m futures-io v0.3.34
--+2026-11-24T20:09:05.1205935Z [1m[92m  Downloaded[0m futures-task v0.3.34
--+2026-11-24T20:09:05.1215990Z [1m[92m  Downloaded[0m tracing-attributes v0.1.31
--+2026-11-24T20:09:05.1253890Z [1m[92m  Downloaded[0m anyhow v1.0.104
--+2026-11-24T20:09:05.1319569Z [1m[92m  Downloaded[0m errno v0.3.15
--+2026-11-24T20:09:05.1338364Z [1m[92m  Downloaded[0m quote v1.0.47
--+2026-11-24T20:09:05.1375541Z [1m[92m  Downloaded[0m async-trait v0.1.92
--+2026-11-24T20:09:05.1411246Z [1m[92m  Downloaded[0m pin-project-lite v0.2.17
--+2026-11-24T20:09:05.1466974Z [1m[92m  Downloaded[0m futures-channel v0.3.34
--+2026-11-24T20:09:05.1485467Z [1m[92m  Downloaded[0m once_cell v1.21.4
--+2026-11-24T20:09:05.1509357Z [1m[92m  Downloaded[0m serde_derive v1.0.229
--+2026-11-24T20:09:05.1538960Z [1m[92m  Downloaded[0m tracing-core v0.1.36
--+2026-11-24T20:09:05.1564687Z [1m[92m  Downloaded[0m serde_core v1.0.229
--+2026-11-24T20:09:05.1591168Z [1m[92m  Downloaded[0m serde v1.0.229
--+2026-11-24T20:09:05.1625060Z [1m[92m  Downloaded[0m proc-macro2 v1.0.107
--+2026-11-24T20:09:05.1660108Z [1m[92m  Downloaded[0m slab v0.4.12
--+2026-11-24T20:09:05.1673370Z [1m[92m  Downloaded[0m syn v3.0.10
--+2026-11-24T20:09:05.1776488Z [1m[92m  Downloaded[0m socket2 v0.6.5
--+2026-11-24T20:09:05.1794782Z [1m[92m  Downloaded[0m futures-executor v0.3.34
--+2026-11-24T20:09:05.1808291Z [1m[92m  Downloaded[0m mio v1.2.5
--+2026-11-24T20:09:05.1869070Z [1m[92m  Downloaded[0m futures-util v0.3.34
--+2026-11-24T20:09:05.1996443Z [1m[92m  Downloaded[0m memchr v2.8.3
--+2026-11-24T20:09:05.2049887Z [1m[92m  Downloaded[0m futures-sink v0.3.34
--+2026-11-24T20:09:05.2058553Z [1m[92m  Downloaded[0m syn v2.0.119
--+2026-11-24T20:09:05.2160068Z [1m[92m  Downloaded[0m ipnet v2.12.2
--+2026-11-24T20:09:05.2176083Z [1m[92m  Downloaded[0m thiserror v2.0.21
--+2026-11-24T20:09:05.2237425Z [1m[92m  Downloaded[0m tracing v0.1.44
--+2026-11-24T20:09:05.2389977Z [1m[92m  Downloaded[0m unicode-ident v1.0.27
--+2026-11-24T20:09:05.2416330Z [1m[92m  Downloaded[0m tokio v1.54.1
--+2026-11-24T20:09:05.2866433Z [1m[92m  Downloaded[0m libc v0.2.191
--+2026-11-24T20:09:05.3585629Z [1m[92m   Compiling[0m proc-macro2 v1.0.107
--+2026-11-24T20:09:05.3586616Z [1m[92m   Compiling[0m unicode-ident v1.0.27
--+2026-11-24T20:09:05.3586988Z [1m[92m   Compiling[0m quote v1.0.47
--+2026-11-24T20:09:05.3587316Z [1m[92m   Compiling[0m libc v0.2.191
--+2026-11-24T20:09:05.6296812Z [1m[92m    Checking[0m pin-project-lite v0.2.17
--+2026-11-24T20:09:05.6714720Z [1m[92m    Checking[0m futures-sink v0.3.34
--+2026-11-24T20:09:05.7130425Z [1m[92m    Checking[0m futures-core v0.3.34
--+2026-11-24T20:09:05.7720540Z [1m[92m    Checking[0m futures-channel v0.3.34
--+2026-11-24T20:09:05.8836341Z [1m[92m    Checking[0m slab v0.4.12
--+2026-11-24T20:09:05.9306922Z [1m[92m    Checking[0m futures-task v0.3.34
--+2026-11-24T20:09:05.9815900Z [1m[92m    Checking[0m futures-io v0.3.34
--+2026-11-24T20:09:06.0275980Z [1m[92m    Checking[0m memchr v2.8.3
--+2026-11-24T20:09:06.0426057Z [1m[92m   Compiling[0m serde_core v1.0.229
--+2026-11-24T20:09:06.2576054Z [1m[92m   Compiling[0m serde v1.0.229
--+2026-11-24T20:09:06.4293048Z [1m[92m    Checking[0m once_cell v1.21.4
--+2026-11-24T20:09:06.4446180Z [1m[92m   Compiling[0m thiserror v2.0.21
--+2026-11-24T20:09:06.6628915Z [1m[92m   Compiling[0m anyhow v1.0.104
--+2026-11-24T20:09:06.7226178Z [1m[92m   Compiling[0m syn v3.0.10
--+2026-11-24T20:09:06.7539455Z [1m[92m   Compiling[0m syn v2.0.119
--+2026-11-24T20:09:06.9175929Z [1m[92m    Checking[0m tracing-core v0.1.36
--+2026-11-24T20:09:07.0576607Z [1m[92m    Checking[0m errno v0.3.15
--+2026-11-24T20:09:07.1506316Z [1m[92m    Checking[0m signal-hook-registry v1.4.8
--+2026-11-24T20:09:07.2215903Z [1m[92m    Checking[0m mio v1.2.5
--+2026-11-24T20:09:07.2580545Z [1m[92m    Checking[0m socket2 v0.6.5
--+2026-11-24T20:09:07.5315982Z [1m[92m    Checking[0m bytes v1.12.1
--+2026-11-24T20:09:08.1125932Z [1m[92m    Checking[0m ipnet v2.12.2
--+2026-11-24T20:09:10.0375344Z [1m[92m   Compiling[0m futures-macro v0.3.34
--+2026-11-24T20:09:10.0475977Z [1m[92m   Compiling[0m serde_derive v1.0.229
--+2026-11-24T20:09:10.1578331Z [1m[92m   Compiling[0m thiserror-impl v2.0.21
--+2026-11-24T20:09:10.4068942Z [1m[92m   Compiling[0m tracing-attributes v0.1.31
--+2026-11-24T20:09:10.6985937Z [1m[92m    Checking[0m futures-util v0.3.34
--+2026-11-24T20:09:11.2909737Z [1m[92m   Compiling[0m tokio-macros v2.7.2
--+2026-11-24T20:09:11.4496058Z [1m[92m    Checking[0m tracing v0.1.44
--+2026-11-24T20:09:11.6477982Z [1m[92m   Compiling[0m async-trait v0.1.92
--+2026-11-24T20:09:11.8751186Z [1m[92m    Checking[0m tokio v1.54.1
--+2026-11-24T20:09:13.1396206Z [1m[92m    Checking[0m futures-executor v0.3.34
--+2026-11-24T20:09:13.2229599Z [1m[92m    Checking[0m futures v0.3.34
--+2026-11-24T20:09:14.7440419Z [1m[92m    Checking[0m mt-net v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net)
--+2026-11-24T20:09:15.8142721Z [1m[92m    Checking[0m mt-bond v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond)
--+2026-11-24T20:09:16.5977534Z [1m[92m    Finished[0m `dev` profile [unoptimized + debuginfo] target(s) in 12.39s
--+2026-11-24T20:09:16.6833256Z ##[group]Run cargo test -p mt-bond
--+2026-11-24T20:09:16.6833618Z [36;1mcargo test -p mt-bond[0m
--+2026-11-24T20:09:16.6865979Z shell: /usr/bin/bash -e {0}
--+2026-11-24T20:09:16.6866246Z env:
--+2026-11-24T20:09:16.6866460Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:09:16.6866746Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:09:16.6866989Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:09:16.6867242Z   CACHE_ON_FAILURE: false
--+2026-11-24T20:09:16.6867477Z ##[endgroup]
--+2026-11-24T20:09:16.8166140Z [1m[92m   Compiling[0m pin-project-lite v0.2.17
--+2026-11-24T20:09:16.8167016Z [1m[92m   Compiling[0m futures-sink v0.3.34
--+2026-11-24T20:09:16.8167554Z [1m[92m   Compiling[0m libc v0.2.191
--+2026-11-24T20:09:16.8168064Z [1m[92m   Compiling[0m futures-core v0.3.34
--+2026-11-24T20:09:16.8616860Z [1m[92m   Compiling[0m memchr v2.8.3
--+2026-11-24T20:09:16.8730412Z [1m[92m   Compiling[0m futures-channel v0.3.34
--+2026-11-24T20:09:16.9035996Z [1m[92m   Compiling[0m slab v0.4.12
--+2026-11-24T20:09:17.0352571Z [1m[92m   Compiling[0m futures-task v0.3.34
--+2026-11-24T20:09:17.0515601Z [1m[92m   Compiling[0m futures-io v0.3.34
--+2026-11-24T20:09:17.1486377Z [1m[92m   Compiling[0m once_cell v1.21.4
--+2026-11-24T20:09:17.1736314Z [1m[92m   Compiling[0m serde_core v1.0.229
--+2026-11-24T20:09:17.3121390Z [1m[92m   Compiling[0m tracing-core v0.1.36
--+2026-11-24T20:09:17.5846701Z [1m[92m   Compiling[0m futures-util v0.3.34
--+2026-11-24T20:09:17.9552679Z [1m[92m   Compiling[0m bytes v1.12.1
--+2026-11-24T20:09:18.0576488Z [1m[92m   Compiling[0m errno v0.3.15
--+2026-11-24T20:09:18.1466415Z [1m[92m   Compiling[0m signal-hook-registry v1.4.8
--+2026-11-24T20:09:18.4015859Z [1m[92m   Compiling[0m socket2 v0.6.5
--+2026-11-24T20:09:18.6357696Z [1m[92m   Compiling[0m mio v1.2.5
--+2026-11-24T20:09:18.9355795Z [1m[92m   Compiling[0m thiserror v2.0.21
--+2026-11-24T20:09:18.9895886Z [1m[92m   Compiling[0m tokio v1.54.1
--+2026-11-24T20:09:19.1736144Z [1m[92m   Compiling[0m tracing v0.1.44
--+2026-11-24T20:09:19.3466061Z [1m[92m   Compiling[0m anyhow v1.0.104
--+2026-11-24T20:09:19.6516463Z [1m[92m   Compiling[0m serde v1.0.229
--+2026-11-24T20:09:19.6785815Z [1m[92m   Compiling[0m ipnet v2.12.2
--+2026-11-24T20:09:20.3058768Z [1m[92m   Compiling[0m futures-executor v0.3.34
--+2026-11-24T20:09:20.3939955Z [1m[92m   Compiling[0m futures v0.3.34
--+2026-11-24T20:09:22.3696055Z [1m[92m   Compiling[0m mt-net v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net)
--+2026-11-24T20:09:23.2475437Z [1m[92m   Compiling[0m mt-bond v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond)
--+2026-11-24T20:09:26.1377441Z [1m[92m    Finished[0m `test` profile [unoptimized + debuginfo] target(s) in 9.43s
--+2026-11-24T20:09:26.1451912Z [1m[92m     Running[0m unittests src/lib.rs (target/debug/build/mt-bond/a79068680de7bb57/out/mt_bond-a79068680de7bb57)
--+2026-11-24T20:09:26.1465366Z 
--+2026-11-24T20:09:26.1465575Z running 33 tests
--+2026-11-24T20:09:26.1476825Z test endpoint::scheduling_tests::balanced_blest_guard_never_bypasses_the_score_leader ... ok
--+2026-11-24T20:09:26.1478660Z test endpoint::scheduling_tests::balanced_capacity_aware_bypasses_the_saturated_adapter ... ok
--+2026-11-24T20:09:26.1480483Z test endpoint::scheduling_tests::balanced_blest_guard_skips_slow_draining_tied_carrier ... ok
--+2026-11-24T20:09:26.1482137Z test endpoint::scheduling_tests::balanced_picks_the_least_backlogged_carrier ... ok
--+2026-11-24T20:09:26.1483733Z test endpoint::scheduling_tests::balanced_spreads_proportionally_to_reported_capacity ... ok
--+2026-11-24T20:09:26.1485006Z test endpoint::scheduling_tests::balanced_ties_rotate ... ok
--+2026-11-24T20:09:26.1486106Z test endpoint::scheduling_tests::carrier_score_is_relative_load_with_probe_floor ... ok
--+2026-11-24T20:09:26.1486998Z test endpoint::scheduling_tests::ecf_completion_is_rtt_plus_drain_time ... ok
--+2026-11-24T20:09:26.1487683Z test endpoint::scheduling_tests::failover_takes_the_first_live_carrier ... ok
--+2026-11-24T20:09:26.1488701Z test endpoint::scheduling_tests::lowest_latency_ecf_prefers_earliest_completion ... ok
--+2026-11-24T20:09:26.1489474Z test endpoint::scheduling_tests::lowest_latency_prefers_sampled_rtt_then_known_over_unknown ... ok
--+2026-11-24T20:09:26.1490191Z test endpoint::scheduling_tests::mixed_reports_probe_the_unreported_carrier ... ok
--+2026-11-24T20:09:26.1490872Z test endpoint::scheduling_tests::priority_fills_in_order_until_the_queue_backs_up ... ok
--+2026-11-24T20:09:26.1491575Z test endpoint::scheduling_tests::priority_spills_when_relative_load_exceeds_ratio ... ok
--+2026-11-24T20:09:26.1492216Z test endpoint::scheduling_tests::round_robin_cycles_the_cursor ... ok
--+2026-11-24T20:09:26.1492774Z test frame::tests::encode_session_borrowed_is_wire_identical ... ok
--+2026-11-24T20:09:26.1493334Z test frame::tests::header_rejects_bad_magic_version_kind_and_size ... ok
--+2026-11-24T20:09:26.1493880Z test frame::tests::empty_and_max_payloads_roundtrip ... ok
--+2026-11-24T20:09:26.1494393Z test frame::tests::roundtrip_all_kinds ... ok
--+2026-11-24T20:09:26.1495316Z test frame::tests::target_rejects_truncated_and_garbage ... ok
--+2026-11-24T20:09:26.1495997Z test frame::tests::read_write_frame_over_duplex ... ok
--+2026-11-24T20:09:26.1496446Z test frame::tests::target_roundtrip_domain_ipv4_ipv6 ... ok
--+2026-11-24T20:09:26.1496831Z test reorder::tests::cap_zero_still_delivers_in_order ... ok
--+2026-11-24T20:09:26.1497259Z test outbound::tests::ensure_carriers_fails_cleanly_with_no_links ... ok
--+2026-11-24T20:09:26.1497717Z test reorder::tests::duplicates_and_replays_are_idempotent ... ok
--+2026-11-24T20:09:26.1498145Z test outbound::tests::ensure_carriers_reports_all_dead_links ... ok
--+2026-11-24T20:09:26.1498540Z test reorder::tests::gap_buffers_then_flushes ... ok
--+2026-11-24T20:09:26.1498896Z test reorder::tests::in_order_delivery ... ok
--+2026-11-24T20:09:26.1499265Z test reorder::tests::long_out_of_order_run_reorders_completely ... ok
--+2026-11-24T20:09:26.1499879Z test reorder::tests::overflow_is_an_error ... ok
--+2026-11-24T20:09:26.1500260Z test reorder::tests::would_overflow_mirrors_the_insert_boundary ... ok
--+2026-11-24T20:09:26.1500646Z test tests::constants_are_coherent ... ok
--+2026-11-24T20:09:26.3491361Z test daemon::tests::serve_binds_and_shuts_down ... ok
--+2026-11-24T20:09:26.3491883Z 
--+2026-11-24T20:09:26.3492405Z test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
--+2026-11-24T20:09:26.3493160Z 
--+2026-11-24T20:09:26.3498383Z [1m[92m     Running[0m unittests src/bin/mt_merge_server.rs (target/debug/build/mt-bond/9915eef574e68264/out/mt_merge_server-9915eef574e68264)
--+2026-11-24T20:09:26.3511273Z 
--+2026-11-24T20:09:26.3511508Z running 0 tests
--+2026-11-24T20:09:26.3511853Z 
--+2026-11-24T20:09:26.3512506Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
--+2026-11-24T20:09:26.3513211Z 
--+2026-11-24T20:09:26.3515021Z [1m[92m     Running[0m tests/integration.rs (target/debug/build/mt-bond/3d8a2918f951a063/out/integration-3d8a2918f951a063)
--+2026-11-24T20:09:26.3526299Z 
--+2026-11-24T20:09:26.3526541Z running 6 tests
--+2026-11-24T20:09:27.4199444Z test echo_roundtrip_1mib_across_two_carriers ... ok
--+2026-11-24T20:09:29.4418224Z test domain_targets_dial_through_the_daemon ... ok
--+2026-11-24T20:09:29.4817493Z test concurrent_sessions_multiplex_over_the_same_carriers ... ok
--+2026-11-24T20:09:29.5169266Z test refused_target_surfaces_as_connection_reset ... ok
--+2026-11-24T20:09:30.5467404Z test one_dead_link_still_yields_a_working_bond ... ok
--+2026-11-24T20:09:31.6307616Z test carrier_death_mid_stream_fails_over_without_data_loss ... FAILED
--+2026-11-24T20:09:31.6308662Z 
--+2026-11-24T20:09:31.6308957Z failures:
--+2026-11-24T20:09:31.6309288Z 
--+2026-11-24T20:09:31.6309847Z ---- carrier_death_mid_stream_fails_over_without_data_loss stdout ----
--+2026-11-24T20:09:31.6310998Z [mt-merge-server] listening on 127.0.0.1:45149
--+2026-11-24T20:09:31.6311626Z 
--+2026-11-24T20:09:31.6312740Z thread 'carrier_death_mid_stream_fails_over_without_data_loss' (3542) panicked at crates/mt-bond/tests/integration.rs:304:14:
--+2026-11-24T20:09:31.6315688Z second half: Custom { kind: ConnectionReset, error: "bond send failed: no live bond carrier" }
--+2026-11-24T20:09:31.6317404Z note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
--+2026-11-24T20:09:31.6318316Z 
--+2026-11-24T20:09:31.6318326Z 
--+2026-11-24T20:09:31.6318579Z failures:
--+2026-11-24T20:09:31.6319273Z     carrier_death_mid_stream_fails_over_without_data_loss
--+2026-11-24T20:09:31.6319839Z 
--+2026-11-24T20:09:31.6320349Z test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.28s
--+2026-11-24T20:09:31.6321078Z 
--+2026-11-24T20:09:31.6324651Z [1m[91merror[0m: test failed, to rerun pass `-p mt-bond --test integration`
--+2026-11-24T20:09:31.6401326Z ##[error]Process completed with exit code 101.
--+2026-11-24T20:09:31.6571586Z Post job cleanup.
--+2026-11-24T20:09:31.7450136Z [command]/usr/bin/git version
--+2026-11-24T20:09:31.7512057Z git version 2.56.1
--+2026-11-24T20:09:31.7554957Z Temporarily overriding HOME='/home/runner/work/_temp/d3b4da46-a437-4e58-ab1a-ca98ecca3f9e' before making global git config changes
--+2026-11-24T20:09:31.7556453Z Adding repository directory to the temporary git global config as a safe directory
--+2026-11-24T20:09:31.7562113Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/MultiTunnel/MultiTunnel
--+2026-11-24T20:09:31.7605965Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
--+2026-11-24T20:09:31.7646163Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
--+2026-11-24T20:09:31.7965867Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
--+2026-11-24T20:09:31.7994975Z http.https://github.com/.extraheader
--+2026-11-24T20:09:31.8007863Z [command]/usr/bin/git config --local --unset-all http.https://github.com/.extraheader
--+2026-11-24T20:09:31.8057730Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
--+2026-11-24T20:09:31.8386796Z [command]/usr/bin/git config --local --name-only --get-regexp ^includeIf\.gitdir:
--+2026-11-24T20:09:31.8435711Z [command]/usr/bin/git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url
--+2026-11-24T20:09:31.8947033Z Cleaning up orphan processes
--+2026-11-24T20:09:31.9349207Z ##[warning]Node.js 20 is deprecated. The following actions target Node.js 20 but are being forced to run on Node.js 24: actions/checkout@v4. For more information see: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/
--diff --git a/harness/memory/agent-reports/s21-ci-domain-targets-release-run2.log b/harness/memory/agent-reports/s21-ci-domain-targets-release-run2.log
--new file mode 100644
--index 0000000..579fcaf
----- /dev/null
--+++ b/harness/memory/agent-reports/s21-ci-domain-targets-release-run2.log
--@@ -0,0 +1,591 @@
--+﻿2026-11-24T20:27:03.9790698Z Current runner version: '2.338.2'
--+2026-11-24T20:27:03.9812772Z ##[group]Runner Image Provisioner
--+2026-11-24T20:27:03.9813661Z Hosted Compute Agent
--+2026-11-24T20:27:03.9814101Z Version: 20261111.635
--+2026-11-24T20:27:03.9814631Z Commit: b8a594ea514ff44cadb5342986c68b1eac6e22ed
--+2026-11-24T20:27:03.9815216Z Build Date: 2026-11-11T01:49:01Z
--+2026-11-24T20:27:03.9815719Z Worker ID: {12b39b16-eba8-4485-ad1f-90f29aae1e1f}
--+2026-11-24T20:27:03.9816253Z Region: centralus
--+2026-11-24T20:27:03.9816666Z Cloud: Azure
--+2026-11-24T20:27:03.9817067Z ##[endgroup]
--+2026-11-24T20:27:03.9818161Z ##[group]Operating System
--+2026-11-24T20:27:03.9818615Z Microsoft Windows Server 2025
--+2026-11-24T20:27:03.9819094Z 10.0.26100
--+2026-11-24T20:27:03.9819484Z Datacenter
--+2026-11-24T20:27:03.9819868Z ##[endgroup]
--+2026-11-24T20:27:03.9820310Z ##[group]Runner Image
--+2026-11-24T20:27:03.9820733Z Image: windows-2025-vs2026
--+2026-11-24T20:27:03.9821198Z Version: 20261116.315.1
--+2026-11-24T20:27:03.9822346Z Included Software: https://github.com/actions/runner-images/blob/win25-vs2026/20261116.315/images/windows/Windows2025-VS2026-Readme.md
--+2026-11-24T20:27:03.9823698Z Image Release: https://github.com/actions/runner-images/releases/tag/win25-vs2026%2F20261116.315
--+2026-11-24T20:27:03.9824437Z ##[endgroup]
--+2026-11-24T20:27:03.9825301Z ##[group]GITHUB_TOKEN Permissions
--+2026-11-24T20:27:03.9826790Z Contents: write
--+2026-11-24T20:27:03.9827262Z Metadata: read
--+2026-11-24T20:27:03.9827668Z ##[endgroup]
--+2026-11-24T20:27:03.9829799Z Secret source: Actions
--+2026-11-24T20:27:03.9830511Z Cache mode: write
--+2026-11-24T20:27:03.9831054Z Prepare workflow directory
--+2026-11-24T20:27:04.0176609Z Prepare all required actions
--+2026-11-24T20:27:04.0216180Z Getting action download info
--+2026-11-24T20:27:04.2490281Z Download action repository 'actions/checkout@v4' (SHA:11d5960a326750d5838078e36cf38b85af677262)
--+2026-11-24T20:27:04.4910712Z Download action repository 'dtolnay/rust-toolchain@stable' (SHA:39008827e6b4319a349e9a42dfc90579dd25f1ca)
--+2026-11-24T20:27:04.6743741Z Download action repository 'Swatinem/rust-cache@v2' (SHA:d1527c8f67ef9af357290475d4af4b0fab8f12e0)
--+2026-11-24T20:27:05.1650630Z Download action repository 'actions/upload-artifact@v4' (SHA:ea165f8d65b6e75b540449e92b4886f43607fa02)
--+2026-11-24T20:27:05.3360084Z Download action repository 'softprops/action-gh-release@v2' (SHA:3bb12739c298aeb8a4eeaf626c5b8d85266b0e65)
--+2026-11-24T20:27:05.7911518Z Complete job name: build-windows
--+2026-11-24T20:27:05.9187811Z ##[group]Run actions/checkout@v4
--+2026-11-24T20:27:05.9188592Z with:
--+2026-11-24T20:27:05.9188899Z   repository: maniesh-NR/MultiTunnel
--+2026-11-24T20:27:05.9191377Z   token: ***
--+2026-11-24T20:27:05.9191647Z   ssh-strict: true
--+2026-11-24T20:27:05.9191925Z   ssh-user: git
--+2026-11-24T20:27:05.9192213Z   persist-credentials: true
--+2026-11-24T20:27:05.9192519Z   clean: true
--+2026-11-24T20:27:05.9192792Z   sparse-checkout-cone-mode: true
--+2026-11-24T20:27:05.9193103Z   fetch-depth: 1
--+2026-11-24T20:27:05.9193372Z   fetch-tags: false
--+2026-11-24T20:27:05.9193672Z   show-progress: true
--+2026-11-24T20:27:05.9194264Z   lfs: false
--+2026-11-24T20:27:05.9194529Z   submodules: false
--+2026-11-24T20:27:05.9194796Z   set-safe-directory: true
--+2026-11-24T20:27:05.9195091Z   allow-unsafe-pr-checkout: false
--+2026-11-24T20:27:05.9195626Z ##[endgroup]
--+2026-11-24T20:27:06.0381862Z Syncing repository: maniesh-NR/MultiTunnel
--+2026-11-24T20:27:06.0383813Z ##[group]Getting Git version info
--+2026-11-24T20:27:06.0384514Z Working directory is 'D:\a\MultiTunnel\MultiTunnel'
--+2026-11-24T20:27:06.1002394Z [command]"C:\Program Files\Git\bin\git.exe" version
--+2026-11-24T20:27:06.4246221Z git version 2.56.1.windows.1
--+2026-11-24T20:27:06.4288495Z ##[endgroup]
--+2026-11-24T20:27:06.4304931Z Temporarily overriding HOME='D:\a\_temp\24fc1558-ebb6-443f-a71c-62afb36e780e' before making global git config changes
--+2026-11-24T20:27:06.4305750Z Adding repository directory to the temporary git global config as a safe directory
--+2026-11-24T20:27:06.4313445Z [command]"C:\Program Files\Git\bin\git.exe" config --global --add safe.directory D:\a\MultiTunnel\MultiTunnel
--+2026-11-24T20:27:06.4892256Z Deleting the contents of 'D:\a\MultiTunnel\MultiTunnel'
--+2026-11-24T20:27:06.4897344Z ##[group]Initializing the repository
--+2026-11-24T20:27:06.4904340Z [command]"C:\Program Files\Git\bin\git.exe" init D:\a\MultiTunnel\MultiTunnel
--+2026-11-24T20:27:06.5658949Z Initialized empty Git repository in D:/a/MultiTunnel/MultiTunnel/.git/
--+2026-11-24T20:27:06.5699847Z [command]"C:\Program Files\Git\bin\git.exe" remote add origin https://github.com/maniesh-NR/MultiTunnel
--+2026-11-24T20:27:06.6939564Z ##[endgroup]
--+2026-11-24T20:27:06.6940074Z ##[group]Disabling automatic garbage collection
--+2026-11-24T20:27:06.6948134Z [command]"C:\Program Files\Git\bin\git.exe" config --local gc.auto 0
--+2026-11-24T20:27:06.7263166Z ##[endgroup]
--+2026-11-24T20:27:06.7263664Z ##[group]Setting up auth
--+2026-11-24T20:27:06.7273773Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp core\.sshCommand
--+2026-11-24T20:27:06.7588759Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "sh -c \"git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :\""
--+2026-11-24T20:27:08.0124739Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
--+2026-11-24T20:27:08.0421099Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "sh -c \"git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :\""
--+2026-11-24T20:27:08.4870329Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp ^includeIf\.gitdir:
--+2026-11-24T20:27:08.5176924Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "git config --local --show-origin --name-only --get-regexp remote.origin.url"
--+2026-11-24T20:27:08.9819633Z [command]"C:\Program Files\Git\bin\git.exe" config --local http.https://github.com/.extraheader "AUTHORIZATION: basic ***"
--+2026-11-24T20:27:09.0134250Z ##[endgroup]
--+2026-11-24T20:27:09.0144912Z ##[group]Fetching the repository
--+2026-11-24T20:27:09.0148856Z [command]"C:\Program Files\Git\bin\git.exe" -c protocol.version=2 fetch --no-tags --prune --no-recurse-submodules --depth=1 origin +e510e439249fe4072477c05040bd5b9d0678db6a:refs/tags/v0.14.0
--+2026-11-24T20:27:11.2409285Z From https://github.com/maniesh-NR/MultiTunnel
--+2026-11-24T20:27:11.2442167Z  * [new ref]         e510e439249fe4072477c05040bd5b9d0678db6a -> v0.14.0
--+2026-11-24T20:27:11.2443768Z ##[endgroup]
--+2026-11-24T20:27:11.2444209Z ##[group]Determining the checkout info
--+2026-11-24T20:27:11.2445819Z ##[endgroup]
--+2026-11-24T20:27:11.2454617Z [command]"C:\Program Files\Git\bin\git.exe" sparse-checkout disable
--+2026-11-24T20:27:11.2880326Z [command]"C:\Program Files\Git\bin\git.exe" config --local --unset-all extensions.worktreeConfig
--+2026-11-24T20:27:11.3189140Z ##[group]Checking out the ref
--+2026-11-24T20:27:11.3196664Z [command]"C:\Program Files\Git\bin\git.exe" checkout --progress --force refs/tags/v0.14.0
--+2026-11-24T20:27:11.4353574Z Note: switching to 'refs/tags/v0.14.0'.
--+2026-11-24T20:27:11.4353996Z 
--+2026-11-24T20:27:11.4354210Z You are in 'detached HEAD' state. You can look around, make experimental
--+2026-11-24T20:27:11.4354838Z changes and commit them, and you can discard any commits you make in this
--+2026-11-24T20:27:11.4355386Z state without impacting any branches by switching back to a branch.
--+2026-11-24T20:27:11.4355706Z 
--+2026-11-24T20:27:11.4355918Z If you want to create a new branch to retain commits you create, you may
--+2026-11-24T20:27:11.4356437Z do so (now or later) by using -c with the switch command. Example:
--+2026-11-24T20:27:11.4356747Z 
--+2026-11-24T20:27:11.4356882Z   git switch -c <new-branch-name>
--+2026-11-24T20:27:11.4357115Z 
--+2026-11-24T20:27:11.4357221Z Or undo this operation with:
--+2026-11-24T20:27:11.4357451Z 
--+2026-11-24T20:27:11.4358570Z   git switch -
--+2026-11-24T20:27:11.4358761Z 
--+2026-11-24T20:27:11.4359094Z Turn off this advice by setting config variable advice.detachedHead to false
--+2026-11-24T20:27:11.4359498Z 
--+2026-11-24T20:27:11.4359731Z HEAD is now at e510e43 backup(s21-release-prep-2): 2026-11-24T20:08:15Z
--+2026-11-24T20:27:11.4413922Z ##[endgroup]
--+2026-11-24T20:27:11.4783798Z [command]"C:\Program Files\Git\bin\git.exe" log -1 --format=%H
--+2026-11-24T20:27:11.5055448Z e510e439249fe4072477c05040bd5b9d0678db6a
--+2026-11-24T20:27:11.5504985Z ##[group]Run dtolnay/rust-toolchain@stable
--+2026-11-24T20:27:11.5505268Z with:
--+2026-11-24T20:27:11.5505435Z   components: rustfmt, clippy
--+2026-11-24T20:27:11.5505621Z   toolchain: stable
--+2026-11-24T20:27:11.5505775Z ##[endgroup]
--+2026-11-24T20:27:11.5558247Z ##[start-action display=Parse toolchain version;id=__dtolnay_rust-toolchain.parse]
--+2026-11-24T20:27:11.5694646Z ##[group]Run if [[ -z $toolchain ]]; then
--+2026-11-24T20:27:11.5694931Z [36;1mif [[ -z $toolchain ]]; then[0m
--+2026-11-24T20:27:11.5695368Z [36;1m  # GitHub does not enforce `required: true` inputs itself. https://github.com/actions/runner/issues/1070[0m
--+2026-11-24T20:27:11.5695802Z [36;1m  echo "'toolchain' is a required input" >&2[0m
--+2026-11-24T20:27:11.5696038Z [36;1m  exit 1[0m
--+2026-11-24T20:27:11.5696288Z [36;1melif [[ $toolchain =~ ^stable' '[0-9]+' '(year|month|week|day)s?' 'ago$ ]]; then[0m
--+2026-11-24T20:27:11.5696591Z [36;1m  if [[ Windows == macOS ]]; then[0m
--+2026-11-24T20:27:11.5697022Z [36;1m    echo "toolchain=1.$((($(date -v-$(sed 's/stable \([0-9]*\) \(.\).*/\1\2/' <<< $toolchain) +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:27:11.5697398Z [36;1m  else[0m
--+2026-11-24T20:27:11.5697690Z [36;1m    echo "toolchain=1.$((($(date --date "${toolchain#stable }" +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:27:11.5698022Z [36;1m  fi[0m
--+2026-11-24T20:27:11.5698236Z [36;1melif [[ $toolchain =~ ^stable' 'minus' '[0-9]+' 'releases?$ ]]; then[0m
--+2026-11-24T20:27:11.5698634Z [36;1m  echo "toolchain=1.$((($(date +%s)/60/60/24-16569)/7/6-${toolchain//[^0-9]/}))" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:27:11.5698973Z [36;1melif [[ $toolchain =~ ^1\.[0-9]+$ ]]; then[0m
--+2026-11-24T20:27:11.5699354Z [36;1m  echo "toolchain=1.$((i=${toolchain#1.}, c=($(date +%s)/60/60/24-16569)/7/6, i+9*i*(10*i<=c)+90*i*(100*i<=c)))" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:27:11.5699705Z [36;1melse[0m
--+2026-11-24T20:27:11.5699854Z [36;1m  delimiter=EOF[0m
--+2026-11-24T20:27:11.5700087Z [36;1m  while [[ $toolchain == *$delimiter* ]]; do delimiter+=_; done[0m
--+2026-11-24T20:27:11.5700465Z [36;1m  printf 'toolchain<<%s\n%s\n%s\n' "$delimiter" "$toolchain" "$delimiter" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:27:11.5700773Z [36;1mfi[0m
--+2026-11-24T20:27:11.5715861Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:27:11.5716173Z env:
--+2026-11-24T20:27:11.5716327Z   toolchain: stable
--+2026-11-24T20:27:11.5716490Z ##[endgroup]
--+2026-11-24T20:27:11.7484880Z ##[end-action id=__dtolnay_rust-toolchain.parse;outcome=success;conclusion=success;duration_ms=192]
--+2026-11-24T20:27:11.7489065Z ##[start-action display=Set $CARGO_HOME;id=__dtolnay_rust-toolchain.__run]
--+2026-11-24T20:27:11.7601834Z ##[group]Run echo CARGO_HOME=${CARGO_HOME:-"$USERPROFILE\.cargo"} >> $GITHUB_ENV
--+2026-11-24T20:27:11.7602287Z [36;1mecho CARGO_HOME=${CARGO_HOME:-"$USERPROFILE\.cargo"} >> $GITHUB_ENV[0m
--+2026-11-24T20:27:11.7613427Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:27:11.7613736Z ##[endgroup]
--+2026-11-24T20:27:11.8053001Z ##[end-action id=__dtolnay_rust-toolchain.__run;outcome=success;conclusion=success;duration_ms=56]
--+2026-11-24T20:27:11.8055028Z ##[start-action display=Install rustup if needed;id=__dtolnay_rust-toolchain.__run_2]
--+2026-11-24T20:27:11.8064233Z ##[end-action id=__dtolnay_rust-toolchain.__run_2;outcome=skipped;conclusion=skipped;duration_ms=0]
--+2026-11-24T20:27:11.8066138Z ##[start-action display=Install rustup if needed on windows;id=__dtolnay_rust-toolchain.__run_3]
--+2026-11-24T20:27:11.8168176Z ##[group]Run if ! command -v rustup &>/dev/null; then
--+2026-11-24T20:27:11.8168455Z [36;1mif ! command -v rustup &>/dev/null; then[0m
--+2026-11-24T20:27:11.8169029Z [36;1m  curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused --location --silent --show-error --fail https://win.rustup.rs/x86_64 --output 'D:\a\_temp\rustup-init.exe'[0m
--+2026-11-24T20:27:11.8171080Z [36;1m  'D:\a\_temp\rustup-init.exe' --default-toolchain none --no-modify-path -y[0m
--+2026-11-24T20:27:11.8171459Z [36;1m  echo "$CARGO_HOME\bin" >> $GITHUB_PATH[0m
--+2026-11-24T20:27:11.8171669Z [36;1mfi[0m
--+2026-11-24T20:27:11.8181957Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:27:11.8182253Z env:
--+2026-11-24T20:27:11.8182407Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:27:11.8182616Z ##[endgroup]
--+2026-11-24T20:27:11.8589763Z ##[end-action id=__dtolnay_rust-toolchain.__run_3;outcome=success;conclusion=success;duration_ms=52]
--+2026-11-24T20:27:11.8603691Z ##[start-action display=rustup toolchain install stable;id=__dtolnay_rust-toolchain.__run_4]
--+2026-11-24T20:27:11.8708203Z ##[group]Run set -f  # split targets and components on commas and whitespace, without glob expansion
--+2026-11-24T20:27:11.8708725Z [36;1mset -f  # split targets and components on commas and whitespace, without glob expansion[0m
--+2026-11-24T20:27:11.8709045Z [36;1mflags=()[0m
--+2026-11-24T20:27:11.8709272Z [36;1mfor t in ${targets//,/ }; do flags+=(--target "$t"); done[0m
--+2026-11-24T20:27:11.8709573Z [36;1mfor c in ${components//,/ }; do flags+=(--component "$c"); done[0m
--+2026-11-24T20:27:11.8709943Z [36;1mif [[ $toolchain == nightly && -n $components ]]; then flags+=(--allow-downgrade); fi[0m
--+2026-11-24T20:27:11.8710248Z [36;1mlog=$(mktemp)[0m
--+2026-11-24T20:27:11.8710427Z [36;1mtrap 'rm -f "$log"' EXIT[0m
--+2026-11-24T20:27:11.8710615Z [36;1mfor attempt in 1 2 3 4 5; do[0m
--+2026-11-24T20:27:11.8711046Z [36;1m  if rustup toolchain install "$toolchain" "${flags[@]}" --profile minimal --no-self-update --force-non-host 2>&1 | tee "$log"; then[0m
--+2026-11-24T20:27:11.8711451Z [36;1m    break[0m
--+2026-11-24T20:27:11.8711593Z [36;1m  else[0m
--+2026-11-24T20:27:11.8711734Z [36;1m    status=$?[0m
--+2026-11-24T20:27:11.8711887Z [36;1m  fi[0m
--+2026-11-24T20:27:11.8712100Z [36;1m  if [[ $attempt == 5 ]] || ! grep -Fq 'checksum failed for ' "$log" \[0m
--+2026-11-24T20:27:11.8712507Z [36;1m      || ! grep -Fq 'this is likely due to an ongoing update of the official release server' "$log"; then[0m
--+2026-11-24T20:27:11.8712861Z [36;1m    exit "$status"[0m
--+2026-11-24T20:27:11.8713047Z [36;1m  fi[0m
--+2026-11-24T20:27:11.8713320Z [36;1m  echo "Retrying rustup toolchain install in 30 seconds (retry $attempt of 4)" >&2[0m
--+2026-11-24T20:27:11.8713646Z [36;1m  sleep 30[0m
--+2026-11-24T20:27:11.8713804Z [36;1mdone[0m
--+2026-11-24T20:27:11.8739839Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:27:11.8740152Z env:
--+2026-11-24T20:27:11.8740306Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:27:11.8740501Z   toolchain: stable
--+2026-11-24T20:27:11.8740660Z   targets: 
--+2026-11-24T20:27:11.8740808Z   components: rustfmt, clippy
--+2026-11-24T20:27:11.8740979Z   RUSTUP_PERMIT_COPY_RENAME: 1
--+2026-11-24T20:27:11.8741144Z ##[endgroup]
--+2026-11-24T20:27:15.3536443Z info: syncing channel updates for stable-x86_64-pc-windows-msvc
--+2026-11-24T20:27:15.9083691Z info: latest update on 2026-11-12 for version 1.100.0 (0ef1bc386 2026-11-09)
--+2026-11-24T20:27:15.9237794Z info: component clippy is up to date
--+2026-11-24T20:27:15.9238549Z info: component rustfmt is up to date
--+2026-11-24T20:27:15.9394695Z 
--+2026-11-24T20:27:15.9555014Z   stable-x86_64-pc-windows-msvc unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
--+2026-11-24T20:27:15.9555307Z 
--+2026-11-24T20:27:15.9818562Z ##[end-action id=__dtolnay_rust-toolchain.__run_4;outcome=success;conclusion=success;duration_ms=4121]
--+2026-11-24T20:27:15.9824832Z ##[start-action display=rustup default stable;id=__dtolnay_rust-toolchain.__run_5]
--+2026-11-24T20:27:15.9920751Z ##[group]Run rustup default "$toolchain" --force-non-host
--+2026-11-24T20:27:15.9921060Z [36;1mrustup default "$toolchain" --force-non-host[0m
--+2026-11-24T20:27:15.9931362Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:27:15.9931657Z env:
--+2026-11-24T20:27:15.9932390Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:27:15.9932602Z   toolchain: stable
--+2026-11-24T20:27:15.9932751Z ##[endgroup]
--+2026-11-24T20:27:16.0489845Z info: using existing install for stable-x86_64-pc-windows-msvc
--+2026-11-24T20:27:16.0555213Z info: default toolchain set to stable-x86_64-pc-windows-msvc
--+2026-11-24T20:27:16.0555480Z 
--+2026-11-24T20:27:16.0740959Z   stable-x86_64-pc-windows-msvc unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
--+2026-11-24T20:27:16.0806652Z 
--+2026-11-24T20:27:16.0812759Z ##[end-action id=__dtolnay_rust-toolchain.__run_5;outcome=success;conclusion=success;duration_ms=98]
--+2026-11-24T20:27:16.0816126Z ##[start-action display=Create cachekey;id=__dtolnay_rust-toolchain.rustc-version]
--+2026-11-24T20:27:16.0922483Z ##[group]Run DATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')
--+2026-11-24T20:27:16.0923281Z [36;1mDATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')[0m
--+2026-11-24T20:27:16.0923777Z [36;1mHASH=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-hash: //p')[0m
--+2026-11-24T20:27:16.0924158Z [36;1mecho "cachekey=$(echo $DATE$HASH | head -c12)" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:27:16.0933561Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:27:16.0933864Z env:
--+2026-11-24T20:27:16.0934021Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:27:16.0934222Z   toolchain: stable
--+2026-11-24T20:27:16.0934387Z ##[endgroup]
--+2026-11-24T20:27:16.8931708Z ##[end-action id=__dtolnay_rust-toolchain.rustc-version;outcome=success;conclusion=success;duration_ms=811]
--+2026-11-24T20:27:16.8933795Z ##[start-action display=Disable incremental compilation;id=__dtolnay_rust-toolchain.__run_6]
--+2026-11-24T20:27:16.9027647Z ##[group]Run if [ -z "${CARGO_INCREMENTAL+set}" ]; then
--+2026-11-24T20:27:16.9027943Z [36;1mif [ -z "${CARGO_INCREMENTAL+set}" ]; then[0m
--+2026-11-24T20:27:16.9028206Z [36;1m  echo CARGO_INCREMENTAL=0 >> $GITHUB_ENV[0m
--+2026-11-24T20:27:16.9028409Z [36;1mfi[0m
--+2026-11-24T20:27:16.9037398Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:27:16.9037690Z env:
--+2026-11-24T20:27:16.9037849Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:27:16.9038050Z ##[endgroup]
--+2026-11-24T20:27:16.9356585Z ##[end-action id=__dtolnay_rust-toolchain.__run_6;outcome=success;conclusion=success;duration_ms=42]
--+2026-11-24T20:27:16.9359237Z ##[start-action display=Enable colors in Cargo output;id=__dtolnay_rust-toolchain.__run_7]
--+2026-11-24T20:27:16.9460010Z ##[group]Run if [ -z "${CARGO_TERM_COLOR+set}" ]; then
--+2026-11-24T20:27:16.9460307Z [36;1mif [ -z "${CARGO_TERM_COLOR+set}" ]; then[0m
--+2026-11-24T20:27:16.9460561Z [36;1m  echo CARGO_TERM_COLOR=always >> $GITHUB_ENV[0m
--+2026-11-24T20:27:16.9460779Z [36;1mfi[0m
--+2026-11-24T20:27:16.9469884Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:27:16.9470193Z env:
--+2026-11-24T20:27:16.9470352Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:27:16.9470564Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:27:16.9470722Z ##[endgroup]
--+2026-11-24T20:27:16.9774396Z ##[end-action id=__dtolnay_rust-toolchain.__run_7;outcome=success;conclusion=success;duration_ms=41]
--+2026-11-24T20:27:16.9777663Z ##[start-action display=Enable Cargo sparse registry;id=__dtolnay_rust-toolchain.__run_8]
--+2026-11-24T20:27:16.9869729Z ##[group]Run # implemented in 1.66, stabilized in 1.68, made default in 1.70
--+2026-11-24T20:27:16.9870687Z [36;1m# implemented in 1.66, stabilized in 1.68, made default in 1.70[0m
--+2026-11-24T20:27:16.9871180Z [36;1mif [ -z "${CARGO_REGISTRIES_CRATES_IO_PROTOCOL+set}" -o -f "D:\a\_temp"/.implicit_cargo_registries_crates_io_protocol ]; then[0m
--+2026-11-24T20:27:16.9871685Z [36;1m  if rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[89]\.'; then[0m
--+2026-11-24T20:27:16.9872184Z [36;1m    touch "D:\a\_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
--+2026-11-24T20:27:16.9872539Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse >> $GITHUB_ENV[0m
--+2026-11-24T20:27:16.9872911Z [36;1m  elif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[67]\.'; then[0m
--+2026-11-24T20:27:16.9873284Z [36;1m    touch "D:\a\_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
--+2026-11-24T20:27:16.9873624Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git >> $GITHUB_ENV[0m
--+2026-11-24T20:27:16.9873879Z [36;1m  fi[0m
--+2026-11-24T20:27:16.9874361Z [36;1mfi[0m
--+2026-11-24T20:27:16.9883549Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:27:16.9883843Z env:
--+2026-11-24T20:27:16.9884001Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:27:16.9884212Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:27:16.9884374Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:27:16.9884535Z   toolchain: stable
--+2026-11-24T20:27:16.9884685Z ##[endgroup]
--+2026-11-24T20:27:17.1800627Z ##[end-action id=__dtolnay_rust-toolchain.__run_8;outcome=success;conclusion=success;duration_ms=202]
--+2026-11-24T20:27:17.1804033Z ##[start-action display=Work around spurious network errors in curl 8.0;id=__dtolnay_rust-toolchain.__run_9]
--+2026-11-24T20:27:17.1910078Z ##[group]Run # https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation
--+2026-11-24T20:27:17.1910627Z [36;1m# https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation[0m
--+2026-11-24T20:27:17.1911114Z [36;1mif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.7[01]\.'; then[0m
--+2026-11-24T20:27:17.1911465Z [36;1m  echo CARGO_HTTP_MULTIPLEXING=false >> $GITHUB_ENV[0m
--+2026-11-24T20:27:17.1911692Z [36;1mfi[0m
--+2026-11-24T20:27:17.1921566Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:27:17.1921870Z env:
--+2026-11-24T20:27:17.1922026Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:27:17.1922237Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:27:17.1922400Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:27:17.1922569Z   toolchain: stable
--+2026-11-24T20:27:17.1922717Z ##[endgroup]
--+2026-11-24T20:27:17.2733961Z ##[end-action id=__dtolnay_rust-toolchain.__run_9;outcome=success;conclusion=success;duration_ms=92]
--+2026-11-24T20:27:17.2737754Z ##[start-action display=rustc +stable --version --verbose;id=__dtolnay_rust-toolchain.__run_10]
--+2026-11-24T20:27:17.2840444Z ##[group]Run rustc +"$toolchain" --version --verbose
--+2026-11-24T20:27:17.2840740Z [36;1mrustc +"$toolchain" --version --verbose[0m
--+2026-11-24T20:27:17.2864872Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:27:17.2865321Z env:
--+2026-11-24T20:27:17.2865533Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:27:17.2865809Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:27:17.2866042Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:27:17.2866349Z   toolchain: stable
--+2026-11-24T20:27:17.2866612Z ##[endgroup]
--+2026-11-24T20:27:17.3542348Z rustc 1.100.0 (0ef1bc386 2026-11-09)
--+2026-11-24T20:27:17.3542742Z binary: rustc
--+2026-11-24T20:27:17.3543036Z commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a
--+2026-11-24T20:27:17.3543397Z commit-date: 2026-11-09
--+2026-11-24T20:27:17.3543660Z host: x86_64-pc-windows-msvc
--+2026-11-24T20:27:17.3543928Z release: 1.100.0
--+2026-11-24T20:27:17.3544170Z LLVM version: 23.1.5
--+2026-11-24T20:27:17.3650648Z ##[end-action id=__dtolnay_rust-toolchain.__run_10;outcome=success;conclusion=success;duration_ms=91]
--+2026-11-24T20:27:17.3827594Z ##[group]Run Swatinem/rust-cache@v2
--+2026-11-24T20:27:17.3827861Z with:
--+2026-11-24T20:27:17.3828011Z   prefix-key: v0-rust
--+2026-11-24T20:27:17.3828184Z   add-job-id-key: true
--+2026-11-24T20:27:17.3828359Z   add-rust-environment-hash-key: true
--+2026-11-24T20:27:17.3828563Z   cache-targets: true
--+2026-11-24T20:27:17.3828725Z   cache-all-crates: false
--+2026-11-24T20:27:17.3828903Z   cache-workspace-crates: false
--+2026-11-24T20:27:17.3829083Z   save-if: true
--+2026-11-24T20:27:17.3829235Z   cache-provider: github
--+2026-11-24T20:27:17.3829396Z   cache-bin: true
--+2026-11-24T20:27:17.3829538Z   lookup-only: false
--+2026-11-24T20:27:17.3829684Z   cmd-format: {0}
--+2026-11-24T20:27:17.3829814Z env:
--+2026-11-24T20:27:17.3829957Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:27:17.3830157Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:27:17.3830313Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:27:17.3830480Z ##[endgroup]
--+2026-11-24T20:27:19.4290079Z ##[group]Cache Configuration
--+2026-11-24T20:27:19.4290901Z Cache Provider:
--+2026-11-24T20:27:19.4291313Z     github
--+2026-11-24T20:27:19.4298199Z Workspaces:
--+2026-11-24T20:27:19.4298635Z     D:\a\MultiTunnel\MultiTunnel
--+2026-11-24T20:27:19.4299065Z Cache Paths:
--+2026-11-24T20:27:19.4299444Z     C:\Users\runneradmin\.cargo\bin
--+2026-11-24T20:27:19.4299981Z     C:\Users\runneradmin\.cargo\.crates.toml
--+2026-11-24T20:27:19.4307915Z     C:\Users\runneradmin\.cargo\.crates2.json
--+2026-11-24T20:27:19.4314335Z     C:\Users\runneradmin\.cargo\registry
--+2026-11-24T20:27:19.4324500Z     C:\Users\runneradmin\.cargo\git
--+2026-11-24T20:27:19.4325343Z     D:\a\MultiTunnel\MultiTunnel\target
--+2026-11-24T20:27:19.4325960Z Restore Key:
--+2026-11-24T20:27:19.4326325Z     v0-rust-build-windows-Windows_NT-x64-ea627442
--+2026-11-24T20:27:19.4326783Z Cache Key:
--+2026-11-24T20:27:19.4327074Z     v0-rust-build-windows-Windows_NT-x64-ea627442-39107a77
--+2026-11-24T20:27:19.4327443Z .. Prefix:
--+2026-11-24T20:27:19.4327688Z   - v0-rust-build-windows-Windows_NT-x64
--+2026-11-24T20:27:19.4328762Z .. Environment considered:
--+2026-11-24T20:27:19.4329040Z   - Rust Versions:
--+2026-11-24T20:27:19.4329400Z     - 1.100.0 x86_64-pc-windows-msvc 0ef1bc386059596dada8fb65b50425aab2ea637a
--+2026-11-24T20:27:19.4329836Z   - CARGO_HOME
--+2026-11-24T20:27:19.4330036Z   - CARGO_INCREMENTAL
--+2026-11-24T20:27:19.4330197Z   - CARGO_TERM_COLOR
--+2026-11-24T20:27:19.4330393Z .. Lockfiles considered:
--+2026-11-24T20:27:19.4330604Z   - D:\a\MultiTunnel\MultiTunnel\Cargo.lock
--+2026-11-24T20:27:19.4330831Z   - D:\a\MultiTunnel\MultiTunnel\Cargo.toml
--+2026-11-24T20:27:19.4331088Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-app\Cargo.toml
--+2026-11-24T20:27:19.4331558Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-bond\Cargo.toml
--+2026-11-24T20:27:19.4331999Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-engine\Cargo.toml
--+2026-11-24T20:27:19.4332360Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-links\Cargo.toml
--+2026-11-24T20:27:19.4332775Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-m3\Cargo.toml
--+2026-11-24T20:27:19.4333212Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-net\Cargo.toml
--+2026-11-24T20:27:19.4333678Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-platform\Cargo.toml
--+2026-11-24T20:27:19.4334144Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-service\Cargo.toml
--+2026-11-24T20:27:19.4334786Z ##[endgroup]
--+2026-11-24T20:27:19.4334922Z 
--+2026-11-24T20:27:19.4335023Z ... Restoring cache ...
--+2026-11-24T20:27:19.6581004Z Cache hit for: v0-rust-build-windows-Windows_NT-x64-ea627442-39107a77
--+2026-11-24T20:27:21.1462642Z Received 134217728 of 598956331 (22.4%), 93.2 MBs/sec
--+2026-11-24T20:27:22.1482439Z Received 268435456 of 598956331 (44.8%), 107.8 MBs/sec
--+2026-11-24T20:27:23.1897918Z Received 402653184 of 598956331 (67.2%), 112.4 MBs/sec
--+2026-11-24T20:27:24.1926266Z Received 536870912 of 598956331 (89.6%), 115.8 MBs/sec
--+2026-11-24T20:27:24.6811307Z Received 598956331 of 598956331 (100.0%), 116.5 MBs/sec
--+2026-11-24T20:27:24.6811725Z Cache Size: ~571 MB (598956331 B)
--+2026-11-24T20:27:24.6814940Z [command]"C:\Program Files\Git\usr\bin\tar.exe" -xf D:/a/_temp/f5c90829-09f3-4bf1-bc31-4605e71c97e5/cache.tzst -P -C D:/a/MultiTunnel/MultiTunnel --force-local --use-compress-program "zstd -d"
--+2026-11-24T20:27:38.6100704Z Cache restored successfully
--+2026-11-24T20:27:38.6394092Z Restored from cache key "v0-rust-build-windows-Windows_NT-x64-ea627442-39107a77" full match: true.
--+2026-11-24T20:27:38.6642626Z ##[group]Run cargo fmt --all --check
--+2026-11-24T20:27:38.6642965Z [36;1mcargo fmt --all --check[0m
--+2026-11-24T20:27:38.7326909Z shell: C:\Program Files\PowerShell\7\pwsh.EXE -command ". '{0}'"
--+2026-11-24T20:27:38.7327205Z env:
--+2026-11-24T20:27:38.7327388Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:27:38.7327613Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:27:38.7327788Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:27:38.7327967Z   CACHE_ON_FAILURE: false
--+2026-11-24T20:27:38.7328146Z ##[endgroup]
--+2026-11-24T20:27:41.3404215Z ##[group]Run cargo clippy --workspace --all-targets -- -D warnings
--+2026-11-24T20:27:41.3404896Z [36;1mcargo clippy --workspace --all-targets -- -D warnings[0m
--+2026-11-24T20:27:41.3480180Z shell: C:\Program Files\PowerShell\7\pwsh.EXE -command ". '{0}'"
--+2026-11-24T20:27:41.3480470Z env:
--+2026-11-24T20:27:41.3480659Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:27:41.3480889Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:27:41.3481077Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:27:41.3481264Z   CACHE_ON_FAILURE: false
--+2026-11-24T20:27:41.3481443Z ##[endgroup]
--+2026-11-24T20:27:42.8608792Z [1m[92m    Updating[0m crates.io index
--+2026-11-24T20:27:58.2375009Z [1m[92m    Checking[0m iced_graphics v0.13.0 (D:\a\MultiTunnel\MultiTunnel\vendor\iced_graphics)
--+2026-11-24T20:27:58.2377458Z [1m[92m    Checking[0m mt-net v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-net)
--+2026-11-24T20:27:58.2413766Z [1m[92m    Checking[0m mt-links v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-links)
--+2026-11-24T20:27:58.2414619Z [1m[92m    Checking[0m mt-service v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-service)
--+2026-11-24T20:28:06.5980044Z [1m[92m   Compiling[0m mt-platform v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-platform)
--+2026-11-24T20:28:06.6264720Z [1m[92m    Checking[0m iced_tiny_skia v0.13.0 (D:\a\MultiTunnel\MultiTunnel\vendor\iced_tiny_skia)
--+2026-11-24T20:28:07.0559391Z [1m[92m    Checking[0m iced_renderer v0.13.0
--+2026-11-24T20:28:07.0900169Z [1m[92m    Checking[0m iced_winit v0.13.0
--+2026-11-24T20:28:07.2132871Z [1m[92m    Checking[0m iced_widget v0.13.4
--+2026-11-24T20:28:07.5124473Z [1m[92m    Checking[0m mt-engine v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-engine)
--+2026-11-24T20:28:07.6436646Z [1m[92m    Checking[0m mt-bond v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-bond)
--+2026-11-24T20:28:08.6358638Z [1m[92m    Checking[0m iced v0.13.1
--+2026-11-24T20:28:08.9119916Z [1m[92m    Checking[0m mt-m3 v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-m3)
--+2026-11-24T20:28:09.2452422Z [1m[92m    Checking[0m mt-app v0.14.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-app)
--+2026-11-24T20:28:11.2203316Z [1m[92m    Checking[0m multitunnel v0.14.0 (D:\a\MultiTunnel\MultiTunnel)
--+2026-11-24T20:28:13.7062184Z [1m[92m    Finished[0m `dev` profile [unoptimized + debuginfo] target(s) in 31.49s
--+2026-11-24T20:28:13.8503479Z ##[group]Run cargo test --workspace
--+2026-11-24T20:28:13.8503820Z [36;1mcargo test --workspace[0m
--+2026-11-24T20:28:13.8556810Z shell: C:\Program Files\PowerShell\7\pwsh.EXE -command ". '{0}'"
--+2026-11-24T20:28:13.8557097Z env:
--+2026-11-24T20:28:13.8557275Z   CARGO_HOME: C:\Users\runneradmin\.cargo
--+2026-11-24T20:28:13.8557505Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:28:13.8557684Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:28:13.8557867Z   CACHE_ON_FAILURE: false
--+2026-11-24T20:28:13.8558050Z ##[endgroup]
--+2026-11-24T20:28:14.4870720Z [1m[92m   Compiling[0m mt-net v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-net)
--+2026-11-24T20:28:14.4929622Z [1m[92m   Compiling[0m iced_graphics v0.13.0 (D:\a\MultiTunnel\MultiTunnel\vendor\iced_graphics)
--+2026-11-24T20:28:14.4977014Z [1m[92m   Compiling[0m mt-links v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-links)
--+2026-11-24T20:28:14.4977972Z [1m[92m   Compiling[0m mt-service v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-service)
--+2026-11-24T20:28:15.7009512Z [1m[92m   Compiling[0m iced_tiny_skia v0.13.0 (D:\a\MultiTunnel\MultiTunnel\vendor\iced_tiny_skia)
--+2026-11-24T20:28:16.7186690Z [1m[92m   Compiling[0m iced_renderer v0.13.0
--+2026-11-24T20:28:16.7819101Z [1m[92m   Compiling[0m iced_winit v0.13.0
--+2026-11-24T20:28:16.9037426Z [1m[92m   Compiling[0m iced_widget v0.13.4
--+2026-11-24T20:28:16.9424326Z [1m[92m   Compiling[0m mt-engine v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-engine)
--+2026-11-24T20:28:17.3547602Z [1m[92m   Compiling[0m mt-bond v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-bond)
--+2026-11-24T20:28:17.4927874Z [1m[92m   Compiling[0m mt-platform v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-platform)
--+2026-11-24T20:28:18.6486951Z [1m[92m   Compiling[0m iced v0.13.1
--+2026-11-24T20:28:18.9856217Z [1m[92m   Compiling[0m mt-m3 v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-m3)
--+2026-11-24T20:28:19.4128979Z [1m[92m   Compiling[0m mt-app v0.14.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-app)
--+2026-11-24T20:28:21.9836900Z [1m[92m   Compiling[0m multitunnel v0.14.0 (D:\a\MultiTunnel\MultiTunnel)
--+2026-11-24T20:28:47.7056064Z [1m[92m    Finished[0m `test` profile [unoptimized + debuginfo] target(s) in 33.61s
--+2026-11-24T20:28:47.7304869Z [1m[92m     Running[0m unittests src\lib.rs (target\debug\build\mt-app/4c747a949941af2f\out\mt_app-4c747a949941af2f.exe)
--+2026-11-24T20:28:47.7422691Z 
--+2026-11-24T20:28:47.7422949Z running 64 tests
--+2026-11-24T20:28:47.7469233Z test app::tests::app_version_is_release_triplet ... ok
--+2026-11-24T20:28:47.7525534Z test app::tests::adapter_rate_sampling_deltas_and_resets ... ok
--+2026-11-24T20:28:47.7528579Z test app::tests::close_action_decision ... ok
--+2026-11-24T20:28:47.7535407Z test app::tests::close_to_tray_settings_roundtrip ... ok
--+2026-11-24T20:28:47.7582713Z test app::tests::begin_exit_is_idempotent_without_tray ... ok
--+2026-11-24T20:28:47.7584250Z test app::tests::always_on_top_menu_item_roundtrips ... ok
--+2026-11-24T20:28:47.7587230Z test app::tests::fmt_helpers ... ok
--+2026-11-24T20:28:47.7589828Z test app::tests::kill_switch_banner_gate ... ok
--+2026-11-24T20:28:47.7592637Z test app::tests::link_matches_adapter_shapes ... ok
--+2026-11-24T20:28:47.7595031Z test app::tests::log_filter_matches ... ok
--+2026-11-24T20:28:47.7649839Z test app::tests::adapters_screen_toggle_move_apply_revert ... ok
--+2026-11-24T20:28:47.7663482Z test app::tests::logs_autoscroll_default_and_toggle ... ok
--+2026-11-24T20:28:47.7666209Z test app::tests::logs_scroll_id_stable ... ok
--+2026-11-24T20:28:47.7668799Z test app::tests::rule_editor_add_edit_remove ... ok
--+2026-11-24T20:28:47.7671289Z test app::tests::edit_dialog_open_type_save_close ... ok
--+2026-11-24T20:28:47.7719549Z test app::tests::edit_save_failure_is_logged ... ok
--+2026-11-24T20:28:47.7722296Z test app::tests::logs_autoscroll_gated_on_screen ... ok
--+2026-11-24T20:28:47.7723257Z test app::tests::seed_parse ... ok
--+2026-11-24T20:28:47.7725640Z test app::tests::smoke_flag_parse ... ok
--+2026-11-24T20:28:47.7794320Z test app::tests::session_totals_accumulate_and_reset ... ok
--+2026-11-24T20:28:47.7798110Z test app::tests::sort_adapter_rows_selected_first_in_saved_order ... ok
--+2026-11-24T20:28:47.7803222Z test app::tests::s21_qos_toggle_updates_draft ... ok
--+2026-11-24T20:28:47.7810735Z test app::tests::s19_settings_toggles_and_modal_gating ... ok
--+2026-11-24T20:28:47.7877916Z test app::tests::title_menu_anchor_clamps ... ok
--+2026-11-24T20:28:47.7879763Z test app::tests::titlebar_double_click_window ... ok
--+2026-11-24T20:28:47.8190012Z test app::tests::theme_reroutes_on_settings_messages ... ok
--+2026-11-24T20:28:47.8383462Z test app::tests::wheel_drag_updates_seed_live_and_persists_on_release ... ok
--+2026-11-24T20:28:47.8385577Z test graph::tests::local_bounds_strips_widget_offset_regression ... ok
--+2026-11-24T20:28:47.8387971Z test graph::tests::nice_ceiling_covers_and_rounds ... ok
--+2026-11-24T20:28:47.8388968Z test graph::tests::points_stay_inside_bounds_and_right_align ... ok
--+2026-11-24T20:28:47.8390877Z test graph::tests::rates_format_human ... ok
--+2026-11-24T20:28:47.8392446Z test mock::tests::fmt_utc_known_values ... ok
--+2026-11-24T20:28:47.8491041Z test app::tests::smoke_run_drives_app_with_mock ... ok
--+2026-11-24T20:28:47.9125940Z test mock::tests::mock_latency_test ... ok
--+2026-11-24T20:28:47.9130914Z test mock::tests::mock_profiles_import_remove ... ok
--+2026-11-24T20:28:47.9135058Z test mock::tests::mock_settings_and_rules ... ok
--+2026-11-24T20:28:47.9185289Z test theme::tests::button_states_use_state_layers ... ok
--+2026-11-24T20:28:47.9187455Z test theme::tests::hsv_conversions_roundtrip_primaries ... ok
--+2026-11-24T20:28:47.9235858Z test theme::tests::switch_styles_differ_by_state ... ok
--+2026-11-24T20:28:47.9239314Z test theme::tests::system_prefers_dark_env_override_wins ... ok
--+2026-11-24T20:28:47.9287743Z test theme::tests::text_color_from_m3 ... ok
--+2026-11-24T20:28:47.9474295Z test theme::tests::theme_derivation_seed_changes_scheme ... ok
--+2026-11-24T20:28:47.9570948Z test theme::tests::theme_from_settings_and_rederive ... ok
--+2026-11-24T20:28:47.9572777Z test theme::tests::wheel_asset_dimensions ... ok
--+2026-11-24T20:28:47.9574559Z test theme::tests::wheel_marker_roundtrips_picked_points ... ok
--+2026-11-24T20:28:47.9576103Z test theme::tests::wheel_seed_clamps_outside_points_to_the_rim ... ok
--+2026-11-24T20:28:47.9577842Z test theme::tests::wheel_seed_matches_cardinal_hues ... ok
--+2026-11-24T20:28:47.9579918Z test tray::tests::ico_embedded_asset_parses_with_32_entry ... ok
--+2026-11-24T20:28:47.9581761Z test tray::tests::ico_embedded_mono_asset_parses_with_32_entry ... ok
--+2026-11-24T20:28:47.9583455Z test tray::tests::ico_rejects_bad_directories ... ok
--+2026-11-24T20:28:47.9585485Z test tray::tests::ids_roundtrip_and_skip_zero ... ok
--+2026-11-24T20:28:47.9587248Z test tray::tests::menu_connected_shows_disconnect_and_servers ... ok
--+2026-11-24T20:28:47.9588831Z test tray::tests::menu_disconnected_shows_connect ... ok
--+2026-11-24T20:28:47.9590771Z test tray::tests::menu_failed_is_disconnected_shape_with_honest_header ... ok
--+2026-11-24T20:28:47.9592444Z test tray::tests::menu_transition_disables_the_toggle ... ok
--+2026-11-24T20:28:47.9594219Z test tray::tests::model_from_snapshot_states ... ok
--+2026-11-24T20:28:47.9596393Z test tray::tests::tip_array_never_overruns_or_splits_surrogates ... ok
--+2026-11-24T20:28:47.9598174Z test tray::tests::to_wide_is_nul_terminated ... ok
--+2026-11-24T20:28:47.9600246Z test tray::tests::tooltip_live_formats_speeds_and_totals ... ok
--+2026-11-24T20:28:47.9601929Z test tray::tests::tooltip_states ... ok
--+2026-11-24T20:28:47.9603396Z test tray::tests::tray_icon_follows_connection_state ... ok
--+2026-11-24T20:28:48.6071109Z test app::tests::speed_test_message_flow ... ok
--+2026-11-24T20:28:48.7042689Z test app::tests::update_loop_screens_and_rules ... ok
--+2026-11-24T20:28:48.7488505Z test mock::tests::mock_connect_disconnect_lifecycle ... ok
--+2026-11-24T20:28:48.7488757Z 
--+2026-11-24T20:28:48.7490045Z test result: ok. 64 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.01s
--+2026-11-24T20:28:48.7490335Z 
--+2026-11-24T20:28:48.7514433Z [1m[92m     Running[0m unittests src\main.rs (target\debug\build\mt-app/7c07808781ddcf4a\out\mt_app-7c07808781ddcf4a.exe)
--+2026-11-24T20:28:48.7597526Z 
--+2026-11-24T20:28:48.7598025Z running 0 tests
--+2026-11-24T20:28:48.7598427Z 
--+2026-11-24T20:28:48.7598894Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
--+2026-11-24T20:28:48.7599186Z 
--+2026-11-24T20:28:48.7612126Z [1m[92m     Running[0m unittests src\lib.rs (target\debug\build\mt-bond/6410ae8cc506944c\out\mt_bond-6410ae8cc506944c.exe)
--+2026-11-24T20:28:48.7693636Z 
--+2026-11-24T20:28:48.7693889Z running 33 tests
--+2026-11-24T20:28:48.7702553Z test endpoint::scheduling_tests::balanced_blest_guard_never_bypasses_the_score_leader ... ok
--+2026-11-24T20:28:48.7704685Z test endpoint::scheduling_tests::balanced_blest_guard_skips_slow_draining_tied_carrier ... ok
--+2026-11-24T20:28:48.7706230Z test endpoint::scheduling_tests::balanced_capacity_aware_bypasses_the_saturated_adapter ... ok
--+2026-11-24T20:28:48.7707790Z test endpoint::scheduling_tests::balanced_picks_the_least_backlogged_carrier ... ok
--+2026-11-24T20:28:48.7709175Z test endpoint::scheduling_tests::balanced_ties_rotate ... ok
--+2026-11-24T20:28:48.7709873Z test endpoint::scheduling_tests::balanced_spreads_proportionally_to_reported_capacity ... ok
--+2026-11-24T20:28:48.7710688Z test endpoint::scheduling_tests::carrier_score_is_relative_load_with_probe_floor ... ok
--+2026-11-24T20:28:48.7711868Z test endpoint::scheduling_tests::ecf_completion_is_rtt_plus_drain_time ... ok
--+2026-11-24T20:28:48.7713487Z test endpoint::scheduling_tests::failover_takes_the_first_live_carrier ... ok
--+2026-11-24T20:28:48.7714936Z test endpoint::scheduling_tests::lowest_latency_ecf_prefers_earliest_completion ... ok
--+2026-11-24T20:28:48.7716089Z test endpoint::scheduling_tests::lowest_latency_prefers_sampled_rtt_then_known_over_unknown ... ok
--+2026-11-24T20:28:48.7716905Z test endpoint::scheduling_tests::priority_fills_in_order_until_the_queue_backs_up ... ok
--+2026-11-24T20:28:48.7717342Z test endpoint::scheduling_tests::mixed_reports_probe_the_unreported_carrier ... ok
--+2026-11-24T20:28:48.7720163Z test endpoint::scheduling_tests::priority_spills_when_relative_load_exceeds_ratio ... ok
--+2026-11-24T20:28:48.7721507Z test endpoint::scheduling_tests::round_robin_cycles_the_cursor ... ok
--+2026-11-24T20:28:48.7722915Z test frame::tests::empty_and_max_payloads_roundtrip ... ok
--+2026-11-24T20:28:48.7724555Z test frame::tests::encode_session_borrowed_is_wire_identical ... ok
--+2026-11-24T20:28:48.7725304Z test frame::tests::header_rejects_bad_magic_version_kind_and_size ... ok
--+2026-11-24T20:28:48.7727569Z test frame::tests::target_rejects_truncated_and_garbage ... ok
--+2026-11-24T20:28:48.7728468Z test frame::tests::roundtrip_all_kinds ... ok
--+2026-11-24T20:28:48.7729888Z test frame::tests::read_write_frame_over_duplex ... ok
--+2026-11-24T20:28:48.7731741Z test frame::tests::target_roundtrip_domain_ipv4_ipv6 ... ok
--+2026-11-24T20:28:48.7733892Z test reorder::tests::cap_zero_still_delivers_in_order ... ok
--+2026-11-24T20:28:48.7735473Z test outbound::tests::ensure_carriers_fails_cleanly_with_no_links ... ok
--+2026-11-24T20:28:48.7736825Z test outbound::tests::ensure_carriers_reports_all_dead_links ... ok
--+2026-11-24T20:28:48.7737790Z test reorder::tests::duplicates_and_replays_are_idempotent ... ok
--+2026-11-24T20:28:48.7738442Z test reorder::tests::gap_buffers_then_flushes ... ok
--+2026-11-24T20:28:48.7739131Z test reorder::tests::in_order_delivery ... ok
--+2026-11-24T20:28:48.7740453Z test reorder::tests::long_out_of_order_run_reorders_completely ... ok
--+2026-11-24T20:28:48.7741474Z test reorder::tests::overflow_is_an_error ... ok
--+2026-11-24T20:28:48.7742112Z test reorder::tests::would_overflow_mirrors_the_insert_boundary ... ok
--+2026-11-24T20:28:48.7742757Z test tests::constants_are_coherent ... ok
--+2026-11-24T20:28:48.9861447Z test daemon::tests::serve_binds_and_shuts_down ... ok
--+2026-11-24T20:28:48.9861761Z 
--+2026-11-24T20:28:48.9862615Z test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
--+2026-11-24T20:28:48.9862903Z 
--+2026-11-24T20:28:48.9883143Z [1m[92m     Running[0m unittests src\bin\mt_merge_server.rs (target\debug\build\mt-bond/fbee07725169cdd9\out\mt_merge_server-fbee07725169cdd9.exe)
--+2026-11-24T20:28:48.9964542Z 
--+2026-11-24T20:28:48.9964886Z running 0 tests
--+2026-11-24T20:28:48.9965095Z 
--+2026-11-24T20:28:48.9965742Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
--+2026-11-24T20:28:48.9966196Z 
--+2026-11-24T20:28:48.9977426Z [1m[92m     Running[0m tests\integration.rs (target\debug\build\mt-bond/38eeb9b5e0d206b1\out\integration-38eeb9b5e0d206b1.exe)
--+2026-11-24T20:28:49.0060098Z 
--+2026-11-24T20:28:49.0060574Z running 6 tests
--+2026-11-24T20:28:49.3648070Z test carrier_death_mid_stream_fails_over_without_data_loss ... ok
--+2026-11-24T20:28:50.0910437Z test echo_roundtrip_1mib_across_two_carriers ... ok
--+2026-11-24T20:28:52.1268799Z test concurrent_sessions_multiplex_over_the_same_carriers ... ok
--+2026-11-24T20:28:52.1737792Z test refused_target_surfaces_as_connection_reset ... ok
--+2026-11-24T20:28:52.4742712Z test one_dead_link_still_yields_a_working_bond ... ok
--+2026-11-24T20:29:49.0111481Z test domain_targets_dial_through_the_daemon has been running for over 60 seconds
--+2026-11-24T20:29:49.0113106Z test domain_targets_dial_through_the_daemon ... FAILED
--+2026-11-24T20:29:49.0113628Z 
--+2026-11-24T20:29:49.0113723Z failures:
--+2026-11-24T20:29:49.0113811Z 
--+2026-11-24T20:29:49.0113939Z ---- domain_targets_dial_through_the_daemon stdout ----
--+2026-11-24T20:29:49.0114203Z [mt-merge-server] listening on 127.0.0.1:52542
--+2026-11-24T20:29:49.0114368Z 
--+2026-11-24T20:29:49.0114632Z thread 'domain_targets_dial_through_the_daemon' (6544) panicked at crates\mt-bond\tests\integration.rs:382:6:
--+2026-11-24T20:29:49.0115484Z deadline: domain_targets wedged (documented runner flake — see LEDGER): Elapsed(())
--+2026-11-24T20:29:49.0115906Z note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
--+2026-11-24T20:29:49.0116137Z 
--+2026-11-24T20:29:49.0116140Z 
--+2026-11-24T20:29:49.0116199Z failures:
--+2026-11-24T20:29:49.0116386Z     domain_targets_dial_through_the_daemon
--+2026-11-24T20:29:49.0116551Z 
--+2026-11-24T20:29:49.0116780Z test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 60.01s
--+2026-11-24T20:29:49.0117091Z 
--+2026-11-24T20:29:49.0146157Z [1m[91merror[0m: test failed, to rerun pass `-p mt-bond --test integration`
--+2026-11-24T20:29:49.1071748Z ##[error]Process completed with exit code 1.
--+2026-11-24T20:29:49.1284324Z Post job cleanup.
--+2026-11-24T20:29:49.2584864Z [command]"C:\Program Files\Git\bin\git.exe" version
--+2026-11-24T20:29:49.2874683Z git version 2.56.1.windows.1
--+2026-11-24T20:29:49.2933790Z Temporarily overriding HOME='D:\a\_temp\2d504455-8d0b-4102-8e16-e59c48f8f959' before making global git config changes
--+2026-11-24T20:29:49.2934448Z Adding repository directory to the temporary git global config as a safe directory
--+2026-11-24T20:29:49.2942313Z [command]"C:\Program Files\Git\bin\git.exe" config --global --add safe.directory D:\a\MultiTunnel\MultiTunnel
--+2026-11-24T20:29:49.3324160Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp core\.sshCommand
--+2026-11-24T20:29:49.3661191Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "sh -c \"git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :\""
--+2026-11-24T20:29:49.8475726Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
--+2026-11-24T20:29:49.8752733Z http.https://github.com/.extraheader
--+2026-11-24T20:29:49.8800726Z [command]"C:\Program Files\Git\bin\git.exe" config --local --unset-all http.https://github.com/.extraheader
--+2026-11-24T20:29:49.9109184Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "sh -c \"git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :\""
--+2026-11-24T20:29:50.4182064Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp ^includeIf\.gitdir:
--+2026-11-24T20:29:50.4534671Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "git config --local --show-origin --name-only --get-regexp remote.origin.url"
--+2026-11-24T20:29:50.9242819Z Cleaning up orphan processes
--+2026-11-24T20:29:50.9732521Z Terminate orphan process: pid (7108) (vctip)
--+2026-11-24T20:29:51.0355927Z ##[warning]Node.js 20 is deprecated. The following actions target Node.js 20 but are being forced to run on Node.js 24: actions/checkout@v4. For more information see: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/
--diff --git a/harness/memory/agent-reports/s21-ci-domain-targets-release-run3.log b/harness/memory/agent-reports/s21-ci-domain-targets-release-run3.log
--new file mode 100644
--index 0000000..c743b0f
----- /dev/null
--+++ b/harness/memory/agent-reports/s21-ci-domain-targets-release-run3.log
--@@ -0,0 +1,497 @@
--+﻿2026-11-24T20:36:56.7468464Z Current runner version: '2.338.2'
--+2026-11-24T20:36:56.7492979Z ##[group]Runner Image Provisioner
--+2026-11-24T20:36:56.7493795Z Hosted Compute Agent
--+2026-11-24T20:36:56.7494430Z Version: 20261111.635
--+2026-11-24T20:36:56.7495023Z Commit: b8a594ea514ff44cadb5342986c68b1eac6e22ed
--+2026-11-24T20:36:56.7495709Z Build Date: 2026-11-11T01:49:01Z
--+2026-11-24T20:36:56.7496378Z Worker ID: {8af1622b-9c23-4c29-a178-070895d02a66}
--+2026-11-24T20:36:56.7497063Z Region: westus3
--+2026-11-24T20:36:56.7497598Z Cloud: Azure
--+2026-11-24T20:36:56.7498097Z ##[endgroup]
--+2026-11-24T20:36:56.7499542Z ##[group]Operating System
--+2026-11-24T20:36:56.7500177Z Ubuntu
--+2026-11-24T20:36:56.7500717Z 26.04.1
--+2026-11-24T20:36:56.7501209Z LTS
--+2026-11-24T20:36:56.7501705Z ##[endgroup]
--+2026-11-24T20:36:56.7502231Z ##[group]Runner Image
--+2026-11-24T20:36:56.7502952Z Image: ubuntu-26.04
--+2026-11-24T20:36:56.7503506Z Version: 20261116.202.1
--+2026-11-24T20:36:56.7504661Z Included Software: https://github.com/actions/runner-images/blob/ubuntu26/20261116.202/images/ubuntu/Ubuntu2604-Readme.md
--+2026-11-24T20:36:56.7506075Z Image Release: https://github.com/actions/runner-images/releases/tag/ubuntu26%2F20261116.202
--+2026-11-24T20:36:56.7506969Z ##[endgroup]
--+2026-11-24T20:36:56.7508008Z ##[group]GITHUB_TOKEN Permissions
--+2026-11-24T20:36:56.7510328Z Contents: write
--+2026-11-24T20:36:56.7510904Z Metadata: read
--+2026-11-24T20:36:56.7511502Z ##[endgroup]
--+2026-11-24T20:36:56.7513820Z Secret source: Actions
--+2026-11-24T20:36:56.7514667Z Cache mode: write
--+2026-11-24T20:36:56.7515443Z Prepare workflow directory
--+2026-11-24T20:36:56.7905686Z Prepare all required actions
--+2026-11-24T20:36:56.7954522Z Getting action download info
--+2026-11-24T20:36:57.1280272Z Download action repository 'actions/checkout@v4' (SHA:11d5960a326750d5838078e36cf38b85af677262)
--+2026-11-24T20:36:57.2637874Z Download action repository 'dtolnay/rust-toolchain@stable' (SHA:39008827e6b4319a349e9a42dfc90579dd25f1ca)
--+2026-11-24T20:36:57.4927585Z Download action repository 'Swatinem/rust-cache@v2' (SHA:d1527c8f67ef9af357290475d4af4b0fab8f12e0)
--+2026-11-24T20:36:58.1017173Z Download action repository 'actions/upload-artifact@v4' (SHA:ea165f8d65b6e75b540449e92b4886f43607fa02)
--+2026-11-24T20:36:58.1655955Z Download action repository 'softprops/action-gh-release@v2' (SHA:3bb12739c298aeb8a4eeaf626c5b8d85266b0e65)
--+2026-11-24T20:36:58.8080048Z Complete job name: build-linux-daemon
--+2026-11-24T20:36:58.9025651Z ##[group]Run actions/checkout@v4
--+2026-11-24T20:36:58.9026900Z with:
--+2026-11-24T20:36:58.9027700Z   repository: maniesh-NR/MultiTunnel
--+2026-11-24T20:36:58.9035332Z   token: ***
--+2026-11-24T20:36:58.9036083Z   ssh-strict: true
--+2026-11-24T20:36:58.9036876Z   ssh-user: git
--+2026-11-24T20:36:58.9037674Z   persist-credentials: true
--+2026-11-24T20:36:58.9038534Z   clean: true
--+2026-11-24T20:36:58.9039329Z   sparse-checkout-cone-mode: true
--+2026-11-24T20:36:58.9040259Z   fetch-depth: 1
--+2026-11-24T20:36:58.9041030Z   fetch-tags: false
--+2026-11-24T20:36:58.9041817Z   show-progress: true
--+2026-11-24T20:36:58.9042739Z   lfs: false
--+2026-11-24T20:36:58.9043529Z   submodules: false
--+2026-11-24T20:36:58.9044322Z   set-safe-directory: true
--+2026-11-24T20:36:58.9045223Z   allow-unsafe-pr-checkout: false
--+2026-11-24T20:36:58.9046429Z ##[endgroup]
--+2026-11-24T20:36:59.1725168Z Syncing repository: maniesh-NR/MultiTunnel
--+2026-11-24T20:36:59.1729448Z ##[group]Getting Git version info
--+2026-11-24T20:36:59.1731074Z Working directory is '/home/runner/work/MultiTunnel/MultiTunnel'
--+2026-11-24T20:36:59.1734140Z [command]/usr/bin/git version
--+2026-11-24T20:36:59.1761131Z git version 2.56.1
--+2026-11-24T20:36:59.1789132Z ##[endgroup]
--+2026-11-24T20:36:59.1801509Z Temporarily overriding HOME='/home/runner/work/_temp/a5f6893c-8af6-4f36-8c6d-8cf562b6250a' before making global git config changes
--+2026-11-24T20:36:59.1806263Z Adding repository directory to the temporary git global config as a safe directory
--+2026-11-24T20:36:59.1810131Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/MultiTunnel/MultiTunnel
--+2026-11-24T20:36:59.1880833Z Deleting the contents of '/home/runner/work/MultiTunnel/MultiTunnel'
--+2026-11-24T20:36:59.1888576Z ##[group]Initializing the repository
--+2026-11-24T20:36:59.1893336Z [command]/usr/bin/git init /home/runner/work/MultiTunnel/MultiTunnel
--+2026-11-24T20:36:59.1990347Z hint: Using 'master' as the name for the initial branch. This default branch name
--+2026-11-24T20:36:59.1994769Z hint: will change to "main" in Git 3.0. To configure the initial branch name
--+2026-11-24T20:36:59.1998370Z hint: to use in all of your new repositories, which will suppress this warning,
--+2026-11-24T20:36:59.2001132Z hint: call:
--+2026-11-24T20:36:59.2002978Z hint:
--+2026-11-24T20:36:59.2004836Z hint: 	git config --global init.defaultBranch <name>
--+2026-11-24T20:36:59.2007042Z hint:
--+2026-11-24T20:36:59.2009430Z hint: Names commonly chosen instead of 'master' are 'main', 'trunk' and
--+2026-11-24T20:36:59.2013021Z hint: 'development'. The just-created branch can be renamed via this command:
--+2026-11-24T20:36:59.2015820Z hint:
--+2026-11-24T20:36:59.2017774Z hint: 	git branch -m <name>
--+2026-11-24T20:36:59.2019563Z hint:
--+2026-11-24T20:36:59.2021847Z hint: Disable this message with "git config set advice.defaultBranchName false"
--+2026-11-24T20:36:59.2026091Z Initialized empty Git repository in /home/runner/work/MultiTunnel/MultiTunnel/.git/
--+2026-11-24T20:36:59.2032669Z [command]/usr/bin/git remote add origin https://github.com/maniesh-NR/MultiTunnel
--+2026-11-24T20:36:59.2061599Z ##[endgroup]
--+2026-11-24T20:36:59.2064025Z ##[group]Disabling automatic garbage collection
--+2026-11-24T20:36:59.2066119Z [command]/usr/bin/git config --local gc.auto 0
--+2026-11-24T20:36:59.2098365Z ##[endgroup]
--+2026-11-24T20:36:59.2100852Z ##[group]Setting up auth
--+2026-11-24T20:36:59.2106726Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
--+2026-11-24T20:36:59.2141603Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
--+2026-11-24T20:36:59.2569880Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
--+2026-11-24T20:36:59.2617242Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
--+2026-11-24T20:36:59.2851867Z [command]/usr/bin/git config --local --name-only --get-regexp ^includeIf\.gitdir:
--+2026-11-24T20:36:59.2885867Z [command]/usr/bin/git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url
--+2026-11-24T20:36:59.3109341Z [command]/usr/bin/git config --local http.https://github.com/.extraheader AUTHORIZATION: basic ***
--+2026-11-24T20:36:59.3145197Z ##[endgroup]
--+2026-11-24T20:36:59.3153778Z ##[group]Fetching the repository
--+2026-11-24T20:36:59.3156162Z [command]/usr/bin/git -c protocol.version=2 fetch --no-tags --prune --no-recurse-submodules --depth=1 origin +e510e439249fe4072477c05040bd5b9d0678db6a:refs/tags/v0.14.0
--+2026-11-24T20:37:00.1205240Z From https://github.com/maniesh-NR/MultiTunnel
--+2026-11-24T20:37:00.1205777Z  * [new ref]         e510e439249fe4072477c05040bd5b9d0678db6a -> v0.14.0
--+2026-11-24T20:37:00.1209105Z ##[endgroup]
--+2026-11-24T20:37:00.1209755Z ##[group]Determining the checkout info
--+2026-11-24T20:37:00.1211716Z ##[endgroup]
--+2026-11-24T20:37:00.1217697Z [command]/usr/bin/git sparse-checkout disable
--+2026-11-24T20:37:00.1265386Z [command]/usr/bin/git config --local --unset-all extensions.worktreeConfig
--+2026-11-24T20:37:00.1291344Z ##[group]Checking out the ref
--+2026-11-24T20:37:00.1296969Z [command]/usr/bin/git checkout --progress --force refs/tags/v0.14.0
--+2026-11-24T20:37:00.1614021Z Note: switching to 'refs/tags/v0.14.0'.
--+2026-11-24T20:37:00.1614457Z 
--+2026-11-24T20:37:00.1614846Z You are in 'detached HEAD' state. You can look around, make experimental
--+2026-11-24T20:37:00.1615654Z changes and commit them, and you can discard any commits you make in this
--+2026-11-24T20:37:00.1616130Z state without impacting any branches by switching back to a branch.
--+2026-11-24T20:37:00.1616887Z 
--+2026-11-24T20:37:00.1617209Z If you want to create a new branch to retain commits you create, you may
--+2026-11-24T20:37:00.1617904Z do so (now or later) by using -c with the switch command. Example:
--+2026-11-24T20:37:00.1618327Z 
--+2026-11-24T20:37:00.1618534Z   git switch -c <new-branch-name>
--+2026-11-24T20:37:00.1618831Z 
--+2026-11-24T20:37:00.1618994Z Or undo this operation with:
--+2026-11-24T20:37:00.1619251Z 
--+2026-11-24T20:37:00.1619384Z   git switch -
--+2026-11-24T20:37:00.1619607Z 
--+2026-11-24T20:37:00.1620019Z Turn off this advice by setting config variable advice.detachedHead to false
--+2026-11-24T20:37:00.1620487Z 
--+2026-11-24T20:37:00.1620771Z HEAD is now at e510e43 backup(s21-release-prep-2): 2026-11-24T20:08:15Z
--+2026-11-24T20:37:00.1622574Z ##[endgroup]
--+2026-11-24T20:37:00.1667398Z [command]/usr/bin/git log -1 --format=%H
--+2026-11-24T20:37:00.1687967Z e510e439249fe4072477c05040bd5b9d0678db6a
--+2026-11-24T20:37:00.2067272Z ##[group]Run dtolnay/rust-toolchain@stable
--+2026-11-24T20:37:00.2067607Z with:
--+2026-11-24T20:37:00.2067828Z   components: rustfmt, clippy
--+2026-11-24T20:37:00.2068080Z   toolchain: stable
--+2026-11-24T20:37:00.2068290Z ##[endgroup]
--+2026-11-24T20:37:00.2142933Z ##[start-action display=Parse toolchain version;id=__dtolnay_rust-toolchain.parse]
--+2026-11-24T20:37:00.2201582Z ##[group]Run if [[ -z $toolchain ]]; then
--+2026-11-24T20:37:00.2202059Z [36;1mif [[ -z $toolchain ]]; then[0m
--+2026-11-24T20:37:00.2202927Z [36;1m  # GitHub does not enforce `required: true` inputs itself. https://github.com/actions/runner/issues/1070[0m
--+2026-11-24T20:37:00.2203585Z [36;1m  echo "'toolchain' is a required input" >&2[0m
--+2026-11-24T20:37:00.2203950Z [36;1m  exit 1[0m
--+2026-11-24T20:37:00.2204353Z [36;1melif [[ $toolchain =~ ^stable' '[0-9]+' '(year|month|week|day)s?' 'ago$ ]]; then[0m
--+2026-11-24T20:37:00.2204859Z [36;1m  if [[ Linux == macOS ]]; then[0m
--+2026-11-24T20:37:00.2205551Z [36;1m    echo "toolchain=1.$((($(date -v-$(sed 's/stable \([0-9]*\) \(.\).*/\1\2/' <<< $toolchain) +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:37:00.2206178Z [36;1m  else[0m
--+2026-11-24T20:37:00.2206660Z [36;1m    echo "toolchain=1.$((($(date --date "${toolchain#stable }" +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:37:00.2207197Z [36;1m  fi[0m
--+2026-11-24T20:37:00.2207501Z [36;1melif [[ $toolchain =~ ^stable' 'minus' '[0-9]+' 'releases?$ ]]; then[0m
--+2026-11-24T20:37:00.2208031Z [36;1m  echo "toolchain=1.$((($(date +%s)/60/60/24-16569)/7/6-${toolchain//[^0-9]/}))" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:37:00.2208494Z [36;1melif [[ $toolchain =~ ^1\.[0-9]+$ ]]; then[0m
--+2026-11-24T20:37:00.2209016Z [36;1m  echo "toolchain=1.$((i=${toolchain#1.}, c=($(date +%s)/60/60/24-16569)/7/6, i+9*i*(10*i<=c)+90*i*(100*i<=c)))" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:37:00.2209504Z [36;1melse[0m
--+2026-11-24T20:37:00.2209751Z [36;1m  delimiter=EOF[0m
--+2026-11-24T20:37:00.2210070Z [36;1m  while [[ $toolchain == *$delimiter* ]]; do delimiter+=_; done[0m
--+2026-11-24T20:37:00.2210575Z [36;1m  printf 'toolchain<<%s\n%s\n%s\n' "$delimiter" "$toolchain" "$delimiter" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:37:00.2210984Z [36;1mfi[0m
--+2026-11-24T20:37:00.2234897Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:37:00.2235277Z env:
--+2026-11-24T20:37:00.2235501Z   toolchain: stable
--+2026-11-24T20:37:00.2235722Z ##[endgroup]
--+2026-11-24T20:37:00.2338971Z ##[end-action id=__dtolnay_rust-toolchain.parse;outcome=success;conclusion=success;duration_ms=19]
--+2026-11-24T20:37:00.2343707Z ##[start-action display=Set $CARGO_HOME;id=__dtolnay_rust-toolchain.__run]
--+2026-11-24T20:37:00.2378357Z ##[group]Run echo CARGO_HOME=${CARGO_HOME:-"$HOME/.cargo"} >> $GITHUB_ENV
--+2026-11-24T20:37:00.2378874Z [36;1mecho CARGO_HOME=${CARGO_HOME:-"$HOME/.cargo"} >> $GITHUB_ENV[0m
--+2026-11-24T20:37:00.2398282Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:37:00.2398636Z ##[endgroup]
--+2026-11-24T20:37:00.2446765Z ##[end-action id=__dtolnay_rust-toolchain.__run;outcome=success;conclusion=success;duration_ms=10]
--+2026-11-24T20:37:00.2449440Z ##[start-action display=Install rustup if needed;id=__dtolnay_rust-toolchain.__run_2]
--+2026-11-24T20:37:00.2476211Z ##[group]Run if ! command -v rustup &>/dev/null; then
--+2026-11-24T20:37:00.2476596Z [36;1mif ! command -v rustup &>/dev/null; then[0m
--+2026-11-24T20:37:00.2477338Z [36;1m  curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused --location --silent --show-error --fail https://sh.rustup.rs | sh -s -- --default-toolchain none -y[0m
--+2026-11-24T20:37:00.2478043Z [36;1m  echo "$CARGO_HOME/bin" >> $GITHUB_PATH[0m
--+2026-11-24T20:37:00.2478407Z [36;1mfi[0m
--+2026-11-24T20:37:00.2496949Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:37:00.2497303Z env:
--+2026-11-24T20:37:00.2497523Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:00.2498031Z ##[endgroup]
--+2026-11-24T20:37:00.2541904Z ##[end-action id=__dtolnay_rust-toolchain.__run_2;outcome=success;conclusion=success;duration_ms=9]
--+2026-11-24T20:37:00.2544742Z ##[start-action display=Install rustup if needed on windows;id=__dtolnay_rust-toolchain.__run_3]
--+2026-11-24T20:37:00.2552121Z ##[end-action id=__dtolnay_rust-toolchain.__run_3;outcome=skipped;conclusion=skipped;duration_ms=0]
--+2026-11-24T20:37:00.2566771Z ##[start-action display=rustup toolchain install stable;id=__dtolnay_rust-toolchain.__run_4]
--+2026-11-24T20:37:00.2585619Z ##[group]Run set -f  # split targets and components on commas and whitespace, without glob expansion
--+2026-11-24T20:37:00.2586378Z [36;1mset -f  # split targets and components on commas and whitespace, without glob expansion[0m
--+2026-11-24T20:37:00.2586858Z [36;1mflags=()[0m
--+2026-11-24T20:37:00.2587155Z [36;1mfor t in ${targets//,/ }; do flags+=(--target "$t"); done[0m
--+2026-11-24T20:37:00.2587567Z [36;1mfor c in ${components//,/ }; do flags+=(--component "$c"); done[0m
--+2026-11-24T20:37:00.2588078Z [36;1mif [[ $toolchain == nightly && -n $components ]]; then flags+=(--allow-downgrade); fi[0m
--+2026-11-24T20:37:00.2588513Z [36;1mlog=$(mktemp)[0m
--+2026-11-24T20:37:00.2588805Z [36;1mtrap 'rm -f "$log"' EXIT[0m
--+2026-11-24T20:37:00.2589075Z [36;1mfor attempt in 1 2 3 4 5; do[0m
--+2026-11-24T20:37:00.2589642Z [36;1m  if rustup toolchain install "$toolchain" "${flags[@]}" --profile minimal --no-self-update --force-non-host 2>&1 | tee "$log"; then[0m
--+2026-11-24T20:37:00.2590197Z [36;1m    break[0m
--+2026-11-24T20:37:00.2590408Z [36;1m  else[0m
--+2026-11-24T20:37:00.2590622Z [36;1m    status=$?[0m
--+2026-11-24T20:37:00.2590836Z [36;1m  fi[0m
--+2026-11-24T20:37:00.2591130Z [36;1m  if [[ $attempt == 5 ]] || ! grep -Fq 'checksum failed for ' "$log" \[0m
--+2026-11-24T20:37:00.2591681Z [36;1m      || ! grep -Fq 'this is likely due to an ongoing update of the official release server' "$log"; then[0m
--+2026-11-24T20:37:00.2592211Z [36;1m    exit "$status"[0m
--+2026-11-24T20:37:00.2592724Z [36;1m  fi[0m
--+2026-11-24T20:37:00.2593186Z [36;1m  echo "Retrying rustup toolchain install in 30 seconds (retry $attempt of 4)" >&2[0m
--+2026-11-24T20:37:00.2593610Z [36;1m  sleep 30[0m
--+2026-11-24T20:37:00.2593817Z [36;1mdone[0m
--+2026-11-24T20:37:00.2612945Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:37:00.2613298Z env:
--+2026-11-24T20:37:00.2613525Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:00.2613780Z   toolchain: stable
--+2026-11-24T20:37:00.2614002Z   targets: 
--+2026-11-24T20:37:00.2614212Z   components: rustfmt, clippy
--+2026-11-24T20:37:00.2614473Z   RUSTUP_PERMIT_COPY_RENAME: 1
--+2026-11-24T20:37:00.2614716Z ##[endgroup]
--+2026-11-24T20:37:00.4365927Z info: syncing channel updates for stable-x86_64-unknown-linux-gnu
--+2026-11-24T20:37:00.5959286Z info: latest update on 2026-11-12 for version 1.100.0 (0ef1bc386 2026-11-09)
--+2026-11-24T20:37:00.6094391Z info: component clippy is up to date
--+2026-11-24T20:37:00.6094800Z info: component rustfmt is up to date
--+2026-11-24T20:37:00.6106568Z 
--+2026-11-24T20:37:00.6179683Z   stable-x86_64-unknown-linux-gnu unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
--+2026-11-24T20:37:00.6180593Z 
--+2026-11-24T20:37:00.6217254Z ##[end-action id=__dtolnay_rust-toolchain.__run_4;outcome=success;conclusion=success;duration_ms=364]
--+2026-11-24T20:37:00.6223008Z ##[start-action display=rustup default stable;id=__dtolnay_rust-toolchain.__run_5]
--+2026-11-24T20:37:00.6240520Z ##[group]Run rustup default "$toolchain" --force-non-host
--+2026-11-24T20:37:00.6240931Z [36;1mrustup default "$toolchain" --force-non-host[0m
--+2026-11-24T20:37:00.6348310Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:37:00.6348676Z env:
--+2026-11-24T20:37:00.6348905Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:00.6349168Z   toolchain: stable
--+2026-11-24T20:37:00.6349385Z ##[endgroup]
--+2026-11-24T20:37:00.6429037Z info: using existing install for stable-x86_64-unknown-linux-gnu
--+2026-11-24T20:37:00.6442747Z info: default toolchain set to stable-x86_64-unknown-linux-gnu
--+2026-11-24T20:37:00.6443352Z 
--+2026-11-24T20:37:00.6508825Z   stable-x86_64-unknown-linux-gnu unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
--+2026-11-24T20:37:00.6509233Z 
--+2026-11-24T20:37:00.6523348Z ##[end-action id=__dtolnay_rust-toolchain.__run_5;outcome=success;conclusion=success;duration_ms=29]
--+2026-11-24T20:37:00.6527294Z ##[start-action display=Create cachekey;id=__dtolnay_rust-toolchain.rustc-version]
--+2026-11-24T20:37:00.6546468Z ##[group]Run DATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')
--+2026-11-24T20:37:00.6547330Z [36;1mDATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')[0m
--+2026-11-24T20:37:00.6547997Z [36;1mHASH=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-hash: //p')[0m
--+2026-11-24T20:37:00.6548515Z [36;1mecho "cachekey=$(echo $DATE$HASH | head -c12)" >> $GITHUB_OUTPUT[0m
--+2026-11-24T20:37:00.6567966Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:37:00.6568325Z env:
--+2026-11-24T20:37:00.6568559Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:00.6568814Z   toolchain: stable
--+2026-11-24T20:37:00.6569027Z ##[endgroup]
--+2026-11-24T20:37:00.6884625Z ##[end-action id=__dtolnay_rust-toolchain.rustc-version;outcome=success;conclusion=success;duration_ms=35]
--+2026-11-24T20:37:00.6887330Z ##[start-action display=Disable incremental compilation;id=__dtolnay_rust-toolchain.__run_6]
--+2026-11-24T20:37:00.6906299Z ##[group]Run if [ -z "${CARGO_INCREMENTAL+set}" ]; then
--+2026-11-24T20:37:00.6906702Z [36;1mif [ -z "${CARGO_INCREMENTAL+set}" ]; then[0m
--+2026-11-24T20:37:00.6907038Z [36;1m  echo CARGO_INCREMENTAL=0 >> $GITHUB_ENV[0m
--+2026-11-24T20:37:00.6907336Z [36;1mfi[0m
--+2026-11-24T20:37:00.6926533Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:37:00.6926885Z env:
--+2026-11-24T20:37:00.6927101Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:00.6927354Z ##[endgroup]
--+2026-11-24T20:37:00.6970856Z ##[end-action id=__dtolnay_rust-toolchain.__run_6;outcome=success;conclusion=success;duration_ms=8]
--+2026-11-24T20:37:00.6973429Z ##[start-action display=Enable colors in Cargo output;id=__dtolnay_rust-toolchain.__run_7]
--+2026-11-24T20:37:00.6991114Z ##[group]Run if [ -z "${CARGO_TERM_COLOR+set}" ]; then
--+2026-11-24T20:37:00.6991501Z [36;1mif [ -z "${CARGO_TERM_COLOR+set}" ]; then[0m
--+2026-11-24T20:37:00.6991834Z [36;1m  echo CARGO_TERM_COLOR=always >> $GITHUB_ENV[0m
--+2026-11-24T20:37:00.6992134Z [36;1mfi[0m
--+2026-11-24T20:37:00.7009764Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:37:00.7010111Z env:
--+2026-11-24T20:37:00.7010327Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:00.7010593Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:37:00.7010813Z ##[endgroup]
--+2026-11-24T20:37:00.7053835Z ##[end-action id=__dtolnay_rust-toolchain.__run_7;outcome=success;conclusion=success;duration_ms=7]
--+2026-11-24T20:37:00.7057382Z ##[start-action display=Enable Cargo sparse registry;id=__dtolnay_rust-toolchain.__run_8]
--+2026-11-24T20:37:00.7076335Z ##[group]Run # implemented in 1.66, stabilized in 1.68, made default in 1.70
--+2026-11-24T20:37:00.7076970Z [36;1m# implemented in 1.66, stabilized in 1.68, made default in 1.70[0m
--+2026-11-24T20:37:00.7077651Z [36;1mif [ -z "${CARGO_REGISTRIES_CRATES_IO_PROTOCOL+set}" -o -f "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol ]; then[0m
--+2026-11-24T20:37:00.7078371Z [36;1m  if rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[89]\.'; then[0m
--+2026-11-24T20:37:00.7078949Z [36;1m    touch "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
--+2026-11-24T20:37:00.7079478Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse >> $GITHUB_ENV[0m
--+2026-11-24T20:37:00.7079986Z [36;1m  elif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[67]\.'; then[0m
--+2026-11-24T20:37:00.7080556Z [36;1m    touch "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
--+2026-11-24T20:37:00.7081170Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git >> $GITHUB_ENV[0m
--+2026-11-24T20:37:00.7081520Z [36;1m  fi[0m
--+2026-11-24T20:37:00.7081720Z [36;1mfi[0m
--+2026-11-24T20:37:00.7100009Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:37:00.7100350Z env:
--+2026-11-24T20:37:00.7100562Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:00.7100831Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:37:00.7101062Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:37:00.7101294Z   toolchain: stable
--+2026-11-24T20:37:00.7101499Z ##[endgroup]
--+2026-11-24T20:37:00.7364040Z ##[end-action id=__dtolnay_rust-toolchain.__run_8;outcome=success;conclusion=success;duration_ms=30]
--+2026-11-24T20:37:00.7368082Z ##[start-action display=Work around spurious network errors in curl 8.0;id=__dtolnay_rust-toolchain.__run_9]
--+2026-11-24T20:37:00.7389896Z ##[group]Run # https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation
--+2026-11-24T20:37:00.7390663Z [36;1m# https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation[0m
--+2026-11-24T20:37:00.7391311Z [36;1mif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.7[01]\.'; then[0m
--+2026-11-24T20:37:00.7391773Z [36;1m  echo CARGO_HTTP_MULTIPLEXING=false >> $GITHUB_ENV[0m
--+2026-11-24T20:37:00.7392084Z [36;1mfi[0m
--+2026-11-24T20:37:00.7411234Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:37:00.7411572Z env:
--+2026-11-24T20:37:00.7411791Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:00.7412056Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:37:00.7412282Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:37:00.7412855Z   toolchain: stable
--+2026-11-24T20:37:00.7413075Z ##[endgroup]
--+2026-11-24T20:37:00.7571328Z ##[end-action id=__dtolnay_rust-toolchain.__run_9;outcome=success;conclusion=success;duration_ms=20]
--+2026-11-24T20:37:00.7576602Z ##[start-action display=rustc +stable --version --verbose;id=__dtolnay_rust-toolchain.__run_10]
--+2026-11-24T20:37:00.7595992Z ##[group]Run rustc +"$toolchain" --version --verbose
--+2026-11-24T20:37:00.7596410Z [36;1mrustc +"$toolchain" --version --verbose[0m
--+2026-11-24T20:37:00.7615959Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
--+2026-11-24T20:37:00.7616300Z env:
--+2026-11-24T20:37:00.7616516Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:00.7616773Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:37:00.7617011Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:37:00.7617245Z   toolchain: stable
--+2026-11-24T20:37:00.7617456Z ##[endgroup]
--+2026-11-24T20:37:00.7778121Z rustc 1.100.0 (0ef1bc386 2026-11-09)
--+2026-11-24T20:37:00.7778726Z binary: rustc
--+2026-11-24T20:37:00.7779172Z commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a
--+2026-11-24T20:37:00.7779705Z commit-date: 2026-11-09
--+2026-11-24T20:37:00.7780109Z host: x86_64-unknown-linux-gnu
--+2026-11-24T20:37:00.7780476Z release: 1.100.0
--+2026-11-24T20:37:00.7780797Z LLVM version: 23.1.5
--+2026-11-24T20:37:00.7800035Z ##[end-action id=__dtolnay_rust-toolchain.__run_10;outcome=success;conclusion=success;duration_ms=22]
--+2026-11-24T20:37:00.7913091Z ##[group]Run Swatinem/rust-cache@v2
--+2026-11-24T20:37:00.7913559Z with:
--+2026-11-24T20:37:00.7913767Z   prefix-key: v0-rust
--+2026-11-24T20:37:00.7913997Z   add-job-id-key: true
--+2026-11-24T20:37:00.7914246Z   add-rust-environment-hash-key: true
--+2026-11-24T20:37:00.7914523Z   cache-targets: true
--+2026-11-24T20:37:00.7914755Z   cache-all-crates: false
--+2026-11-24T20:37:00.7915015Z   cache-workspace-crates: false
--+2026-11-24T20:37:00.7915262Z   save-if: true
--+2026-11-24T20:37:00.7915476Z   cache-provider: github
--+2026-11-24T20:37:00.7915709Z   cache-bin: true
--+2026-11-24T20:37:00.7915920Z   lookup-only: false
--+2026-11-24T20:37:00.7916138Z   cmd-format: {0}
--+2026-11-24T20:37:00.7916334Z env:
--+2026-11-24T20:37:00.7916536Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:00.7916791Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:37:00.7917017Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:37:00.7917252Z ##[endgroup]
--+2026-11-24T20:37:01.1525055Z ##[group]Cache Configuration
--+2026-11-24T20:37:01.1527013Z Cache Provider:
--+2026-11-24T20:37:01.1527662Z     github
--+2026-11-24T20:37:01.1528162Z Workspaces:
--+2026-11-24T20:37:01.1528788Z     /home/runner/work/MultiTunnel/MultiTunnel
--+2026-11-24T20:37:01.1529553Z Cache Paths:
--+2026-11-24T20:37:01.1530104Z     /home/runner/.cargo/bin
--+2026-11-24T20:37:01.1530747Z     /home/runner/.cargo/.crates.toml
--+2026-11-24T20:37:01.1531458Z     /home/runner/.cargo/.crates2.json
--+2026-11-24T20:37:01.1532309Z     /home/runner/.cargo/registry
--+2026-11-24T20:37:01.1540315Z     /home/runner/.cargo/git
--+2026-11-24T20:37:01.1541149Z     /home/runner/work/MultiTunnel/MultiTunnel/target
--+2026-11-24T20:37:01.1541866Z Restore Key:
--+2026-11-24T20:37:01.1542615Z     v0-rust-build-linux-daemon-Linux-x64-2fd77104
--+2026-11-24T20:37:01.1543844Z Cache Key:
--+2026-11-24T20:37:01.1544463Z     v0-rust-build-linux-daemon-Linux-x64-2fd77104-39107a77
--+2026-11-24T20:37:01.1545058Z .. Prefix:
--+2026-11-24T20:37:01.1545485Z   - v0-rust-build-linux-daemon-Linux-x64
--+2026-11-24T20:37:01.1546035Z .. Environment considered:
--+2026-11-24T20:37:01.1546515Z   - Rust Versions:
--+2026-11-24T20:37:01.1547248Z     - 1.100.0 x86_64-unknown-linux-gnu 0ef1bc386059596dada8fb65b50425aab2ea637a
--+2026-11-24T20:37:01.1547920Z   - CARGO_HOME
--+2026-11-24T20:37:01.1548282Z   - CARGO_INCREMENTAL
--+2026-11-24T20:37:01.1548663Z   - CARGO_TERM_COLOR
--+2026-11-24T20:37:01.1549081Z .. Lockfiles considered:
--+2026-11-24T20:37:01.1549630Z   - /home/runner/work/MultiTunnel/MultiTunnel/Cargo.lock
--+2026-11-24T20:37:01.1550311Z   - /home/runner/work/MultiTunnel/MultiTunnel/Cargo.toml
--+2026-11-24T20:37:01.1551088Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-app/Cargo.toml
--+2026-11-24T20:37:01.1551982Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond/Cargo.toml
--+2026-11-24T20:37:01.1553065Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-engine/Cargo.toml
--+2026-11-24T20:37:01.1553910Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-links/Cargo.toml
--+2026-11-24T20:37:01.1554668Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-m3/Cargo.toml
--+2026-11-24T20:37:01.1555461Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net/Cargo.toml
--+2026-11-24T20:37:01.1556339Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-platform/Cargo.toml
--+2026-11-24T20:37:01.1557245Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-service/Cargo.toml
--+2026-11-24T20:37:01.1558412Z ##[endgroup]
--+2026-11-24T20:37:01.1558670Z 
--+2026-11-24T20:37:01.1558857Z ... Restoring cache ...
--+2026-11-24T20:37:01.3827302Z Cache hit for: v0-rust-build-linux-daemon-Linux-x64-2fd77104-39107a77
--+2026-11-24T20:37:02.5997494Z Received 71303168 of 217294952 (32.8%), 68.0 MBs/sec
--+2026-11-24T20:37:03.5204106Z Received 217294952 of 217294952 (100.0%), 107.9 MBs/sec
--+2026-11-24T20:37:03.5205574Z Cache Size: ~207 MB (217294952 B)
--+2026-11-24T20:37:03.5323042Z [command]/usr/bin/tar -xf /home/runner/work/_temp/9bedf71f-3618-4c40-ac45-6cc06b5374fa/cache.tzst -P -C /home/runner/work/MultiTunnel/MultiTunnel --use-compress-program unzstd
--+2026-11-24T20:37:04.4208774Z Cache restored successfully
--+2026-11-24T20:37:04.4278133Z Restored from cache key "v0-rust-build-linux-daemon-Linux-x64-2fd77104-39107a77" full match: true.
--+2026-11-24T20:37:04.4386296Z ##[group]Run cargo fmt --all --check
--+2026-11-24T20:37:04.4386657Z [36;1mcargo fmt --all --check[0m
--+2026-11-24T20:37:04.4407912Z shell: /usr/bin/bash -e {0}
--+2026-11-24T20:37:04.4408222Z env:
--+2026-11-24T20:37:04.4408438Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:04.4408703Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:37:04.4408930Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:37:04.4409188Z   CACHE_ON_FAILURE: false
--+2026-11-24T20:37:04.4409438Z ##[endgroup]
--+2026-11-24T20:37:04.9116989Z ##[group]Run cargo clippy -p mt-bond --all-targets -- -D warnings
--+2026-11-24T20:37:04.9117488Z [36;1mcargo clippy -p mt-bond --all-targets -- -D warnings[0m
--+2026-11-24T20:37:04.9136654Z shell: /usr/bin/bash -e {0}
--+2026-11-24T20:37:04.9136936Z env:
--+2026-11-24T20:37:04.9137144Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:04.9137410Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:37:04.9137651Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:37:04.9137912Z   CACHE_ON_FAILURE: false
--+2026-11-24T20:37:04.9138142Z ##[endgroup]
--+2026-11-24T20:37:05.2628623Z [1m[92m    Updating[0m crates.io index
--+2026-11-24T20:37:05.5636947Z [1m[92m    Checking[0m mt-net v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net)
--+2026-11-24T20:37:07.8990819Z [1m[92m    Checking[0m mt-bond v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond)
--+2026-11-24T20:37:08.7755798Z [1m[92m    Finished[0m `dev` profile [unoptimized + debuginfo] target(s) in 3.76s
--+2026-11-24T20:37:08.7919048Z ##[group]Run cargo test -p mt-bond
--+2026-11-24T20:37:08.7919467Z [36;1mcargo test -p mt-bond[0m
--+2026-11-24T20:37:08.7938957Z shell: /usr/bin/bash -e {0}
--+2026-11-24T20:37:08.7939232Z env:
--+2026-11-24T20:37:08.7939443Z   CARGO_HOME: /home/runner/.cargo
--+2026-11-24T20:37:08.7939716Z   CARGO_INCREMENTAL: 0
--+2026-11-24T20:37:08.7939952Z   CARGO_TERM_COLOR: always
--+2026-11-24T20:37:08.7940203Z   CACHE_ON_FAILURE: false
--+2026-11-24T20:37:08.7940430Z ##[endgroup]
--+2026-11-24T20:37:08.9032319Z [1m[92m   Compiling[0m mt-net v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net)
--+2026-11-24T20:37:09.5953828Z [1m[92m   Compiling[0m mt-bond v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond)
--+2026-11-24T20:37:12.4119615Z [1m[92m    Finished[0m `test` profile [unoptimized + debuginfo] target(s) in 3.60s
--+2026-11-24T20:37:12.4188313Z [1m[92m     Running[0m unittests src/lib.rs (target/debug/build/mt-bond/a79068680de7bb57/out/mt_bond-a79068680de7bb57)
--+2026-11-24T20:37:12.4201804Z 
--+2026-11-24T20:37:12.4202011Z running 33 tests
--+2026-11-24T20:37:12.4214075Z test endpoint::scheduling_tests::balanced_blest_guard_skips_slow_draining_tied_carrier ... ok
--+2026-11-24T20:37:12.4215740Z test endpoint::scheduling_tests::balanced_picks_the_least_backlogged_carrier ... ok
--+2026-11-24T20:37:12.4216943Z test endpoint::scheduling_tests::balanced_blest_guard_never_bypasses_the_score_leader ... ok
--+2026-11-24T20:37:12.4218203Z test endpoint::scheduling_tests::balanced_capacity_aware_bypasses_the_saturated_adapter ... ok
--+2026-11-24T20:37:12.4219625Z test endpoint::scheduling_tests::balanced_spreads_proportionally_to_reported_capacity ... ok
--+2026-11-24T20:37:12.4220692Z test endpoint::scheduling_tests::balanced_ties_rotate ... ok
--+2026-11-24T20:37:12.4221633Z test endpoint::scheduling_tests::ecf_completion_is_rtt_plus_drain_time ... ok
--+2026-11-24T20:37:12.4223363Z test endpoint::scheduling_tests::carrier_score_is_relative_load_with_probe_floor ... ok
--+2026-11-24T20:37:12.4224284Z test endpoint::scheduling_tests::failover_takes_the_first_live_carrier ... ok
--+2026-11-24T20:37:12.4224859Z test endpoint::scheduling_tests::lowest_latency_prefers_sampled_rtt_then_known_over_unknown ... ok
--+2026-11-24T20:37:12.4225492Z test endpoint::scheduling_tests::lowest_latency_ecf_prefers_earliest_completion ... ok
--+2026-11-24T20:37:12.4226035Z test endpoint::scheduling_tests::mixed_reports_probe_the_unreported_carrier ... ok
--+2026-11-24T20:37:12.4226606Z test endpoint::scheduling_tests::priority_fills_in_order_until_the_queue_backs_up ... ok
--+2026-11-24T20:37:12.4227456Z test endpoint::scheduling_tests::priority_spills_when_relative_load_exceeds_ratio ... ok
--+2026-11-24T20:37:12.4227949Z test frame::tests::empty_and_max_payloads_roundtrip ... ok
--+2026-11-24T20:37:12.4228361Z test endpoint::scheduling_tests::round_robin_cycles_the_cursor ... ok
--+2026-11-24T20:37:12.4228797Z test frame::tests::encode_session_borrowed_is_wire_identical ... ok
--+2026-11-24T20:37:12.4229249Z test frame::tests::header_rejects_bad_magic_version_kind_and_size ... ok
--+2026-11-24T20:37:12.4229642Z test frame::tests::roundtrip_all_kinds ... ok
--+2026-11-24T20:37:12.4229983Z test frame::tests::read_write_frame_over_duplex ... ok
--+2026-11-24T20:37:12.4230362Z test frame::tests::target_rejects_truncated_and_garbage ... ok
--+2026-11-24T20:37:12.4230762Z test frame::tests::target_roundtrip_domain_ipv4_ipv6 ... ok
--+2026-11-24T20:37:12.4231194Z test outbound::tests::ensure_carriers_fails_cleanly_with_no_links ... ok
--+2026-11-24T20:37:12.4231648Z test outbound::tests::ensure_carriers_reports_all_dead_links ... ok
--+2026-11-24T20:37:12.4232067Z test reorder::tests::cap_zero_still_delivers_in_order ... ok
--+2026-11-24T20:37:12.4232724Z test reorder::tests::duplicates_and_replays_are_idempotent ... ok
--+2026-11-24T20:37:12.4233317Z test reorder::tests::in_order_delivery ... ok
--+2026-11-24T20:37:12.4233885Z test reorder::tests::gap_buffers_then_flushes ... ok
--+2026-11-24T20:37:12.4234271Z test reorder::tests::overflow_is_an_error ... ok
--+2026-11-24T20:37:12.4235042Z test reorder::tests::would_overflow_mirrors_the_insert_boundary ... ok
--+2026-11-24T20:37:12.4235504Z test reorder::tests::long_out_of_order_run_reorders_completely ... ok
--+2026-11-24T20:37:12.4235879Z test tests::constants_are_coherent ... ok
--+2026-11-24T20:37:12.6229878Z test daemon::tests::serve_binds_and_shuts_down ... ok
--+2026-11-24T20:37:12.6230460Z 
--+2026-11-24T20:37:12.6230967Z test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
--+2026-11-24T20:37:12.6231708Z 
--+2026-11-24T20:37:12.6233749Z [1m[92m     Running[0m unittests src/bin/mt_merge_server.rs (target/debug/build/mt-bond/9915eef574e68264/out/mt_merge_server-9915eef574e68264)
--+2026-11-24T20:37:12.6245243Z 
--+2026-11-24T20:37:12.6245788Z running 0 tests
--+2026-11-24T20:37:12.6246188Z 
--+2026-11-24T20:37:12.6247128Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
--+2026-11-24T20:37:12.6248057Z 
--+2026-11-24T20:37:12.6249472Z [1m[92m     Running[0m tests/integration.rs (target/debug/build/mt-bond/3d8a2918f951a063/out/integration-3d8a2918f951a063)
--+2026-11-24T20:37:12.6260511Z 
--+2026-11-24T20:37:12.6261246Z running 6 tests
--+2026-11-24T20:37:13.6939582Z test carrier_death_mid_stream_fails_over_without_data_loss ... ok
--+2026-11-24T20:37:15.7584266Z test concurrent_sessions_multiplex_over_the_same_carriers ... ok
--+2026-11-24T20:37:15.7672272Z test echo_roundtrip_1mib_across_two_carriers ... ok
--+2026-11-24T20:37:15.8110694Z test refused_target_surfaces_as_connection_reset ... ok
--+2026-11-24T20:37:16.7880589Z test one_dead_link_still_yields_a_working_bond ... ok
--+2026-11-24T20:38:12.6273497Z test domain_targets_dial_through_the_daemon has been running for over 60 seconds
--+2026-11-24T20:38:12.6297928Z test domain_targets_dial_through_the_daemon ... FAILED
--+2026-11-24T20:38:12.6298455Z 
--+2026-11-24T20:38:12.6298633Z failures:
--+2026-11-24T20:38:12.6298834Z 
--+2026-11-24T20:38:12.6299016Z ---- domain_targets_dial_through_the_daemon stdout ----
--+2026-11-24T20:38:12.6299427Z [mt-merge-server] listening on 127.0.0.1:38591
--+2026-11-24T20:38:12.6299711Z 
--+2026-11-24T20:38:12.6300085Z thread 'domain_targets_dial_through_the_daemon' (2745) panicked at crates/mt-bond/tests/integration.rs:382:6:
--+2026-11-24T20:38:12.6301055Z deadline: domain_targets wedged (documented runner flake — see LEDGER): Elapsed(())
--+2026-11-24T20:38:12.6301730Z note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
--+2026-11-24T20:38:12.6302085Z 
--+2026-11-24T20:38:12.6302090Z 
--+2026-11-24T20:38:12.6302197Z failures:
--+2026-11-24T20:38:12.6303339Z     domain_targets_dial_through_the_daemon
--+2026-11-24T20:38:12.6303574Z 
--+2026-11-24T20:38:12.6303881Z test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 60.00s
--+2026-11-24T20:38:12.6304291Z 
--+2026-11-24T20:38:12.6307998Z [1m[91merror[0m: test failed, to rerun pass `-p mt-bond --test integration`
--+2026-11-24T20:38:12.6362351Z ##[error]Process completed with exit code 101.
--+2026-11-24T20:38:12.6527178Z Post job cleanup.
--+2026-11-24T20:38:12.7379358Z [command]/usr/bin/git version
--+2026-11-24T20:38:12.7422773Z git version 2.56.1
--+2026-11-24T20:38:12.7460068Z Temporarily overriding HOME='/home/runner/work/_temp/58dd0450-01a4-4e8a-8b78-e4f4f99e7daa' before making global git config changes
--+2026-11-24T20:38:12.7461536Z Adding repository directory to the temporary git global config as a safe directory
--+2026-11-24T20:38:12.7466435Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/MultiTunnel/MultiTunnel
--+2026-11-24T20:38:12.7504488Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
--+2026-11-24T20:38:12.7538378Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
--+2026-11-24T20:38:12.7787549Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
--+2026-11-24T20:38:12.7810765Z http.https://github.com/.extraheader
--+2026-11-24T20:38:12.7821251Z [command]/usr/bin/git config --local --unset-all http.https://github.com/.extraheader
--+2026-11-24T20:38:12.7853240Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
--+2026-11-24T20:38:12.8139962Z [command]/usr/bin/git config --local --name-only --get-regexp ^includeIf\.gitdir:
--+2026-11-24T20:38:12.8177570Z [command]/usr/bin/git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url
--+2026-11-24T20:38:12.8581674Z Cleaning up orphan processes
--+2026-11-24T20:38:12.8877184Z ##[warning]Node.js 20 is deprecated. The following actions target Node.js 20 but are being forced to run on Node.js 24: actions/checkout@v4. For more information see: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/
--diff --git a/harness/memory/curated/edges.json b/harness/memory/curated/edges.json
--index f04fdfa..2251d71 100644
----- a/harness/memory/curated/edges.json
--+++ b/harness/memory/curated/edges.json
--@@ -60,6 +60,18 @@
--       "rel": "part_of",
--       "evidence": "workspace crate"
--     },
--+    {
--+      "src": "decision:D29",
--+      "dst": "ledger:s21",
--+      "rel": "decided_in",
--+      "evidence": "agent-curated"
--+    },
--+    {
--+      "src": "decision:D30",
--+      "dst": "ledger:s21",
--+      "rel": "decided_in",
--+      "evidence": "agent-curated"
--+    },
--     {
--       "src": "protocol:memory-graph",
--       "dst": "protocol:pstack",
--diff --git a/harness/memory/curated/nodes.json b/harness/memory/curated/nodes.json
--index 577db45..df12b95 100644
----- a/harness/memory/curated/nodes.json
--+++ b/harness/memory/curated/nodes.json
--@@ -221,6 +221,60 @@
--         }
--       ]
--     },
--+    {
--+      "id": "decision:D29",
--+      "type": "decision",
--+      "title": "D29 P0 reconnect force-close + bind retry",
--+      "tags": [],
--+      "sources": [
--+        {
--+          "file": "harness/memory/curated/",
--+          "anchor": ""
--+        }
--+      ],
--+      "passages": [
--+        {
--+          "text": "Second-connection bug root cause: untracked connection tasks pinned listener-port sockets past the drain; Windows re-bind WSAEADDRINUSE (Linux tolerates re-bind over non-listening sockets — proven by scripts/rebind_probe.rs). Fix: Shared.conns AbortHandles + end-of-drain force-close + AbortPumpsOnDrop (pumps detach on JoinHandle drop) + bind_with_retry 5s. engine_file_stem FNV identity hash separates same-tag profiles (H2); store_mtx serializes profiles.json RMW (H3). source=src/real_service.rs#connect",
--+          "kind": "body"
--+        }
--+      ]
--+    },
--+    {
--+      "id": "decision:D30",
--+      "type": "decision",
--+      "title": "D30 QoS fair share + merged Exit&speed panel",
--+      "tags": [],
--+      "sources": [
--+        {
--+          "file": "harness/memory/curated/",
--+          "anchor": ""
--+        }
--+      ],
--+      "passages": [
--+        {
--+          "text": "QoS: per-destination groups, 1 s tick refills aggregate/groups only under contention (>=2 groups AND >=2 MiB/s aggregate floor), pump read gate blocks until budget, scheduler ALWAYS attached so the Settings qos_fair toggle flips live (no reconnect). Merged Exit & speed Status panel (view-only merge, D24-safe, both triggers side-by-side). P1 parity: TCP_NODELAY x4 legs, ACK coalescing 8 frames/20ms with CLOSE+PING flush, same-carrier PONG echo (RTT was cross-contaminated), send-path copies 5->3 (encode_session_borrowed, wire-identity test), writev batching (<=16 frames), quarantine last-resort fallback (no sole-link blackout), production prober uses user probe target. source=crates/mt-net/src/qos.rs",
--+          "kind": "body"
--+        }
--+      ]
--+    },
--+    {
--+      "id": "ledger:s21",
--+      "type": "ledger",
--+      "title": "S21: P0 second-connection fix, perf parity, QoS toggle, merged panel (v0.14.0)",
--+      "tags": [],
--+      "sources": [
--+        {
--+          "file": "harness/memory/curated/",
--+          "anchor": ""
--+        }
--+      ],
--+      "passages": [
--+        {
--+          "text": "2026-11-24T21:00Z | s21 | IMPLEMENTED + RELEASED v0.14.0: P0 = tracked connection tasks + end-of-drain force-close + pump-abort guard + bind retry (Windows WSAEADDRINUSE root cause; Linux-tolerant re-bind proven; regression test fails pre-fix) + engine_file_stem FNV identity hash + store_mtx; P1 = TCP_NODELAY x4 legs, ACK coalescing 8/20ms, same-carrier PONG echo, copies 5->3, writev <=16, quarantine last-resort fallback, user probe target; o = QoS fair share (per-destination groups, >=2 groups + >=2 MiB/s floor, live toggle, no reconnect); p = merged Exit & speed panel. 490/0; all 8 gates; release run 45477702090 SUCCESS attempt-3 (2 documented runner-flake legs rerun; logs archived s21-ci-*); assets verified. source=harness/memory/LEDGER.md#s21",
--+          "kind": "body"
--+        }
--+      ]
--+    },
--     {
--       "id": "project:multitunnel",
--       "type": "project",
--diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
--index f5a4fba..8477f2b 100644
----- a/harness/memory/store/memory_graph.json
--+++ b/harness/memory/store/memory_graph.json
--@@ -1,6 +1,6 @@
-- {
--   "schema_version": 1,
---  "as_of": "c0d8f15 2026-11-22T04:16:12Z",
--+  "as_of": "e510e43 2026-11-24T20:08:15Z",
--   "sources": [
--     "PROJECT_MEMORY.md",
--     "harness/memory/STATE.md",
--@@ -11,8 +11,8 @@
--     "git (tags + full log)",
--     "harness/memory/curated/"
--   ],
---  "node_count": 281,
---  "edge_count": 287,
--+  "node_count": 290,
--+  "edge_count": 298,
--   "nodes": [
--     {
--       "id": "bug:freedom-loopback-blackhole",
--@@ -335,6 +335,26 @@
--         }
--       ]
--     },
--+    {
--+      "id": "commit:1a9849f",
--+      "type": "commit",
--+      "title": "backup(s21-release-prep): 2026-11-24T20:08:00Z",
--+      "tags": [
--+        "2026-11-24"
--+      ],
--+      "sources": [
--+        {
--+          "file": "git:log",
--+          "anchor": "1a9849f"
--+        }
--+      ],
--+      "passages": [
--+        {
--+          "text": "backup(s21-release-prep): 2026-11-24T20:08:00Z",
--+          "kind": "title"
--+        }
--+      ]
--+    },
--     {
--       "id": "commit:1b2fe0b",
--       "type": "commit",
--@@ -555,6 +575,26 @@
--         }
--       ]
--     },
--+    {
--+      "id": "commit:3cd904d",
--+      "type": "commit",
--+      "title": "backup(s21-p0-second-connection): 2026-11-24T18:40:42Z",
--+      "tags": [
--+        "2026-11-24"
--+      ],
--+      "sources": [
--+        {
--+          "file": "git:log",
--+          "anchor": "3cd904d"
--+        }
--+      ],
--+      "passages": [
--+        {
--+          "text": "backup(s21-p0-second-connection): 2026-11-24T18:40:42Z",
--+          "kind": "title"
--+        }
--+      ]
--+    },
--     {
--       "id": "commit:3da17ad",
--       "type": "commit",
--@@ -995,6 +1035,26 @@
--         }
--       ]
--     },
--+    {
--+      "id": "commit:685e95c",
--+      "type": "commit",
--+      "title": "backup(s21-features-qos-merged-panel): 2026-11-24T20:07:19Z",
--+      "tags": [
--+        "2026-11-24"
--+      ],
--+      "sources": [
--+        {
--+          "file": "git:log",
--+          "anchor": "685e95c"
--+        }
--+      ],
--+      "passages": [
--+        {
--+          "text": "backup(s21-features-qos-merged-panel): 2026-11-24T20:07:19Z",
--+          "kind": "title"
--+        }
--+      ]
--+    },
--     {
--       "id": "commit:6a4bd7c",
--       "type": "commit",
--@@ -1835,6 +1895,26 @@
--         }
--       ]
--     },
--+    {
--+      "id": "commit:cccc77a",
--+      "type": "commit",
--+      "title": "backup(s21-p1-perf-stability): 2026-11-24T18:51:55Z",
--+      "tags": [
--+        "2026-11-24"
--+      ],
--+      "sources": [
--+        {
--+          "file": "git:log",
--+          "anchor": "cccc77a"
--+        }
--+      ],
--+      "passages": [
--+        {
--+          "text": "backup(s21-p1-perf-stability): 2026-11-24T18:51:55Z",
--+          "kind": "title"
--+        }
--+      ]
--+    },
--     {
--       "id": "commit:ce6af07",
--       "type": "commit",
--@@ -2075,6 +2155,26 @@
--         }
--       ]
--     },
--+    {
--+      "id": "commit:e510e43",
--+      "type": "commit",
--+      "title": "backup(s21-release-prep-2): 2026-11-24T20:08:15Z",
--+      "tags": [
--+        "2026-11-24"
--+      ],
--+      "sources": [
--+        {
--+          "file": "git:log",
--+          "anchor": "e510e43"
--+        }
--+      ],
--+      "passages": [
--+        {
--+          "text": "backup(s21-release-prep-2): 2026-11-24T20:08:15Z",
--+          "kind": "title"
--+        }
--+      ]
--+    },
--     {
--       "id": "commit:eaaa8c3",
--       "type": "commit",
--@@ -3271,6 +3371,24 @@
--         }
--       ]
--     },
--+    {
--+      "id": "decision:D29",
--+      "type": "decision",
--+      "title": "D29 P0 reconnect force-close + bind retry",
--+      "tags": [],
--+      "sources": [
--+        {
--+          "file": "harness/memory/curated/",
--+          "anchor": ""
--+        }
--+      ],
--+      "passages": [
--+        {
--+          "text": "Second-connection bug root cause: untracked connection tasks pinned listener-port sockets past the drain; Windows re-bind WSAEADDRINUSE (Linux tolerates re-bind over non-listening sockets — proven by scripts/rebind_probe.rs). Fix: Shared.conns AbortHandles + end-of-drain force-close + AbortPumpsOnDrop (pumps detach on JoinHandle drop) + bind_with_retry 5s. engine_file_stem FNV identity hash separates same-tag profiles (H2); store_mtx serializes profiles.json RMW (H3). source=src/real_service.rs#connect",
--+          "kind": "body"
--+        }
--+      ]
--+    },
--     {
--       "id": "decision:D3",
--       "type": "decision",
--@@ -3291,6 +3409,24 @@
--         }
--       ]
--     },
--+    {
--+      "id": "decision:D30",
--+      "type": "decision",
--+      "title": "D30 QoS fair share + merged Exit&speed panel",
--+      "tags": [],
--+      "sources": [
--+        {
--+          "file": "harness/memory/curated/",
--+          "anchor": ""
--+        }
--+      ],
--+      "passages": [
--+        {
--+          "text": "QoS: per-destination groups, 1 s tick refills aggregate/groups only under contention (>=2 groups AND >=2 MiB/s aggregate floor), pump read gate blocks until budget, scheduler ALWAYS attached so the Settings qos_fair toggle flips live (no reconnect). Merged Exit & speed Status panel (view-only merge, D24-safe, both triggers side-by-side). P1 parity: TCP_NODELAY x4 legs, ACK coalescing 8 frames/20ms with CLOSE+PING flush, same-carrier PONG echo (RTT was cross-contaminated), send-path copies 5->3 (encode_session_borrowed, wire-identity test), writev batching (<=16 frames), quarantine last-resort fallback (no sole-link blackout), production prober uses user probe target. source=crates/mt-net/src/qos.rs",
--+          "kind": "body"
--+        }
--+      ]
--+    },
--     {
--       "id": "decision:D4",
--       "type": "decision",
--@@ -4828,6 +4964,24 @@
--         }
--       ]
--     },
--+    {
--+      "id": "ledger:s21",
--+      "type": "ledger",
--+      "title": "S21: P0 second-connection fix, perf parity, QoS toggle, merged panel (v0.14.0)",
--+      "tags": [],
--+      "sources": [
--+        {
--+          "file": "harness/memory/curated/",
--+          "anchor": ""
--+        }
--+      ],
--+      "passages": [
--+        {
--+          "text": "2026-11-24T21:00Z | s21 | IMPLEMENTED + RELEASED v0.14.0: P0 = tracked connection tasks + end-of-drain force-close + pump-abort guard + bind retry (Windows WSAEADDRINUSE root cause; Linux-tolerant re-bind proven; regression test fails pre-fix) + engine_file_stem FNV identity hash + store_mtx; P1 = TCP_NODELAY x4 legs, ACK coalescing 8/20ms, same-carrier PONG echo, copies 5->3, writev <=16, quarantine last-resort fallback, user probe target; o = QoS fair share (per-destination groups, >=2 groups + >=2 MiB/s floor, live toggle, no reconnect); p = merged Exit & speed panel. 490/0; all 8 gates; release run 45477702090 SUCCESS attempt-3 (2 documented runner-flake legs rerun; logs archived s21-ci-*); assets verified. source=harness/memory/LEDGER.md#s21",
--+          "kind": "body"
--+        }
--+      ]
--+    },
--     {
--       "id": "ledger:s3",
--       "type": "ledger",
--@@ -5507,6 +5661,10 @@
--         }
--       ],
--       "passages": [
--+        {
--+          "text": "- **Stage**: S21 CLOSED — v0.14.0 released and verified (tag on e510e43, release run 45477702090 SUCCESS on attempt 3, same-commit ci green on both OS legs, assets download-verified). The user's four asks shipped: (1) P0 SECOND-CONNECTION BUG — mid-traffic reconnect re-binds the fixed listener ports while untracked in-flight connection tasks pin their sockets (no SO_REUSEADDR); on Windows the re-bind lands WSAEADDRINUSE → connect Failed → dead tunnel until restart. Linux tolerates the re-bind (std rebind probe), so the regression test asserts the platform-independent observable: the lingering client connection sees EOF after the picker switch (pre-fix FAILS, post-fix passes). Fix: Shared.conns tracks every connection task's AbortHandle; run() force-closes whatever the 3 s drain could not reap; AbortPumpsOnDrop aborts the DETACHED pump children (dropping JoinHandles leaves them running — found when the first EOF assertion failed); bind_with_retry 5 s (BIND_WAIT 2→8 s). H2: engine config/log stems carry a deterministic FNV-1a hash of the full endpoint identity (engine_file_stem) — same-tag different-endpoint profiles can no longer overwrite each other's Xray config (temp probe tunnels corrupting the LIVE engine → phantom respawn → DEGRADED while GUI showed Connected). H3: Core.store_mtx serializes profiles.json read-modify-write.",
--+          "kind": "body"
--+        },
--         {
--           "text": "- **Stage**: S20 CLOSED — the S19 leftover (Copilot code review) COMPLETED and shipped as v0.13.1 (tag on 4923566, release run 44983522874 SUCCESS, ci ×2 green, assets download-verified). The cloud Copilot reviewer ran via `POST /repos/maniesh-NR/MultiTunnel/pulls/1/copilot-code-review` (needs only pull-requests:write, which the PAT has): effort ladder per the fallback instruction — max → quota-rejected, balanced → quota-rejected, **lite → ran**. Round 1 (on 52f1d3c): 12 findings (5 High: step-0 self-heal never cleared sysproxy_set → no-snapshot restore disables a user's pre-existing proxy; give_up unserialized with disconnect → black-hole re-installed AFTER an explicit disconnect; set-BEFORE-bind → a foreign port-9 owner receives the blocked traffic; listener stopped even when restore failed → stranded block, no retry, false \"restored\" log; diag header wrote user URLs verbatim → credential leak. 7 Medium: live diag toggle misses engine debug, failed enable leaves the toggle on, probes ride a session that is not the target profile, unbounded head buffer, port-0 edit, unvalidated REALITY edits, pop() = lowest-priority adapter). All 12 fixed (Core::proxy_mtx serializes every proxy hand-off with the bind hoisted out of the lock; bind-before-set with no-set-on-bind- failure; restore-first release + retry; redact_url; diag-on-live reconnect; failed-enable revert; Session.profile_id pins both probes; 64 KiB head cap; port-0 + REALITY structural validation; first-link selection). Round 2 (lite, on the fixes): CLEAN PASS, zero comments.",
--           "kind": "body"
--@@ -5725,6 +5883,26 @@
--         }
--       ]
--     },
--+    {
--+      "id": "release:v0.14.0",
--+      "type": "release",
--+      "title": "Release v0.14.0 (tagged e510e43)",
--+      "tags": [
--+        "v0.14.0"
--+      ],
--+      "sources": [
--+        {
--+          "file": "git:tags/v0.14.0",
--+          "anchor": "e510e43"
--+        }
--+      ],
--+      "passages": [
--+        {
--+          "text": "Release v0.14.0: tag points at commit e510e43.",
--+          "kind": "body"
--+        }
--+      ]
--+    },
--     {
--       "id": "release:v0.2.0",
--       "type": "release",
--@@ -8159,6 +8337,12 @@
--       "rel": "part_of",
--       "evidence": "subject stage label (v0.3.1)"
--     },
--+    {
--+      "src": "commit:1a9849f",
--+      "dst": "ledger:s21",
--+      "rel": "part_of",
--+      "evidence": "subject stage label (s21)"
--+    },
--     {
--       "src": "commit:1b2fe0b",
--       "dst": "ledger:s6-wave1",
--@@ -8213,6 +8397,12 @@
--       "rel": "part_of",
--       "evidence": "subject stage label (s2)"
--     },
--+    {
--+      "src": "commit:3cd904d",
--+      "dst": "ledger:s21",
--+      "rel": "part_of",
--+      "evidence": "subject stage label (s21)"
--+    },
--     {
--       "src": "commit:3da17ad",
--       "dst": "ledger:s19",
--@@ -8327,6 +8517,12 @@
--       "rel": "part_of",
--       "evidence": "subject stage label (s19)"
--     },
--+    {
--+      "src": "commit:685e95c",
--+      "dst": "ledger:s21",
--+      "rel": "part_of",
--+      "evidence": "subject stage label (s21)"
--+    },
--     {
--       "src": "commit:6a4bd7c",
--       "dst": "ledger:s9.1",
--@@ -8519,6 +8715,12 @@
--       "rel": "part_of",
--       "evidence": "subject stage label (s13)"
--     },
--+    {
--+      "src": "commit:cccc77a",
--+      "dst": "ledger:s21",
--+      "rel": "part_of",
--+      "evidence": "subject stage label (s21)"
--+    },
--     {
--       "src": "commit:ce6af07",
--       "dst": "ledger:s17",
--@@ -8579,6 +8781,12 @@
--       "rel": "part_of",
--       "evidence": "subject stage label (s2)"
--     },
--+    {
--+      "src": "commit:e510e43",
--+      "dst": "ledger:s21",
--+      "rel": "part_of",
--+      "evidence": "subject stage label (s21)"
--+    },
--     {
--       "src": "commit:eb1235a",
--       "dst": "ledger:s2-fix",
--@@ -8825,12 +9033,36 @@
--       "rel": "part_of",
--       "evidence": "project decision log"
--     },
--+    {
--+      "src": "decision:D29",
--+      "dst": "ledger:s21",
--+      "rel": "decided_in",
--+      "evidence": "agent-curated"
--+    },
--+    {
--+      "src": "decision:D29",
--+      "dst": "project:multitunnel",
--+      "rel": "part_of",
--+      "evidence": "project decision log"
--+    },
--     {
--       "src": "decision:D3",
--       "dst": "project:multitunnel",
--       "rel": "part_of",
--       "evidence": "project decision log"
--     },
--+    {
--+      "src": "decision:D30",
--+      "dst": "ledger:s21",
--+      "rel": "decided_in",
--+      "evidence": "agent-curated"
--+    },
--+    {
--+      "src": "decision:D30",
--+      "dst": "project:multitunnel",
--+      "rel": "part_of",
--+      "evidence": "project decision log"
--+    },
--     {
--       "src": "decision:D4",
--       "dst": "project:multitunnel",
--@@ -9239,6 +9471,12 @@
--       "rel": "released_in",
--       "evidence": "2026-11-22T04:16Z | v0.13.1 RELEASED: tag on 4923566 → release run 44983522874 SUCCESS (~4 min, no cold-cache stall); ci ×2 on main (44983512609/44983518575) ALL GREEN first try. Assets verified by download+unzip: multitunnel-v0.13.1-windows-x64.zip 6,511,395 B integrity OK (multitunnel.exe PE32+ console 13.8 MB + multitunnel-tray.exe PE32+ GUI 2.95 MB + README v0.13.1 section + QUICKSTART v0.13.1 note + server scripts + LICENSE + THIRD_PARTY_NOTICES + config.example) + mt-merge-server {x86_64 1,826,272 B, aarch64 1,860,808 B} ELFs. Verified copies staged in sandbox download/ (v0.13.1-verify-*). S20 CLOSED."
--     },
--+    {
--+      "src": "ledger:s21",
--+      "dst": "project:multitunnel",
--+      "rel": "part_of",
--+      "evidence": "project ledger"
--+    },
--     {
--       "src": "ledger:s3",
--       "dst": "project:multitunnel",
--@@ -9449,6 +9687,12 @@
--       "rel": "built_from",
--       "evidence": "git tag v0.13.1"
--     },
--+    {
--+      "src": "release:v0.14.0",
--+      "dst": "commit:e510e43",
--+      "rel": "built_from",
--+      "evidence": "git tag v0.14.0"
--+    },
--     {
--       "src": "release:v0.2.0",
--       "dst": "commit:06881a6",
+-/// S19: the default speed-test URL (mirrors the mt-service serde default
+-/// so a fresh SettingsFile and a freshly-parsed one agree).
+-fn default_speed_test_url() -> String {
+-    "http://speed.cloudflare.com/__down?bytes=25000000".to_string()
+-}
 -
--### Commit: e510e43 - backup(s21-release-prep-2): 2026-11-24T20:08:15Z (Z User, 2026-11-24)
--diff --git a/QUICKSTART.md b/QUICKSTART.md
--index 2662060..7cf41a5 100755
----- a/QUICKSTART.md
--+++ b/QUICKSTART.md
--@@ -98,6 +98,17 @@ needed for the usual per-user system-proxy writes (expect the SmartScreen
-- > DIAGNOSTICS mode that writes one paste-ready support log file
-- > (connection lifecycle + engine output, credentials redacted).
-- >
--+> v0.14: adding a second server and switching to it while connected no
--+> longer kills the tunnel until restart (lingering proxy connections are
--+> force-closed at shutdown and busy listener binds retry briefly);
--+> throughput/stability parity work (TCP_NODELAY on every carrier leg,
--+> coalesced ACKs, batched carrier writes, honest per-carrier RTTs, no
--+> more probe blackouts on the only link); Settings → Network gained a
--+> **QoS FAIR SHARE** toggle — when two sites race for a loaded tunnel,
--+> each gets an equal share instead of one download hogging everything
--+> (flips live, no reconnect); the Status tab's exit-IP and speed-test
--+> cards merged into one compact **Exit & speed** panel.
--+>
-- > v0.13.1 (Copilot code-review round): kill-switch hardening — the
-- > black-hole port is owned BEFORE the proxy points at it, the proxy
-- > hand-off is serialized with disconnect, a failed restore keeps the
+-/// S22: the default upload-test URL (mirrors the mt-service serde
+-/// default, same reason).
+-fn default_speed_test_upload_url() -> String {
+-    "http://speed.cloudflare.com/__up".to_string()
+-}
 -
--### Commit: 1a9849f - backup(s21-release-prep): 2026-11-24T20:08:00Z (Z User, 2026-11-24)
- diff --git a/Cargo.toml b/Cargo.toml
--index e436401..c96f339 100755
-+index c96f339..e386a49 100755
- --- a/Cargo.toml
- +++ b/Cargo.toml
- @@ -18,7 +18,7 @@ members = ["crates/*"]
-  resolver = "2"
-  
-  [workspace.package]
---version = "0.13.1"
--+version = "0.14.0"
-+-version = "0.14.0"
-++version = "0.15.0"
-  
-  [lib]
-  name = "multitunnel"
-+diff --git a/QUICKSTART.md b/QUICKSTART.md
-+index 7cf41a5..94e9118 100755
-+--- a/QUICKSTART.md
-++++ b/QUICKSTART.md
-+@@ -109,6 +109,15 @@ needed for the usual per-user system-proxy writes (expect the SmartScreen
-+ > (flips live, no reconnect); the Status tab's exit-IP and speed-test
-+ > cards merged into one compact **Exit & speed** panel.
-+ >
-++> v0.15: the Exit & speed panel splits into two columns — exit-IP
-++> outputs on the LEFT, speed-test outputs on the RIGHT; after a download
-++> test concludes a **Test upload speed** button appears (result in the
-++> right column); every link shows **which adapter it goes through**
-++> (`via <adapter>`); Settings → Diagnostics' **Open log folder** works
-++> whether the mode is on or off (and a diagnostics toggle persisted ON
-++> writes again after a restart); the app icon fills its tile — the
-++> glyph grew ~24% in the tray.
-++>
-+ > v0.13.1 (Copilot code-review round): kill-switch hardening — the
-+ > black-hole port is owned BEFORE the proxy points at it, the proxy
-+ > hand-off is serialized with disconnect, a failed restore keeps the
- diff --git a/README.md b/README.md
--index beca1eb..580d5db 100755
-+index 580d5db..e3ef43d 100755
- --- a/README.md
- +++ b/README.md
--@@ -374,6 +374,31 @@ GUI, no new dependencies):
--   color-scheme), re-checked every few seconds while the app runs. A
--   survey of the multipath-bonding literature (BLEST, ECF, MP-QUIC…) and
--   how it maps onto the scheduler shipped alongside in `docs/research/`.
--+- **P0 reconnect fix + performance parity work + QoS + merged panel
--+  (v0.14)**: adding a second server and switching to it while connected
--+  no longer kills the tunnel until restart — in-flight proxy connections
--+  are now tracked and force-closed when the drain grace expires (on
--+  Windows their sockets used to pin the 10808/10809 local endpoints and
--+  the next connect landed `ADDRINUSE` → `Failed`), the listeners retry a
--+  busy bind for a few seconds instead of failing instantly, engine
--+  config/log files carry an endpoint-identity hash so two profiles
--+  sharing a tag can no longer overwrite each other's Xray config, and
--+  profiles.json edits are serialized. Throughput/stability parity work:
--+  TCP_NODELAY on every carrier/target leg (Nagle + delayed-ACK used to
--+  stall each 16 KiB bonding frame behind a 40 ms ACK), cumulative ACKs
--+  coalesced (8 frames / 20 ms, flushed by CLOSE and the 1 s heartbeat),
--+  the carrier writer batches co-queued frames into one vectored write,
--+  the send path drops two of its five per-frame copies, PONGs echo on
--+  the carrier they arrived on (per-carrier RTT samples were
--+  cross-contaminated), a probe can no longer quarantine the ONLY link
--+  into a blackout (fail-open dial instead), and the background prober
--+  now uses the user-configured probe target. Settings → Network gained
--+  a **QoS fair share** toggle: connections are grouped by destination
--+  and, when ≥2 groups race for a loaded tunnel, each gets an equal
--+  share — one site/download can no longer dominate all bandwidth
--+  (applies live, no reconnect; a single group or an idle tunnel runs at
--+  full speed). The Status tab's exit-IP and speed-test cards merged into
--+  one compact **Exit & speed** panel with both triggers side by side.
-- - **Server editing, offline probes, kill switch, diagnostics (v0.13)**:
--   every server card gained an **Edit** button — name, remark, address,
--   port, UUID and the REALITY fields (SNI, fingerprint, public key, short
-+@@ -442,6 +442,28 @@ GUI, no new dependencies):
-+   response head independently of the payload limit, and the Edit dialog
-+   rejects port 0 and structurally invalid REALITY fields (empty SNI,
-+   malformed public key / short ID).
-++- **v0.15.0 — the Exit & speed panel reads like a form**: the panel's
-++  two halves are now two COLUMNS — every exit-IP output (address,
-++  location/ISP chips, single-hop verdict) sits on the LEFT and every
-++  speed-test output (download rate, volume/duration chips, per-adapter
-++  spread note) sits on the RIGHT, each under its own small heading.
-++  After a download test concludes, a **Test upload speed** button
-++  appears in the right column: it POSTs a 25 MB payload through the
-++  tunnel (URL configurable in Advanced settings) and reports the send
-++  rate, volume and duration beside the download result. Every link in
-++  the Links card now shows **which adapter it goes through** — a
-++  `via <adapter>` line under the link (parsed from the engine's
-++  `"profile [adapter]"` link names, dedup ordinals stripped, so the
-++  adapter survives even when a long profile tag truncates the name).
-++  Settings → Diagnostics' **Open log folder** works in every state: it
-++  opens the log folder itself (created on demand) instead of the
-++  diagnostics file's parent, so it no longer reports "turn diagnostics
-++  on first" — and a diagnostics toggle persisted ON now starts writing
-++  again after an app restart (the sink used to start only on an
-++  off→on transition). The app icon (window, taskbar, tray, every ICO
-++  size) was regenerated so the glyph fills the tile — the mark grew
-++  ~24% in the tray after a dead gradient band was cropped out of the
-++  source and the margins tightened (93–97% width, 75–77% height fill).
-+ - **Quick Settings on right-click**: the tray menu is built fresh from
-+   live state — Connect/Disconnect (state-reflecting), the server list
-+   (radio-checked on the active profile), Open MultiTunnel and Exit.
-+
-+### Commit: 51e0144 - backup(s22-rig-verified): 2026-11-25T19:10:03Z (Z User, 2026-11-25)
-+diff --git a/crates/mt-app/src/app.rs b/crates/mt-app/src/app.rs
-+index 9f0a41d..f1b0892 100755
-+--- a/crates/mt-app/src/app.rs
-++++ b/crates/mt-app/src/app.rs
-+@@ -815,6 +815,7 @@ impl App {
-+             app.load_settings(),
-+             app.load_adapters(),
-+             app.fetch_diag_path(),
-++            app.fetch_log_dir(),
-+         ];
-+         (app, iced::Task::batch(tasks))
-+     }
-+@@ -908,6 +909,13 @@ impl App {
-+         )
-+     }
-+ 
-++    /// S22: the log directory for the "Open log folder" button (fetched
-++    /// once at boot; the button opens it in every state).
-++    fn fetch_log_dir(&self) -> iced::Task<Message> {
-++        let svc = Arc::clone(&self.service);
-++        iced::Task::perform(async move { svc.log_dir().await }, Message::LogDirLoaded)
-++    }
-++
-+     fn load_adapters(&self) -> iced::Task<Message> {
-+         let svc = Arc::clone(&self.service);
-+         iced::Task::perform(async move { svc.adapters().await }, Message::AdaptersLoaded)
+ impl SettingsFile {
+     fn load(path: &Path) -> Self {
+         let mut file = match std::fs::read_to_string(path) {
+@@ -3110,13 +3098,14 @@ mod tests {
+         let file = SettingsFile {
+             settings: {
+                 // v0.5-era file shape: NO theme_mode key (the S18 legacy
+-                // migration tests build on files like this one) — build
+-                // off the stock view and blank exactly that field.
+-                let mut s = SettingsView::default();
+-                s.theme_mode = String::new();
+-                s.system_proxy = system_proxy;
+-                s.merge_policy = "failover".to_string();
+-                s
++                // migration tests build on files like this one) — the
++                // stock view with exactly that field blanked.
++                SettingsView {
++                    theme_mode: String::new(),
++                    system_proxy,
++                    merge_policy: "failover".to_string(),
++                    ..SettingsView::default()
++                }
+             },
+             rules: Vec::new(),
+             socks_port: 0,
+@@ -4523,16 +4512,15 @@ mod tests {
+         let paths = scratch("settings");
+         let svc = RealService::with(paths.clone(), Arc::new(FakeFactory { echo, fail: false }));
+ 
+-        let view = {
+-            let mut v = SettingsView::default();
+-            v.theme_seed = 0xEA4335;
+-            v.dark_mode = true;
+-            v.theme_mode = "dark".to_string();
+-            v.merge_policy = "lowestlatency".to_string();
+-            v.socks_port = 0; // ephemeral — tests never bind fixed ports
+-            v.http_port = 0;
+-            v.qos_fair = true; // round-trips a non-default value
+-            v
++        let view = SettingsView {
++            theme_seed: 0xEA4335,
++            dark_mode: true,
++            theme_mode: "dark".to_string(),
++            merge_policy: "lowestlatency".to_string(),
++            socks_port: 0, // ephemeral — tests never bind fixed ports
++            http_port: 0,
++            qos_fair: true, // round-trips a non-default value
++            ..SettingsView::default()
+         };
+         svc.set_settings(view.clone()).await.expect("save settings");
+         assert_eq!(svc.settings().await, view);
+@@ -4580,13 +4568,15 @@ mod tests {
+         // Dirty: non-default settings + rules + a profile. The bonded
+         // selection needs a REAL adapter id when the host has one (the
+         // machine-independent parts run either way).
+-        let mut dirty = SettingsView::default();
+-        dirty.theme_seed = 0xEA4335;
+-        dirty.merge_policy = "priority".to_string();
+-        dirty.bond_mode = "stream".to_string();
+-        dirty.kill_switch = true;
+-        dirty.window_w = 1400;
+-        dirty.window_h = 900;
++        let dirty = SettingsView {
++            theme_seed: 0xEA4335,
++            merge_policy: "priority".to_string(),
++            bond_mode: "stream".to_string(),
++            kill_switch: true,
++            window_w: 1400,
++            window_h: 900,
++            ..SettingsView::default()
++        };
+         svc.set_settings(dirty.clone()).await.expect("dirty save");
+         let host_adapter = svc
+             .adapters()
+@@ -4654,9 +4644,11 @@ mod tests {
+         let paths = scratch("winsize");
+         let svc = RealService::with(paths.clone(), Arc::new(FakeFactory { echo, fail: false }));
+ 
+-        let mut custom = SettingsView::default();
+-        custom.window_w = 1400;
+-        custom.window_h = 900;
++        let custom = SettingsView {
++            window_w: 1400,
++            window_h: 900,
++            ..SettingsView::default()
++        };
+         svc.set_settings(custom.clone()).await.expect("save");
+         assert_eq!(svc.settings().await, custom);
+         assert!(!svc.settings().await.window_size_is_default());
+
+### Commit: ab91166 - perf(mt-bond): 1 MiB WAN socket buffers on the merge daemon + research doc (Z User, 2026-11-27)
+diff --git a/Cargo.lock b/Cargo.lock
+index 8b3e2b9..539fbf6 100755
+--- a/Cargo.lock
++++ b/Cargo.lock
+@@ -1982,6 +1982,7 @@ dependencies = [
+  "anyhow",
+  "async-trait",
+  "mt-net",
++ "socket2",
+  "tokio",
+  "tracing",
+ ]
+diff --git a/crates/mt-bond/Cargo.toml b/crates/mt-bond/Cargo.toml
+index 07db106..1de6f62 100755
+--- a/crates/mt-bond/Cargo.toml
++++ b/crates/mt-bond/Cargo.toml
+@@ -20,3 +20,6 @@ tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time"
+ async-trait = "0.1"
+ anyhow = "1"
+ tracing = "0.1"
++# S23: SO_SNDBUF/SO_RCVBUF on the daemon's WAN-facing streams (already a
++# transitive dep — no new code enters the lockfile).
++socket2 = "0.6"
+diff --git a/crates/mt-bond/src/daemon.rs b/crates/mt-bond/src/daemon.rs
+index d7c7700..dc04426 100755
+--- a/crates/mt-bond/src/daemon.rs
++++ b/crates/mt-bond/src/daemon.rs
+@@ -33,6 +33,26 @@ pub const BOND_IDLE_GC: Duration = Duration::from_secs(60);
+ const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
+ /// GC sweep cadence.
+ const GC_INTERVAL: Duration = Duration::from_secs(15);
++/// S23: SO_SNDBUF/SO_RCVBUF for the daemon's WAN-facing streams. 1 MiB
++/// covers the bandwidth-delay product of a ~150 Mbps carrier at ~50 ms
++/// RTT (18.75 MB/s × 50 ms ≈ 0.94 MiB) without double-buffering orders
++/// of magnitude beyond it; the OS default (often 128–256 KiB without
++/// autotuning) caps a single carrier's throughput on high-BDP paths.
++/// Loopback legs (the client side's engine hops) keep OS defaults —
++/// buffers there only cost memory.
++const TUNED_SOCKET_BUFS: usize = 1024 * 1024;
 +
-+### Commit: 6fc0708 - backup(s22-log-folder-fix): 2026-11-25T18:43:12Z (Z User, 2026-11-25)
-+diff --git a/src/real_service.rs b/src/real_service.rs
-+index 7fa6545..8243b99 100755
-+--- a/src/real_service.rs
-++++ b/src/real_service.rs
-+@@ -3388,7 +3388,7 @@ mod tests {
-+                                     }
-+                                 })
-+                                 .unwrap_or(0);
-+-                            drained = head.len() - split as usize;
-++                            drained = head.len() - split;
-+                             break;
-+                         }
-+                     }
++/// S23: apply the WAN-stream socket tuning: TCP_NODELAY (S21) plus
++/// 1 MiB send/receive buffers. Best-effort — every option is advisory
++/// (an OS may clamp or ignore it) and a failure never kills the stream.
++fn tune_wan_stream(stream: &TcpStream) {
++    let _ = stream.set_nodelay(true);
++    let sock = socket2::SockRef::from(stream);
++    // SAFETY-free: socket2 wraps the raw fd; setsockopt is fallible and
++    // its errors are advisory here (documented in TUNED_SOCKET_BUFS).
++    let _ = sock.set_send_buffer_size(TUNED_SOCKET_BUFS);
++    let _ = sock.set_recv_buffer_size(TUNED_SOCKET_BUFS);
++}
  
- ( PROJECT_MEMORY.md's own diff is elided here — the durable
-   sections above are hand-maintained; see harness/memory/LEDGER.md
-diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
-index 2fef1de..4ae4b3a 100755
---- a/harness/memory/LEDGER.md
-+++ b/harness/memory/LEDGER.md
-@@ -219,3 +219,4 @@
- 2026-11-22T04:20Z | s20 | COPILOT CODE REVIEW COMPLETED (the S19 leftover): the cloud Copilot reviewer finally ran from this sandbox via POST /repos/maniesh-NR/MultiTunnel/pulls/1/copilot-code-review — the user's runbook pointed at the standalone @github/copilot CLI, which is installed and network-verified but hard-blocked by the PAT: api.individual.githubcopilot.com answers 401 "Personal Access Token does not have \"Copilot Requests\" permission" (authentic server response, NOT a proxy block; the permission can only be added by editing the PAT in the GitHub web UI — told the user exactly that). The REST code-review endpoint needs only pull-requests:write, which the PAT has. Effort ladder per the fallback instruction: max → quota-rejected ("insufficient remaining quota"), balanced → quota-rejected, LITE → ran (review 6010002263 on 52f1d3c): 12 findings (5 High: step-0 self-heal never cleared sysproxy_set → later no-snapshot restore DISABLES a user's pre-existing proxy; give_up check-and-set unserialized with disconnect → black-hole re-installed AFTER an explicit disconnect; proxy pointed at 127.0.0.1:9 BEFORE the bind → a foreign port-9 owner receives the blocked traffic; black-hole listener stopped even when restore FAILED → stranded blocked proxy, no retry, false "restored" log; diag header wrote user URLs verbatim → credential leak in the paste-ready file; 7 Medium: diag-on-while-connected misses engine debug, failed enable leaves persisted toggle on, exit probe/speed test ride a session that is not the target profile, unbounded pre-\r\n\r\n head buffer, port-0 edit accepted, edited REALITY material unvalidated, temp_link_for pop() = lowest-priority adapter). ALL 12 fixed in 4923566: step-0 clears ownership on successful restore (keeps flag+listener on failure for retry); new Core::proxy_mtx serializes every proxy hand-off (give_up/disconnect×2/toggle-off/step-0/arming; the black-hole BIND hoisted outside the lock so no std lock crosses an await — spawn_blackhole split into bind_blackhole+engage_blackhole); bind-BEFORE-set with no-set-on-bind-failure (stale refused entry keeps blocking, nothing routes to a foreign listener); restore-first release on all paths with ERROR log + retry on failure; redact_url() (scheme://host:port/path, userinfo/query/fragment stripped, invalid → placeholder); diag-on-live reconnects (engine level is spawn-time-only); failed enable reverts the persisted toggle; Session.profile_id pins the live-session match for BOTH probes; SPEED_TEST_MAX_HEAD 64 KiB independent cap; port-0 + REALITY structural validation (vless parser rules, is_base64url now crate-visible; fingerprint deliberately unchecked = import parity); first-link selection (temp_link_for AND test_latency). ROUND 2 (lite, on 4923566): CLEAN PASS — review 6010239912, zero inline comments. PR #1: 12 per-finding maintainer replies + summary, CLOSED. Transcript: harness/memory/agent-reports/s20-copilot-review-transcript.md. TESTS 474/0 (+8: connect_selfheal_clears_stale_proxy_ownership, update_profile_rejects_port_zero_and_invalid_reality, diag_header_redacts_url_credentials, speed_test_rejects_a_header_flood, probes_fall_back_when_active_profile_switched, temp_link_uses_the_top_priority_first_link, diagnostics_enable_failure_reverts_the_toggle, diagnostics_enable_while_connected_reconnects). Gates: fmt clean; clippy host + windows-gnu -D warnings clean; vendored iced 6/0; mt-app --smoke 0; rig launch+alive+render-healthy (82% non-white, 1738 colors — zero GUI code changed this round); zigbuild PE32+ ×4 + ELF ×2. SANDBOX RESTORE NOTES: rustc 1.100.0 reinstalled (rustup minimal), tools/pypkgs ziglang wheel was INCOMPLETE (libc only musl/wasi — stddef.h compile failure; reinstalled 0.14.1 from PyPI), python-xlib reinstalled, /tmp/xkblibs re-extracted from the my-project root debs; DISK: /tmp/mt-target debug trees wiped twice (ENOSPC at 94% — stage release binaries before wiping, per the S19 lesson).
- 2026-11-22T04:16Z | s20 | v0.13.1 RELEASED: tag on 4923566 → release run 44983522874 SUCCESS (~4 min, no cold-cache stall); ci ×2 on main (44983512609/44983518575) ALL GREEN first try. Assets verified by download+unzip: multitunnel-v0.13.1-windows-x64.zip 6,511,395 B integrity OK (multitunnel.exe PE32+ console 13.8 MB + multitunnel-tray.exe PE32+ GUI 2.95 MB + README v0.13.1 section + QUICKSTART v0.13.1 note + server scripts + LICENSE + THIRD_PARTY_NOTICES + config.example) + mt-merge-server {x86_64 1,826,272 B, aarch64 1,860,808 B} ELFs. Verified copies staged in sandbox download/ (v0.13.1-verify-*). S20 CLOSED.
- 2026-11-24T21:00Z | s21 | IMPLEMENTED (four asks: P0 bug, P1 perf parity, QoS toggle, merged panel; v0.14.0). Sandbox recycled AGAIN (rustup/zigbuild/xkblibs gone; tools/pypkgs ziglang wheel INCOMPLETE again — no lib/libc/include → ring stddef.h failure mid-gates; reinstalled 0.14.1 from PyPI per the S20 note; local clone was STALE at s13 with a dirty PROJECT_MEMORY.md — remote authoritative at s20/v0.13.1, snapshot→/tmp then fast-forward). P0 (user: "stops working when a second connection is added, restart required"): 3 Explore agents mapped the paths; H1 = mid-traffic reconnect re-binds 10808/10809 while untracked in-flight connection tasks pin their sockets (no SO_REUSEADDR anywhere) → bind WSAEADDRINUSE → Failed; PROVEN platform-split: Linux tolerates a listen re-bind over non-listening sockets (std rebind probe in scripts/rebind_probe.rs) while Windows does NOT — the sandbox cannot reproduce the bind refusal, so the regression test asserts the platform-independent observable (lingering client sees EOF after the switch) and pre-fix it FAILS (8.0 s timeout) / post-fix passes. FIX: Shared.conns tracks every connection task's AbortHandle (pruned at 4096 push), run() force-closes whatever the 3 s drain could not reap; AbortPumpsOnDrop aborts the DETACHED pump children too (dropping their JoinHandles leaves them running — found when the first EOF assertion failed; pumps held the socket halves forever); bind_with_retry (5 s budget, 250 ms interval; BIND_WAIT 2→8 s). H2: engine config/log stems now carry a deterministic FNV-1a hash of the FULL endpoint identity (engine_file_stem; same profile keeps its file across reconnects, same-tag different-endpoint profiles never share engine-<tag>.json again — temp probe/speed-test tunnels could rewrite the LIVE engine's config → phantom-live respawn budget exhaustion → DEGRADED while GUI showed Connected). H3: Core.store_mtx serializes profiles.json read-modify-write (import/remove/set_active/update; no await inside the critical sections). H1+H3 tests: picker_switch_with_lingering_client_reconnects, shutdown_force_closes_lingering_connections_and_frees_the_port (mt-net level: EOF + immediate re-bind). P1 PERF (user: v2rayN faster on ONE adapter): (1) TCP_NODELAY on all four legs (client→engine loopback socks5.rs establish; daemon accepted carrier; daemon dial_target ×2 — Nagle+delayed-ACK stalled each 16 KiB frame's tail segment ~40 ms on Linux peers ≈ Mbps-per-carrier stop-and-wait); (2) quarantine can no longer BLACKHOLE the sole link (pick_excluding last-resort fallback over quarantined/down links when no healthy candidate; the dial decides, fail-open; retry-exclusion contract kept; module doc + contract test updated, new sole-link test); (3) production background prober now uses the user-configured probe_target (was hardcoded 1.1.1.1:443); (4) ACK coalescing (ACK_COALESCE_FRAMES=8 / 20 ms window, per-session AckCoalescer, flushed by CLOSE handling + the 1 s PING heartbeat → ≤1 s ACK latency; cumulative semantics unchanged; guard scoped before the await for Send); (5) PONG echoes on the RECEIVING carrier (schedule_bytes re-picked — rtt_ms was cross-contaminated across carriers feeding ECF/Balanced wrong data); (6) send-path copies 5→3 per frame (encode_session_borrowed encodes header+payload in one pass from the slice; send_data sends slices; wire-format identity test); (7) carrier writer batches up to 16 co-queued frames into ONE write_vectored (write_all_vectored with partial-write advance). O — QoS FAIR SHARE toggle (Settings → Network, qos_fair serde-default bool): new mt-net/src/qos.rs QosScheduler groups flows by TARGET, 1 s tick refills buckets at aggregate/groups ONLY under real contention (≥2 groups AND aggregate ≥ 2 MiB/s floor; single group/idle = unlimited), pump read gate blocks until budget (20 ms retry), ALWAYS attached at connect so set_settings flips it LIVE (no reconnect — scheduler flag atomic; log line emitted); 5 scheduler unit tests + live-flip persistence test + GUI draft test. P — merged Exit & speed panel: one Status card (shared header, both triggers side-by-side Fill-width, exit row + verdict + speed row + hint stacked compactly; all messages/state unchanged; D24-safe inside the page scroll), rig-verified sf=1.0 (VLM + knob-pixel-diff: 1957 px changed, knob center 39→55) + sf=1.5. TESTS 490/0 (+16: mt-engine stem ×4, merge sole-link ×1, mt-net shutdown-force-close ×1 + qos ×5, frame wire-identity ×1, root P0 ×1 + qos ×1, mt-app qos ×1, mt-service serde ×1). GATES: fmt clean; clippy host + windows-gnu -D warnings clean (zig shims + fresh ziglang wheel); cargo test --workspace 490/0; vendored iced 6/0; mt-app --smoke 0; rig sf=1.0+1.5; zigbuild PE32+ ×4 + ELF ×2. RELEASE: v0.14.0 tag → run 45477702090 — attempt-1 ubuntu leg carrier_death 'no live bond carrier' (documented S11/S18/S19 pre-existing runner race, signature exactly as backlogged), attempt-2 windows leg domain_targets 60 s deadline, attempt-3 SUCCESS; same-commit ci.yml runs green on BOTH OS legs throughout + local 2-core taskset stress 8/8; all four logs archived under harness/memory/agent-reports/s21-ci-*.log. Assets download-verified: multitunnel-v0.14.0-windows-x64.zip 6,540,352 B (11-entry manifest identical shape to v0.13.1: multitunnel.exe + tray + docs + server scripts) + mt-merge-server x86_64 1,845,824 B + aarch64 1,864,608 B. PARKED (needs user input or new scope): single-instance guard (H4 — two app instances race the sysproxy snapshot; needs a CreateMutexW design decision), BufReader-style carrier read batching (2 read_exact syscalls/frame), MAX_PAYLOAD 64 KiB protocol bump (needs a coordinated mt-merge-server update on the user's VM), carrier-lifecycle race root-cause (S19 backlog — this session hit it 2/3 release legs; reruns shipped), upstream-iced migration, subscription auto-update, vmess validation, profiles.json locking (partially addressed by store_mtx in-process; cross-process locking still open), engine log rotation, listener max_conns. Copilot CLI still blocked by PAT (S20 finding unchanged — cloud route works, needs only pull-requests:write).
-+2026-11-26T03:30Z | s22 | IMPLEMENTED (five user asks; v0.15.0). Sandbox recycled AGAIN (rustup 1.100.0 + ziglang 0.14.1 + cargo-zigbuild 0.24.1 + xkblibs + python-xlib rebuilt per the ritual; the pypkgs ziglang wheel was INCOMPLETE AGAIN mid-gates — ring stddef.h — reinstalled from PyPI, S20/S21 pattern). Local clone was stale at s13 with a dirty (stale script-generated) PROJECT_MEMORY.md — discarded, fast-forwarded to origin (S21 had already shipped v0.14.0 with the P0/P1/QoS/merged-panel brief; the FIVE asks below landed after that close, so they were all pending). (1) TWO-COLUMN Exit & speed split (user: "exit ip outputs on the left side and the speed test outputs on the right side"): screens.rs status() now builds exit_out/speed_out columns (headings "Exit IP"/"Speed test", quiet placeholders while idle) pushed as row![exit_out, speed_out] — verdict card + IP/chips rows stacked LEFT, Mbps + chips + note RIGHT; pure view change, D24-safe in the page scrollable. (2) UPLOAD SPEED TEST (user: "After the speed test has concluded, show another button to test upload speed"): mt-service contract + SettingsView.speed_test_upload_url (serde default http://speed.cloudflare.com/__up, D8 migration), RealService::speed_test_upload POSTs a fixed 25 MiB body through the same tunnel selection (S20 profile_id rule), clock after connect, 64 KiB chunks, same 30 s/5 s stall/64 MiB caps, partial-measurement-stands semantics, status guard ONLY on clean completion (a truncated body may never get a response); SpeedTestUpload/SpeedUploadTested messages + speed_up/speed_up_testing state + the button appears ONLY after the download concluded (gate app.speed.is_some() && !speed_testing), mutual exclusion across all three triggers; upload URL field in Settings → Advanced; redact_url in the diag header; MockService simulated. Tests: temp-tunnel measurement + 403-rejection (spawn_upload_server fixture drains Content-Length then answers), mock message flow, mt-service serde migration/roundtrip. (3) PER-LINK ADAPTER (user: "Under the links, show which adapter each link is going through"): mt-service::adapter_of_link_name parses the engine's "<tag> [adapter]" grammar (dedup " #N" stripped for display; rate-sampler's tricky cases pinned: "Wi-Fi Direct"/"Ethernet 2" verbatim), LinkStatus.adapter_name (serde default) populated in link_status_of (per-connection) + carrier_snapshot map (stream mode) + mock seeds; the Links card renders a dim `via <adapter>` line UNDER each link row — survives util::short's 32-char truncation that previously hid the [adapter] suffix. (4) OPEN LOG FOLDER FIX (user: "clicking the 'Open log folder button' does not open the log folder"): ROOT CAUSE was the diagnostics-file gate — the button resolved diag_path (None unless the sink started THIS process lifetime) so after every restart it printed "turn diagnostics on first"; FIXED by a new MtService::log_dir() (always resolvable), the handler create_dir_all's + opens the folder itself via open_folder_command (explorer/open/xdg-open per platform, spawn-result reported, never silent — Settings status + Logs-tab WARN), fetched at boot (fetch_log_dir in BOTH init paths — the iced init batch was the second gap the rig caught); ADJACENT ROOT CAUSE fixed: a diagnostics toggle persisted ON now re-enables the sink AT CONSTRUCTION (RealService::with) — the toggle used to lie after a restart; diagnostics_persisted_on_starts_at_construction pins it. (5) ICON FILL (user: "have the icon utilize as much of the available space as possible"): make_logo_assets.py re-crops emblem+lockup to their ALPHA bboxes after key-out (a dead ~71px left gradient band made the mark render off-center at 62% height fill), margins 0.04→0.02 (16/32/48/24/64) + 0.10→0.03 (256 lockup); measured fills 93.8-96.9%w × 75.0-76.6%h (was 75-81%w × 61-64%h — glyph ~24% larger in the tray's 32px tile); all test-pinned invariants hold (4-entry ICOs 16/32/48/256, biWidth=32/biHeight=64, logo_64=16384 B, logo_24=2304 B, deterministic re-runs, banner/logo/color_wheel untouched). RIG (sf=1.0 + 1.5, mock): panel split + placeholders, download result right-column, upload button appears → "testing upload…" → result rows, exit probe left-column verdict card, via-adapter lines, Open-log-folder click → folder created + honest per-platform failure surfaced (no xdg-open in bare Xvfb — the Windows path spawns explorer; spawn success unit-pinned via open_folder_command_targets_the_os_file_manager). LESSONS: (a) gui_drive click on a button that APPEARED after a reflow needs a pointer-jiggle first (no motion event → iced hover stale → first click no-op); (b) content appended BELOW a scrollable's initial viewport looks like a stale frame in shots — damage-instrumented the vendored compositor (reverted byte-identical) to prove the renderer was right: the rows were below the fold, scroll at a y INSIDE the viewport (400 was above it, 500 works). TESTS 497/0 (+7). GATES: fmt clean; clippy host + windows-gnu -D warnings clean (zig shims + fresh wheel); vendored iced 6/0; mt-app --smoke 0; zigbuild PE32+ x4 (multitunnel 19.3MB, mt-app 14.9MB, tray 10.2MB, merge-server 2.4MB) + ELF x2 (x86_64 1.25MB, aarch64 1.23MB); version 0.15.0; README v0.15 section + QUICKSTART note. DISK: 9.9G rootfs hit 99% mid-gates — rm -rf target/debug + target/x86_64-pc-windows-gnu between phases (S15 sequence discipline).
-diff --git a/harness/memory/agent-reports/s22-ci-ubuntu-flake.log b/harness/memory/agent-reports/s22-ci-ubuntu-flake.log
-new file mode 100644
-index 0000000..4631430
---- /dev/null
-+++ b/harness/memory/agent-reports/s22-ci-ubuntu-flake.log
-@@ -0,0 +1,571 @@
-+﻿2026-11-25T19:24:10.8664974Z Current runner version: '2.338.2'
-+2026-11-25T19:24:10.8689301Z ##[group]Runner Image Provisioner
-+2026-11-25T19:24:10.8690175Z Hosted Compute Agent
-+2026-11-25T19:24:10.8690790Z Version: 20261111.635
-+2026-11-25T19:24:10.8691569Z Commit: b8a594ea514ff44cadb5342986c68b1eac6e22ed
-+2026-11-25T19:24:10.8692300Z Build Date: 2026-11-11T01:49:01Z
-+2026-11-25T19:24:10.8693012Z Worker ID: {1d27284e-f9f5-4689-9010-fc5dae2ce851}
-+2026-11-25T19:24:10.8694089Z Region: centralus
-+2026-11-25T19:24:10.8694659Z Cloud: Azure
-+2026-11-25T19:24:10.8695242Z ##[endgroup]
-+2026-11-25T19:24:10.8696597Z ##[group]Operating System
-+2026-11-25T19:24:10.8697219Z Ubuntu
-+2026-11-25T19:24:10.8697746Z 26.04.1
-+2026-11-25T19:24:10.8698284Z LTS
-+2026-11-25T19:24:10.8698845Z ##[endgroup]
-+2026-11-25T19:24:10.8699387Z ##[group]Runner Image
-+2026-11-25T19:24:10.8699979Z Image: ubuntu-26.04
-+2026-11-25T19:24:10.8700557Z Version: 20261116.202.1
-+2026-11-25T19:24:10.8701833Z Included Software: https://github.com/actions/runner-images/blob/ubuntu26/20261116.202/images/ubuntu/Ubuntu2604-Readme.md
-+2026-11-25T19:24:10.8703565Z Image Release: https://github.com/actions/runner-images/releases/tag/ubuntu26%2F20261116.202
-+2026-11-25T19:24:10.8704533Z ##[endgroup]
-+2026-11-25T19:24:10.8705761Z ##[group]GITHUB_TOKEN Permissions
-+2026-11-25T19:24:10.8707901Z Contents: read
-+2026-11-25T19:24:10.8708552Z Metadata: read
-+2026-11-25T19:24:10.8709114Z Packages: read
-+2026-11-25T19:24:10.8710123Z ##[endgroup]
-+2026-11-25T19:24:10.8712555Z Secret source: Actions
-+2026-11-25T19:24:10.8713782Z Cache mode: write
-+2026-11-25T19:24:10.8714655Z Prepare workflow directory
-+2026-11-25T19:24:10.9053531Z Prepare all required actions
-+2026-11-25T19:24:10.9102952Z Getting action download info
-+2026-11-25T19:24:11.2658847Z Download action repository 'actions/checkout@v4' (SHA:11d5960a326750d5838078e36cf38b85af677262)
-+2026-11-25T19:24:11.3636703Z Download action repository 'dtolnay/rust-toolchain@stable' (SHA:39008827e6b4319a349e9a42dfc90579dd25f1ca)
-+2026-11-25T19:24:11.5168440Z Download action repository 'Swatinem/rust-cache@v2' (SHA:d1527c8f67ef9af357290475d4af4b0fab8f12e0)
-+2026-11-25T19:24:12.1052902Z Complete job name: test (ubuntu-latest)
-+2026-11-25T19:24:12.1901773Z ##[group]Run actions/checkout@v4
-+2026-11-25T19:24:12.1902753Z with:
-+2026-11-25T19:24:12.1903794Z   repository: maniesh-NR/MultiTunnel
-+2026-11-25T19:24:12.1908822Z   token: ***
-+2026-11-25T19:24:12.1909383Z   ssh-strict: true
-+2026-11-25T19:24:12.1909977Z   ssh-user: git
-+2026-11-25T19:24:12.1910695Z   persist-credentials: true
-+2026-11-25T19:24:12.1911312Z   clean: true
-+2026-11-25T19:24:12.1911937Z   sparse-checkout-cone-mode: true
-+2026-11-25T19:24:12.1912619Z   fetch-depth: 1
-+2026-11-25T19:24:12.1913587Z   fetch-tags: false
-+2026-11-25T19:24:12.1914215Z   show-progress: true
-+2026-11-25T19:24:12.1914780Z   lfs: false
-+2026-11-25T19:24:12.1915408Z   submodules: false
-+2026-11-25T19:24:12.1916011Z   set-safe-directory: true
-+2026-11-25T19:24:12.1916663Z   allow-unsafe-pr-checkout: false
-+2026-11-25T19:24:12.1917612Z ##[endgroup]
-+2026-11-25T19:24:12.4787343Z Syncing repository: maniesh-NR/MultiTunnel
-+2026-11-25T19:24:12.4791866Z ##[group]Getting Git version info
-+2026-11-25T19:24:12.4794161Z Working directory is '/home/runner/work/MultiTunnel/MultiTunnel'
-+2026-11-25T19:24:12.4797575Z [command]/usr/bin/git version
-+2026-11-25T19:24:12.4834128Z git version 2.56.1
-+2026-11-25T19:24:12.4855906Z ##[endgroup]
-+2026-11-25T19:24:12.4867693Z Temporarily overriding HOME='/home/runner/work/_temp/ab98ff89-ea3e-4d9d-a40f-c9b9e32eecc4' before making global git config changes
-+2026-11-25T19:24:12.4872902Z Adding repository directory to the temporary git global config as a safe directory
-+2026-11-25T19:24:12.4878355Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/MultiTunnel/MultiTunnel
-+2026-11-25T19:24:12.4939104Z Deleting the contents of '/home/runner/work/MultiTunnel/MultiTunnel'
-+2026-11-25T19:24:12.4943878Z ##[group]Initializing the repository
-+2026-11-25T19:24:12.4946984Z [command]/usr/bin/git init /home/runner/work/MultiTunnel/MultiTunnel
-+2026-11-25T19:24:12.5071400Z hint: Using 'master' as the name for the initial branch. This default branch name
-+2026-11-25T19:24:12.5075061Z hint: will change to "main" in Git 3.0. To configure the initial branch name
-+2026-11-25T19:24:12.5077991Z hint: to use in all of your new repositories, which will suppress this warning,
-+2026-11-25T19:24:12.5080448Z hint: call:
-+2026-11-25T19:24:12.5082095Z hint:
-+2026-11-25T19:24:12.5084131Z hint: 	git config --global init.defaultBranch <name>
-+2026-11-25T19:24:12.5086410Z hint:
-+2026-11-25T19:24:12.5088249Z hint: Names commonly chosen instead of 'master' are 'main', 'trunk' and
-+2026-11-25T19:24:12.5090944Z hint: 'development'. The just-created branch can be renamed via this command:
-+2026-11-25T19:24:12.5093493Z hint:
-+2026-11-25T19:24:12.5094930Z hint: 	git branch -m <name>
-+2026-11-25T19:24:12.5096391Z hint:
-+2026-11-25T19:24:12.5098248Z hint: Disable this message with "git config set advice.defaultBranchName false"
-+2026-11-25T19:24:12.5101587Z Initialized empty Git repository in /home/runner/work/MultiTunnel/MultiTunnel/.git/
-+2026-11-25T19:24:12.5109351Z [command]/usr/bin/git remote add origin https://github.com/maniesh-NR/MultiTunnel
-+2026-11-25T19:24:12.5158198Z ##[endgroup]
-+2026-11-25T19:24:12.5160774Z ##[group]Disabling automatic garbage collection
-+2026-11-25T19:24:12.5162862Z [command]/usr/bin/git config --local gc.auto 0
-+2026-11-25T19:24:12.5201322Z ##[endgroup]
-+2026-11-25T19:24:12.5203925Z ##[group]Setting up auth
-+2026-11-25T19:24:12.5208375Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
-+2026-11-25T19:24:12.5245543Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
-+2026-11-25T19:24:12.5687389Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
-+2026-11-25T19:24:12.5724870Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
-+2026-11-25T19:24:12.5956525Z [command]/usr/bin/git config --local --name-only --get-regexp ^includeIf\.gitdir:
-+2026-11-25T19:24:12.5988715Z [command]/usr/bin/git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url
-+2026-11-25T19:24:12.6217709Z [command]/usr/bin/git config --local http.https://github.com/.extraheader AUTHORIZATION: basic ***
-+2026-11-25T19:24:12.6261463Z ##[endgroup]
-+2026-11-25T19:24:12.6264648Z ##[group]Fetching the repository
-+2026-11-25T19:24:12.6275091Z [command]/usr/bin/git -c protocol.version=2 fetch --no-tags --prune --no-recurse-submodules --depth=1 origin +21fd59f9adb3f953247fe2f0d204d95a3d83acbf:refs/remotes/origin/main
-+2026-11-25T19:24:13.2486494Z From https://github.com/maniesh-NR/MultiTunnel
-+2026-11-25T19:24:13.2490201Z  * [new ref]         21fd59f9adb3f953247fe2f0d204d95a3d83acbf -> origin/main
-+2026-11-25T19:24:13.2498049Z ##[endgroup]
-+2026-11-25T19:24:13.2501392Z ##[group]Determining the checkout info
-+2026-11-25T19:24:13.2505004Z ##[endgroup]
-+2026-11-25T19:24:13.2507401Z [command]/usr/bin/git sparse-checkout disable
-+2026-11-25T19:24:13.2563490Z [command]/usr/bin/git config --local --unset-all extensions.worktreeConfig
-+2026-11-25T19:24:13.2595722Z ##[group]Checking out the ref
-+2026-11-25T19:24:13.2598597Z [command]/usr/bin/git checkout --progress --force -B main refs/remotes/origin/main
-+2026-11-25T19:24:13.2990070Z Switched to a new branch 'main'
-+2026-11-25T19:24:13.2995384Z branch 'main' set up to track 'origin/main'.
-+2026-11-25T19:24:13.3003919Z ##[endgroup]
-+2026-11-25T19:24:13.3050174Z [command]/usr/bin/git log -1 --format=%H
-+2026-11-25T19:24:13.3075049Z 21fd59f9adb3f953247fe2f0d204d95a3d83acbf
-+2026-11-25T19:24:13.3731750Z ##[group]Run dtolnay/rust-toolchain@stable
-+2026-11-25T19:24:13.3733079Z with:
-+2026-11-25T19:24:13.3734303Z   toolchain: stable
-+2026-11-25T19:24:13.3735272Z ##[endgroup]
-+2026-11-25T19:24:13.3845660Z ##[start-action display=Parse toolchain version;id=__dtolnay_rust-toolchain.parse]
-+2026-11-25T19:24:13.3938883Z ##[group]Run if [[ -z $toolchain ]]; then
-+2026-11-25T19:24:13.3940409Z [36;1mif [[ -z $toolchain ]]; then[0m
-+2026-11-25T19:24:13.3943080Z [36;1m  # GitHub does not enforce `required: true` inputs itself. https://github.com/actions/runner/issues/1070[0m
-+2026-11-25T19:24:13.3945491Z [36;1m  echo "'toolchain' is a required input" >&2[0m
-+2026-11-25T19:24:13.3946832Z [36;1m  exit 1[0m
-+2026-11-25T19:24:13.3948289Z [36;1melif [[ $toolchain =~ ^stable' '[0-9]+' '(year|month|week|day)s?' 'ago$ ]]; then[0m
-+2026-11-25T19:24:13.3950006Z [36;1m  if [[ Linux == macOS ]]; then[0m
-+2026-11-25T19:24:13.3952275Z [36;1m    echo "toolchain=1.$((($(date -v-$(sed 's/stable \([0-9]*\) \(.\).*/\1\2/' <<< $toolchain) +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
-+2026-11-25T19:24:13.3954498Z [36;1m  else[0m
-+2026-11-25T19:24:13.3956217Z [36;1m    echo "toolchain=1.$((($(date --date "${toolchain#stable }" +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
-+2026-11-25T19:24:13.3958145Z [36;1m  fi[0m
-+2026-11-25T19:24:13.3959511Z [36;1melif [[ $toolchain =~ ^stable' 'minus' '[0-9]+' 'releases?$ ]]; then[0m
-+2026-11-25T19:24:13.3961703Z [36;1m  echo "toolchain=1.$((($(date +%s)/60/60/24-16569)/7/6-${toolchain//[^0-9]/}))" >> $GITHUB_OUTPUT[0m
-+2026-11-25T19:24:13.3963705Z [36;1melif [[ $toolchain =~ ^1\.[0-9]+$ ]]; then[0m
-+2026-11-25T19:24:13.3965892Z [36;1m  echo "toolchain=1.$((i=${toolchain#1.}, c=($(date +%s)/60/60/24-16569)/7/6, i+9*i*(10*i<=c)+90*i*(100*i<=c)))" >> $GITHUB_OUTPUT[0m
-+2026-11-25T19:24:13.3967931Z [36;1melse[0m
-+2026-11-25T19:24:13.3968883Z [36;1m  delimiter=EOF[0m
-+2026-11-25T19:24:13.3970347Z [36;1m  while [[ $toolchain == *$delimiter* ]]; do delimiter+=_; done[0m
-+2026-11-25T19:24:13.3972492Z [36;1m  printf 'toolchain<<%s\n%s\n%s\n' "$delimiter" "$toolchain" "$delimiter" >> $GITHUB_OUTPUT[0m
-+2026-11-25T19:24:13.3974432Z [36;1mfi[0m
-+2026-11-25T19:24:13.4006711Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:13.4008244Z env:
-+2026-11-25T19:24:13.4009148Z   toolchain: stable
-+2026-11-25T19:24:13.4010178Z ##[endgroup]
-+2026-11-25T19:24:13.4144313Z ##[end-action id=__dtolnay_rust-toolchain.parse;outcome=success;conclusion=success;duration_ms=29]
-+2026-11-25T19:24:13.4154242Z ##[start-action display=Set $CARGO_HOME;id=__dtolnay_rust-toolchain.__run]
-+2026-11-25T19:24:13.4216700Z ##[group]Run echo CARGO_HOME=${CARGO_HOME:-"$HOME/.cargo"} >> $GITHUB_ENV
-+2026-11-25T19:24:13.4218694Z [36;1mecho CARGO_HOME=${CARGO_HOME:-"$HOME/.cargo"} >> $GITHUB_ENV[0m
-+2026-11-25T19:24:13.4246299Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:13.4247737Z ##[endgroup]
-+2026-11-25T19:24:13.4324895Z ##[end-action id=__dtolnay_rust-toolchain.__run;outcome=success;conclusion=success;duration_ms=16]
-+2026-11-25T19:24:13.4332139Z ##[start-action display=Install rustup if needed;id=__dtolnay_rust-toolchain.__run_2]
-+2026-11-25T19:24:13.4391950Z ##[group]Run if ! command -v rustup &>/dev/null; then
-+2026-11-25T19:24:13.4393739Z [36;1mif ! command -v rustup &>/dev/null; then[0m
-+2026-11-25T19:24:13.4396630Z [36;1m  curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused --location --silent --show-error --fail https://sh.rustup.rs | sh -s -- --default-toolchain none -y[0m
-+2026-11-25T19:24:13.4399450Z [36;1m  echo "$CARGO_HOME/bin" >> $GITHUB_PATH[0m
-+2026-11-25T19:24:13.4400695Z [36;1mfi[0m
-+2026-11-25T19:24:13.4425230Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:13.4426579Z env:
-+2026-11-25T19:24:13.4427800Z   CARGO_HOME: /home/runner/.cargo
-+2026-11-25T19:24:13.4428900Z ##[endgroup]
-+2026-11-25T19:24:13.4502763Z ##[end-action id=__dtolnay_rust-toolchain.__run_2;outcome=success;conclusion=success;duration_ms=16]
-+2026-11-25T19:24:13.4510320Z ##[start-action display=Install rustup if needed on windows;id=__dtolnay_rust-toolchain.__run_3]
-+2026-11-25T19:24:13.4527212Z ##[end-action id=__dtolnay_rust-toolchain.__run_3;outcome=skipped;conclusion=skipped;duration_ms=0]
-+2026-11-25T19:24:13.4548762Z ##[start-action display=rustup toolchain install stable;id=__dtolnay_rust-toolchain.__run_4]
-+2026-11-25T19:24:13.4596587Z ##[group]Run set -f  # split targets and components on commas and whitespace, without glob expansion
-+2026-11-25T19:24:13.4599145Z [36;1mset -f  # split targets and components on commas and whitespace, without glob expansion[0m
-+2026-11-25T19:24:13.4600905Z [36;1mflags=()[0m
-+2026-11-25T19:24:13.4602158Z [36;1mfor t in ${targets//,/ }; do flags+=(--target "$t"); done[0m
-+2026-11-25T19:24:13.4603991Z [36;1mfor c in ${components//,/ }; do flags+=(--component "$c"); done[0m
-+2026-11-25T19:24:13.4605996Z [36;1mif [[ $toolchain == nightly && -n $components ]]; then flags+=(--allow-downgrade); fi[0m
-+2026-11-25T19:24:13.4607744Z [36;1mlog=$(mktemp)[0m
-+2026-11-25T19:24:13.4608791Z [36;1mtrap 'rm -f "$log"' EXIT[0m
-+2026-11-25T19:24:13.4609975Z [36;1mfor attempt in 1 2 3 4 5; do[0m
-+2026-11-25T19:24:13.4612152Z [36;1m  if rustup toolchain install "$toolchain" "${flags[@]}" --profile minimal --no-self-update --force-non-host 2>&1 | tee "$log"; then[0m
-+2026-11-25T19:24:13.4614582Z [36;1m    break[0m
-+2026-11-25T19:24:13.4615533Z [36;1m  else[0m
-+2026-11-25T19:24:13.4616465Z [36;1m    status=$?[0m
-+2026-11-25T19:24:13.4617478Z [36;1m  fi[0m
-+2026-11-25T19:24:13.4618706Z [36;1m  if [[ $attempt == 5 ]] || ! grep -Fq 'checksum failed for ' "$log" \[0m
-+2026-11-25T19:24:13.4620991Z [36;1m      || ! grep -Fq 'this is likely due to an ongoing update of the official release server' "$log"; then[0m
-+2026-11-25T19:24:13.4622878Z [36;1m    exit "$status"[0m
-+2026-11-25T19:24:13.4624098Z [36;1m  fi[0m
-+2026-11-25T19:24:13.4625628Z [36;1m  echo "Retrying rustup toolchain install in 30 seconds (retry $attempt of 4)" >&2[0m
-+2026-11-25T19:24:13.4627365Z [36;1m  sleep 30[0m
-+2026-11-25T19:24:13.4628340Z [36;1mdone[0m
-+2026-11-25T19:24:13.4655225Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:13.4656623Z env:
-+2026-11-25T19:24:13.4657538Z   CARGO_HOME: /home/runner/.cargo
-+2026-11-25T19:24:13.4658680Z   toolchain: stable
-+2026-11-25T19:24:13.4659623Z   targets: 
-+2026-11-25T19:24:13.4660516Z   components: 
-+2026-11-25T19:24:13.4661655Z   RUSTUP_PERMIT_COPY_RENAME: 1
-+2026-11-25T19:24:13.4662708Z ##[endgroup]
-+2026-11-25T19:24:13.7968100Z info: syncing channel updates for stable-x86_64-unknown-linux-gnu
-+2026-11-25T19:24:14.0676903Z 
-+2026-11-25T19:24:14.0772163Z   stable-x86_64-unknown-linux-gnu unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
-+2026-11-25T19:24:14.0774521Z 
-+2026-11-25T19:24:14.0827265Z ##[end-action id=__dtolnay_rust-toolchain.__run_4;outcome=success;conclusion=success;duration_ms=627]
-+2026-11-25T19:24:14.0841357Z ##[start-action display=rustup default stable;id=__dtolnay_rust-toolchain.__run_5]
-+2026-11-25T19:24:14.0891848Z ##[group]Run rustup default "$toolchain" --force-non-host
-+2026-11-25T19:24:14.0893532Z [36;1mrustup default "$toolchain" --force-non-host[0m
-+2026-11-25T19:24:14.0923254Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:14.0924769Z env:
-+2026-11-25T19:24:14.0925708Z   CARGO_HOME: /home/runner/.cargo
-+2026-11-25T19:24:14.0926827Z   toolchain: stable
-+2026-11-25T19:24:14.0927789Z ##[endgroup]
-+2026-11-25T19:24:14.1033807Z info: using existing install for stable-x86_64-unknown-linux-gnu
-+2026-11-25T19:24:14.1074127Z info: default toolchain set to stable-x86_64-unknown-linux-gnu
-+2026-11-25T19:24:14.1075230Z 
-+2026-11-25T19:24:14.1147111Z   stable-x86_64-unknown-linux-gnu unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
-+2026-11-25T19:24:14.1148657Z 
-+2026-11-25T19:24:14.1169862Z ##[end-action id=__dtolnay_rust-toolchain.__run_5;outcome=success;conclusion=success;duration_ms=32]
-+2026-11-25T19:24:14.1179152Z ##[start-action display=Create cachekey;id=__dtolnay_rust-toolchain.rustc-version]
-+2026-11-25T19:24:14.1227819Z ##[group]Run DATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')
-+2026-11-25T19:24:14.1231015Z [36;1mDATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')[0m
-+2026-11-25T19:24:14.1233758Z [36;1mHASH=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-hash: //p')[0m
-+2026-11-25T19:24:14.1235793Z [36;1mecho "cachekey=$(echo $DATE$HASH | head -c12)" >> $GITHUB_OUTPUT[0m
-+2026-11-25T19:24:14.1264612Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:14.1265971Z env:
-+2026-11-25T19:24:14.1266934Z   CARGO_HOME: /home/runner/.cargo
-+2026-11-25T19:24:14.1268156Z   toolchain: stable
-+2026-11-25T19:24:14.1269106Z ##[endgroup]
-+2026-11-25T19:24:14.1757931Z ##[end-action id=__dtolnay_rust-toolchain.rustc-version;outcome=success;conclusion=success;duration_ms=57]
-+2026-11-25T19:24:14.1766008Z ##[start-action display=Disable incremental compilation;id=__dtolnay_rust-toolchain.__run_6]
-+2026-11-25T19:24:14.1816060Z ##[group]Run if [ -z "${CARGO_INCREMENTAL+set}" ]; then
-+2026-11-25T19:24:14.1817608Z [36;1mif [ -z "${CARGO_INCREMENTAL+set}" ]; then[0m
-+2026-11-25T19:24:14.1818988Z [36;1m  echo CARGO_INCREMENTAL=0 >> $GITHUB_ENV[0m
-+2026-11-25T19:24:14.1820263Z [36;1mfi[0m
-+2026-11-25T19:24:14.1849090Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:14.1850512Z env:
-+2026-11-25T19:24:14.1851455Z   CARGO_HOME: /home/runner/.cargo
-+2026-11-25T19:24:14.1852679Z ##[endgroup]
-+2026-11-25T19:24:14.1925024Z ##[end-action id=__dtolnay_rust-toolchain.__run_6;outcome=success;conclusion=success;duration_ms=15]
-+2026-11-25T19:24:14.1932162Z ##[start-action display=Enable colors in Cargo output;id=__dtolnay_rust-toolchain.__run_7]
-+2026-11-25T19:24:14.1979175Z ##[group]Run if [ -z "${CARGO_TERM_COLOR+set}" ]; then
-+2026-11-25T19:24:14.1980680Z [36;1mif [ -z "${CARGO_TERM_COLOR+set}" ]; then[0m
-+2026-11-25T19:24:14.1982096Z [36;1m  echo CARGO_TERM_COLOR=always >> $GITHUB_ENV[0m
-+2026-11-25T19:24:14.1983552Z [36;1mfi[0m
-+2026-11-25T19:24:14.2008480Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:14.2009931Z env:
-+2026-11-25T19:24:14.2010861Z   CARGO_HOME: /home/runner/.cargo
-+2026-11-25T19:24:14.2012027Z   CARGO_INCREMENTAL: 0
-+2026-11-25T19:24:14.2013006Z ##[endgroup]
-+2026-11-25T19:24:14.2084547Z ##[end-action id=__dtolnay_rust-toolchain.__run_7;outcome=success;conclusion=success;duration_ms=14]
-+2026-11-25T19:24:14.2093401Z ##[start-action display=Enable Cargo sparse registry;id=__dtolnay_rust-toolchain.__run_8]
-+2026-11-25T19:24:14.2142159Z ##[group]Run # implemented in 1.66, stabilized in 1.68, made default in 1.70
-+2026-11-25T19:24:14.2144223Z [36;1m# implemented in 1.66, stabilized in 1.68, made default in 1.70[0m
-+2026-11-25T19:24:14.2146750Z [36;1mif [ -z "${CARGO_REGISTRIES_CRATES_IO_PROTOCOL+set}" -o -f "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol ]; then[0m
-+2026-11-25T19:24:14.2149408Z [36;1m  if rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[89]\.'; then[0m
-+2026-11-25T19:24:14.2151657Z [36;1m    touch "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
-+2026-11-25T19:24:14.2154014Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse >> $GITHUB_ENV[0m
-+2026-11-25T19:24:14.2156031Z [36;1m  elif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[67]\.'; then[0m
-+2026-11-25T19:24:14.2158277Z [36;1m    touch "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
-+2026-11-25T19:24:14.2160239Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git >> $GITHUB_ENV[0m
-+2026-11-25T19:24:14.2161683Z [36;1m  fi[0m
-+2026-11-25T19:24:14.2162621Z [36;1mfi[0m
-+2026-11-25T19:24:14.2188040Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:14.2189446Z env:
-+2026-11-25T19:24:14.2190586Z   CARGO_HOME: /home/runner/.cargo
-+2026-11-25T19:24:14.2191682Z   CARGO_INCREMENTAL: 0
-+2026-11-25T19:24:14.2192697Z   CARGO_TERM_COLOR: always
-+2026-11-25T19:24:14.2193900Z   toolchain: stable
-+2026-11-25T19:24:14.2194805Z ##[endgroup]
-+2026-11-25T19:24:14.2493034Z ##[end-action id=__dtolnay_rust-toolchain.__run_8;outcome=success;conclusion=success;duration_ms=39]
-+2026-11-25T19:24:14.2502630Z ##[start-action display=Work around spurious network errors in curl 8.0;id=__dtolnay_rust-toolchain.__run_9]
-+2026-11-25T19:24:14.2552911Z ##[group]Run # https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation
-+2026-11-25T19:24:14.2555639Z [36;1m# https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation[0m
-+2026-11-25T19:24:14.2557947Z [36;1mif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.7[01]\.'; then[0m
-+2026-11-25T19:24:14.2559863Z [36;1m  echo CARGO_HTTP_MULTIPLEXING=false >> $GITHUB_ENV[0m
-+2026-11-25T19:24:14.2561216Z [36;1mfi[0m
-+2026-11-25T19:24:14.2588777Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:14.2590204Z env:
-+2026-11-25T19:24:14.2591150Z   CARGO_HOME: /home/runner/.cargo
-+2026-11-25T19:24:14.2592239Z   CARGO_INCREMENTAL: 0
-+2026-11-25T19:24:14.2593547Z   CARGO_TERM_COLOR: always
-+2026-11-25T19:24:14.2594568Z   toolchain: stable
-+2026-11-25T19:24:14.2595461Z ##[endgroup]
-+2026-11-25T19:24:14.2782681Z ##[end-action id=__dtolnay_rust-toolchain.__run_9;outcome=success;conclusion=success;duration_ms=27]
-+2026-11-25T19:24:14.2793756Z ##[start-action display=rustc +stable --version --verbose;id=__dtolnay_rust-toolchain.__run_10]
-+2026-11-25T19:24:14.2842501Z ##[group]Run rustc +"$toolchain" --version --verbose
-+2026-11-25T19:24:14.2844642Z [36;1mrustc +"$toolchain" --version --verbose[0m
-+2026-11-25T19:24:14.2875013Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:14.2876472Z env:
-+2026-11-25T19:24:14.2877371Z   CARGO_HOME: /home/runner/.cargo
-+2026-11-25T19:24:14.2878518Z   CARGO_INCREMENTAL: 0
-+2026-11-25T19:24:14.2879534Z   CARGO_TERM_COLOR: always
-+2026-11-25T19:24:14.2880518Z   toolchain: stable
-+2026-11-25T19:24:14.2881504Z ##[endgroup]
-+2026-11-25T19:24:14.3045644Z rustc 1.100.0 (0ef1bc386 2026-11-09)
-+2026-11-25T19:24:14.3047443Z binary: rustc
-+2026-11-25T19:24:14.3048708Z commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a
-+2026-11-25T19:24:14.3050016Z commit-date: 2026-11-09
-+2026-11-25T19:24:14.3051080Z host: x86_64-unknown-linux-gnu
-+2026-11-25T19:24:14.3052115Z release: 1.100.0
-+2026-11-25T19:24:14.3053063Z LLVM version: 23.1.5
-+2026-11-25T19:24:14.3075105Z ##[end-action id=__dtolnay_rust-toolchain.__run_10;outcome=success;conclusion=success;duration_ms=27]
-+2026-11-25T19:24:14.3332443Z ##[group]Run Swatinem/rust-cache@v2
-+2026-11-25T19:24:14.3333685Z with:
-+2026-11-25T19:24:14.3334540Z   prefix-key: v0-rust
-+2026-11-25T19:24:14.3335539Z   add-job-id-key: true
-+2026-11-25T19:24:14.3336602Z   add-rust-environment-hash-key: true
-+2026-11-25T19:24:14.3337749Z   cache-targets: true
-+2026-11-25T19:24:14.3338733Z   cache-all-crates: false
-+2026-11-25T19:24:14.3339768Z   cache-workspace-crates: false
-+2026-11-25T19:24:14.3340819Z   save-if: true
-+2026-11-25T19:24:14.3342162Z   cache-provider: github
-+2026-11-25T19:24:14.3343289Z   cache-bin: true
-+2026-11-25T19:24:14.3344224Z   lookup-only: false
-+2026-11-25T19:24:14.3345148Z   cmd-format: {0}
-+2026-11-25T19:24:14.3346042Z env:
-+2026-11-25T19:24:14.3346930Z   CARGO_HOME: /home/runner/.cargo
-+2026-11-25T19:24:14.3347975Z   CARGO_INCREMENTAL: 0
-+2026-11-25T19:24:14.3348966Z   CARGO_TERM_COLOR: always
-+2026-11-25T19:24:14.3349927Z ##[endgroup]
-+2026-11-25T19:24:15.5665306Z ##[group]Cache Configuration
-+2026-11-25T19:24:15.5667114Z Cache Provider:
-+2026-11-25T19:24:15.5667752Z     github
-+2026-11-25T19:24:15.5668500Z Workspaces:
-+2026-11-25T19:24:15.5669166Z     /home/runner/work/MultiTunnel/MultiTunnel
-+2026-11-25T19:24:15.5670026Z Cache Paths:
-+2026-11-25T19:24:15.5670698Z     /home/runner/.cargo/bin
-+2026-11-25T19:24:15.5671167Z     /home/runner/.cargo/.crates.toml
-+2026-11-25T19:24:15.5671700Z     /home/runner/.cargo/.crates2.json
-+2026-11-25T19:24:15.5672395Z     /home/runner/.cargo/registry
-+2026-11-25T19:24:15.5672919Z     /home/runner/.cargo/git
-+2026-11-25T19:24:15.5673871Z     /home/runner/work/MultiTunnel/MultiTunnel/target
-+2026-11-25T19:24:15.5674704Z Restore Key:
-+2026-11-25T19:24:15.5675194Z     v0-rust-test-Linux-x64-2fd77104
-+2026-11-25T19:24:15.5675708Z Cache Key:
-+2026-11-25T19:24:15.5676225Z     v0-rust-test-Linux-x64-2fd77104-95f3ec8d
-+2026-11-25T19:24:15.5676985Z .. Prefix:
-+2026-11-25T19:24:15.5677404Z   - v0-rust-test-Linux-x64
-+2026-11-25T19:24:15.5677948Z .. Environment considered:
-+2026-11-25T19:24:15.5678597Z   - Rust Versions:
-+2026-11-25T19:24:15.5679286Z     - 1.100.0 x86_64-unknown-linux-gnu 0ef1bc386059596dada8fb65b50425aab2ea637a
-+2026-11-25T19:24:15.5680120Z   - CARGO_HOME
-+2026-11-25T19:24:15.5680672Z   - CARGO_INCREMENTAL
-+2026-11-25T19:24:15.5681287Z   - CARGO_TERM_COLOR
-+2026-11-25T19:24:15.5682020Z .. Lockfiles considered:
-+2026-11-25T19:24:15.5682741Z   - /home/runner/work/MultiTunnel/MultiTunnel/Cargo.lock
-+2026-11-25T19:24:15.5683781Z   - /home/runner/work/MultiTunnel/MultiTunnel/Cargo.toml
-+2026-11-25T19:24:15.5684880Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-app/Cargo.toml
-+2026-11-25T19:24:15.5686281Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond/Cargo.toml
-+2026-11-25T19:24:15.5687548Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-engine/Cargo.toml
-+2026-11-25T19:24:15.5688519Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-links/Cargo.toml
-+2026-11-25T19:24:15.5689455Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-m3/Cargo.toml
-+2026-11-25T19:24:15.5690195Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net/Cargo.toml
-+2026-11-25T19:24:15.5691070Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-platform/Cargo.toml
-+2026-11-25T19:24:15.5691890Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-service/Cargo.toml
-+2026-11-25T19:24:15.5692968Z ##[endgroup]
-+2026-11-25T19:24:15.5693573Z 
-+2026-11-25T19:24:15.5694033Z ... Restoring cache ...
-+2026-11-25T19:24:15.7235791Z Cache hit for restore-key: v0-rust-test-Linux-x64-2fd77104-39107a77
-+2026-11-25T19:24:16.9104498Z Received 83886080 of 490334055 (17.1%), 80.0 MBs/sec
-+2026-11-25T19:24:17.9162944Z Received 260046848 of 490334055 (53.0%), 123.9 MBs/sec
-+2026-11-25T19:24:18.9125367Z Received 402653184 of 490334055 (82.1%), 127.9 MBs/sec
-+2026-11-25T19:24:19.2656402Z Received 490334055 of 490334055 (100.0%), 139.4 MBs/sec
-+2026-11-25T19:24:19.2657324Z Cache Size: ~468 MB (490334055 B)
-+2026-11-25T19:24:19.2776314Z [command]/usr/bin/tar -xf /home/runner/work/_temp/9f574106-7ad0-4f2c-8d7e-64543291bc81/cache.tzst -P -C /home/runner/work/MultiTunnel/MultiTunnel --use-compress-program unzstd
-+2026-11-25T19:24:21.9208555Z Cache restored successfully
-+2026-11-25T19:24:21.9378748Z Restored from cache key "v0-rust-test-Linux-x64-2fd77104-39107a77" full match: false.
-+2026-11-25T19:24:22.0385568Z ##[group]Run cargo test --workspace
-+2026-11-25T19:24:22.0386048Z [36;1mcargo test --workspace[0m
-+2026-11-25T19:24:22.0409697Z shell: /usr/bin/bash -e {0}
-+2026-11-25T19:24:22.0410120Z env:
-+2026-11-25T19:24:22.0410420Z   CARGO_HOME: /home/runner/.cargo
-+2026-11-25T19:24:22.0410811Z   CARGO_INCREMENTAL: 0
-+2026-11-25T19:24:22.0411182Z   CARGO_TERM_COLOR: always
-+2026-11-25T19:24:22.0411560Z   CACHE_ON_FAILURE: false
-+2026-11-25T19:24:22.0411872Z ##[endgroup]
-+2026-11-25T19:24:22.3939438Z [1m[92m    Updating[0m crates.io index
-+2026-11-25T19:24:24.1152924Z [1m[92m   Compiling[0m tokio v1.54.1
-+2026-11-25T19:24:24.1191005Z [1m[92m   Compiling[0m num-traits v0.2.19
-+2026-11-25T19:24:24.1199235Z [1m[92m   Compiling[0m arrayvec v0.7.8
-+2026-11-25T19:24:24.1283713Z [1m[92m   Compiling[0m winit v0.30.13
-+2026-11-25T19:24:26.8328015Z [1m[92m   Compiling[0m tiny-skia v0.11.4
-+2026-11-25T19:24:28.4285175Z [1m[92m   Compiling[0m kurbo v0.10.4
-+2026-11-25T19:24:29.1711359Z [1m[92m   Compiling[0m approx v0.5.1
-+2026-11-25T19:24:29.2465850Z [1m[92m   Compiling[0m palette v0.7.7
-+2026-11-25T19:24:29.5464823Z [1m[92m   Compiling[0m euclid v0.22.14
-+2026-11-25T19:24:29.7745007Z [1m[92m   Compiling[0m image v0.24.9
-+2026-11-25T19:24:30.4162662Z [1m[92m   Compiling[0m lyon_geom v1.0.20
-+2026-11-25T19:24:30.8374139Z [1m[92m   Compiling[0m mt-net v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net)
-+2026-11-25T19:24:32.2997445Z [1m[92m   Compiling[0m lyon_path v1.0.19
-+2026-11-25T19:24:32.5974846Z [1m[92m   Compiling[0m hyper v1.12.0
-+2026-11-25T19:24:32.8162652Z [1m[92m   Compiling[0m tokio-rustls v0.26.6
-+2026-11-25T19:24:33.0535466Z [1m[92m   Compiling[0m tower v0.5.3
-+2026-11-25T19:24:33.2965932Z [1m[92m   Compiling[0m sctk-adwaita v0.10.1
-+2026-11-25T19:24:33.5401750Z [1m[92m   Compiling[0m hyper-util v0.1.21
-+2026-11-25T19:24:33.8484665Z [1m[92m   Compiling[0m tower-http v0.6.11
-+2026-11-25T19:24:34.1945452Z [1m[92m   Compiling[0m mt-engine v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-engine)
-+2026-11-25T19:24:35.1865161Z [1m[92m   Compiling[0m iced_core v0.13.2
-+2026-11-25T19:24:35.2588869Z [1m[92m   Compiling[0m hyper-rustls v0.27.10
-+2026-11-25T19:24:35.3635275Z [1m[92m   Compiling[0m mt-service v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-service)
-+2026-11-25T19:24:35.4765472Z [1m[92m   Compiling[0m reqwest v0.12.28
-+2026-11-25T19:24:35.9185325Z [1m[92m   Compiling[0m mt-bond v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond)
-+2026-11-25T19:24:36.6496793Z [1m[92m   Compiling[0m iced_futures v0.13.2
-+2026-11-25T19:24:36.9542483Z [1m[92m   Compiling[0m iced_graphics v0.13.0 (/home/runner/work/MultiTunnel/MultiTunnel/vendor/iced_graphics)
-+2026-11-25T19:24:38.4394022Z [1m[92m   Compiling[0m iced_tiny_skia v0.13.0 (/home/runner/work/MultiTunnel/MultiTunnel/vendor/iced_tiny_skia)
-+2026-11-25T19:24:39.6525690Z [1m[92m   Compiling[0m iced_runtime v0.13.2
-+2026-11-25T19:24:39.8805303Z [1m[92m   Compiling[0m iced_renderer v0.13.0
-+2026-11-25T19:24:39.8981356Z [1m[92m   Compiling[0m mt-links v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-links)
-+2026-11-25T19:24:40.0165288Z [1m[92m   Compiling[0m iced_widget v0.13.4
-+2026-11-25T19:24:40.4575105Z [1m[92m   Compiling[0m iced_winit v0.13.0
-+2026-11-25T19:24:41.0385527Z [1m[92m   Compiling[0m mt-platform v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-platform)
-+2026-11-25T19:24:41.7519532Z [1m[92m   Compiling[0m iced v0.13.1
-+2026-11-25T19:24:42.0555872Z [1m[92m   Compiling[0m mt-m3 v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-m3)
-+2026-11-25T19:24:42.4550288Z [1m[92m   Compiling[0m mt-app v0.15.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-app)
-+2026-11-25T19:24:45.3801602Z [1m[92m   Compiling[0m multitunnel v0.15.0 (/home/runner/work/MultiTunnel/MultiTunnel)
-+2026-11-25T19:24:59.4684949Z [1m[92m    Finished[0m `test` profile [unoptimized + debuginfo] target(s) in 37.40s
-+2026-11-25T19:24:59.4967422Z [1m[92m     Running[0m unittests src/lib.rs (target/debug/build/mt-app/f6713c862b4ed9fb/out/mt_app-f6713c862b4ed9fb)
-+2026-11-25T19:24:59.4991656Z 
-+2026-11-25T19:24:59.4991847Z running 66 tests
-+2026-11-25T19:24:59.5009436Z test app::tests::app_version_is_release_triplet ... ok
-+2026-11-25T19:24:59.5374981Z test app::tests::adapter_rate_sampling_deltas_and_resets ... ok
-+2026-11-25T19:24:59.5404193Z test app::tests::close_action_decision ... ok
-+2026-11-25T19:24:59.5405614Z test app::tests::close_to_tray_settings_roundtrip ... ok
-+2026-11-25T19:24:59.5469601Z test app::tests::begin_exit_is_idempotent_without_tray ... ok
-+2026-11-25T19:24:59.5470821Z test app::tests::always_on_top_menu_item_roundtrips ... ok
-+2026-11-25T19:24:59.5471730Z test app::tests::fmt_helpers ... ok
-+2026-11-25T19:24:59.5472583Z test app::tests::kill_switch_banner_gate ... ok
-+2026-11-25T19:24:59.5473690Z test app::tests::link_matches_adapter_shapes ... ok
-+2026-11-25T19:24:59.5474565Z test app::tests::log_filter_matches ... ok
-+2026-11-25T19:24:59.5564437Z test app::tests::adapters_screen_toggle_move_apply_revert ... ok
-+2026-11-25T19:24:59.5565340Z test app::tests::logs_autoscroll_default_and_toggle ... ok
-+2026-11-25T19:24:59.5614151Z test app::tests::logs_scroll_id_stable ... ok
-+2026-11-25T19:24:59.5644149Z test app::tests::open_folder_command_targets_the_os_file_manager ... ok
-+2026-11-25T19:24:59.5645281Z test app::tests::rule_editor_add_edit_remove ... ok
-+2026-11-25T19:24:59.5645928Z test app::tests::edit_dialog_open_type_save_close ... ok
-+2026-11-25T19:24:59.5646723Z test app::tests::logs_autoscroll_gated_on_screen ... ok
-+2026-11-25T19:24:59.5647386Z test app::tests::edit_save_failure_is_logged ... ok
-+2026-11-25T19:24:59.5647993Z test app::tests::seed_parse ... ok
-+2026-11-25T19:24:59.5665359Z test app::tests::smoke_flag_parse ... ok
-+2026-11-25T19:24:59.5744708Z test app::tests::session_totals_accumulate_and_reset ... ok
-+2026-11-25T19:24:59.5745726Z test app::tests::sort_adapter_rows_selected_first_in_saved_order ... ok
-+2026-11-25T19:24:59.5784567Z test app::tests::s21_qos_toggle_updates_draft ... ok
-+2026-11-25T19:24:59.5793978Z test app::tests::s19_settings_toggles_and_modal_gating ... ok
-+2026-11-25T19:24:59.6157684Z test app::tests::theme_reroutes_on_settings_messages ... ok
-+2026-11-25T19:24:59.6225631Z test app::tests::title_menu_anchor_clamps ... ok
-+2026-11-25T19:24:59.6226327Z test app::tests::titlebar_double_click_window ... ok
-+2026-11-25T19:24:59.6459037Z test app::tests::smoke_run_drives_app_with_mock ... ok
-+2026-11-25T19:24:59.6674812Z test app::tests::wheel_drag_updates_seed_live_and_persists_on_release ... ok
-+2026-11-25T19:24:59.6675963Z test graph::tests::local_bounds_strips_widget_offset_regression ... ok
-+2026-11-25T19:24:59.6676658Z test graph::tests::nice_ceiling_covers_and_rounds ... ok
-+2026-11-25T19:24:59.6677313Z test graph::tests::points_stay_inside_bounds_and_right_align ... ok
-+2026-11-25T19:24:59.6678253Z test graph::tests::rates_format_human ... ok
-+2026-11-25T19:24:59.6678718Z test mock::tests::fmt_utc_known_values ... ok
-+2026-11-25T19:25:00.3920014Z test app::tests::speed_test_message_flow ... ok
-+2026-11-25T19:25:00.3946948Z test app::tests::speed_upload_test_message_flow ... ok
-+2026-11-25T19:25:00.3950092Z test mock::tests::mock_profiles_import_remove ... ok
-+2026-11-25T19:25:00.3954524Z test mock::tests::mock_settings_and_rules ... ok
-+2026-11-25T19:25:00.3998956Z test theme::tests::button_states_use_state_layers ... ok
-+2026-11-25T19:25:00.3999944Z test theme::tests::hsv_conversions_roundtrip_primaries ... ok
-+2026-11-25T19:25:00.4044994Z test theme::tests::switch_styles_differ_by_state ... ok
-+2026-11-25T19:25:00.4045747Z test theme::tests::system_prefers_dark_env_override_wins ... ok
-+2026-11-25T19:25:00.4090301Z test theme::tests::text_color_from_m3 ... ok
-+2026-11-25T19:25:00.4264727Z test theme::tests::theme_derivation_seed_changes_scheme ... ok
-+2026-11-25T19:25:00.4352846Z test theme::tests::theme_from_settings_and_rederive ... ok
-+2026-11-25T19:25:00.4353684Z test theme::tests::wheel_asset_dimensions ... ok
-+2026-11-25T19:25:00.4354854Z test theme::tests::wheel_marker_roundtrips_picked_points ... ok
-+2026-11-25T19:25:00.4355766Z test theme::tests::wheel_seed_clamps_outside_points_to_the_rim ... ok
-+2026-11-25T19:25:00.4356569Z test theme::tests::wheel_seed_matches_cardinal_hues ... ok
-+2026-11-25T19:25:00.4357149Z test tray::tests::ico_embedded_asset_parses_with_32_entry ... ok
-+2026-11-25T19:25:00.4358084Z test tray::tests::ico_embedded_mono_asset_parses_with_32_entry ... ok
-+2026-11-25T19:25:00.4358736Z test tray::tests::ico_rejects_bad_directories ... ok
-+2026-11-25T19:25:00.4359678Z test tray::tests::ids_roundtrip_and_skip_zero ... ok
-+2026-11-25T19:25:00.4360611Z test tray::tests::menu_connected_shows_disconnect_and_servers ... ok
-+2026-11-25T19:25:00.4361337Z test tray::tests::menu_disconnected_shows_connect ... ok
-+2026-11-25T19:25:00.4362253Z test tray::tests::menu_failed_is_disconnected_shape_with_honest_header ... ok
-+2026-11-25T19:25:00.4363218Z test tray::tests::menu_transition_disables_the_toggle ... ok
-+2026-11-25T19:25:00.4364038Z test tray::tests::model_from_snapshot_states ... ok
-+2026-11-25T19:25:00.4364951Z test tray::tests::tip_array_never_overruns_or_splits_surrogates ... ok
-+2026-11-25T19:25:00.4365707Z test tray::tests::to_wide_is_nul_terminated ... ok
-+2026-11-25T19:25:00.4366344Z test tray::tests::tooltip_live_formats_speeds_and_totals ... ok
-+2026-11-25T19:25:00.4367435Z test tray::tests::tooltip_states ... ok
-+2026-11-25T19:25:00.4368338Z test tray::tests::tray_icon_follows_connection_state ... ok
-+2026-11-25T19:25:00.4535831Z test mock::tests::mock_latency_test ... ok
-+2026-11-25T19:25:00.5365419Z test app::tests::update_loop_screens_and_rules ... ok
-+2026-11-25T19:25:00.5700833Z test mock::tests::mock_connect_disconnect_lifecycle ... ok
-+2026-11-25T19:25:00.5701305Z 
-+2026-11-25T19:25:00.5701681Z test result: ok. 66 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.07s
-+2026-11-25T19:25:00.5702321Z 
-+2026-11-25T19:25:00.5706924Z [1m[92m     Running[0m unittests src/main.rs (target/debug/build/mt-app/340f1dcc4f8193e0/out/mt_app-340f1dcc4f8193e0)
-+2026-11-25T19:25:00.5723368Z 
-+2026-11-25T19:25:00.5724049Z running 0 tests
-+2026-11-25T19:25:00.5724454Z 
-+2026-11-25T19:25:00.5724830Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
-+2026-11-25T19:25:00.5725519Z 
-+2026-11-25T19:25:00.5726457Z [1m[92m     Running[0m unittests src/lib.rs (target/debug/build/mt-bond/47daf16dc832a37b/out/mt_bond-47daf16dc832a37b)
-+2026-11-25T19:25:00.5736676Z 
-+2026-11-25T19:25:00.5736883Z running 33 tests
-+2026-11-25T19:25:00.5740284Z test endpoint::scheduling_tests::balanced_blest_guard_never_bypasses_the_score_leader ... ok
-+2026-11-25T19:25:00.5741593Z test endpoint::scheduling_tests::balanced_capacity_aware_bypasses_the_saturated_adapter ... ok
-+2026-11-25T19:25:00.5742684Z test endpoint::scheduling_tests::balanced_blest_guard_skips_slow_draining_tied_carrier ... ok
-+2026-11-25T19:25:00.5743845Z test endpoint::scheduling_tests::balanced_picks_the_least_backlogged_carrier ... ok
-+2026-11-25T19:25:00.5744853Z test endpoint::scheduling_tests::balanced_spreads_proportionally_to_reported_capacity ... ok
-+2026-11-25T19:25:00.5745725Z test endpoint::scheduling_tests::balanced_ties_rotate ... ok
-+2026-11-25T19:25:00.5746540Z test endpoint::scheduling_tests::carrier_score_is_relative_load_with_probe_floor ... ok
-+2026-11-25T19:25:00.5747985Z test endpoint::scheduling_tests::failover_takes_the_first_live_carrier ... ok
-+2026-11-25T19:25:00.5748863Z test endpoint::scheduling_tests::ecf_completion_is_rtt_plus_drain_time ... ok
-+2026-11-25T19:25:00.5749736Z test endpoint::scheduling_tests::lowest_latency_ecf_prefers_earliest_completion ... ok
-+2026-11-25T19:25:00.5750844Z test endpoint::scheduling_tests::lowest_latency_prefers_sampled_rtt_then_known_over_unknown ... ok
-+2026-11-25T19:25:00.5751841Z test endpoint::scheduling_tests::mixed_reports_probe_the_unreported_carrier ... ok
-+2026-11-25T19:25:00.5752797Z test endpoint::scheduling_tests::priority_fills_in_order_until_the_queue_backs_up ... ok
-+2026-11-25T19:25:00.5754573Z test endpoint::scheduling_tests::priority_spills_when_relative_load_exceeds_ratio ... ok
-+2026-11-25T19:25:00.5755521Z test endpoint::scheduling_tests::round_robin_cycles_the_cursor ... ok
-+2026-11-25T19:25:00.5756290Z test frame::tests::empty_and_max_payloads_roundtrip ... ok
-+2026-11-25T19:25:00.5757008Z test frame::tests::encode_session_borrowed_is_wire_identical ... ok
-+2026-11-25T19:25:00.5757773Z test frame::tests::header_rejects_bad_magic_version_kind_and_size ... ok
-+2026-11-25T19:25:00.5758350Z test frame::tests::roundtrip_all_kinds ... ok
-+2026-11-25T19:25:00.5758927Z test frame::tests::target_rejects_truncated_and_garbage ... ok
-+2026-11-25T19:25:00.5759502Z test frame::tests::read_write_frame_over_duplex ... ok
-+2026-11-25T19:25:00.5760083Z test frame::tests::target_roundtrip_domain_ipv4_ipv6 ... ok
-+2026-11-25T19:25:00.5760784Z test outbound::tests::ensure_carriers_fails_cleanly_with_no_links ... ok
-+2026-11-25T19:25:00.5761445Z test outbound::tests::ensure_carriers_reports_all_dead_links ... ok
-+2026-11-25T19:25:00.5762421Z test reorder::tests::duplicates_and_replays_are_idempotent ... ok
-+2026-11-25T19:25:00.5763325Z test reorder::tests::cap_zero_still_delivers_in_order ... ok
-+2026-11-25T19:25:00.5763993Z test reorder::tests::gap_buffers_then_flushes ... ok
-+2026-11-25T19:25:00.5764786Z test reorder::tests::long_out_of_order_run_reorders_completely ... ok
-+2026-11-25T19:25:00.5765727Z test reorder::tests::in_order_delivery ... ok
-+2026-11-25T19:25:00.5766393Z test reorder::tests::overflow_is_an_error ... ok
-+2026-11-25T19:25:00.5766863Z test reorder::tests::would_overflow_mirrors_the_insert_boundary ... ok
-+2026-11-25T19:25:00.5767332Z test tests::constants_are_coherent ... ok
-+2026-11-25T19:25:00.7766898Z test daemon::tests::serve_binds_and_shuts_down ... ok
-+2026-11-25T19:25:00.7767498Z 
-+2026-11-25T19:25:00.7767927Z test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
-+2026-11-25T19:25:00.7768440Z 
-+2026-11-25T19:25:00.7770026Z [1m[92m     Running[0m unittests src/bin/mt_merge_server.rs (target/debug/build/mt-bond/9c1ad43eeb883f2c/out/mt_merge_server-9c1ad43eeb883f2c)
-+2026-11-25T19:25:00.7780283Z 
-+2026-11-25T19:25:00.7780501Z running 0 tests
-+2026-11-25T19:25:00.7780718Z 
-+2026-11-25T19:25:00.7781780Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
-+2026-11-25T19:25:00.7782449Z 
-+2026-11-25T19:25:00.7783935Z [1m[92m     Running[0m tests/integration.rs (target/debug/build/mt-bond/1488f9338504bb39/out/integration-1488f9338504bb39)
-+2026-11-25T19:25:00.7794979Z 
-+2026-11-25T19:25:00.7795183Z running 6 tests
-+2026-11-25T19:25:01.8425451Z test echo_roundtrip_1mib_across_two_carriers ... ok
-+2026-11-25T19:25:03.8656070Z test concurrent_sessions_multiplex_over_the_same_carriers ... ok
-+2026-11-25T19:25:03.9405989Z test refused_target_surfaces_as_connection_reset ... ok
-+2026-11-25T19:25:04.1406733Z test carrier_death_mid_stream_fails_over_without_data_loss ... ok
-+2026-11-25T19:25:04.9261588Z test one_dead_link_still_yields_a_working_bond ... ok
-+2026-11-25T19:26:00.7797750Z test domain_targets_dial_through_the_daemon has been running for over 60 seconds
-+2026-11-25T19:26:00.7847507Z test domain_targets_dial_through_the_daemon ... FAILED
-+2026-11-25T19:26:00.7848345Z 
-+2026-11-25T19:26:00.7848469Z failures:
-+2026-11-25T19:26:00.7848658Z 
-+2026-11-25T19:26:00.7848953Z ---- domain_targets_dial_through_the_daemon stdout ----
-+2026-11-25T19:26:00.7850244Z [mt-merge-server] listening on 127.0.0.1:41533
-+2026-11-25T19:26:00.7850624Z 
-+2026-11-25T19:26:00.7851163Z thread 'domain_targets_dial_through_the_daemon' (3608) panicked at crates/mt-bond/tests/integration.rs:382:6:
-+2026-11-25T19:26:00.7852485Z deadline: domain_targets wedged (documented runner flake — see LEDGER): Elapsed(())
-+2026-11-25T19:26:00.7853551Z note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
-+2026-11-25T19:26:00.7854085Z 
-+2026-11-25T19:26:00.7854155Z 
-+2026-11-25T19:26:00.7854311Z failures:
-+2026-11-25T19:26:00.7855221Z     domain_targets_dial_through_the_daemon
-+2026-11-25T19:26:00.7855584Z 
-+2026-11-25T19:26:00.7856100Z test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 60.01s
-+2026-11-25T19:26:00.7856479Z 
-+2026-11-25T19:26:00.7857464Z [1m[91merror[0m: test failed, to rerun pass `-p mt-bond --test integration`
-+2026-11-25T19:26:00.7922071Z ##[error]Process completed with exit code 101.
-+2026-11-25T19:26:00.8092529Z Post job cleanup.
-+2026-11-25T19:26:00.9033533Z [command]/usr/bin/git version
-+2026-11-25T19:26:00.9075179Z git version 2.56.1
-+2026-11-25T19:26:00.9112300Z Temporarily overriding HOME='/home/runner/work/_temp/fcdddce4-c7ef-43f6-a960-863a09dbd4f9' before making global git config changes
-+2026-11-25T19:26:00.9114100Z Adding repository directory to the temporary git global config as a safe directory
-+2026-11-25T19:26:00.9117934Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/MultiTunnel/MultiTunnel
-+2026-11-25T19:26:00.9159599Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
-+2026-11-25T19:26:00.9194529Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
-+2026-11-25T19:26:00.9452644Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
-+2026-11-25T19:26:00.9476301Z http.https://github.com/.extraheader
-+2026-11-25T19:26:00.9489044Z [command]/usr/bin/git config --local --unset-all http.https://github.com/.extraheader
-+2026-11-25T19:26:00.9530202Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
-+2026-11-25T19:26:00.9826036Z [command]/usr/bin/git config --local --name-only --get-regexp ^includeIf\.gitdir:
-+2026-11-25T19:26:00.9862189Z [command]/usr/bin/git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url
-+2026-11-25T19:26:01.0285319Z Cleaning up orphan processes
-+2026-11-25T19:26:01.0576515Z ##[warning]Node.js 20 is deprecated. The following actions target Node.js 20 but are being forced to run on Node.js 24: actions/checkout@v4. For more information see: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/
-diff --git a/harness/memory/agent-reports/s22-ci-windows-flake.log b/harness/memory/agent-reports/s22-ci-windows-flake.log
+ /// Poison-tolerant lock (repo rule).
+ fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
+@@ -85,11 +105,12 @@ async fn accept_loop(
+         };
+         let bonds = Arc::clone(bonds);
+         tokio::spawn(async move {
+-            // S21 P1 perf: the carrier leg is bulk+latency mixed; Nagle on
+-            // the public-internet carrier turns each 16 KiB frame's tail
+-            // segment into a delayed-ACK stall (~40 ms per frame on Linux
+-            // peers) — TCP_NODELAY, matching the client side.
+-            let _ = conn.set_nodelay(true);
++            // S21 P1 perf + S23: the carrier leg is bulk+latency mixed;
++            // Nagle on the public-internet carrier turns each 16 KiB
++            // frame's tail segment into a delayed-ACK stall (~40 ms per
++            // frame on Linux peers), and the WAN buffers (see
++            // TUNED_SOCKET_BUFS) keep a full-BDP window in flight.
++            tune_wan_stream(&conn);
+             if let Err(e) = handle_conn(bonds, conn).await {
+                 tracing::debug!("bond carrier connection ended: {e}");
+             }
+@@ -269,20 +290,14 @@ async fn session_pump(
+ /// Resolve + dial a target (no dependency on a resolver crate: tokio's
+ /// `ToSocketAddrs` handles domains via getaddrinfo).
+ async fn dial_target(target: &TargetAddr) -> anyhow::Result<TcpStream> {
+-    match &target.host {
+-        Host::Domain(d) => {
+-            let stream = TcpStream::connect((d.as_str(), target.port)).await?;
+-            // S21 P1 perf: Nagle here stalls the session pump's 16 KiB
+-            // writes behind the target's delayed ACK.
+-            let _ = stream.set_nodelay(true);
+-            Ok(stream)
+-        }
+-        Host::Ip(ip) => {
+-            let stream = TcpStream::connect(SocketAddr::new(*ip, target.port)).await?;
+-            let _ = stream.set_nodelay(true);
+-            Ok(stream)
+-        }
+-    }
++    let stream = match &target.host {
++        // S21 P1 perf (Nagle stalls the session pump's 16 KiB writes
++        // behind the target's delayed ACK) + S23 buffer sizing.
++        Host::Domain(d) => TcpStream::connect((d.as_str(), target.port)).await?,
++        Host::Ip(ip) => TcpStream::connect(SocketAddr::new(*ip, target.port)).await?,
++    };
++    tune_wan_stream(&stream);
++    Ok(stream)
+ }
+ 
+ /// Periodic GC: drop bonds whose carriers are all dead for BOND_IDLE_GC.
+@@ -325,4 +340,31 @@ mod tests {
+             .expect("serve task")
+             .expect("serve returns Ok on shutdown");
+     }
++
++    /// S23: the WAN tuning applies without error to a real connected
++    /// stream (loopback pair). The actual buffer sizes are OS policy —
++    /// this proves the setsockopt plumbing, not kernel clamps.
++    #[tokio::test]
++    async fn tune_wan_stream_applies_to_a_live_stream() {
++        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
++        let addr = listener.local_addr().unwrap();
++        let server = tokio::spawn(async move {
++            let (stream, _) = listener.accept().await.unwrap();
++            stream
++        });
++        let client = TcpStream::connect(addr).await.unwrap();
++        let accepted = server.await.unwrap();
++        // Neither side may fail (failures are swallowed by design — so
++        // assert the OBSERVABLE nodelay, which socket2 reads back).
++        tune_wan_stream(&client);
++        tune_wan_stream(&accepted);
++        assert!(
++            accepted.nodelay().unwrap_or(false),
++            "tuning includes TCP_NODELAY"
++        );
++        // The requested buffer landed (or was clamped UP — never zero).
++        let sock = socket2::SockRef::from(&accepted);
++        assert!(sock.recv_buffer_size().unwrap_or(0) >= 128 * 1024);
++        assert!(sock.send_buffer_size().unwrap_or(0) >= 128 * 1024);
++    }
+ }
+diff --git a/docs/research/2026-11-27-stability-changelog-survey.md b/docs/research/2026-11-27-stability-changelog-survey.md
 new file mode 100644
-index 0000000..4150f8a
+index 0000000..ba956c5
 --- /dev/null
-+++ b/harness/memory/agent-reports/s22-ci-windows-flake.log
-@@ -0,0 +1,674 @@
-+﻿2026-11-25T19:24:11.1585385Z Current runner version: '2.338.2'
-+2026-11-25T19:24:11.1605278Z ##[group]Runner Image Provisioner
-+2026-11-25T19:24:11.1606061Z Hosted Compute Agent
-+2026-11-25T19:24:11.1606395Z Version: 20261111.635
-+2026-11-25T19:24:11.1606743Z Commit: b8a594ea514ff44cadb5342986c68b1eac6e22ed
-+2026-11-25T19:24:11.1607148Z Build Date: 2026-11-11T01:49:01Z
-+2026-11-25T19:24:11.1607515Z Worker ID: {12260160-1ad0-4f63-8ef1-bb20cdac6f10}
-+2026-11-25T19:24:11.1607898Z Region: southcentralus
-+2026-11-25T19:24:11.1608223Z Cloud: Azure
-+2026-11-25T19:24:11.1608514Z ##[endgroup]
-+2026-11-25T19:24:11.1609304Z ##[group]Operating System
-+2026-11-25T19:24:11.1609649Z Microsoft Windows Server 2025
-+2026-11-25T19:24:11.1609977Z 10.0.26100
-+2026-11-25T19:24:11.1610270Z Datacenter
-+2026-11-25T19:24:11.1610555Z ##[endgroup]
-+2026-11-25T19:24:11.1610849Z ##[group]Runner Image
-+2026-11-25T19:24:11.1611220Z Image: windows-2025-vs2026
-+2026-11-25T19:24:11.1611542Z Version: 20261116.315.1
-+2026-11-25T19:24:11.1612376Z Included Software: https://github.com/actions/runner-images/blob/win25-vs2026/20261116.315/images/windows/Windows2025-VS2026-Readme.md
-+2026-11-25T19:24:11.1613605Z Image Release: https://github.com/actions/runner-images/releases/tag/win25-vs2026%2F20261116.315
-+2026-11-25T19:24:11.1614216Z ##[endgroup]
-+2026-11-25T19:24:11.1614997Z ##[group]GITHUB_TOKEN Permissions
-+2026-11-25T19:24:11.1616611Z Contents: read
-+2026-11-25T19:24:11.1616926Z Metadata: read
-+2026-11-25T19:24:11.1617315Z Packages: read
-+2026-11-25T19:24:11.1617601Z ##[endgroup]
-+2026-11-25T19:24:11.1619050Z Secret source: Actions
-+2026-11-25T19:24:11.1619520Z Cache mode: write
-+2026-11-25T19:24:11.1619941Z Prepare workflow directory
-+2026-11-25T19:24:11.1849829Z Prepare all required actions
-+2026-11-25T19:24:11.1884145Z Getting action download info
-+2026-11-25T19:24:11.4076899Z Download action repository 'actions/checkout@v4' (SHA:11d5960a326750d5838078e36cf38b85af677262)
-+2026-11-25T19:24:11.4912243Z Download action repository 'dtolnay/rust-toolchain@stable' (SHA:39008827e6b4319a349e9a42dfc90579dd25f1ca)
-+2026-11-25T19:24:11.6832506Z Download action repository 'Swatinem/rust-cache@v2' (SHA:d1527c8f67ef9af357290475d4af4b0fab8f12e0)
-+2026-11-25T19:24:12.2566760Z Complete job name: test (windows-latest)
-+2026-11-25T19:24:12.4003710Z ##[group]Run actions/checkout@v4
-+2026-11-25T19:24:12.4004440Z with:
-+2026-11-25T19:24:12.4004697Z   repository: maniesh-NR/MultiTunnel
-+2026-11-25T19:24:12.4007165Z   token: ***
-+2026-11-25T19:24:12.4007402Z   ssh-strict: true
-+2026-11-25T19:24:12.4007641Z   ssh-user: git
-+2026-11-25T19:24:12.4007885Z   persist-credentials: true
-+2026-11-25T19:24:12.4008155Z   clean: true
-+2026-11-25T19:24:12.4008399Z   sparse-checkout-cone-mode: true
-+2026-11-25T19:24:12.4008689Z   fetch-depth: 1
-+2026-11-25T19:24:12.4008923Z   fetch-tags: false
-+2026-11-25T19:24:12.4009176Z   show-progress: true
-+2026-11-25T19:24:12.4009417Z   lfs: false
-+2026-11-25T19:24:12.4009644Z   submodules: false
-+2026-11-25T19:24:12.4009904Z   set-safe-directory: true
-+2026-11-25T19:24:12.4010213Z   allow-unsafe-pr-checkout: false
-+2026-11-25T19:24:12.4010678Z ##[endgroup]
-+2026-11-25T19:24:12.5007424Z Syncing repository: maniesh-NR/MultiTunnel
-+2026-11-25T19:24:12.5009350Z ##[group]Getting Git version info
-+2026-11-25T19:24:12.5009829Z Working directory is 'D:\a\MultiTunnel\MultiTunnel'
-+2026-11-25T19:24:12.5450663Z [command]"C:\Program Files\Git\bin\git.exe" version
-+2026-11-25T19:24:12.8520767Z git version 2.56.1.windows.1
-+2026-11-25T19:24:12.8565594Z ##[endgroup]
-+2026-11-25T19:24:12.8582212Z Temporarily overriding HOME='D:\a\_temp\b7d07e2b-eae9-49da-9f04-3ab18cf72481' before making global git config changes
-+2026-11-25T19:24:12.8584394Z Adding repository directory to the temporary git global config as a safe directory
-+2026-11-25T19:24:12.8590719Z [command]"C:\Program Files\Git\bin\git.exe" config --global --add safe.directory D:\a\MultiTunnel\MultiTunnel
-+2026-11-25T19:24:12.9279925Z Deleting the contents of 'D:\a\MultiTunnel\MultiTunnel'
-+2026-11-25T19:24:12.9284998Z ##[group]Initializing the repository
-+2026-11-25T19:24:12.9293436Z [command]"C:\Program Files\Git\bin\git.exe" init D:\a\MultiTunnel\MultiTunnel
-+2026-11-25T19:24:13.0023345Z Initialized empty Git repository in D:/a/MultiTunnel/MultiTunnel/.git/
-+2026-11-25T19:24:13.0073266Z [command]"C:\Program Files\Git\bin\git.exe" remote add origin https://github.com/maniesh-NR/MultiTunnel
-+2026-11-25T19:24:13.0833449Z ##[endgroup]
-+2026-11-25T19:24:13.0834785Z ##[group]Disabling automatic garbage collection
-+2026-11-25T19:24:13.0841295Z [command]"C:\Program Files\Git\bin\git.exe" config --local gc.auto 0
-+2026-11-25T19:24:13.1128036Z ##[endgroup]
-+2026-11-25T19:24:13.1129565Z ##[group]Setting up auth
-+2026-11-25T19:24:13.1137082Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp core\.sshCommand
-+2026-11-25T19:24:13.1427548Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "sh -c \"git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :\""
-+2026-11-25T19:24:14.4273841Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
-+2026-11-25T19:24:14.4545306Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "sh -c \"git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :\""
-+2026-11-25T19:24:14.8749748Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp ^includeIf\.gitdir:
-+2026-11-25T19:24:14.9031930Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "git config --local --show-origin --name-only --get-regexp remote.origin.url"
-+2026-11-25T19:24:15.3487269Z [command]"C:\Program Files\Git\bin\git.exe" config --local http.https://github.com/.extraheader "AUTHORIZATION: basic ***"
-+2026-11-25T19:24:15.3780746Z ##[endgroup]
-+2026-11-25T19:24:15.3781373Z ##[group]Fetching the repository
-+2026-11-25T19:24:15.3791277Z [command]"C:\Program Files\Git\bin\git.exe" -c protocol.version=2 fetch --no-tags --prune --no-recurse-submodules --depth=1 origin +21fd59f9adb3f953247fe2f0d204d95a3d83acbf:refs/remotes/origin/main
-+2026-11-25T19:24:16.7114492Z From https://github.com/maniesh-NR/MultiTunnel
-+2026-11-25T19:24:16.7115252Z  * [new ref]         21fd59f9adb3f953247fe2f0d204d95a3d83acbf -> origin/main
-+2026-11-25T19:24:16.7152564Z ##[endgroup]
-+2026-11-25T19:24:16.7152954Z ##[group]Determining the checkout info
-+2026-11-25T19:24:16.7154443Z ##[endgroup]
-+2026-11-25T19:24:16.7162410Z [command]"C:\Program Files\Git\bin\git.exe" sparse-checkout disable
-+2026-11-25T19:24:16.7564746Z [command]"C:\Program Files\Git\bin\git.exe" config --local --unset-all extensions.worktreeConfig
-+2026-11-25T19:24:16.7932377Z ##[group]Checking out the ref
-+2026-11-25T19:24:16.7939396Z [command]"C:\Program Files\Git\bin\git.exe" checkout --progress --force -B main refs/remotes/origin/main
-+2026-11-25T19:24:16.8955569Z Switched to a new branch 'main'
-+2026-11-25T19:24:16.8967951Z branch 'main' set up to track 'origin/main'.
-+2026-11-25T19:24:16.9013623Z ##[endgroup]
-+2026-11-25T19:24:16.9418164Z [command]"C:\Program Files\Git\bin\git.exe" log -1 --format=%H
-+2026-11-25T19:24:16.9684950Z 21fd59f9adb3f953247fe2f0d204d95a3d83acbf
-+2026-11-25T19:24:17.0162173Z ##[group]Run dtolnay/rust-toolchain@stable
-+2026-11-25T19:24:17.0162439Z with:
-+2026-11-25T19:24:17.0162558Z   toolchain: stable
-+2026-11-25T19:24:17.0162674Z ##[endgroup]
-+2026-11-25T19:24:17.0217953Z ##[start-action display=Parse toolchain version;id=__dtolnay_rust-toolchain.parse]
-+2026-11-25T19:24:17.0350377Z ##[group]Run if [[ -z $toolchain ]]; then
-+2026-11-25T19:24:17.0350650Z [36;1mif [[ -z $toolchain ]]; then[0m
-+2026-11-25T19:24:17.0351000Z [36;1m  # GitHub does not enforce `required: true` inputs itself. https://github.com/actions/runner/issues/1070[0m
-+2026-11-25T19:24:17.0351370Z [36;1m  echo "'toolchain' is a required input" >&2[0m
-+2026-11-25T19:24:17.0352255Z [36;1m  exit 1[0m
-+2026-11-25T19:24:17.0352448Z [36;1melif [[ $toolchain =~ ^stable' '[0-9]+' '(year|month|week|day)s?' 'ago$ ]]; then[0m
-+2026-11-25T19:24:17.0352688Z [36;1m  if [[ Windows == macOS ]]; then[0m
-+2026-11-25T19:24:17.0353044Z [36;1m    echo "toolchain=1.$((($(date -v-$(sed 's/stable \([0-9]*\) \(.\).*/\1\2/' <<< $toolchain) +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
-+2026-11-25T19:24:17.0353340Z [36;1m  else[0m
-+2026-11-25T19:24:17.0353564Z [36;1m    echo "toolchain=1.$((($(date --date "${toolchain#stable }" +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
-+2026-11-25T19:24:17.0353815Z [36;1m  fi[0m
-+2026-11-25T19:24:17.0353983Z [36;1melif [[ $toolchain =~ ^stable' 'minus' '[0-9]+' 'releases?$ ]]; then[0m
-+2026-11-25T19:24:17.0354277Z [36;1m  echo "toolchain=1.$((($(date +%s)/60/60/24-16569)/7/6-${toolchain//[^0-9]/}))" >> $GITHUB_OUTPUT[0m
-+2026-11-25T19:24:17.0354531Z [36;1melif [[ $toolchain =~ ^1\.[0-9]+$ ]]; then[0m
-+2026-11-25T19:24:17.0354819Z [36;1m  echo "toolchain=1.$((i=${toolchain#1.}, c=($(date +%s)/60/60/24-16569)/7/6, i+9*i*(10*i<=c)+90*i*(100*i<=c)))" >> $GITHUB_OUTPUT[0m
-+2026-11-25T19:24:17.0355098Z [36;1melse[0m
-+2026-11-25T19:24:17.0355211Z [36;1m  delimiter=EOF[0m
-+2026-11-25T19:24:17.0355389Z [36;1m  while [[ $toolchain == *$delimiter* ]]; do delimiter+=_; done[0m
-+2026-11-25T19:24:17.0355669Z [36;1m  printf 'toolchain<<%s\n%s\n%s\n' "$delimiter" "$toolchain" "$delimiter" >> $GITHUB_OUTPUT[0m
-+2026-11-25T19:24:17.0355896Z [36;1mfi[0m
-+2026-11-25T19:24:17.0368027Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:17.0368265Z env:
-+2026-11-25T19:24:17.0368380Z   toolchain: stable
-+2026-11-25T19:24:17.0368497Z ##[endgroup]
-+2026-11-25T19:24:17.1912908Z ##[end-action id=__dtolnay_rust-toolchain.parse;outcome=success;conclusion=success;duration_ms=169]
-+2026-11-25T19:24:17.1916690Z ##[start-action display=Set $CARGO_HOME;id=__dtolnay_rust-toolchain.__run]
-+2026-11-25T19:24:17.2031403Z ##[group]Run echo CARGO_HOME=${CARGO_HOME:-"$USERPROFILE\.cargo"} >> $GITHUB_ENV
-+2026-11-25T19:24:17.2031738Z [36;1mecho CARGO_HOME=${CARGO_HOME:-"$USERPROFILE\.cargo"} >> $GITHUB_ENV[0m
-+2026-11-25T19:24:17.2041140Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:17.2041398Z ##[endgroup]
-+2026-11-25T19:24:17.2360055Z ##[end-action id=__dtolnay_rust-toolchain.__run;outcome=success;conclusion=success;duration_ms=44]
-+2026-11-25T19:24:17.2362147Z ##[start-action display=Install rustup if needed;id=__dtolnay_rust-toolchain.__run_2]
-+2026-11-25T19:24:17.2370451Z ##[end-action id=__dtolnay_rust-toolchain.__run_2;outcome=skipped;conclusion=skipped;duration_ms=0]
-+2026-11-25T19:24:17.2371946Z ##[start-action display=Install rustup if needed on windows;id=__dtolnay_rust-toolchain.__run_3]
-+2026-11-25T19:24:17.2474738Z ##[group]Run if ! command -v rustup &>/dev/null; then
-+2026-11-25T19:24:17.2474994Z [36;1mif ! command -v rustup &>/dev/null; then[0m
-+2026-11-25T19:24:17.2475827Z [36;1m  curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused --location --silent --show-error --fail https://win.rustup.rs/x86_64 --output 'D:\a\_temp\rustup-init.exe'[0m
-+2026-11-25T19:24:17.2476399Z [36;1m  'D:\a\_temp\rustup-init.exe' --default-toolchain none --no-modify-path -y[0m
-+2026-11-25T19:24:17.2477969Z [36;1m  echo "$CARGO_HOME\bin" >> $GITHUB_PATH[0m
-+2026-11-25T19:24:17.2478140Z [36;1mfi[0m
-+2026-11-25T19:24:17.2486614Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:17.2486838Z env:
-+2026-11-25T19:24:17.2486958Z   CARGO_HOME: C:\Users\runneradmin\.cargo
-+2026-11-25T19:24:17.2487131Z ##[endgroup]
-+2026-11-25T19:24:17.2875978Z ##[end-action id=__dtolnay_rust-toolchain.__run_3;outcome=success;conclusion=success;duration_ms=50]
-+2026-11-25T19:24:17.2890222Z ##[start-action display=rustup toolchain install stable;id=__dtolnay_rust-toolchain.__run_4]
-+2026-11-25T19:24:17.2987724Z ##[group]Run set -f  # split targets and components on commas and whitespace, without glob expansion
-+2026-11-25T19:24:17.2988301Z [36;1mset -f  # split targets and components on commas and whitespace, without glob expansion[0m
-+2026-11-25T19:24:17.2988546Z [36;1mflags=()[0m
-+2026-11-25T19:24:17.2989032Z [36;1mfor t in ${targets//,/ }; do flags+=(--target "$t"); done[0m
-+2026-11-25T19:24:17.2989295Z [36;1mfor c in ${components//,/ }; do flags+=(--component "$c"); done[0m
-+2026-11-25T19:24:17.2989580Z [36;1mif [[ $toolchain == nightly && -n $components ]]; then flags+=(--allow-downgrade); fi[0m
-+2026-11-25T19:24:17.2989815Z [36;1mlog=$(mktemp)[0m
-+2026-11-25T19:24:17.2989952Z [36;1mtrap 'rm -f "$log"' EXIT[0m
-+2026-11-25T19:24:17.2990101Z [36;1mfor attempt in 1 2 3 4 5; do[0m
-+2026-11-25T19:24:17.2990424Z [36;1m  if rustup toolchain install "$toolchain" "${flags[@]}" --profile minimal --no-self-update --force-non-host 2>&1 | tee "$log"; then[0m
-+2026-11-25T19:24:17.2990739Z [36;1m    break[0m
-+2026-11-25T19:24:17.2990850Z [36;1m  else[0m
-+2026-11-25T19:24:17.2990965Z [36;1m    status=$?[0m
-+2026-11-25T19:24:17.2991189Z [36;1m  fi[0m
-+2026-11-25T19:24:17.2991398Z [36;1m  if [[ $attempt == 5 ]] || ! grep -Fq 'checksum failed for ' "$log" \[0m
-+2026-11-25T19:24:17.2991725Z [36;1m      || ! grep -Fq 'this is likely due to an ongoing update of the official release server' "$log"; then[0m
-+2026-11-25T19:24:17.2991985Z [36;1m    exit "$status"[0m
-+2026-11-25T19:24:17.2992111Z [36;1m  fi[0m
-+2026-11-25T19:24:17.2992308Z [36;1m  echo "Retrying rustup toolchain install in 30 seconds (retry $attempt of 4)" >&2[0m
-+2026-11-25T19:24:17.2992538Z [36;1m  sleep 30[0m
-+2026-11-25T19:24:17.2992651Z [36;1mdone[0m
-+2026-11-25T19:24:17.3001317Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:17.3001545Z env:
-+2026-11-25T19:24:17.3001668Z   CARGO_HOME: C:\Users\runneradmin\.cargo
-+2026-11-25T19:24:17.3001822Z   toolchain: stable
-+2026-11-25T19:24:17.3001940Z   targets: 
-+2026-11-25T19:24:17.3002054Z   components: 
-+2026-11-25T19:24:17.3002172Z   RUSTUP_PERMIT_COPY_RENAME: 1
-+2026-11-25T19:24:17.3002312Z ##[endgroup]
-+2026-11-25T19:24:20.9477854Z info: syncing channel updates for stable-x86_64-pc-windows-msvc
-+2026-11-25T19:24:21.3702355Z 
-+2026-11-25T19:24:21.3859229Z   stable-x86_64-pc-windows-msvc unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
-+2026-11-25T19:24:21.3859471Z 
-+2026-11-25T19:24:21.4140966Z ##[end-action id=__dtolnay_rust-toolchain.__run_4;outcome=success;conclusion=success;duration_ms=4124]
-+2026-11-25T19:24:21.4146877Z ##[start-action display=rustup default stable;id=__dtolnay_rust-toolchain.__run_5]
-+2026-11-25T19:24:21.4246463Z ##[group]Run rustup default "$toolchain" --force-non-host
-+2026-11-25T19:24:21.4246716Z [36;1mrustup default "$toolchain" --force-non-host[0m
-+2026-11-25T19:24:21.4255607Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:21.4255893Z env:
-+2026-11-25T19:24:21.4256018Z   CARGO_HOME: C:\Users\runneradmin\.cargo
-+2026-11-25T19:24:21.4256185Z   toolchain: stable
-+2026-11-25T19:24:21.4256300Z ##[endgroup]
-+2026-11-25T19:24:21.4774380Z info: using existing install for stable-x86_64-pc-windows-msvc
-+2026-11-25T19:24:21.4858757Z info: default toolchain set to stable-x86_64-pc-windows-msvc
-+2026-11-25T19:24:21.4859695Z 
-+2026-11-25T19:24:21.5039262Z   stable-x86_64-pc-windows-msvc unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
-+2026-11-25T19:24:21.5039516Z 
-+2026-11-25T19:24:21.5108498Z ##[end-action id=__dtolnay_rust-toolchain.__run_5;outcome=success;conclusion=success;duration_ms=96]
-+2026-11-25T19:24:21.5111707Z ##[start-action display=Create cachekey;id=__dtolnay_rust-toolchain.rustc-version]
-+2026-11-25T19:24:21.5205304Z ##[group]Run DATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')
-+2026-11-25T19:24:21.5205822Z [36;1mDATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')[0m
-+2026-11-25T19:24:21.5206340Z [36;1mHASH=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-hash: //p')[0m
-+2026-11-25T19:24:21.5206626Z [36;1mecho "cachekey=$(echo $DATE$HASH | head -c12)" >> $GITHUB_OUTPUT[0m
-+2026-11-25T19:24:21.5214743Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:21.5214960Z env:
-+2026-11-25T19:24:21.5215075Z   CARGO_HOME: C:\Users\runneradmin\.cargo
-+2026-11-25T19:24:21.5215221Z   toolchain: stable
-+2026-11-25T19:24:21.5215328Z ##[endgroup]
-+2026-11-25T19:24:22.3652233Z ##[end-action id=__dtolnay_rust-toolchain.rustc-version;outcome=success;conclusion=success;duration_ms=853]
-+2026-11-25T19:24:22.3654306Z ##[start-action display=Disable incremental compilation;id=__dtolnay_rust-toolchain.__run_6]
-+2026-11-25T19:24:22.3756280Z ##[group]Run if [ -z "${CARGO_INCREMENTAL+set}" ]; then
-+2026-11-25T19:24:22.3756543Z [36;1mif [ -z "${CARGO_INCREMENTAL+set}" ]; then[0m
-+2026-11-25T19:24:22.3756748Z [36;1m  echo CARGO_INCREMENTAL=0 >> $GITHUB_ENV[0m
-+2026-11-25T19:24:22.3756907Z [36;1mfi[0m
-+2026-11-25T19:24:22.3765883Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:22.3766116Z env:
-+2026-11-25T19:24:22.3766242Z   CARGO_HOME: C:\Users\runneradmin\.cargo
-+2026-11-25T19:24:22.3766412Z ##[endgroup]
-+2026-11-25T19:24:22.4085256Z ##[end-action id=__dtolnay_rust-toolchain.__run_6;outcome=success;conclusion=success;duration_ms=43]
-+2026-11-25T19:24:22.4087184Z ##[start-action display=Enable colors in Cargo output;id=__dtolnay_rust-toolchain.__run_7]
-+2026-11-25T19:24:22.4178863Z ##[group]Run if [ -z "${CARGO_TERM_COLOR+set}" ]; then
-+2026-11-25T19:24:22.4179096Z [36;1mif [ -z "${CARGO_TERM_COLOR+set}" ]; then[0m
-+2026-11-25T19:24:22.4179279Z [36;1m  echo CARGO_TERM_COLOR=always >> $GITHUB_ENV[0m
-+2026-11-25T19:24:22.4179442Z [36;1mfi[0m
-+2026-11-25T19:24:22.4187144Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:22.4187382Z env:
-+2026-11-25T19:24:22.4187501Z   CARGO_HOME: C:\Users\runneradmin\.cargo
-+2026-11-25T19:24:22.4187654Z   CARGO_INCREMENTAL: 0
-+2026-11-25T19:24:22.4187773Z ##[endgroup]
-+2026-11-25T19:24:22.4488287Z ##[end-action id=__dtolnay_rust-toolchain.__run_7;outcome=success;conclusion=success;duration_ms=40]
-+2026-11-25T19:24:22.4492676Z ##[start-action display=Enable Cargo sparse registry;id=__dtolnay_rust-toolchain.__run_8]
-+2026-11-25T19:24:22.4580742Z ##[group]Run # implemented in 1.66, stabilized in 1.68, made default in 1.70
-+2026-11-25T19:24:22.4581059Z [36;1m# implemented in 1.66, stabilized in 1.68, made default in 1.70[0m
-+2026-11-25T19:24:22.4581420Z [36;1mif [ -z "${CARGO_REGISTRIES_CRATES_IO_PROTOCOL+set}" -o -f "D:\a\_temp"/.implicit_cargo_registries_crates_io_protocol ]; then[0m
-+2026-11-25T19:24:22.4581809Z [36;1m  if rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[89]\.'; then[0m
-+2026-11-25T19:24:22.4582102Z [36;1m    touch "D:\a\_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
-+2026-11-25T19:24:22.4582382Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse >> $GITHUB_ENV[0m
-+2026-11-25T19:24:22.4582662Z [36;1m  elif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[67]\.'; then[0m
-+2026-11-25T19:24:22.4583084Z [36;1m    touch "D:\a\_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
-+2026-11-25T19:24:22.4583342Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git >> $GITHUB_ENV[0m
-+2026-11-25T19:24:22.4583533Z [36;1m  fi[0m
-+2026-11-25T19:24:22.4583635Z [36;1mfi[0m
-+2026-11-25T19:24:22.4591811Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:22.4592028Z env:
-+2026-11-25T19:24:22.4592148Z   CARGO_HOME: C:\Users\runneradmin\.cargo
-+2026-11-25T19:24:22.4592308Z   CARGO_INCREMENTAL: 0
-+2026-11-25T19:24:22.4592432Z   CARGO_TERM_COLOR: always
-+2026-11-25T19:24:22.4592555Z   toolchain: stable
-+2026-11-25T19:24:22.4592667Z ##[endgroup]
-+2026-11-25T19:24:22.6170275Z ##[end-action id=__dtolnay_rust-toolchain.__run_8;outcome=success;conclusion=success;duration_ms=167]
-+2026-11-25T19:24:22.6173803Z ##[start-action display=Work around spurious network errors in curl 8.0;id=__dtolnay_rust-toolchain.__run_9]
-+2026-11-25T19:24:22.6266088Z ##[group]Run # https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation
-+2026-11-25T19:24:22.6266523Z [36;1m# https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation[0m
-+2026-11-25T19:24:22.6266873Z [36;1mif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.7[01]\.'; then[0m
-+2026-11-25T19:24:22.6267483Z [36;1m  echo CARGO_HTTP_MULTIPLEXING=false >> $GITHUB_ENV[0m
-+2026-11-25T19:24:22.6267665Z [36;1mfi[0m
-+2026-11-25T19:24:22.6276285Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:22.6276506Z env:
-+2026-11-25T19:24:22.6276625Z   CARGO_HOME: C:\Users\runneradmin\.cargo
-+2026-11-25T19:24:22.6276789Z   CARGO_INCREMENTAL: 0
-+2026-11-25T19:24:22.6276914Z   CARGO_TERM_COLOR: always
-+2026-11-25T19:24:22.6277037Z   toolchain: stable
-+2026-11-25T19:24:22.6277147Z ##[endgroup]
-+2026-11-25T19:24:22.7279321Z ##[end-action id=__dtolnay_rust-toolchain.__run_9;outcome=success;conclusion=success;duration_ms=110]
-+2026-11-25T19:24:22.7283238Z ##[start-action display=rustc +stable --version --verbose;id=__dtolnay_rust-toolchain.__run_10]
-+2026-11-25T19:24:22.7378401Z ##[group]Run rustc +"$toolchain" --version --verbose
-+2026-11-25T19:24:22.7378615Z [36;1mrustc +"$toolchain" --version --verbose[0m
-+2026-11-25T19:24:22.7386636Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
-+2026-11-25T19:24:22.7386852Z env:
-+2026-11-25T19:24:22.7386967Z   CARGO_HOME: C:\Users\runneradmin\.cargo
-+2026-11-25T19:24:22.7387113Z   CARGO_INCREMENTAL: 0
-+2026-11-25T19:24:22.7387231Z   CARGO_TERM_COLOR: always
-+2026-11-25T19:24:22.7387349Z   toolchain: stable
-+2026-11-25T19:24:22.7387459Z ##[endgroup]
-+2026-11-25T19:24:22.8022844Z rustc 1.100.0 (0ef1bc386 2026-11-09)
-+2026-11-25T19:24:22.8023158Z binary: rustc
-+2026-11-25T19:24:22.8023744Z commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a
-+2026-11-25T19:24:22.8024067Z commit-date: 2026-11-09
-+2026-11-25T19:24:22.8024274Z host: x86_64-pc-windows-msvc
-+2026-11-25T19:24:22.8024511Z release: 1.100.0
-+2026-11-25T19:24:22.8024844Z LLVM version: 23.1.5
-+2026-11-25T19:24:22.8135657Z ##[end-action id=__dtolnay_rust-toolchain.__run_10;outcome=success;conclusion=success;duration_ms=85]
-+2026-11-25T19:24:22.8300558Z ##[group]Run Swatinem/rust-cache@v2
-+2026-11-25T19:24:22.8300750Z with:
-+2026-11-25T19:24:22.8300865Z   prefix-key: v0-rust
-+2026-11-25T19:24:22.8300990Z   add-job-id-key: true
-+2026-11-25T19:24:22.8301117Z   add-rust-environment-hash-key: true
-+2026-11-25T19:24:22.8301264Z   cache-targets: true
-+2026-11-25T19:24:22.8301384Z   cache-all-crates: false
-+2026-11-25T19:24:22.8301517Z   cache-workspace-crates: false
-+2026-11-25T19:24:22.8301647Z   save-if: true
-+2026-11-25T19:24:22.8301780Z   cache-provider: github
-+2026-11-25T19:24:22.8301896Z   cache-bin: true
-+2026-11-25T19:24:22.8302001Z   lookup-only: false
-+2026-11-25T19:24:22.8302114Z   cmd-format: {0}
-+2026-11-25T19:24:22.8302216Z env:
-+2026-11-25T19:24:22.8302329Z   CARGO_HOME: C:\Users\runneradmin\.cargo
-+2026-11-25T19:24:22.8302474Z   CARGO_INCREMENTAL: 0
-+2026-11-25T19:24:22.8302590Z   CARGO_TERM_COLOR: always
-+2026-11-25T19:24:22.8302707Z ##[endgroup]
-+2026-11-25T19:24:24.7941234Z ##[group]Cache Configuration
-+2026-11-25T19:24:24.7941918Z Cache Provider:
-+2026-11-25T19:24:24.7942164Z     github
-+2026-11-25T19:24:24.7942415Z Workspaces:
-+2026-11-25T19:24:24.7942648Z     D:\a\MultiTunnel\MultiTunnel
-+2026-11-25T19:24:24.7942926Z Cache Paths:
-+2026-11-25T19:24:24.7943153Z     C:\Users\runneradmin\.cargo\bin
-+2026-11-25T19:24:24.7943462Z     C:\Users\runneradmin\.cargo\.crates.toml
-+2026-11-25T19:24:24.7943793Z     C:\Users\runneradmin\.cargo\.crates2.json
-+2026-11-25T19:24:24.7944103Z     C:\Users\runneradmin\.cargo\registry
-+2026-11-25T19:24:24.7944776Z     C:\Users\runneradmin\.cargo\git
-+2026-11-25T19:24:24.7947215Z     D:\a\MultiTunnel\MultiTunnel\target
-+2026-11-25T19:24:24.7947523Z Restore Key:
-+2026-11-25T19:24:24.7947759Z     v0-rust-test-Windows_NT-x64-ea627442
-+2026-11-25T19:24:24.7948042Z Cache Key:
-+2026-11-25T19:24:24.7948368Z     v0-rust-test-Windows_NT-x64-ea627442-95f3ec8d
-+2026-11-25T19:24:24.7957452Z .. Prefix:
-+2026-11-25T19:24:24.7973935Z   - v0-rust-test-Windows_NT-x64
-+2026-11-25T19:24:24.7974312Z .. Environment considered:
-+2026-11-25T19:24:24.7974591Z   - Rust Versions:
-+2026-11-25T19:24:24.7974954Z     - 1.100.0 x86_64-pc-windows-msvc 0ef1bc386059596dada8fb65b50425aab2ea637a
-+2026-11-25T19:24:24.7975597Z   - CARGO_HOME
-+2026-11-25T19:24:24.7975992Z   - CARGO_INCREMENTAL
-+2026-11-25T19:24:24.7976515Z   - CARGO_TERM_COLOR
-+2026-11-25T19:24:24.7977004Z .. Lockfiles considered:
-+2026-11-25T19:24:24.7977520Z   - D:\a\MultiTunnel\MultiTunnel\Cargo.lock
-+2026-11-25T19:24:24.7978172Z   - D:\a\MultiTunnel\MultiTunnel\Cargo.toml
-+2026-11-25T19:24:24.7979409Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-app\Cargo.toml
-+2026-11-25T19:24:24.7980291Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-bond\Cargo.toml
-+2026-11-25T19:24:24.7980604Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-engine\Cargo.toml
-+2026-11-25T19:24:24.7981224Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-links\Cargo.toml
-+2026-11-25T19:24:24.7981598Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-m3\Cargo.toml
-+2026-11-25T19:24:24.7981952Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-net\Cargo.toml
-+2026-11-25T19:24:24.7982295Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-platform\Cargo.toml
-+2026-11-25T19:24:24.7982655Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-service\Cargo.toml
-+2026-11-25T19:24:24.7983183Z ##[endgroup]
-+2026-11-25T19:24:24.7983290Z 
-+2026-11-25T19:24:24.7983371Z ... Restoring cache ...
-+2026-11-25T19:24:25.0208117Z Cache hit for restore-key: v0-rust-test-Windows_NT-x64-ea627442-39107a77
-+2026-11-25T19:24:26.1796902Z Received 100663296 of 477668883 (21.1%), 95.1 MBs/sec
-+2026-11-25T19:24:27.1819943Z Received 134217728 of 477668883 (28.1%), 63.6 MBs/sec
-+2026-11-25T19:24:28.2086379Z Received 268435456 of 477668883 (56.2%), 84.3 MBs/sec
-+2026-11-25T19:24:29.5299299Z Received 402653184 of 477668883 (84.3%), 88.2 MBs/sec
-+2026-11-25T19:24:30.1890693Z Received 477668883 of 477668883 (100.0%), 90.9 MBs/sec
-+2026-11-25T19:24:30.1890971Z Cache Size: ~456 MB (477668883 B)
-+2026-11-25T19:24:30.1893460Z [command]"C:\Program Files\Git\usr\bin\tar.exe" -xf D:/a/_temp/bb6ff527-90c0-48fb-9dfd-e0810962176d/cache.tzst -P -C D:/a/MultiTunnel/MultiTunnel --force-local --use-compress-program "zstd -d"
-+2026-11-25T19:24:40.0395865Z Cache restored successfully
-+2026-11-25T19:24:40.0698458Z Restored from cache key "v0-rust-test-Windows_NT-x64-ea627442-39107a77" full match: false.
-+2026-11-25T19:24:40.2999025Z ##[group]Run cargo test --workspace
-+2026-11-25T19:24:40.2999278Z [36;1mcargo test --workspace[0m
-+2026-11-25T19:24:40.3785764Z shell: C:\Program Files\PowerShell\7\pwsh.EXE -command ". '{0}'"
-+2026-11-25T19:24:40.3786011Z env:
-+2026-11-25T19:24:40.3786148Z   CARGO_HOME: C:\Users\runneradmin\.cargo
-+2026-11-25T19:24:40.3786314Z   CARGO_INCREMENTAL: 0
-+2026-11-25T19:24:40.3786441Z   CARGO_TERM_COLOR: always
-+2026-11-25T19:24:40.3786570Z   CACHE_ON_FAILURE: false
-+2026-11-25T19:24:40.3786697Z ##[endgroup]
-+2026-11-25T19:24:41.3481301Z [1m[92m    Updating[0m crates.io index
-+2026-11-25T19:24:53.0021223Z [1m[92m   Compiling[0m windows-sys v0.61.2
-+2026-11-25T19:24:53.0023641Z [1m[92m   Compiling[0m syn v3.0.10
-+2026-11-25T19:24:53.0024165Z [1m[92m   Compiling[0m syn v2.0.119
-+2026-11-25T19:24:53.0040527Z [1m[92m   Compiling[0m libm v0.2.16
-+2026-11-25T19:25:04.4217651Z [1m[92m   Compiling[0m socket2 v0.6.5
-+2026-11-25T19:25:04.5160824Z [1m[92m   Compiling[0m tokio-macros v2.7.2
-+2026-11-25T19:25:04.5717754Z [1m[92m   Compiling[0m mio v1.2.5
-+2026-11-25T19:25:05.1216007Z [1m[92m   Compiling[0m futures-macro v0.3.34
-+2026-11-25T19:25:05.6471547Z [1m[92m   Compiling[0m tracing-attributes v0.1.31
-+2026-11-25T19:25:05.7419676Z [1m[92m   Compiling[0m tokio v1.54.1
-+2026-11-25T19:25:05.8897172Z [1m[92m   Compiling[0m futures-util v0.3.34
-+2026-11-25T19:25:06.1393173Z [1m[92m   Compiling[0m serde_derive v1.0.229
-+2026-11-25T19:25:07.2185614Z [1m[92m   Compiling[0m tracing v0.1.44
-+2026-11-25T19:25:07.4553932Z [1m[92m   Compiling[0m synstructure v0.14.0
-+2026-11-25T19:25:08.1128138Z [1m[92m   Compiling[0m bitflags v2.13.2
-+2026-11-25T19:25:08.9146322Z [1m[92m   Compiling[0m futures-executor v0.3.34
-+2026-11-25T19:25:09.3639963Z [1m[92m   Compiling[0m serde v1.0.229
-+2026-11-25T19:25:09.6861341Z [1m[92m   Compiling[0m futures v0.3.34
-+2026-11-25T19:25:09.7574507Z [1m[92m   Compiling[0m zerofrom-derive v0.1.8
-+2026-11-25T19:25:10.0406584Z [1m[92m   Compiling[0m thiserror v2.0.21
-+2026-11-25T19:25:10.3310014Z [1m[92m   Compiling[0m yoke-derive v0.8.4
-+2026-11-25T19:25:10.6511457Z [1m[92m   Compiling[0m thiserror-impl v2.0.21
-+2026-11-25T19:25:10.9886267Z [1m[92m   Compiling[0m zerofrom v0.1.8
-+2026-11-25T19:25:11.0782296Z [1m[92m   Compiling[0m autocfg v1.5.1
-+2026-11-25T19:25:11.4093286Z [1m[92m   Compiling[0m yoke v0.8.4
-+2026-11-25T19:25:11.5339290Z [1m[92m   Compiling[0m num-traits v0.2.19
-+2026-11-25T19:25:11.5886603Z [1m[92m   Compiling[0m async-trait v0.1.92
-+2026-11-25T19:25:11.8377396Z [1m[92m   Compiling[0m bytemuck_derive v1.12.1
-+2026-11-25T19:25:12.0447990Z [1m[92m   Compiling[0m zerovec-derive v0.11.7
-+2026-11-25T19:25:13.0504189Z [1m[92m   Compiling[0m bytemuck v1.25.2
-+2026-11-25T19:25:13.2020215Z [1m[92m   Compiling[0m zerovec v0.11.9
-+2026-11-25T19:25:13.3040112Z [1m[92m   Compiling[0m displaydoc v0.2.7
-+2026-11-25T19:25:13.4169936Z [1m[92m   Compiling[0m simd-adler32 v0.3.10
-+2026-11-25T19:25:13.7583410Z [1m[92m   Compiling[0m crossbeam-utils v0.8.23
-+2026-11-25T19:25:13.7822918Z [1m[92m   Compiling[0m thiserror v1.0.69
-+2026-11-25T19:25:13.9783708Z [1m[92m   Compiling[0m tinystr v0.8.5
-+2026-11-25T19:25:14.0076535Z [1m[92m   Compiling[0m crossbeam-epoch v0.9.21
-+2026-11-25T19:25:14.0115232Z [1m[92m   Compiling[0m siphasher v1.0.4
-+2026-11-25T19:25:14.1463723Z [1m[92m   Compiling[0m phf_shared v0.13.1
-+2026-11-25T19:25:14.1874706Z [1m[92m   Compiling[0m icu_locale_core v2.4.0
-+2026-11-25T19:25:14.2988891Z [1m[92m   Compiling[0m zerotrie v0.2.5
-+2026-11-25T19:25:14.3978300Z [1m[92m   Compiling[0m miniz_oxide v0.9.1
-+2026-11-25T19:25:14.6255003Z [1m[92m   Compiling[0m potential_utf v0.1.6
-+2026-11-25T19:25:14.7597911Z [1m[92m   Compiling[0m thiserror-impl v1.0.69
-+2026-11-25T19:25:14.8236907Z [1m[92m   Compiling[0m crossbeam-deque v0.8.8
-+2026-11-25T19:25:15.0199922Z [1m[92m   Compiling[0m by_address v1.2.1
-+2026-11-25T19:25:15.0922576Z [1m[92m   Compiling[0m palette_derive v0.7.7
-+2026-11-25T19:25:15.2889171Z [1m[92m   Compiling[0m icu_provider v2.4.0
-+2026-11-25T19:25:15.6295543Z [1m[92m   Compiling[0m flate2 v1.1.11
-+2026-11-25T19:25:15.6796207Z [1m[92m   Compiling[0m icu_collections v2.4.0
-+2026-11-25T19:25:16.0457976Z [1m[92m   Compiling[0m phf v0.13.1
-+2026-11-25T19:25:16.1458434Z [1m[92m   Compiling[0m approx v0.5.1
-+2026-11-25T19:25:16.1523898Z [1m[92m   Compiling[0m font-types v0.7.3
-+2026-11-25T19:25:16.2460317Z [1m[92m   Compiling[0m slotmap v1.1.1
-+2026-11-25T19:25:16.3902872Z [1m[92m   Compiling[0m raw-window-handle v0.6.2
-+2026-11-25T19:25:16.3907893Z [1m[92m   Compiling[0m arrayvec v0.7.8
-+2026-11-25T19:25:16.4358361Z [1m[92m   Compiling[0m rayon-core v1.13.0
-+2026-11-25T19:25:16.5570609Z [1m[92m   Compiling[0m palette_math v0.7.7
-+2026-11-25T19:25:16.5786676Z [1m[92m   Compiling[0m smol_str v0.2.2
-+2026-11-25T19:25:16.5976070Z [1m[92m   Compiling[0m read-fonts v0.22.7
-+2026-11-25T19:25:16.6891199Z [1m[92m   Compiling[0m palette v0.7.7
-+2026-11-25T19:25:16.8004088Z [1m[92m   Compiling[0m glam v0.25.0
-+2026-11-25T19:25:16.8805866Z [1m[92m   Compiling[0m zerocopy v0.8.63
-+2026-11-25T19:25:17.1926392Z [1m[92m   Compiling[0m rustc-hash v2.1.3
-+2026-11-25T19:25:17.2720001Z [1m[92m   Compiling[0m web-time v1.1.0
-+2026-11-25T19:25:18.2293852Z [1m[92m   Compiling[0m icu_normalizer v2.4.0
-+2026-11-25T19:25:18.9757254Z [1m[92m   Compiling[0m icu_properties v2.4.0
-+2026-11-25T19:25:20.1237022Z [1m[92m   Compiling[0m mt-net v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-net)
-+2026-11-25T19:25:20.7684379Z [1m[92m   Compiling[0m euclid v0.22.14
-+2026-11-25T19:25:21.4815369Z [1m[92m   Compiling[0m zerocopy-derive v0.8.63
-+2026-11-25T19:25:22.0422674Z [1m[92m   Compiling[0m unicode-segmentation v1.13.3
-+2026-11-25T19:25:22.5143332Z [1m[92m   Compiling[0m iced_core v0.13.2
-+2026-11-25T19:25:22.6017452Z [1m[92m   Compiling[0m skrifa v0.22.3
-+2026-11-25T19:25:23.3555536Z [1m[92m   Compiling[0m zeno v0.2.3
-+2026-11-25T19:25:24.0336103Z [1m[92m   Compiling[0m ttf-parser v0.20.0
-+2026-11-25T19:25:24.1340564Z [1m[92m   Compiling[0m unicode-ccc v0.2.0
-+2026-11-25T19:25:24.2133837Z [1m[92m   Compiling[0m tinyvec v1.13.3
-+2026-11-25T19:25:24.5372818Z [1m[92m   Compiling[0m ttf-parser v0.21.1
-+2026-11-25T19:25:27.0584620Z [1m[92m   Compiling[0m unicode-bidi-mirroring v0.2.0
-+2026-11-25T19:25:27.1386657Z [1m[92m   Compiling[0m yazi v0.1.6
-+2026-11-25T19:25:27.7586734Z [1m[92m   Compiling[0m unicode-script v0.5.9
-+2026-11-25T19:25:27.9449455Z [1m[92m   Compiling[0m memmap2 v0.9.12
-+2026-11-25T19:25:28.1023150Z [1m[92m   Compiling[0m unicode-properties v0.1.5
-+2026-11-25T19:25:28.1328179Z [1m[92m   Compiling[0m either v1.19.0
-+2026-11-25T19:25:28.2639436Z [1m[92m   Compiling[0m rustybuzz v0.14.1
-+2026-11-25T19:25:28.2775564Z [1m[92m   Compiling[0m rayon v1.12.0
-+2026-11-25T19:25:28.2988037Z [1m[92m   Compiling[0m fontdb v0.16.2
-+2026-11-25T19:25:28.9303073Z [1m[92m   Compiling[0m swash v0.1.19
-+2026-11-25T19:25:28.9923040Z [1m[92m   Compiling[0m iced_futures v0.13.2
-+2026-11-25T19:25:30.4070233Z [1m[92m   Compiling[0m lyon_geom v1.0.20
-+2026-11-25T19:25:30.8623068Z [1m[92m   Compiling[0m idna_adapter v1.2.2
-+2026-11-25T19:25:30.9596348Z [1m[92m   Compiling[0m self_cell v1.3.0
-+2026-11-25T19:25:31.0354732Z [1m[92m   Compiling[0m windows_x86_64_msvc v0.52.6
-+2026-11-25T19:25:31.2372570Z [1m[92m   Compiling[0m unicode-linebreak v0.1.5
-+2026-11-25T19:25:31.4665393Z [1m[92m   Compiling[0m rustc-hash v1.1.0
-+2026-11-25T19:25:31.5293845Z [1m[92m   Compiling[0m rangemap v1.8.0
-+2026-11-25T19:25:31.7983698Z [1m[92m   Compiling[0m unicode-bidi v0.3.18
-+2026-11-25T19:25:32.4184678Z [1m[92m   Compiling[0m sys-locale v0.3.2
-+2026-11-25T19:25:32.5299823Z [1m[92m   Compiling[0m cosmic-text v0.12.1
-+2026-11-25T19:25:32.5428019Z [1m[92m   Compiling[0m image v0.24.9
-+2026-11-25T19:25:33.5398294Z [1m[92m   Compiling[0m half v2.7.1
-+2026-11-25T19:25:34.0721736Z [1m[92m   Compiling[0m hyper v1.12.0
-+2026-11-25T19:25:34.3471742Z [1m[92m   Compiling[0m idna v1.1.0
-+2026-11-25T19:25:35.2521246Z [1m[92m   Compiling[0m lyon_path v1.0.19
-+2026-11-25T19:25:35.3739604Z [1m[92m   Compiling[0m miniz_oxide v0.8.9
-+2026-11-25T19:25:35.4840778Z [1m[92m   Compiling[0m fdeflate v0.3.7
-+2026-11-25T19:25:35.8578552Z [1m[92m   Compiling[0m arrayref v0.3.9
-+2026-11-25T19:25:35.9010753Z [1m[92m   Compiling[0m bitflags v1.3.2
-+2026-11-25T19:25:35.9481477Z [1m[92m   Compiling[0m strict-num v0.1.1
-+2026-11-25T19:25:35.9596436Z [1m[92m   Compiling[0m hyper-util v0.1.21
-+2026-11-25T19:25:36.0383119Z [1m[92m   Compiling[0m tiny-skia-path v0.11.4
-+2026-11-25T19:25:36.1404801Z [1m[92m   Compiling[0m tower v0.5.3
-+2026-11-25T19:25:36.1979122Z [1m[92m   Compiling[0m png v0.17.16
-+2026-11-25T19:25:36.6859024Z [1m[92m   Compiling[0m iced_graphics v0.13.0 (D:\a\MultiTunnel\MultiTunnel\vendor\iced_graphics)
-+2026-11-25T19:25:36.9984915Z [1m[92m   Compiling[0m url v2.5.8
-+2026-11-25T19:25:37.6186351Z [1m[92m   Compiling[0m tokio-rustls v0.26.6
-+2026-11-25T19:25:37.7106084Z [1m[92m   Compiling[0m cfg_aliases v0.2.2
-+2026-11-25T19:25:37.7831273Z [1m[92m   Compiling[0m serde_urlencoded v0.7.1
-+2026-11-25T19:25:37.8665963Z [1m[92m   Compiling[0m hyper-rustls v0.27.10
-+2026-11-25T19:25:37.9974535Z [1m[92m   Compiling[0m winit v0.30.13
-+2026-11-25T19:25:38.0037233Z [1m[92m   Compiling[0m windows-targets v0.52.6
-+2026-11-25T19:25:38.1312843Z [1m[92m   Compiling[0m tower-http v0.6.11
-+2026-11-25T19:25:38.1579155Z [1m[92m   Compiling[0m tiny-skia v0.11.4
-+2026-11-25T19:25:38.3204562Z [1m[92m   Compiling[0m kurbo v0.10.4
-+2026-11-25T19:25:38.5270343Z [1m[92m   Compiling[0m softbuffer v0.4.8
-+2026-11-25T19:25:38.5858540Z [1m[92m   Compiling[0m error-code v3.4.0
-+2026-11-25T19:25:38.7318350Z [1m[92m   Compiling[0m winapi v0.3.9
-+2026-11-25T19:25:39.1703203Z [1m[92m   Compiling[0m reqwest v0.12.28
-+2026-11-25T19:25:39.4300700Z [1m[92m   Compiling[0m iced_tiny_skia v0.13.0 (D:\a\MultiTunnel\MultiTunnel\vendor\iced_tiny_skia)
-+2026-11-25T19:25:40.0317854Z [1m[92m   Compiling[0m clipboard-win v5.4.3
-+2026-11-25T19:25:40.4566948Z [1m[92m   Compiling[0m windows-sys v0.52.0
-+2026-11-25T19:25:40.9251368Z [1m[92m   Compiling[0m iced_runtime v0.13.2
-+2026-11-25T19:25:40.9560897Z [1m[92m   Compiling[0m dpi v0.1.2
-+2026-11-25T19:25:41.1016723Z [1m[92m   Compiling[0m cursor-icon v1.2.0
-+2026-11-25T19:25:41.1845864Z [1m[92m   Compiling[0m window_clipboard v0.4.1
-+2026-11-25T19:25:41.2881108Z [1m[92m   Compiling[0m iced_renderer v0.13.0
-+2026-11-25T19:25:41.5062942Z [1m[92m   Compiling[0m iced_widget v0.13.4
-+2026-11-25T19:25:42.0094422Z [1m[92m   Compiling[0m material-colors v0.5.0
-+2026-11-25T19:25:43.2353061Z [1m[92m   Compiling[0m mt-engine v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-engine)
-+2026-11-25T19:25:43.8735124Z [1m[92m   Compiling[0m mt-links v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-links)
-+2026-11-25T19:25:44.7460475Z [1m[92m   Compiling[0m mt-service v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-service)
-+2026-11-25T19:25:45.3674881Z [1m[92m   Compiling[0m zip v2.6.1
-+2026-11-25T19:25:45.4478304Z [1m[92m   Compiling[0m zopfli v0.8.4
-+2026-11-25T19:25:45.5572330Z [1m[92m   Compiling[0m mt-bond v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-bond)
-+2026-11-25T19:25:45.7664603Z [1m[92m   Compiling[0m mt-platform v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-platform)
-+2026-11-25T19:25:46.4720054Z [1m[92m   Compiling[0m iced_winit v0.13.0
-+2026-11-25T19:25:47.1015365Z [1m[92m   Compiling[0m iced v0.13.1
-+2026-11-25T19:25:47.3829484Z [1m[92m   Compiling[0m mt-m3 v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-m3)
-+2026-11-25T19:25:47.4014787Z [1m[92m   Compiling[0m winreg v0.56.0
-+2026-11-25T19:25:47.7959816Z [1m[92m   Compiling[0m mt-app v0.15.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-app)
-+2026-11-25T19:25:50.5573317Z [1m[92m   Compiling[0m multitunnel v0.15.0 (D:\a\MultiTunnel\MultiTunnel)
-+2026-11-25T19:26:13.5347483Z [1m[92m    Finished[0m `test` profile [unoptimized + debuginfo] target(s) in 1m 32s
-+2026-11-25T19:26:13.5606886Z [1m[92m     Running[0m unittests src\lib.rs (target\debug\build\mt-app/d1f15f5586502f32\out\mt_app-d1f15f5586502f32.exe)
-+2026-11-25T19:26:13.5714684Z 
-+2026-11-25T19:26:13.5715686Z running 66 tests
-+2026-11-25T19:26:13.5726533Z test app::tests::app_version_is_release_triplet ... ok
-+2026-11-25T19:26:13.5801477Z test app::tests::adapter_rate_sampling_deltas_and_resets ... ok
-+2026-11-25T19:26:13.5803844Z test app::tests::close_action_decision ... ok
-+2026-11-25T19:26:13.5810019Z test app::tests::close_to_tray_settings_roundtrip ... ok
-+2026-11-25T19:26:13.5847563Z test app::tests::begin_exit_is_idempotent_without_tray ... ok
-+2026-11-25T19:26:13.5847987Z test app::tests::always_on_top_menu_item_roundtrips ... ok
-+2026-11-25T19:26:13.5850520Z test app::tests::fmt_helpers ... ok
-+2026-11-25T19:26:13.5852803Z test app::tests::kill_switch_banner_gate ... ok
-+2026-11-25T19:26:13.5855204Z test app::tests::link_matches_adapter_shapes ... ok
-+2026-11-25T19:26:13.5857269Z test app::tests::log_filter_matches ... ok
-+2026-11-25T19:26:13.5897683Z test app::tests::adapters_screen_toggle_move_apply_revert ... ok
-+2026-11-25T19:26:13.5910819Z test app::tests::logs_autoscroll_default_and_toggle ... ok
-+2026-11-25T19:26:13.5913427Z test app::tests::logs_scroll_id_stable ... ok
-+2026-11-25T19:26:13.5915882Z test app::tests::edit_dialog_open_type_save_close ... ok
-+2026-11-25T19:26:13.5916311Z test app::tests::open_folder_command_targets_the_os_file_manager ... ok
-+2026-11-25T19:26:13.5918029Z test app::tests::rule_editor_add_edit_remove ... ok
-+2026-11-25T19:26:13.5951288Z test app::tests::logs_autoscroll_gated_on_screen ... ok
-+2026-11-25T19:26:13.5953853Z test app::tests::edit_save_failure_is_logged ... ok
-+2026-11-25T19:26:13.5954170Z test app::tests::seed_parse ... ok
-+2026-11-25T19:26:13.5956269Z test app::tests::smoke_flag_parse ... ok
-+2026-11-25T19:26:13.6007273Z test app::tests::session_totals_accumulate_and_reset ... ok
-+2026-11-25T19:26:13.6009666Z test app::tests::sort_adapter_rows_selected_first_in_saved_order ... ok
-+2026-11-25T19:26:13.6021753Z test app::tests::s21_qos_toggle_updates_draft ... ok
-+2026-11-25T19:26:13.6027882Z test app::tests::s19_settings_toggles_and_modal_gating ... ok
-+2026-11-25T19:26:13.6310721Z test app::tests::theme_reroutes_on_settings_messages ... ok
-+2026-11-25T19:26:13.6349970Z test app::tests::title_menu_anchor_clamps ... ok
-+2026-11-25T19:26:13.6351692Z test app::tests::titlebar_double_click_window ... ok
-+2026-11-25T19:26:13.6690273Z test app::tests::smoke_run_drives_app_with_mock ... ok
-+2026-11-25T19:26:13.6838038Z test app::tests::wheel_drag_updates_seed_live_and_persists_on_release ... ok
-+2026-11-25T19:26:13.6840457Z test graph::tests::local_bounds_strips_widget_offset_regression ... ok
-+2026-11-25T19:26:13.6841873Z test graph::tests::nice_ceiling_covers_and_rounds ... ok
-+2026-11-25T19:26:13.6843094Z test graph::tests::points_stay_inside_bounds_and_right_align ... ok
-+2026-11-25T19:26:13.6844690Z test graph::tests::rates_format_human ... ok
-+2026-11-25T19:26:13.6845767Z test mock::tests::fmt_utc_known_values ... ok
-+2026-11-25T19:26:14.4214742Z test app::tests::speed_test_message_flow ... ok
-+2026-11-25T19:26:14.4215666Z test app::tests::speed_upload_test_message_flow ... ok
-+2026-11-25T19:26:14.4220043Z test mock::tests::mock_profiles_import_remove ... ok
-+2026-11-25T19:26:14.4223464Z test mock::tests::mock_settings_and_rules ... ok
-+2026-11-25T19:26:14.4261580Z test theme::tests::button_states_use_state_layers ... ok
-+2026-11-25T19:26:14.4263078Z test theme::tests::hsv_conversions_roundtrip_primaries ... ok
-+2026-11-25T19:26:14.4300188Z test theme::tests::switch_styles_differ_by_state ... ok
-+2026-11-25T19:26:14.4302716Z test theme::tests::system_prefers_dark_env_override_wins ... ok
-+2026-11-25T19:26:14.4339006Z test theme::tests::text_color_from_m3 ... ok
-+2026-11-25T19:26:14.4480254Z test theme::tests::theme_derivation_seed_changes_scheme ... ok
-+2026-11-25T19:26:14.4552559Z test theme::tests::theme_from_settings_and_rederive ... ok
-+2026-11-25T19:26:14.4553918Z test theme::tests::wheel_asset_dimensions ... ok
-+2026-11-25T19:26:14.4555265Z test theme::tests::wheel_marker_roundtrips_picked_points ... ok
-+2026-11-25T19:26:14.4556915Z test theme::tests::wheel_seed_clamps_outside_points_to_the_rim ... ok
-+2026-11-25T19:26:14.4557853Z test theme::tests::wheel_seed_matches_cardinal_hues ... ok
-+2026-11-25T19:26:14.4559354Z test tray::tests::ico_embedded_asset_parses_with_32_entry ... ok
-+2026-11-25T19:26:14.4560782Z test tray::tests::ico_embedded_mono_asset_parses_with_32_entry ... ok
-+2026-11-25T19:26:14.4562128Z test tray::tests::ico_rejects_bad_directories ... ok
-+2026-11-25T19:26:14.4563502Z test tray::tests::ids_roundtrip_and_skip_zero ... ok
-+2026-11-25T19:26:14.4565129Z test tray::tests::menu_connected_shows_disconnect_and_servers ... ok
-+2026-11-25T19:26:14.4566354Z test tray::tests::menu_disconnected_shows_connect ... ok
-+2026-11-25T19:26:14.4567770Z test tray::tests::menu_failed_is_disconnected_shape_with_honest_header ... ok
-+2026-11-25T19:26:14.4569244Z test tray::tests::menu_transition_disables_the_toggle ... ok
-+2026-11-25T19:26:14.4570450Z test tray::tests::model_from_snapshot_states ... ok
-+2026-11-25T19:26:14.4572242Z test tray::tests::tip_array_never_overruns_or_splits_surrogates ... ok
-+2026-11-25T19:26:14.4573737Z test tray::tests::to_wide_is_nul_terminated ... ok
-+2026-11-25T19:26:14.4575343Z test tray::tests::tooltip_live_formats_speeds_and_totals ... ok
-+2026-11-25T19:26:14.4576459Z test tray::tests::tooltip_states ... ok
-+2026-11-25T19:26:14.4577870Z test tray::tests::tray_icon_follows_connection_state ... ok
-+2026-11-25T19:26:14.4834346Z test mock::tests::mock_latency_test ... ok
-+2026-11-25T19:26:14.5462269Z test app::tests::update_loop_screens_and_rules ... ok
-+2026-11-25T19:26:14.5933855Z test mock::tests::mock_connect_disconnect_lifecycle ... ok
-+2026-11-25T19:26:14.5934066Z 
-+2026-11-25T19:26:14.5934264Z test result: ok. 66 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.02s
-+2026-11-25T19:26:14.5934472Z 
-+2026-11-25T19:26:14.5964462Z [1m[92m     Running[0m unittests src\main.rs (target\debug\build\mt-app/95d8485f91625a14\out\mt_app-95d8485f91625a14.exe)
-+2026-11-25T19:26:14.6049241Z 
-+2026-11-25T19:26:14.6049615Z running 0 tests
-+2026-11-25T19:26:14.6049738Z 
-+2026-11-25T19:26:14.6050070Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
-+2026-11-25T19:26:14.6050281Z 
-+2026-11-25T19:26:14.6065105Z [1m[92m     Running[0m unittests src\lib.rs (target\debug\build\mt-bond/6410ae8cc506944c\out\mt_bond-6410ae8cc506944c.exe)
-+2026-11-25T19:26:14.6142839Z 
-+2026-11-25T19:26:14.6143069Z running 33 tests
-+2026-11-25T19:26:14.6149633Z test endpoint::scheduling_tests::balanced_blest_guard_never_bypasses_the_score_leader ... ok
-+2026-11-25T19:26:14.6151555Z test endpoint::scheduling_tests::balanced_capacity_aware_bypasses_the_saturated_adapter ... ok
-+2026-11-25T19:26:14.6152365Z test endpoint::scheduling_tests::balanced_blest_guard_skips_slow_draining_tied_carrier ... ok
-+2026-11-25T19:26:14.6152908Z test endpoint::scheduling_tests::balanced_picks_the_least_backlogged_carrier ... ok
-+2026-11-25T19:26:14.6153454Z test endpoint::scheduling_tests::balanced_spreads_proportionally_to_reported_capacity ... ok
-+2026-11-25T19:26:14.6155045Z test endpoint::scheduling_tests::balanced_ties_rotate ... ok
-+2026-11-25T19:26:14.6156223Z test endpoint::scheduling_tests::carrier_score_is_relative_load_with_probe_floor ... ok
-+2026-11-25T19:26:14.6156748Z test endpoint::scheduling_tests::ecf_completion_is_rtt_plus_drain_time ... ok
-+2026-11-25T19:26:14.6157589Z test endpoint::scheduling_tests::failover_takes_the_first_live_carrier ... ok
-+2026-11-25T19:26:14.6158586Z test endpoint::scheduling_tests::lowest_latency_ecf_prefers_earliest_completion ... ok
-+2026-11-25T19:26:14.6160297Z test endpoint::scheduling_tests::lowest_latency_prefers_sampled_rtt_then_known_over_unknown ... ok
-+2026-11-25T19:26:14.6160874Z test endpoint::scheduling_tests::mixed_reports_probe_the_unreported_carrier ... ok
-+2026-11-25T19:26:14.6161386Z test endpoint::scheduling_tests::priority_fills_in_order_until_the_queue_backs_up ... ok
-+2026-11-25T19:26:14.6161941Z test endpoint::scheduling_tests::priority_spills_when_relative_load_exceeds_ratio ... ok
-+2026-11-25T19:26:14.6165167Z test endpoint::scheduling_tests::round_robin_cycles_the_cursor ... ok
-+2026-11-25T19:26:14.6165987Z test frame::tests::encode_session_borrowed_is_wire_identical ... ok
-+2026-11-25T19:26:14.6166401Z test frame::tests::empty_and_max_payloads_roundtrip ... ok
-+2026-11-25T19:26:14.6168933Z test frame::tests::header_rejects_bad_magic_version_kind_and_size ... ok
-+2026-11-25T19:26:14.6169804Z test frame::tests::roundtrip_all_kinds ... ok
-+2026-11-25T19:26:14.6171698Z test frame::tests::read_write_frame_over_duplex ... ok
-+2026-11-25T19:26:14.6172519Z test frame::tests::target_rejects_truncated_and_garbage ... ok
-+2026-11-25T19:26:14.6172920Z test frame::tests::target_roundtrip_domain_ipv4_ipv6 ... ok
-+2026-11-25T19:26:14.6175550Z test reorder::tests::cap_zero_still_delivers_in_order ... ok
-+2026-11-25T19:26:14.6177482Z test outbound::tests::ensure_carriers_fails_cleanly_with_no_links ... ok
-+2026-11-25T19:26:14.6178409Z test outbound::tests::ensure_carriers_reports_all_dead_links ... ok
-+2026-11-25T19:26:14.6180051Z test reorder::tests::duplicates_and_replays_are_idempotent ... ok
-+2026-11-25T19:26:14.6180521Z test reorder::tests::gap_buffers_then_flushes ... ok
-+2026-11-25T19:26:14.6180835Z test reorder::tests::in_order_delivery ... ok
-+2026-11-25T19:26:14.6182276Z test reorder::tests::long_out_of_order_run_reorders_completely ... ok
-+2026-11-25T19:26:14.6182775Z test reorder::tests::overflow_is_an_error ... ok
-+2026-11-25T19:26:14.6183254Z test reorder::tests::would_overflow_mirrors_the_insert_boundary ... ok
-+2026-11-25T19:26:14.6183612Z test tests::constants_are_coherent ... ok
-+2026-11-25T19:26:14.8196457Z test daemon::tests::serve_binds_and_shuts_down ... ok
-+2026-11-25T19:26:14.8196661Z 
-+2026-11-25T19:26:14.8196817Z test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
-+2026-11-25T19:26:14.8197039Z 
-+2026-11-25T19:26:14.8219663Z [1m[92m     Running[0m unittests src\bin\mt_merge_server.rs (target\debug\build\mt-bond/fbee07725169cdd9\out\mt_merge_server-fbee07725169cdd9.exe)
-+2026-11-25T19:26:14.8298404Z 
-+2026-11-25T19:26:14.8298830Z running 0 tests
-+2026-11-25T19:26:14.8299012Z 
-+2026-11-25T19:26:14.8299272Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
-+2026-11-25T19:26:14.8299584Z 
-+2026-11-25T19:26:14.8312887Z [1m[92m     Running[0m tests\integration.rs (target\debug\build\mt-bond/38eeb9b5e0d206b1\out\integration-38eeb9b5e0d206b1.exe)
-+2026-11-25T19:26:14.8450527Z 
-+2026-11-25T19:26:14.8450761Z running 6 tests
-+2026-11-25T19:26:15.7092937Z test carrier_death_mid_stream_fails_over_without_data_loss ... ok
-+2026-11-25T19:26:15.9260015Z test echo_roundtrip_1mib_across_two_carriers ... ok
-+2026-11-25T19:26:17.9661862Z test concurrent_sessions_multiplex_over_the_same_carriers ... ok
-+2026-11-25T19:26:17.9965191Z test refused_target_surfaces_as_connection_reset ... ok
-+2026-11-25T19:26:18.8089624Z test one_dead_link_still_yields_a_working_bond ... ok
-+2026-11-25T19:27:14.8470176Z test domain_targets_dial_through_the_daemon has been running for over 60 seconds
-+2026-11-25T19:27:14.8602344Z test domain_targets_dial_through_the_daemon ... FAILED
-+2026-11-25T19:27:14.8602705Z 
-+2026-11-25T19:27:14.8602801Z failures:
-+2026-11-25T19:27:14.8602945Z 
-+2026-11-25T19:27:14.8603141Z ---- domain_targets_dial_through_the_daemon stdout ----
-+2026-11-25T19:27:14.8603555Z [mt-merge-server] listening on 127.0.0.1:53382
-+2026-11-25T19:27:14.8603773Z 
-+2026-11-25T19:27:14.8604134Z thread 'domain_targets_dial_through_the_daemon' (5264) panicked at crates\mt-bond\tests\integration.rs:382:6:
-+2026-11-25T19:27:14.8605993Z deadline: domain_targets wedged (documented runner flake — see LEDGER): Elapsed(())
-+2026-11-25T19:27:14.8606674Z note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
-+2026-11-25T19:27:14.8635072Z 
-+2026-11-25T19:27:14.8635095Z 
-+2026-11-25T19:27:14.8635245Z failures:
-+2026-11-25T19:27:14.8635482Z     domain_targets_dial_through_the_daemon
-+2026-11-25T19:27:14.8635717Z 
-+2026-11-25T19:27:14.8636030Z test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 60.01s
-+2026-11-25T19:27:14.8636354Z 
-+2026-11-25T19:27:14.8636889Z [1m[91merror[0m: test failed, to rerun pass `-p mt-bond --test integration`
-+2026-11-25T19:27:14.9661219Z ##[error]Process completed with exit code 1.
-+2026-11-25T19:27:14.9870259Z Post job cleanup.
-+2026-11-25T19:27:15.1215911Z [command]"C:\Program Files\Git\bin\git.exe" version
-+2026-11-25T19:27:15.1493558Z git version 2.56.1.windows.1
-+2026-11-25T19:27:15.1552233Z Temporarily overriding HOME='D:\a\_temp\7d0e8ec3-2f8a-476d-9375-f46060ba4ff6' before making global git config changes
-+2026-11-25T19:27:15.1552972Z Adding repository directory to the temporary git global config as a safe directory
-+2026-11-25T19:27:15.1561204Z [command]"C:\Program Files\Git\bin\git.exe" config --global --add safe.directory D:\a\MultiTunnel\MultiTunnel
-+2026-11-25T19:27:15.1990879Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp core\.sshCommand
-+2026-11-25T19:27:15.2319693Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "sh -c \"git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :\""
-+2026-11-25T19:27:15.8828452Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
-+2026-11-25T19:27:15.9128105Z http.https://github.com/.extraheader
-+2026-11-25T19:27:15.9171266Z [command]"C:\Program Files\Git\bin\git.exe" config --local --unset-all http.https://github.com/.extraheader
-+2026-11-25T19:27:15.9487190Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "sh -c \"git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :\""
-+2026-11-25T19:27:16.6826514Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp ^includeIf\.gitdir:
-+2026-11-25T19:27:16.7160686Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "git config --local --show-origin --name-only --get-regexp remote.origin.url"
-+2026-11-25T19:27:17.4061237Z Cleaning up orphan processes
-+2026-11-25T19:27:17.4369910Z Terminate orphan process: pid (7748) (vctip)
-+2026-11-25T19:27:17.4723374Z ##[warning]Node.js 20 is deprecated. The following actions target Node.js 20 but are being forced to run on Node.js 24: actions/checkout@v4. For more information see: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/
-diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
-index 19fb31f..bf657e1 100644
---- a/harness/memory/store/memory_graph.json
-+++ b/harness/memory/store/memory_graph.json
-@@ -1,6 +1,6 @@
- {
-   "schema_version": 1,
--  "as_of": "033c009 2026-11-24T20:57:46Z",
-+  "as_of": "21fd59f 2026-11-25T19:24:03Z",
-   "sources": [
-     "PROJECT_MEMORY.md",
-     "harness/memory/STATE.md",
-@@ -11,8 +11,8 @@
-     "git (tags + full log)",
-     "harness/memory/curated/"
-   ],
--  "node_count": 291,
--  "edge_count": 299,
-+  "node_count": 300,
-+  "edge_count": 308,
-   "nodes": [
-     {
-       "id": "bug:freedom-loopback-blackhole",
-@@ -215,6 +215,26 @@
-         }
-       ]
-     },
-+    {
-+      "id": "commit:06c87de",
-+      "type": "commit",
-+      "title": "backup(s22-link-adapter): 2026-11-25T18:37:56Z",
-+      "tags": [
-+        "2026-11-25"
-+      ],
-+      "sources": [
-+        {
-+          "file": "git:log",
-+          "anchor": "06c87de"
-+        }
-+      ],
-+      "passages": [
-+        {
-+          "text": "backup(s22-link-adapter): 2026-11-25T18:37:56Z",
-+          "kind": "title"
-+        }
-+      ]
-+    },
-     {
-       "id": "commit:07ef5c5",
-       "type": "commit",
-@@ -355,6 +375,26 @@
-         }
-       ]
-     },
-+    {
-+      "id": "commit:194eddc",
-+      "type": "commit",
-+      "title": "backup(s22-upload-test): 2026-11-25T18:34:16Z",
-+      "tags": [
-+        "2026-11-25"
-+      ],
-+      "sources": [
-+        {
-+          "file": "git:log",
-+          "anchor": "194eddc"
-+        }
-+      ],
-+      "passages": [
-+        {
-+          "text": "backup(s22-upload-test): 2026-11-25T18:34:16Z",
-+          "kind": "title"
-+        }
-+      ]
-+    },
-     {
-       "id": "commit:1a9849f",
-       "type": "commit",
-@@ -415,6 +455,26 @@
-         }
-       ]
-     },
-+    {
-+      "id": "commit:21fd59f",
-+      "type": "commit",
-+      "title": "backup(s22-release-prep): 2026-11-25T19:24:03Z",
-+      "tags": [
-+        "2026-11-25"
-+      ],
-+      "sources": [
-+        {
-+          "file": "git:log",
-+          "anchor": "21fd59f"
-+        }
-+      ],
-+      "passages": [
-+        {
-+          "text": "backup(s22-release-prep): 2026-11-25T19:24:03Z",
-+          "kind": "title"
-+        }
-+      ]
-+    },
-     {
-       "id": "commit:229fda4",
-       "type": "commit",
-@@ -795,6 +855,26 @@
-         }
-       ]
-     },
-+    {
-+      "id": "commit:51e0144",
-+      "type": "commit",
-+      "title": "backup(s22-rig-verified): 2026-11-25T19:10:03Z",
-+      "tags": [
-+        "2026-11-25"
-+      ],
-+      "sources": [
-+        {
-+          "file": "git:log",
-+          "anchor": "51e0144"
-+        }
-+      ],
-+      "passages": [
-+        {
-+          "text": "backup(s22-rig-verified): 2026-11-25T19:10:03Z",
-+          "kind": "title"
-+        }
-+      ]
-+    },
-     {
-       "id": "commit:52f1d3c",
-       "type": "commit",
-@@ -1195,6 +1275,26 @@
-         }
-       ]
-     },
-+    {
-+      "id": "commit:6fc0708",
-+      "type": "commit",
-+      "title": "backup(s22-log-folder-fix): 2026-11-25T18:43:12Z",
-+      "tags": [
-+        "2026-11-25"
-+      ],
-+      "sources": [
-+        {
-+          "file": "git:log",
-+          "anchor": "6fc0708"
-+        }
-+      ],
-+      "passages": [
-+        {
-+          "text": "backup(s22-log-folder-fix): 2026-11-25T18:43:12Z",
-+          "kind": "title"
-+        }
-+      ]
-+    },
-     {
-       "id": "commit:702293b",
-       "type": "commit",
-@@ -1535,6 +1635,26 @@
++++ b/docs/research/2026-11-27-stability-changelog-survey.md
+@@ -0,0 +1,86 @@
++# Stability & changelog survey (S23, 2026-11-27)
++
++Scope: the user's ask — "check for any improvements that can be made to
++the connection in terms of speed and stability, and to the UI by
++checking the changelogs of similar applications and implementing
++anything that has worked for them." This is the second changelog pass
++(first: 2026-11-21, `2026-11-21-ui-changelog-survey.md`; the bonding
++literature lives in `2026-11-21-bonding-performance-survey.md`). Sources
++below were read on 2026-11-27 from the projects' own release pages.
++
++## Sources reviewed
++
++- **v2rayN** 7.25.6 (2026-10-11) — the import-compatibility reference:
++  https://github.com/2dust/v2rayN/releases/tag/7.25.6
++  plus the release-history summaries at v2rayn.org mirrors ("computer
++  base rebuilt to launch faster and run more steadily", "global
++  Fragment control", "a rare crash during server switching repaired",
++  "real delay test").
++- **Clash Verge Rev** v2.5.10 (2026-11-03):
++  https://github.com/clash-verge-rev/clash-verge-rev/releases/tag/v2.5.10
++- **Hiddify** app changelog + core feature pages (Balancer semantics,
++  connection-info home screen).
++- **Speedify** release-notes pages (17: redundant-mode streaming
++  enhancements; bonding-mode overview; the socket/buffer-oriented
++  engineering posts behind them).
++- **Windows engineering references** for the os-error-50 root cause:
++  learn.microsoft.com System Error Codes (ERROR_NOT_SUPPORTED = 50),
++  the CreateProcess-with-explorer error-code-50 Stack Overflow case,
++  and the subprocess handle-inheritance writeups that pin it to
++  GUI-subsystem parents (null STD_INPUT_HANDLE → DuplicateHandle fails).
++
++## Adopted (shipped in this branch)
++
++| Finding (source, verbatim where it matters) | What shipped |
++|---|---|
++| v2rayN 7.25.6: "日志和临时文件自动清理时间默认为 7 天" — automatic log/temp-file cleanup, 7-day default | `diagnostics::sweep_old_logs` — 7-day retention, newest-40 floor, live diagnostics file protected, best-effort, runs at every `RealService::with` construction with an INFO line |
++| Window geometry persistence — a standing convention in every desktop client (v2rayN guiNConfig, Clash Verge, VS Code) | `SettingsView.window_w/window_h` (serde-default 1080×720, D8); debounced save after `Resized` (2 quiet ticks), clamped to the 880×580 window minimum; restored via `iced::window::resize` on load; reset-to-defaults restores stock |
++| Speedify-style socket-buffer engineering on WAN legs (their bonding-mode posts discuss per-path window sizing; the MPTCP literature says the same: window-limited paths cap aggregate throughput) | `daemon::tune_wan_stream` — 1 MiB SO_SNDBUF/SO_RCVBUF + existing TCP_NODELAY on the merge daemon's carrier-accept and target-dial legs (the WAN sockets MultiTunnel owns; loopback engine hops intentionally left on OS defaults). Rationale: BDP of a 150 Mbps × 50 ms carrier ≈ 0.94 MiB |
++| The os-error-50 report itself (user, this session) + the documented Windows cause | `mt-app::shell_open` — ShellExecuteW primary + NUL-stdio explorer fallback; the regression write-up lives in the module docs |
++| Hiddify / v2rayN settings-reset affordances ("How to Reset HiddifyManager to default" is one of their most-linked docs) | The Settings "Reset" card (two-click confirm; scope caption; profiles/rules untouched) |
++
++## Confirmed parity (checked, nothing to do)
++
++- v2rayN "real delay" latency tests: `test_latency` already builds the
++  real links and probes through them (S20's first-link fix).
++- Hiddify Balancer (round-robin / lowest-delay / member stickiness):
++  MultiTunnel's five merge policies (D23 + S18 BLEST/ECF) are a superset.
++- Hiddify "connection info when connected — active node, speed, IP":
++  Status screen + titlebar rates + tray tooltip (S17/S18/S22).
++- v2rayN 7.25.6 "lock the maximum core version": the engine is
++  self-provisioned and pinned (D14).
++- Clash Verge 2.5.10's headline fixes (GLOBAL policy group, SSH
++  private-key service mode, GEO DB sync, DNS-overwrite toggle showing
++  stale state): all are Clash-specific machinery; the "toggle shows a
++  state that is not real" class was already fixed here in S22
++  (diagnostics restore-at-construction).
++
++## Deliberately NOT adopted (with reasons)
++
++- **Mux settings in the core UI** (v2rayN #10055): the client→engine leg
++  already carries every proxied connection over one local hop per
++  engine; adding mux knobs would expose Xray-internal knobs this
++  project deliberately hides behind the profile model.
++- **MASQUE / new core protocols** (v2rayN 7.25.6): protocol surface
++  growth is a maintenance stream; the VLESS+REALITY + stream-bonding
++  path is this project's scope.
++- **Custom HTTP headers for subscription updates** (v2rayN #10118):
++  MultiTunnel's subscription support is vless-link paste/import today;
++  headers belong to that future feature, not now.
++- **Redundant mode** (Speedify 17's streaming duplicate-ship): sending
++  every frame on ALL carriers doubles bandwidth — the right behavior
++  for video calls, wrong default for a general tunnel. MultiTunnel's
++  failover + quarantine + BLEST already cover failure-driven switching
++  without the constant duplicate cost. Worth revisiting only as an
++  opt-in "redundant streaming" policy (roadmap).
++- **TUN mode** (Clash Verge / FlClash): rejected in the 2026-11-21
++  survey and unchanged — driver-level risk class.
++
++## Standing perf findings (unchanged from the 2026-11-21 surveys)
++
++The remaining big-ticket perf items are the roadmap's scheduling and
++HoL-blocking work (RTT-trend score, per-carrier active window
++probing), not connection-setup knobs: the S21 work already removed
++Nagle stalls and send-path copies, and the client's own legs are
++loopback (the WAN legs belong to the Xray engines and now the tuned
++merge daemon).
+diff --git a/src/diagnostics.rs b/src/diagnostics.rs
+index d532bbb..d312b1b 100644
+--- a/src/diagnostics.rs
++++ b/src/diagnostics.rs
+@@ -394,12 +394,17 @@ mod sweep_tests {
+     #[test]
+     fn missing_dir_and_subdirs_are_not_errors() {
+         let dir = scratch("missing").join("nope");
+-        assert!(sweep_old_logs(&dir, SystemTime::now(), std::time::Duration::ZERO, 0, None)
+-            .is_empty());
++        assert!(
++            sweep_old_logs(&dir, SystemTime::now(), std::time::Duration::ZERO, 0, None).is_empty()
++        );
+         // A subdirectory is never touched.
+         let real = scratch("subdir");
+         std::fs::create_dir_all(real.join("nested")).expect("subdir");
+-        let _ = touch(&real, "old.log", Some(std::time::Duration::from_secs(10_000_000)));
++        let _ = touch(
++            &real,
++            "old.log",
++            Some(std::time::Duration::from_secs(10_000_000)),
++        );
+         let pruned = sweep_old_logs(
+             &real,
+             SystemTime::now(),
+diff --git a/src/real_service.rs b/src/real_service.rs
+index 4da6336..90b5627 100755
+--- a/src/real_service.rs
++++ b/src/real_service.rs
+@@ -3509,8 +3509,8 @@ mod tests {
+         let echo = spawn_echo().await;
+         let paths = scratch("sweep");
+         std::fs::create_dir_all(&paths.log_dir).expect("log dir");
+-        let stale_mtime = std::time::SystemTime::now()
+-            - std::time::Duration::from_secs(9 * 24 * 3600);
++        let stale_mtime =
++            std::time::SystemTime::now() - std::time::Duration::from_secs(9 * 24 * 3600);
+         let pin = |p: &std::path::Path| {
+             std::fs::File::options()
+                 .write(true)
+@@ -3527,8 +3527,7 @@ mod tests {
+             pin(&p);
          }
-       ]
-     },
-+    {
-+      "id": "commit:8fdf7d1",
-+      "type": "commit",
-+      "title": "backup(s22-log-folder): 2026-11-25T18:42:08Z",
-+      "tags": [
-+        "2026-11-25"
-+      ],
-+      "sources": [
-+        {
-+          "file": "git:log",
-+          "anchor": "8fdf7d1"
-+        }
-+      ],
-+      "passages": [
-+        {
-+          "text": "backup(s22-log-folder): 2026-11-25T18:42:08Z",
-+          "kind": "title"
-+        }
-+      ]
-+    },
-     {
-       "id": "commit:9d4111b",
-       "type": "commit",
-@@ -1635,6 +1755,26 @@
+         for i in 0..2 {
+-            std::fs::write(paths.log_dir.join(format!("fresh-{i}.log")), b"x")
+-                .expect("write");
++            std::fs::write(paths.log_dir.join(format!("fresh-{i}.log")), b"x").expect("write");
          }
-       ]
-     },
-+    {
-+      "id": "commit:a843d31",
-+      "type": "commit",
-+      "title": "backup(s22-panel-split): 2026-11-25T18:22:48Z",
-+      "tags": [
-+        "2026-11-25"
-+      ],
-+      "sources": [
-+        {
-+          "file": "git:log",
-+          "anchor": "a843d31"
+ 
+         let _svc = RealService::with(paths, Arc::new(FakeFactory { echo, fail: false }));
+@@ -3540,7 +3539,10 @@ mod tests {
+             .map(|e| e.file_name().to_string_lossy().into_owned())
+             .collect();
+         assert_eq!(survived.len(), 40, "48 files − 8 beyond the newest-40 cap");
+-        assert!(survived.iter().any(|n| n.starts_with("fresh-")), "fresh kept");
++        assert!(
++            survived.iter().any(|n| n.starts_with("fresh-")),
++            "fresh kept"
++        );
+         // All 46 stale files share ONE pinned mtime (9 days old); the
+         // newest-40 cap counts the 2 fresh files too, so 38 stale files
+         // survive and 8 are swept.
+
+### Commit: e1ec5cc - feat: automatic stale-log cleanup at startup (v2rayN 7.25.6 parity) (Z User, 2026-11-27)
+diff --git a/src/diagnostics.rs b/src/diagnostics.rs
+index 599a3f7..d532bbb 100644
+--- a/src/diagnostics.rs
++++ b/src/diagnostics.rs
+@@ -104,6 +104,59 @@ pub(crate) fn is_on() -> bool {
+     ENABLED.load(Ordering::Acquire)
+ }
+ 
++/// S23 (v2rayN 7.25.6 parity — "log and temp-file auto cleanup, 7 days
++/// by default"): prune stale files from the log dir.
++///
++/// Policy (all best-effort — cleanup must never break construction):
++/// - regular FILES only (subdirectories are left alone);
++/// - the newest `keep_newest` files are ALWAYS kept (an active burst of
++///   diagnostics sessions never loses its newest material);
++/// - beyond that, files whose mtime is older than `retention` measured
++///   at `now` are deleted;
++/// - the `protected` path (the file the diagnostics sink is writing
++///   right now) is never touched even if it somehow looks stale.
++///
++/// Returns the paths that were deleted (the caller reports the count);
++/// unreadable metadata and failed deletes are silently skipped and
++/// simply do not appear in the list.
++pub(crate) fn sweep_old_logs(
++    dir: &Path,
++    now: SystemTime,
++    retention: std::time::Duration,
++    keep_newest: usize,
++    protected: Option<&Path>,
++) -> Vec<PathBuf> {
++    let Ok(entries) = std::fs::read_dir(dir) else {
++        return Vec::new(); // no dir yet — nothing to sweep (not an error)
++    };
++    // (path, mtime) for the regular files; newest first.
++    let mut files: Vec<(PathBuf, SystemTime)> = entries
++        .flatten()
++        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
++        .filter_map(|e| {
++            let mtime = e.metadata().ok()?.modified().ok()?;
++            Some((e.path(), mtime))
++        })
++        .collect();
++    files.sort_by(|a, b| b.1.cmp(&a.1));
++    let mut pruned = Vec::new();
++    for (i, (path, mtime)) in files.into_iter().enumerate() {
++        if i < keep_newest {
++            continue; // the newest N always survive
 +        }
-+      ],
-+      "passages": [
-+        {
-+          "text": "backup(s22-panel-split): 2026-11-25T18:22:48Z",
-+          "kind": "title"
++        if Some(path.as_path()) == protected {
++            continue; // the live diagnostics file is never swept
 +        }
-+      ]
-+    },
-     {
-       "id": "commit:a9f7aae",
-       "type": "commit",
-@@ -5002,6 +5142,27 @@
-         }
-       ]
-     },
-+    {
-+      "id": "ledger:s22",
-+      "type": "ledger",
-+      "title": "Stage s22 ledger",
-+      "tags": [
-+        "s22",
-+        "append-only"
-+      ],
-+      "sources": [
-+        {
-+          "file": "harness/memory/LEDGER.md",
-+          "anchor": "s22"
++        let Ok(age) = now.duration_since(mtime) else {
++            continue; // clock skew (mtime in the future): keep
++        };
++        if age > retention && std::fs::remove_file(&path).is_ok() {
++            pruned.push(path);
 +        }
-+      ],
-+      "passages": [
-+        {
-+          "text": "2026-11-26T03:30Z | IMPLEMENTED (five user asks; v0.15.0). Sandbox recycled AGAIN (rustup 1.100.0 + ziglang 0.14.1 + cargo-zigbuild 0.24.1 + xkblibs + python-xlib rebuilt per the ritual; the pypkgs ziglang wheel was INCOMPLETE AGAIN mid-gates — ring stddef.h — reinstalled from PyPI, S20/S21 pattern). Local clone was stale at s13 with a dirty (stale script-generated) PROJECT_MEMORY.md — discarded, fast-forwarded to origin (S21 had already shipped v0.14.0 with the P0/P1/QoS/merged-panel brief; the FIVE asks below landed after that close, so they were all pending). (1) TWO-COLUMN Exit & speed split (user: \"exit ip outputs on the left side and the speed test outputs on the right side\"): screens.rs status() now builds exit_out/speed_out columns (headings \"Exit IP\"/\"Speed test\", quiet placeholders while idle) pushed as row![exit_out, speed_out] — verdict card + IP/chips rows stacked LEFT, Mbps + chips + note RIGHT; pure view change, D24-safe in the page scrollable.",
-+          "kind": "body"
++    }
++    pruned
++}
++
+ /// Append one service-side line (the GUI `LogLine` tee; `kind` tags the
+ /// source, e.g. "GUI"). Best-effort — a write failure just disables the
+ /// sink (a broken log file must never break the app).
+@@ -268,3 +321,94 @@ mod tests {
+         assert!(stamp.chars().all(|c| c.is_ascii_digit() || c == '-'));
+     }
+ }
++
++/// S23: the stale-log sweep (v2rayN 7.25.6 parity).
++#[cfg(test)]
++mod sweep_tests {
++    use super::*;
++    use std::fs::FileTimes;
++
++    fn scratch(tag: &str) -> PathBuf {
++        let d = std::env::temp_dir().join(format!(
++            "mt-sweep-{}-{}-{}",
++            tag,
++            std::process::id(),
++            SystemTime::now()
++                .duration_since(UNIX_EPOCH)
++                .map(|d| d.as_nanos())
++                .unwrap_or(0)
++        ));
++        std::fs::create_dir_all(&d).expect("scratch dir");
++        d
++    }
++
++    /// A file with a pinned mtime (std FileTimes, stable since 1.75).
++    fn touch(dir: &Path, name: &str, age: Option<std::time::Duration>) -> PathBuf {
++        let p = dir.join(name);
++        std::fs::write(&p, b"log line\n").expect("write file");
++        if let Some(age) = age {
++            let mtime = SystemTime::now() - age;
++            let f = std::fs::File::options()
++                .write(true)
++                .open(&p)
++                .expect("open for times");
++            f.set_times(FileTimes::new().set_modified(mtime))
++                .expect("set mtime");
 +        }
-+      ]
-+    },
-     {
-       "id": "ledger:s3",
-       "type": "ledger",
-@@ -5681,6 +5842,10 @@
-         }
-       ],
-       "passages": [
-+        {
-+          "text": "- **Stage**: S22 CLOSED — v0.15.0 released (tag on 21fd59f). The user's five asks shipped: (1) Exit & speed panel split into TWO columns — exit-IP outputs LEFT (heading, IP, location/ISP chips, verdict card), speed-test outputs RIGHT (heading, Mbps, volume chips, download note), quiet placeholders while idle; pure view change, D24-safe. (2) UPLOAD SPEED TEST: \"Test upload speed\" button appears ONLY after the download test concluded, result in the right column; speed_test_upload_url setting (serde default cloudflare __up, D8), RealService::speed_test_upload POSTs 25 MiB through the same tunnel selection (S20 profile_id rule; clock after connect; 30 s/5 s stall/ 64 MiB caps; partial stands; status guard only on clean completion); Settings → Advanced \"upload test URL\" field; diag header redacts it. (3) PER-LINK ADAPTER: `via <adapter>` dim line UNDER each link row — mt_service::adapter_of_link_name parses \"<tag> [adapter]\" (dedup \" #N\" stripped; \"Wi-Fi Direct\"/\"Ethernet 2\" verbatim), populated in link_status_of + carrier_snapshot map + mock; survives util::short truncation. (4) OPEN LOG FOLDER FIXED: root cause = the diag-file gate (diag_path None after restart → \"turn diagnostics on first\"); new MtService::log_dir() (always resolvable) + handler create_dir_all + opens the folder itself via open_folder_command (explorer/open/xdg-open; spawn-result reported; Settings status + Logs-tab WARN, never silent); fetched at boot in BOTH init paths; ADJACENT: diagnostics persisted ON re-enables the sink AT CONSTRUCTION (RealService::with) — the toggle no longer lies after a restart.",
-+          "kind": "body"
-+        },
-         {
-           "text": "- **Stage**: S21 CLOSED — v0.14.0 released and verified (tag on e510e43, release run 45477702090 SUCCESS on attempt 3, same-commit ci green on both OS legs, assets download-verified). The user's four asks shipped: (1) P0 SECOND-CONNECTION BUG — mid-traffic reconnect re-binds the fixed listener ports while untracked in-flight connection tasks pin their sockets (no SO_REUSEADDR); on Windows the re-bind lands WSAEADDRINUSE → connect Failed → dead tunnel until restart. Linux tolerates the re-bind (std rebind probe), so the regression test asserts the platform-independent observable: the lingering client connection sees EOF after the picker switch (pre-fix FAILS, post-fix passes). Fix: Shared.conns tracks every connection task's AbortHandle; run() force-closes whatever the 3 s drain could not reap; AbortPumpsOnDrop aborts the DETACHED pump children (dropping JoinHandles leaves them running — found when the first EOF assertion failed); bind_with_retry 5 s (BIND_WAIT 2→8 s). H2: engine config/log stems carry a deterministic FNV-1a hash of the full endpoint identity (engine_file_stem) — same-tag different-endpoint profiles can no longer overwrite each other's Xray config (temp probe tunnels corrupting the LIVE engine → phantom respawn → DEGRADED while GUI showed Connected). H3: Core.store_mtx serializes profiles.json read-modify-write.",
-           "kind": "body"
-@@ -5923,6 +6088,26 @@
++        p
++    }
++
++    #[test]
++    fn old_files_pruned_new_and_protected_kept() {
++        let dir = scratch("old");
++        let now = SystemTime::now();
++        let retention = std::time::Duration::from_secs(7 * 24 * 3600);
++        let stale = touch(&dir, "stale.log", Some(retention * 2));
++        let fresh = touch(&dir, "fresh.log", Some(std::time::Duration::from_secs(60)));
++        let ancient_but_kept = touch(&dir, "kept-protected.log", Some(retention * 4));
++
++        let pruned = sweep_old_logs(&dir, now, retention, 1, Some(&ancient_but_kept));
++        assert_eq!(pruned, vec![stale], "only the stale unprotected file");
++        assert!(fresh.exists(), "fresh file survives");
++        assert!(ancient_but_kept.exists(), "the protected file survives");
++        std::fs::remove_dir_all(&dir).ok();
++    }
++
++    #[test]
++    fn the_newest_n_always_survive_regardless_of_age() {
++        let dir = scratch("newest");
++        let now = SystemTime::now();
++        let retention = std::time::Duration::from_secs(3600);
++        // Three ancient files with distinct ages: `a` newest (10×), `b`
++        // middle (11×), `c` oldest (12×). keep_newest=2 → `a` and `b`
++        // survive, only the oldest `c` is swept.
++        let a = touch(&dir, "a.log", Some(retention * 10));
++        let b = touch(&dir, "b.log", Some(retention * 11));
++        let c = touch(&dir, "c.log", Some(retention * 12));
++        let pruned = sweep_old_logs(&dir, now, retention, 2, None);
++        assert_eq!(pruned, vec![c], "only the oldest of the three");
++        assert!(a.exists() && b.exists(), "the newest two survive");
++        std::fs::remove_dir_all(&dir).ok();
++    }
++
++    #[test]
++    fn missing_dir_and_subdirs_are_not_errors() {
++        let dir = scratch("missing").join("nope");
++        assert!(sweep_old_logs(&dir, SystemTime::now(), std::time::Duration::ZERO, 0, None)
++            .is_empty());
++        // A subdirectory is never touched.
++        let real = scratch("subdir");
++        std::fs::create_dir_all(real.join("nested")).expect("subdir");
++        let _ = touch(&real, "old.log", Some(std::time::Duration::from_secs(10_000_000)));
++        let pruned = sweep_old_logs(
++            &real,
++            SystemTime::now(),
++            std::time::Duration::from_secs(60),
++            0,
++            None,
++        );
++        assert_eq!(pruned.len(), 1, "only the file, never the dir");
++        assert!(real.join("nested").is_dir(), "subdirectory untouched");
++        std::fs::remove_dir_all(&real).ok();
++    }
++}
+diff --git a/src/real_service.rs b/src/real_service.rs
+index 78fb8be..4da6336 100755
+--- a/src/real_service.rs
++++ b/src/real_service.rs
+@@ -149,6 +149,12 @@ const BIND_WAIT: Duration = Duration::from_secs(8);
+ const BIND_POLL: Duration = Duration::from_millis(10);
+ /// `ServiceEvent` broadcast buffer (slow GUI receivers lag, never block).
+ const EVENT_BUFFER: usize = 256;
++/// S23 (v2rayN 7.25.6 parity): stale-log retention — files in the log
++/// dir older than this are pruned at service construction.
++const LOG_RETENTION_SECS: u64 = 7 * 24 * 3600;
++/// S23: the newest N log files always survive the sweep (an active
++/// diagnostics burst never loses its newest material).
++const LOG_SWEEP_KEEP_NEWEST: usize = 40;
+ /// The flow the engine pins (mt-engine hardcodes vision in its config).
+ const ENGINE_FLOW: &str = "xtls-rprx-vision";
+ /// S19: overall budget for one exit-IP probe (unchanged; the temp-tunnel
+@@ -910,6 +916,27 @@ impl RealService {
+                 }
+             }
          }
-       ]
-     },
-+    {
-+      "id": "release:v0.15.0",
-+      "type": "release",
-+      "title": "Release v0.15.0 (tagged 21fd59f)",
-+      "tags": [
-+        "v0.15.0"
-+      ],
-+      "sources": [
-+        {
-+          "file": "git:tags/v0.15.0",
-+          "anchor": "21fd59f"
-+        }
-+      ],
-+      "passages": [
++        // S23 (v2rayN 7.25.6 parity): stale-log hygiene at every service
++        // construction — files older than 7 days are pruned, the newest
++        // 40 always survive, and the file the (possibly just restored)
++        // diagnostics sink is writing is never touched. Best-effort: a
++        // failed sweep is logged and forgotten (never fatal).
 +        {
-+          "text": "Release v0.15.0: tag points at commit 21fd59f.",
-+          "kind": "body"
++            let pruned = crate::diagnostics::sweep_old_logs(
++                &svc.core.paths.log_dir,
++                SystemTime::now(),
++                std::time::Duration::from_secs(LOG_RETENTION_SECS),
++                LOG_SWEEP_KEEP_NEWEST,
++                crate::diagnostics::path().as_deref(),
++            );
++            if !pruned.is_empty() {
++                svc.core.emit(ServiceEvent::LogLine(format!(
++                    "INFO  log cleanup: removed {} stale file(s) older than {} days",
++                    pruned.len(),
++                    LOG_RETENTION_SECS / (24 * 3600)
++                )));
++            }
 +        }
-+      ]
-+    },
-     {
-       "id": "release:v0.2.0",
-       "type": "release",
-@@ -8345,6 +8530,12 @@
-       "rel": "part_of",
-       "evidence": "subject stage label (s3-close)"
-     },
-+    {
-+      "src": "commit:06c87de",
-+      "dst": "ledger:s22",
-+      "rel": "part_of",
-+      "evidence": "subject stage label (s22)"
-+    },
-     {
-       "src": "commit:07ef5c5",
-       "dst": "ledger:s5",
-@@ -8363,6 +8554,12 @@
-       "rel": "part_of",
-       "evidence": "subject stage label (v0.3.1)"
-     },
-+    {
-+      "src": "commit:194eddc",
-+      "dst": "ledger:s22",
-+      "rel": "part_of",
-+      "evidence": "subject stage label (s22)"
-+    },
-     {
-       "src": "commit:1a9849f",
-       "dst": "ledger:s21",
-@@ -8381,6 +8578,12 @@
-       "rel": "part_of",
-       "evidence": "subject stage label (s10)"
-     },
-+    {
-+      "src": "commit:21fd59f",
-+      "dst": "ledger:s22",
-+      "rel": "part_of",
-+      "evidence": "subject stage label (s22)"
-+    },
-     {
-       "src": "commit:231f453",
-       "dst": "ledger:s11",
-@@ -8477,6 +8680,12 @@
-       "rel": "part_of",
-       "evidence": "subject stage label (s15)"
-     },
-+    {
-+      "src": "commit:51e0144",
-+      "dst": "ledger:s22",
-+      "rel": "part_of",
-+      "evidence": "subject stage label (s22)"
-+    },
-     {
-       "src": "commit:52f1d3c",
-       "dst": "ledger:s19",
-@@ -8567,6 +8776,12 @@
-       "rel": "part_of",
-       "evidence": "subject stage label (s11)"
-     },
-+    {
-+      "src": "commit:6fc0708",
-+      "dst": "ledger:s22",
-+      "rel": "part_of",
-+      "evidence": "subject stage label (s22)"
-+    },
-     {
-       "src": "commit:7073762",
-       "dst": "ledger:s11",
-@@ -8651,6 +8866,12 @@
-       "rel": "part_of",
-       "evidence": "subject stage label (s6-close)"
-     },
-+    {
-+      "src": "commit:8fdf7d1",
-+      "dst": "ledger:s22",
-+      "rel": "part_of",
-+      "evidence": "subject stage label (s22)"
-+    },
-     {
-       "src": "commit:9d4111b",
-       "dst": "ledger:s18",
-@@ -8675,6 +8896,12 @@
-       "rel": "part_of",
-       "evidence": "subject stage label (s2-review)"
-     },
-+    {
-+      "src": "commit:a843d31",
-+      "dst": "ledger:s22",
-+      "rel": "part_of",
-+      "evidence": "subject stage label (s22)"
-+    },
-     {
-       "src": "commit:a9f7aae",
-       "dst": "ledger:s6-wave3",
-@@ -9503,6 +9730,12 @@
-       "rel": "part_of",
-       "evidence": "project ledger"
-     },
-+    {
-+      "src": "ledger:s22",
-+      "dst": "project:multitunnel",
-+      "rel": "part_of",
-+      "evidence": "project ledger"
-+    },
-     {
-       "src": "ledger:s3",
-       "dst": "project:multitunnel",
-@@ -9719,6 +9952,12 @@
-       "rel": "built_from",
-       "evidence": "git tag v0.14.0"
-     },
-+    {
-+      "src": "release:v0.15.0",
-+      "dst": "commit:21fd59f",
-+      "rel": "built_from",
-+      "evidence": "git tag v0.15.0"
-+    },
-     {
-       "src": "release:v0.2.0",
-       "dst": "commit:06881a6",
-
-### Commit: 21fd59f - backup(s22-release-prep): 2026-11-25T19:24:03Z (Z User, 2026-11-25)
-diff --git a/Cargo.lock b/Cargo.lock
-index c32d817..8b3e2b9 100755
---- a/Cargo.lock
-+++ b/Cargo.lock
-@@ -1962,7 +1962,7 @@ dependencies = [
- 
- [[package]]
- name = "mt-app"
--version = "0.14.0"
-+version = "0.15.0"
- dependencies = [
-  "async-trait",
-  "iced",
-@@ -2073,7 +2073,7 @@ dependencies = [
- 
- [[package]]
- name = "multitunnel"
--version = "0.14.0"
-+version = "0.15.0"
- dependencies = [
-  "anyhow",
-  "async-trait",
-diff --git a/Cargo.toml b/Cargo.toml
-index c96f339..e386a49 100755
---- a/Cargo.toml
-+++ b/Cargo.toml
-@@ -18,7 +18,7 @@ members = ["crates/*"]
- resolver = "2"
- 
- [workspace.package]
--version = "0.14.0"
-+version = "0.15.0"
- 
- [lib]
- name = "multitunnel"
-diff --git a/QUICKSTART.md b/QUICKSTART.md
-index 7cf41a5..94e9118 100755
---- a/QUICKSTART.md
-+++ b/QUICKSTART.md
-@@ -109,6 +109,15 @@ needed for the usual per-user system-proxy writes (expect the SmartScreen
- > (flips live, no reconnect); the Status tab's exit-IP and speed-test
- > cards merged into one compact **Exit & speed** panel.
- >
-+> v0.15: the Exit & speed panel splits into two columns — exit-IP
-+> outputs on the LEFT, speed-test outputs on the RIGHT; after a download
-+> test concludes a **Test upload speed** button appears (result in the
-+> right column); every link shows **which adapter it goes through**
-+> (`via <adapter>`); Settings → Diagnostics' **Open log folder** works
-+> whether the mode is on or off (and a diagnostics toggle persisted ON
-+> writes again after a restart); the app icon fills its tile — the
-+> glyph grew ~24% in the tray.
-+>
- > v0.13.1 (Copilot code-review round): kill-switch hardening — the
- > black-hole port is owned BEFORE the proxy points at it, the proxy
- > hand-off is serialized with disconnect, a failed restore keeps the
-diff --git a/README.md b/README.md
-index 580d5db..e3ef43d 100755
---- a/README.md
-+++ b/README.md
-@@ -442,6 +442,28 @@ GUI, no new dependencies):
-   response head independently of the payload limit, and the Edit dialog
-   rejects port 0 and structurally invalid REALITY fields (empty SNI,
-   malformed public key / short ID).
-+- **v0.15.0 — the Exit & speed panel reads like a form**: the panel's
-+  two halves are now two COLUMNS — every exit-IP output (address,
-+  location/ISP chips, single-hop verdict) sits on the LEFT and every
-+  speed-test output (download rate, volume/duration chips, per-adapter
-+  spread note) sits on the RIGHT, each under its own small heading.
-+  After a download test concludes, a **Test upload speed** button
-+  appears in the right column: it POSTs a 25 MB payload through the
-+  tunnel (URL configurable in Advanced settings) and reports the send
-+  rate, volume and duration beside the download result. Every link in
-+  the Links card now shows **which adapter it goes through** — a
-+  `via <adapter>` line under the link (parsed from the engine's
-+  `"profile [adapter]"` link names, dedup ordinals stripped, so the
-+  adapter survives even when a long profile tag truncates the name).
-+  Settings → Diagnostics' **Open log folder** works in every state: it
-+  opens the log folder itself (created on demand) instead of the
-+  diagnostics file's parent, so it no longer reports "turn diagnostics
-+  on first" — and a diagnostics toggle persisted ON now starts writing
-+  again after an app restart (the sink used to start only on an
-+  off→on transition). The app icon (window, taskbar, tray, every ICO
-+  size) was regenerated so the glyph fills the tile — the mark grew
-+  ~24% in the tray after a dead gradient band was cropped out of the
-+  source and the margins tightened (93–97% width, 75–77% height fill).
- - **Quick Settings on right-click**: the tray menu is built fresh from
-   live state — Connect/Disconnect (state-reflecting), the server list
-   (radio-checked on the active profile), Open MultiTunnel and Exit.
-
-### Commit: 51e0144 - backup(s22-rig-verified): 2026-11-25T19:10:03Z (Z User, 2026-11-25)
-diff --git a/crates/mt-app/src/app.rs b/crates/mt-app/src/app.rs
-index 9f0a41d..f1b0892 100755
---- a/crates/mt-app/src/app.rs
-+++ b/crates/mt-app/src/app.rs
-@@ -815,6 +815,7 @@ impl App {
-             app.load_settings(),
-             app.load_adapters(),
-             app.fetch_diag_path(),
-+            app.fetch_log_dir(),
-         ];
-         (app, iced::Task::batch(tasks))
+         svc
      }
-@@ -908,6 +909,13 @@ impl App {
-         )
+ 
+@@ -3472,6 +3499,55 @@ mod tests {
+         assert!(paths.log_dir.ends_with("logs"), "{dir}");
      }
  
-+    /// S22: the log directory for the "Open log folder" button (fetched
-+    /// once at boot; the button opens it in every state).
-+    fn fetch_log_dir(&self) -> iced::Task<Message> {
-+        let svc = Arc::clone(&self.service);
-+        iced::Task::perform(async move { svc.log_dir().await }, Message::LogDirLoaded)
++    /// S23 (v2rayN 7.25.6 parity): service construction prunes stale
++    /// log files (7-day retention) while fresh ones survive. The sweep
++    /// keeps the newest 40 files unconditionally, so the fixture needs
++    /// MORE than 40 files for anything to become eligible.
++    #[tokio::test]
++    async fn s23_construction_sweeps_stale_logs() {
++        use std::fs::FileTimes;
++        let echo = spawn_echo().await;
++        let paths = scratch("sweep");
++        std::fs::create_dir_all(&paths.log_dir).expect("log dir");
++        let stale_mtime = std::time::SystemTime::now()
++            - std::time::Duration::from_secs(9 * 24 * 3600);
++        let pin = |p: &std::path::Path| {
++            std::fs::File::options()
++                .write(true)
++                .open(p)
++                .expect("open")
++                .set_times(FileTimes::new().set_modified(stale_mtime))
++                .expect("mtime");
++        };
++        // 46 nine-day-old files (all eligible, only the oldest 6 exceed
++        // the newest-40 cap) + 2 fresh files.
++        for i in 0..46 {
++            let p = paths.log_dir.join(format!("engine-stale-{i:03}.log"));
++            std::fs::write(&p, b"x").expect("write");
++            pin(&p);
++        }
++        for i in 0..2 {
++            std::fs::write(paths.log_dir.join(format!("fresh-{i}.log")), b"x")
++                .expect("write");
++        }
++
++        let _svc = RealService::with(paths, Arc::new(FakeFactory { echo, fail: false }));
++        // Count what survived directly from the log dir.
++        let log_dir = std::path::PathBuf::from(_svc.log_dir().await);
++        let survived: Vec<String> = std::fs::read_dir(&log_dir)
++            .expect("log dir readable")
++            .flatten()
++            .map(|e| e.file_name().to_string_lossy().into_owned())
++            .collect();
++        assert_eq!(survived.len(), 40, "48 files − 8 beyond the newest-40 cap");
++        assert!(survived.iter().any(|n| n.starts_with("fresh-")), "fresh kept");
++        // All 46 stale files share ONE pinned mtime (9 days old); the
++        // newest-40 cap counts the 2 fresh files too, so 38 stale files
++        // survive and 8 are swept.
++        let stale_left = survived.iter().filter(|n| n.contains("stale")).count();
++        assert_eq!(stale_left, 38, "38 stale + 2 fresh = the kept 40");
 +    }
 +
-     fn load_adapters(&self) -> iced::Task<Message> {
-         let svc = Arc::clone(&self.service);
-         iced::Task::perform(async move { svc.adapters().await }, Message::AdaptersLoaded)
+     /// S22: a diagnostics toggle persisted ON writes again after a
+     /// restart — the sink starts AT CONSTRUCTION, not only on an off→on
+     /// transition (the old gap made the Settings toggle lie and the
 ```
 
diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
index 4ae4b3a..f73ad73 100755
--- a/harness/memory/LEDGER.md
+++ b/harness/memory/LEDGER.md
@@ -220,3 +220,4 @@
 2026-11-22T04:16Z | s20 | v0.13.1 RELEASED: tag on 4923566 → release run 44983522874 SUCCESS (~4 min, no cold-cache stall); ci ×2 on main (44983512609/44983518575) ALL GREEN first try. Assets verified by download+unzip: multitunnel-v0.13.1-windows-x64.zip 6,511,395 B integrity OK (multitunnel.exe PE32+ console 13.8 MB + multitunnel-tray.exe PE32+ GUI 2.95 MB + README v0.13.1 section + QUICKSTART v0.13.1 note + server scripts + LICENSE + THIRD_PARTY_NOTICES + config.example) + mt-merge-server {x86_64 1,826,272 B, aarch64 1,860,808 B} ELFs. Verified copies staged in sandbox download/ (v0.13.1-verify-*). S20 CLOSED.
 2026-11-24T21:00Z | s21 | IMPLEMENTED (four asks: P0 bug, P1 perf parity, QoS toggle, merged panel; v0.14.0). Sandbox recycled AGAIN (rustup/zigbuild/xkblibs gone; tools/pypkgs ziglang wheel INCOMPLETE again — no lib/libc/include → ring stddef.h failure mid-gates; reinstalled 0.14.1 from PyPI per the S20 note; local clone was STALE at s13 with a dirty PROJECT_MEMORY.md — remote authoritative at s20/v0.13.1, snapshot→/tmp then fast-forward). P0 (user: "stops working when a second connection is added, restart required"): 3 Explore agents mapped the paths; H1 = mid-traffic reconnect re-binds 10808/10809 while untracked in-flight connection tasks pin their sockets (no SO_REUSEADDR anywhere) → bind WSAEADDRINUSE → Failed; PROVEN platform-split: Linux tolerates a listen re-bind over non-listening sockets (std rebind probe in scripts/rebind_probe.rs) while Windows does NOT — the sandbox cannot reproduce the bind refusal, so the regression test asserts the platform-independent observable (lingering client sees EOF after the switch) and pre-fix it FAILS (8.0 s timeout) / post-fix passes. FIX: Shared.conns tracks every connection task's AbortHandle (pruned at 4096 push), run() force-closes whatever the 3 s drain could not reap; AbortPumpsOnDrop aborts the DETACHED pump children too (dropping their JoinHandles leaves them running — found when the first EOF assertion failed; pumps held the socket halves forever); bind_with_retry (5 s budget, 250 ms interval; BIND_WAIT 2→8 s). H2: engine config/log stems now carry a deterministic FNV-1a hash of the FULL endpoint identity (engine_file_stem; same profile keeps its file across reconnects, same-tag different-endpoint profiles never share engine-<tag>.json again — temp probe/speed-test tunnels could rewrite the LIVE engine's config → phantom-live respawn budget exhaustion → DEGRADED while GUI showed Connected). H3: Core.store_mtx serializes profiles.json read-modify-write (import/remove/set_active/update; no await inside the critical sections). H1+H3 tests: picker_switch_with_lingering_client_reconnects, shutdown_force_closes_lingering_connections_and_frees_the_port (mt-net level: EOF + immediate re-bind). P1 PERF (user: v2rayN faster on ONE adapter): (1) TCP_NODELAY on all four legs (client→engine loopback socks5.rs establish; daemon accepted carrier; daemon dial_target ×2 — Nagle+delayed-ACK stalled each 16 KiB frame's tail segment ~40 ms on Linux peers ≈ Mbps-per-carrier stop-and-wait); (2) quarantine can no longer BLACKHOLE the sole link (pick_excluding last-resort fallback over quarantined/down links when no healthy candidate; the dial decides, fail-open; retry-exclusion contract kept; module doc + contract test updated, new sole-link test); (3) production background prober now uses the user-configured probe_target (was hardcoded 1.1.1.1:443); (4) ACK coalescing (ACK_COALESCE_FRAMES=8 / 20 ms window, per-session AckCoalescer, flushed by CLOSE handling + the 1 s PING heartbeat → ≤1 s ACK latency; cumulative semantics unchanged; guard scoped before the await for Send); (5) PONG echoes on the RECEIVING carrier (schedule_bytes re-picked — rtt_ms was cross-contaminated across carriers feeding ECF/Balanced wrong data); (6) send-path copies 5→3 per frame (encode_session_borrowed encodes header+payload in one pass from the slice; send_data sends slices; wire-format identity test); (7) carrier writer batches up to 16 co-queued frames into ONE write_vectored (write_all_vectored with partial-write advance). O — QoS FAIR SHARE toggle (Settings → Network, qos_fair serde-default bool): new mt-net/src/qos.rs QosScheduler groups flows by TARGET, 1 s tick refills buckets at aggregate/groups ONLY under real contention (≥2 groups AND aggregate ≥ 2 MiB/s floor; single group/idle = unlimited), pump read gate blocks until budget (20 ms retry), ALWAYS attached at connect so set_settings flips it LIVE (no reconnect — scheduler flag atomic; log line emitted); 5 scheduler unit tests + live-flip persistence test + GUI draft test. P — merged Exit & speed panel: one Status card (shared header, both triggers side-by-side Fill-width, exit row + verdict + speed row + hint stacked compactly; all messages/state unchanged; D24-safe inside the page scroll), rig-verified sf=1.0 (VLM + knob-pixel-diff: 1957 px changed, knob center 39→55) + sf=1.5. TESTS 490/0 (+16: mt-engine stem ×4, merge sole-link ×1, mt-net shutdown-force-close ×1 + qos ×5, frame wire-identity ×1, root P0 ×1 + qos ×1, mt-app qos ×1, mt-service serde ×1). GATES: fmt clean; clippy host + windows-gnu -D warnings clean (zig shims + fresh ziglang wheel); cargo test --workspace 490/0; vendored iced 6/0; mt-app --smoke 0; rig sf=1.0+1.5; zigbuild PE32+ ×4 + ELF ×2. RELEASE: v0.14.0 tag → run 45477702090 — attempt-1 ubuntu leg carrier_death 'no live bond carrier' (documented S11/S18/S19 pre-existing runner race, signature exactly as backlogged), attempt-2 windows leg domain_targets 60 s deadline, attempt-3 SUCCESS; same-commit ci.yml runs green on BOTH OS legs throughout + local 2-core taskset stress 8/8; all four logs archived under harness/memory/agent-reports/s21-ci-*.log. Assets download-verified: multitunnel-v0.14.0-windows-x64.zip 6,540,352 B (11-entry manifest identical shape to v0.13.1: multitunnel.exe + tray + docs + server scripts) + mt-merge-server x86_64 1,845,824 B + aarch64 1,864,608 B. PARKED (needs user input or new scope): single-instance guard (H4 — two app instances race the sysproxy snapshot; needs a CreateMutexW design decision), BufReader-style carrier read batching (2 read_exact syscalls/frame), MAX_PAYLOAD 64 KiB protocol bump (needs a coordinated mt-merge-server update on the user's VM), carrier-lifecycle race root-cause (S19 backlog — this session hit it 2/3 release legs; reruns shipped), upstream-iced migration, subscription auto-update, vmess validation, profiles.json locking (partially addressed by store_mtx in-process; cross-process locking still open), engine log rotation, listener max_conns. Copilot CLI still blocked by PAT (S20 finding unchanged — cloud route works, needs only pull-requests:write).
 2026-11-26T03:30Z | s22 | IMPLEMENTED (five user asks; v0.15.0). Sandbox recycled AGAIN (rustup 1.100.0 + ziglang 0.14.1 + cargo-zigbuild 0.24.1 + xkblibs + python-xlib rebuilt per the ritual; the pypkgs ziglang wheel was INCOMPLETE AGAIN mid-gates — ring stddef.h — reinstalled from PyPI, S20/S21 pattern). Local clone was stale at s13 with a dirty (stale script-generated) PROJECT_MEMORY.md — discarded, fast-forwarded to origin (S21 had already shipped v0.14.0 with the P0/P1/QoS/merged-panel brief; the FIVE asks below landed after that close, so they were all pending). (1) TWO-COLUMN Exit & speed split (user: "exit ip outputs on the left side and the speed test outputs on the right side"): screens.rs status() now builds exit_out/speed_out columns (headings "Exit IP"/"Speed test", quiet placeholders while idle) pushed as row![exit_out, speed_out] — verdict card + IP/chips rows stacked LEFT, Mbps + chips + note RIGHT; pure view change, D24-safe in the page scrollable. (2) UPLOAD SPEED TEST (user: "After the speed test has concluded, show another button to test upload speed"): mt-service contract + SettingsView.speed_test_upload_url (serde default http://speed.cloudflare.com/__up, D8 migration), RealService::speed_test_upload POSTs a fixed 25 MiB body through the same tunnel selection (S20 profile_id rule), clock after connect, 64 KiB chunks, same 30 s/5 s stall/64 MiB caps, partial-measurement-stands semantics, status guard ONLY on clean completion (a truncated body may never get a response); SpeedTestUpload/SpeedUploadTested messages + speed_up/speed_up_testing state + the button appears ONLY after the download concluded (gate app.speed.is_some() && !speed_testing), mutual exclusion across all three triggers; upload URL field in Settings → Advanced; redact_url in the diag header; MockService simulated. Tests: temp-tunnel measurement + 403-rejection (spawn_upload_server fixture drains Content-Length then answers), mock message flow, mt-service serde migration/roundtrip. (3) PER-LINK ADAPTER (user: "Under the links, show which adapter each link is going through"): mt-service::adapter_of_link_name parses the engine's "<tag> [adapter]" grammar (dedup " #N" stripped for display; rate-sampler's tricky cases pinned: "Wi-Fi Direct"/"Ethernet 2" verbatim), LinkStatus.adapter_name (serde default) populated in link_status_of (per-connection) + carrier_snapshot map (stream mode) + mock seeds; the Links card renders a dim `via <adapter>` line UNDER each link row — survives util::short's 32-char truncation that previously hid the [adapter] suffix. (4) OPEN LOG FOLDER FIX (user: "clicking the 'Open log folder button' does not open the log folder"): ROOT CAUSE was the diagnostics-file gate — the button resolved diag_path (None unless the sink started THIS process lifetime) so after every restart it printed "turn diagnostics on first"; FIXED by a new MtService::log_dir() (always resolvable), the handler create_dir_all's + opens the folder itself via open_folder_command (explorer/open/xdg-open per platform, spawn-result reported, never silent — Settings status + Logs-tab WARN), fetched at boot (fetch_log_dir in BOTH init paths — the iced init batch was the second gap the rig caught); ADJACENT ROOT CAUSE fixed: a diagnostics toggle persisted ON now re-enables the sink AT CONSTRUCTION (RealService::with) — the toggle used to lie after a restart; diagnostics_persisted_on_starts_at_construction pins it. (5) ICON FILL (user: "have the icon utilize as much of the available space as possible"): make_logo_assets.py re-crops emblem+lockup to their ALPHA bboxes after key-out (a dead ~71px left gradient band made the mark render off-center at 62% height fill), margins 0.04→0.02 (16/32/48/24/64) + 0.10→0.03 (256 lockup); measured fills 93.8-96.9%w × 75.0-76.6%h (was 75-81%w × 61-64%h — glyph ~24% larger in the tray's 32px tile); all test-pinned invariants hold (4-entry ICOs 16/32/48/256, biWidth=32/biHeight=64, logo_64=16384 B, logo_24=2304 B, deterministic re-runs, banner/logo/color_wheel untouched). RIG (sf=1.0 + 1.5, mock): panel split + placeholders, download result right-column, upload button appears → "testing upload…" → result rows, exit probe left-column verdict card, via-adapter lines, Open-log-folder click → folder created + honest per-platform failure surfaced (no xdg-open in bare Xvfb — the Windows path spawns explorer; spawn success unit-pinned via open_folder_command_targets_the_os_file_manager). LESSONS: (a) gui_drive click on a button that APPEARED after a reflow needs a pointer-jiggle first (no motion event → iced hover stale → first click no-op); (b) content appended BELOW a scrollable's initial viewport looks like a stale frame in shots — damage-instrumented the vendored compositor (reverted byte-identical) to prove the renderer was right: the rows were below the fold, scroll at a y INSIDE the viewport (400 was above it, 500 works). TESTS 497/0 (+7). GATES: fmt clean; clippy host + windows-gnu -D warnings clean (zig shims + fresh wheel); vendored iced 6/0; mt-app --smoke 0; zigbuild PE32+ x4 (multitunnel 19.3MB, mt-app 14.9MB, tray 10.2MB, merge-server 2.4MB) + ELF x2 (x86_64 1.25MB, aarch64 1.23MB); version 0.15.0; README v0.15 section + QUICKSTART note. DISK: 9.9G rootfs hit 99% mid-gates — rm -rf target/debug + target/x86_64-pc-windows-gnu between phases (S15 sequence discipline).
+2026-11-27T12:10Z | s23 | IMPLEMENTED (three asks; PR-flow session). Sandbox recycled AGAIN (rustup 1.100.0 + clippy/rustfmt components + ziglang 0.14.1 re-PyPI'd — the wheel was INCOMPLETE for the THIRD time, no lib/libc/include; xkblibs re-extracted to /tmp/xkblibs; python-xlib to tools/pypkgs-xlib). (1) OPEN LOG FOLDER, OS ERROR 50 (user: "could not open the log folder: The request is not supported. (os error 50)"): root cause pinned — v0.15.0 spawned explorer.exe with DEFAULT (inherited) stdio; mt-app is a Windows GUI-subsystem binary whose standard handles are NULL, Rust's spawn duplicates them into the child (STARTF_USESTDHANDLES) and CreateProcessW fails with ERROR_NOT_SUPPORTED; console-subsystem parents never hit it (why every manual probe passed while the user's GUI failed). FIX: crates/mt-app/src/shell_open.rs — Windows ShellExecuteW primary (windows-sys Win32_UI_Shell, already a tray dep; verb "open", rc>32 = ok, documented SE_* code table) + explorer.exe fallback now with Stdio::null() on ALL THREE streams (the inheritance trap is unreachable by construction); macOS open(1)/other xdg-open(1), all NUL-stdio; failures name every mechanism tried + the folder (never silent; Settings status + Logs WARN). Rig: click → "could not open the log folder: xdg-open is not installed (tried to open /tmp/MultiTunnel-mock-logs)" — the actionable format, vs the old bare "(os error 50)". (2) RESET ALL SETTINGS (user ask): Settings "Reset" card, ErrorText, TWO-CLICK confirm (first press arms: button relabels + status line names the exact scope; second executes; any other settings mutation disarms via save_settings). mt_service::SettingsView::default() is now the SINGLE stock source (impl Default; five diverging literals collapsed: SettingsFile, GUI placeholder, mock, theme/real_service test fixtures; serde-defaults == Default pinned by test, theme_mode's absent-key legacy semantics documented). Scope: every Settings field; profiles + routing rules untouched (stated in the card caption); bonded_adapters cleared through set_bonded_adapters([]) — the real set_settings deliberately preserves server-side bonded selection (stale-GUI-copy guard); rows reload selected=false; always-on-top UN-applies at stock(false) via apply_window_level both directions; reset-specific SettingsSaved note. (3) RESEARCH PASS (user: "check for improvements… by checking the changelogs of similar applications"): docs/research/2026-11-27-stability-changelog-survey.md — v2rayN 7.25.6/Clash Verge 2.5.10/Hiddify/Speedify → adopted: 7-day stale-log sweep (v2rayN's headline hygiene item; diagnostics::sweep_old_logs: retention 7d, newest-40 floor, live diag file protected, best-effort, runs at every RealService::with with an INFO line), remembered window size (desktop-client convention; window_w/h serde-default 1080x720 D8, debounced 2-quiet-tick save — a drag never writes per event, clamped 880x580, restored via iced::window::resize on load, reset restores stock), 1 MiB SO_SNDBUF/SO_RCVBUF + existing NODELAY on the merge daemon's two WAN socket populations via socket2 (transitive dep — zero new lockfile entries; BDP 150 Mbps × 50 ms ≈ 0.94 MiB; loopback engine hops intentionally OS-default). Confirmed parity: real-delay latency (already through-tunnel), balancer policies (5 merge policies superset), connection-info card, core pinning (D14). Rejected with reasons: mux knobs, MASQUE, redundant mode (doubles traffic), TUN, subscription headers. TESTS 520/0 (+10: shell_open ×4, stock-defaults ×2, GUI reset ×1 + window ×1, real_service reset round-trip + window round-trip + construction-sweep ×3, daemon tune ×1, diagnostics sweep ×3). GATES all green: fmt; clippy host + windows-gnu (zig shims, fresh wheel) -D warnings; cargo test --workspace 510/0 + s23 subset; vendored iced 6/0 (cargo test -p iced_graphics -p iced_tiny_skia --lib from the workspace root — the standalone-manifest route errors on the parent workspace); mt-app --smoke 0; zigbuild PE32+ ×3 (multitunnel 19.2MB, tray 10.2MB, mt-merge-server) + ELF ×2; rig sf=1.0 + 1.5 (reset two-click arm→confirm→"settings reset to defaults — servers and routing rules untouched", seed reverted 6770852/#6750A4/purple-selected, log-folder honest failure, 0 panics). DELIVERY FLOW CHANGED THIS SESSION per the user's ask: PR + @copilot reviewer instead of direct-to-main; NO tag/release (review first). RIG LESSONS: the Settings page scroll needs ONE wheel tick per verb invocation with ≥1 s between (consecutive ticks in one scroll:X,Y,N verb coalesce — only ~1 lands; a tick over a pick_list is eaten; (1070,400) — the right card-padding strip — is widget-free); VLM on crops beats whole-shot pixel archaeology; row-profile dy alignment can hallucinate (err=0 at impossible shifts on sparse profiles) — track a distinctive feature (the red swatch row) instead. DISK: hit 100% twice — rm -rf target/debug + the /tmp/mt-base worktree between phases (S15/S22 discipline).
diff --git a/harness/memory/STATE.md b/harness/memory/STATE.md
index 74ef545..3aace5d 100755
--- a/harness/memory/STATE.md
+++ b/harness/memory/STATE.md
@@ -1,48 +1,35 @@
-# STATE — rewritten at the end of every stage. Last update: S19 (server editing + offline probes + speed test + diagnostics + kill switch, v0.13.0)
+# STATE — rewritten at the end of every stage. Last update: S23 (open-log-folder os-error-50 fix + reset-to-defaults + changelog-research pass; PR flow)
 
 ## Current position
-- Stage: S19 IMPLEMENTED, all local gates green, review fixes in,
-  releasing v0.13.0.
-- The user's six asks shipped: (1) SERVER EDITING — Servers tab per-card
-  Edit button → full-window modal (name/remark/address/port/UUID +
-  REALITY fields for reality profiles); mt-service ProfileDetails
-  round-trip (profile_details → update_profile); ProfileStore::update
-  replaces BY ID in place (id/position/active preserved, no dedupe on
-  explicit edit); RealService validates primitives + a
-  capability-regression check (an edit that would break a convertible
-  profile is rejected with the reason). (2) EXIT IP WHILE OFFLINE —
-  exit_probe prefers the live session, falls back to
-  temp_link_for() (the same factory path test_latency uses);
-  ExitInfo.via_temp_tunnel drives a "temporary probe tunnel" chip.
-  (3) SPEED TEST — Status card; speed_test(None) rides the live session
-  for the active profile (else the temp tunnel); downloads
-  settings.speed_test_url (default
-  http://speed.cloudflare.com/__down?bytes=25000000, plain-HTTP verified);
-  clock starts after connect (engine spawn excluded); 5 s stall / 30 s
-  budget / 64 MiB caps; ≥400 rejected. (4) DIAGNOSTICS MODE —
-  src/diagnostics.rs: one redacted paste-ready
-  <log_dir>/diagnostics-YYYYMMDD-HHMMSS.log (header + GUI LogLine tee +
-  ALL tracing events via a hand-rolled global subscriber; engine level
-  forced debug; UUID/pbk never written — test-pinned); Settings card
-  with the path + "Open log folder". (5) KILL SWITCH — give_up()
-  fail-CLOSED: system proxy re-pointed at 127.0.0.1:9 (review fix: the
-  app BINDS the port itself while engaged — spawn_blackhole accept loop
-  swallows connections — so a local process cannot pre-bind it); every
-  restore path releases it; explicit disconnect restores even with no
-  session; Status banner "KILL SWITCH ACTIVE" when Failed + armed.
-  (6) CODE REVIEW — Copilot channels unavailable from the sandbox (no
-  account entitlement → reviewer requests silently dropped;
-  models.github.ai proxy-intercepted): a 3-round discussion-based review
-  ran on the strongest reachable model instead (z-ai chat), 4 findings
-  fixed (above), the rest assessed + documented in the transcript;
-  review PR #1 left open (base v0.12-line) for a Copilot pass from an
-  entitled machine.
-- 466/0 workspace tests (+19 vs v0.12); fmt/clippy host+windows-gnu;
-  vendored iced 6/0; smoke 0; rig sf=1.0+1.5 VLM-verified (52.0 Mbps
-  chip row, offline exit-IP chip, modal fields + rename→save→card
-  updated, Cancel + backdrop dismissal, kill switch + diagnostics
-  toggles); PE32+ ×4 + ELF ×2; version 0.13.0.
+- Stage: S23 IMPLEMENTED, all local gates green, delivered as the PR
+  `s23-open-folder-reset-perf` requesting @copilot review (the user's
+  explicit delivery instruction this session — NO tag/release until the
+  review lands).
+- The user's three asks shipped on the branch: (1) OPEN LOG FOLDER
+  FIXED FOR REAL — the v0.15.0 explorer.exe spawn hit
+  ERROR_NOT_SUPPORTED (os error 50) because a GUI-subsystem binary
+  passes NULL stdio handles into CreateProcessW; the fix is
+  crates/mt-app/src/shell_open.rs: ShellExecuteW (shell's open-verb
+  path, no handle inheritance) with an explorer.exe fallback that now
+  uses Stdio::null() on all three streams; failures name every
+  mechanism tried + the folder. (2) RESET ALL SETTINGS — a Settings
+  "Reset" card (ErrorText, two-click confirm, scope caption: servers
+  and routing rules untouched; bonded_adapters cleared through
+  set_bonded_adapters([]); mt_service::SettingsView::default() is the
+  single stock source — five diverging literals collapsed; reset note
+  in SettingsSaved). (3) CHANGELOG RESEARCH APPLIED —
+  docs/research/2026-11-27-stability-changelog-survey.md: adopted the
+  v2rayN 7.25.6 7-day stale-log sweep (diagnostics::sweep_old_logs at
+  every RealService::with), the desktop-client remembered window size
+  (window_w/h serde-default, debounced save, restored on load), and
+  1 MiB SO_SNDBUF/SO_RCVBUF on the merge daemon's WAN streams via
+  socket2 (transitive dep, zero new lockfile entries).
+- Gates: fmt; clippy host + windows-gnu -D warnings; workspace tests
+  520/0 (+10 new); vendored iced 6/0; mt-app --smoke 0; zigbuild
+  PE32+ ×3 + ELF ×2; rig sf=1.0 + 1.5 (reset two-click flow verified
+  end-to-end, seed reverted, log-folder honest failure, 0 panics).
 
+## Prior stages (compressed)
 ## Next actions
 1. Tag v0.13.0 → release.yml; watch the run; verify assets by
    download+unzip (PE32+ in zip, ELFs, README/QUICKSTART carried).

### Commit: a0e0ccb - chore: clippy/fmt pass (struct-update idioms, sort_by_key, drop dead default fns) (Z User, 2026-11-27)
diff --git a/crates/mt-app/src/app.rs b/crates/mt-app/src/app.rs
index 5a4db83..d145b80 100755
--- a/crates/mt-app/src/app.rs
+++ b/crates/mt-app/src/app.rs
@@ -1504,9 +1504,9 @@ impl App {
                 let apply = self.apply_settings_view(stock.clone());
                 // Reset restores the stock window size too (a live
                 // resize to 1080×720 — the next launch reads it back).
-                let resize = if self.window_id.is_some() {
+                let resize = if let Some(id) = self.window_id {
                     iced::window::resize(
-                        self.window_id.expect("checked above"),
+                        id,
                         iced::Size::new(stock.window_w as f32, stock.window_h as f32),
                     )
                 } else {
diff --git a/crates/mt-app/src/mock.rs b/crates/mt-app/src/mock.rs
index ce6667f..7f82c28 100755
--- a/crates/mt-app/src/mock.rs
+++ b/crates/mt-app/src/mock.rs
@@ -77,18 +77,20 @@ impl MockService {
         let active = profiles.first().map(|p| p.id.clone());
         // S23: stock base (theme/system/balanced/…) with ONE demo
         // deviation — the bonded adapters below.
-        let mut settings = SettingsView::default();
         // D22/D23: ALL THREE wired adapters bonded — mirrors the demo
         // setup (balanced needs ≥2 ticked adapters to show its point)
         // and gives every Status-tab adapter graph traffic. The mock's
         // byte split is deliberately UNEVEN (Wi-Fi ≫ Ethernet ≫
         // Cellular) — the "one adapter hogs everything" shape the
         // balanced policy exists to fix.
-        settings.bonded_adapters = vec![
-            "mock-wifi".to_string(),
-            "mock-ethernet".to_string(),
-            "mock-cellular".to_string(),
-        ];
+        let settings = SettingsView {
+            bonded_adapters: vec![
+                "mock-wifi".to_string(),
+                "mock-ethernet".to_string(),
+                "mock-cellular".to_string(),
+            ],
+            ..SettingsView::default()
+        };
         let inner = Inner {
             state: ServiceState::Disconnected,
             active,
diff --git a/crates/mt-app/src/theme.rs b/crates/mt-app/src/theme.rs
index 8cf6715..87d723f 100755
--- a/crates/mt-app/src/theme.rs
+++ b/crates/mt-app/src/theme.rs
@@ -672,15 +672,16 @@ mod tests {
     /// `from_settings` / `rederive` honor the settings view (live re-theme).
     #[test]
     fn theme_from_settings_and_rederive() {
-        let mut t = MtTheme::from_settings(&{
+        let mut t = MtTheme::from_settings(&SettingsView {
             // S23: one base-line diff off the stock view (the fields
-            // from_settings actually reads: seed + mode).
-            let mut s = SettingsView::default();
-            s.theme_seed = 0x4285F4;
-            s.theme_mode = "light".into();
-            s.system_proxy = true;
-            s.merge_policy = "failover".into();
-            s
+            // from_settings actually reads: seed + mode; the system_proxy
+            // / merge_policy values keep the original test's "unrelated
+            // fields are ignored" coverage).
+            theme_seed: 0x4285F4,
+            theme_mode: "light".into(),
+            system_proxy: true,
+            merge_policy: "failover".into(),
+            ..SettingsView::default()
         });
         assert_eq!(t.palette.seed(), 0x4285F4);
         assert!(!t.dark);
diff --git a/crates/mt-service/src/lib.rs b/crates/mt-service/src/lib.rs
index 2da54c6..d7ea35a 100755
--- a/crates/mt-service/src/lib.rs
+++ b/crates/mt-service/src/lib.rs
@@ -992,13 +992,14 @@ mod close_to_tray_migration_tests {
             // S23: build off the stock view, preserving this fixture's
             // historical deviations (seed 0, blank mode, info logs,
             // blank probe/exit URLs).
-            let mut v = SettingsView::default();
-            v.theme_seed = 0;
-            v.theme_mode = String::new();
-            v.log_level = "info".into();
-            v.probe_target = String::new();
-            v.exit_probe_url = String::new();
-            v
+            SettingsView {
+                theme_seed: 0,
+                theme_mode: String::new(),
+                log_level: "info".into(),
+                probe_target: String::new(),
+                exit_probe_url: String::new(),
+                ..SettingsView::default()
+            }
         }
     }
 }
diff --git a/src/diagnostics.rs b/src/diagnostics.rs
index d312b1b..af70eb5 100644
--- a/src/diagnostics.rs
+++ b/src/diagnostics.rs
@@ -138,7 +138,7 @@ pub(crate) fn sweep_old_logs(
             Some((e.path(), mtime))
         })
         .collect();
-    files.sort_by(|a, b| b.1.cmp(&a.1));
+    files.sort_by_key(|(_, mtime)| std::cmp::Reverse(*mtime));
     let mut pruned = Vec::new();
     for (i, (path, mtime)) in files.into_iter().enumerate() {
         if i < keep_newest {
diff --git a/src/real_service.rs b/src/real_service.rs
index 90b5627..2162f2e 100755
--- a/src/real_service.rs
+++ b/src/real_service.rs
@@ -607,18 +607,6 @@ impl Default for SettingsFile {
 /// round-robin; D23 = 2: stock round-robin → balanced).
 const SETTINGS_VERSION: u32 = 2;
 
-/// S19: the default speed-test URL (mirrors the mt-service serde default
-/// so a fresh SettingsFile and a freshly-parsed one agree).
-fn default_speed_test_url() -> String {
-    "http://speed.cloudflare.com/__down?bytes=25000000".to_string()
-}
-
-/// S22: the default upload-test URL (mirrors the mt-service serde
-/// default, same reason).
-fn default_speed_test_upload_url() -> String {
-    "http://speed.cloudflare.com/__up".to_string()
-}
-
 impl SettingsFile {
     fn load(path: &Path) -> Self {
         let mut file = match std::fs::read_to_string(path) {
@@ -3110,13 +3098,14 @@ mod tests {
         let file = SettingsFile {
             settings: {
                 // v0.5-era file shape: NO theme_mode key (the S18 legacy
-                // migration tests build on files like this one) — build
-                // off the stock view and blank exactly that field.
-                let mut s = SettingsView::default();
-                s.theme_mode = String::new();
-                s.system_proxy = system_proxy;
-                s.merge_policy = "failover".to_string();
-                s
+                // migration tests build on files like this one) — the
+                // stock view with exactly that field blanked.
+                SettingsView {
+                    theme_mode: String::new(),
+                    system_proxy,
+                    merge_policy: "failover".to_string(),
+                    ..SettingsView::default()
+                }
             },
             rules: Vec::new(),
             socks_port: 0,
@@ -4523,16 +4512,15 @@ mod tests {
         let paths = scratch("settings");
         let svc = RealService::with(paths.clone(), Arc::new(FakeFactory { echo, fail: false }));
 
-        let view = {
-            let mut v = SettingsView::default();
-            v.theme_seed = 0xEA4335;
-            v.dark_mode = true;
-            v.theme_mode = "dark".to_string();
-            v.merge_policy = "lowestlatency".to_string();
-            v.socks_port = 0; // ephemeral — tests never bind fixed ports
-            v.http_port = 0;
-            v.qos_fair = true; // round-trips a non-default value
-            v
+        let view = SettingsView {
+            theme_seed: 0xEA4335,
+            dark_mode: true,
+            theme_mode: "dark".to_string(),
+            merge_policy: "lowestlatency".to_string(),
+            socks_port: 0, // ephemeral — tests never bind fixed ports
+            http_port: 0,
+            qos_fair: true, // round-trips a non-default value
+            ..SettingsView::default()
         };
         svc.set_settings(view.clone()).await.expect("save settings");
         assert_eq!(svc.settings().await, view);
@@ -4580,13 +4568,15 @@ mod tests {
         // Dirty: non-default settings + rules + a profile. The bonded
         // selection needs a REAL adapter id when the host has one (the
         // machine-independent parts run either way).
-        let mut dirty = SettingsView::default();
-        dirty.theme_seed = 0xEA4335;
-        dirty.merge_policy = "priority".to_string();
-        dirty.bond_mode = "stream".to_string();
-        dirty.kill_switch = true;
-        dirty.window_w = 1400;
-        dirty.window_h = 900;
+        let dirty = SettingsView {
+            theme_seed: 0xEA4335,
+            merge_policy: "priority".to_string(),
+            bond_mode: "stream".to_string(),
+            kill_switch: true,
+            window_w: 1400,
+            window_h: 900,
+            ..SettingsView::default()
+        };
         svc.set_settings(dirty.clone()).await.expect("dirty save");
         let host_adapter = svc
             .adapters()
@@ -4654,9 +4644,11 @@ mod tests {
         let paths = scratch("winsize");
         let svc = RealService::with(paths.clone(), Arc::new(FakeFactory { echo, fail: false }));
 
-        let mut custom = SettingsView::default();
-        custom.window_w = 1400;
-        custom.window_h = 900;
+        let custom = SettingsView {
+            window_w: 1400,
+            window_h: 900,
+            ..SettingsView::default()
+        };
         svc.set_settings(custom.clone()).await.expect("save");
         assert_eq!(svc.settings().await, custom);
         assert!(!svc.settings().await.window_size_is_default());

### Commit: ab91166 - perf(mt-bond): 1 MiB WAN socket buffers on the merge daemon + research doc (Z User, 2026-11-27)
diff --git a/Cargo.lock b/Cargo.lock
index 8b3e2b9..539fbf6 100755
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1982,6 +1982,7 @@ dependencies = [
  "anyhow",
  "async-trait",
  "mt-net",
+ "socket2",
  "tokio",
  "tracing",
 ]
diff --git a/crates/mt-bond/Cargo.toml b/crates/mt-bond/Cargo.toml
index 07db106..1de6f62 100755
--- a/crates/mt-bond/Cargo.toml
+++ b/crates/mt-bond/Cargo.toml
@@ -20,3 +20,6 @@ tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time"
 async-trait = "0.1"
 anyhow = "1"
 tracing = "0.1"
+# S23: SO_SNDBUF/SO_RCVBUF on the daemon's WAN-facing streams (already a
+# transitive dep — no new code enters the lockfile).
+socket2 = "0.6"
diff --git a/crates/mt-bond/src/daemon.rs b/crates/mt-bond/src/daemon.rs
index d7c7700..dc04426 100755
--- a/crates/mt-bond/src/daemon.rs
+++ b/crates/mt-bond/src/daemon.rs
@@ -33,6 +33,26 @@ pub const BOND_IDLE_GC: Duration = Duration::from_secs(60);
 const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
 /// GC sweep cadence.
 const GC_INTERVAL: Duration = Duration::from_secs(15);
+/// S23: SO_SNDBUF/SO_RCVBUF for the daemon's WAN-facing streams. 1 MiB
+/// covers the bandwidth-delay product of a ~150 Mbps carrier at ~50 ms
+/// RTT (18.75 MB/s × 50 ms ≈ 0.94 MiB) without double-buffering orders
+/// of magnitude beyond it; the OS default (often 128–256 KiB without
+/// autotuning) caps a single carrier's throughput on high-BDP paths.
+/// Loopback legs (the client side's engine hops) keep OS defaults —
+/// buffers there only cost memory.
+const TUNED_SOCKET_BUFS: usize = 1024 * 1024;
+
+/// S23: apply the WAN-stream socket tuning: TCP_NODELAY (S21) plus
+/// 1 MiB send/receive buffers. Best-effort — every option is advisory
+/// (an OS may clamp or ignore it) and a failure never kills the stream.
+fn tune_wan_stream(stream: &TcpStream) {
+    let _ = stream.set_nodelay(true);
+    let sock = socket2::SockRef::from(stream);
+    // SAFETY-free: socket2 wraps the raw fd; setsockopt is fallible and
+    // its errors are advisory here (documented in TUNED_SOCKET_BUFS).
+    let _ = sock.set_send_buffer_size(TUNED_SOCKET_BUFS);
+    let _ = sock.set_recv_buffer_size(TUNED_SOCKET_BUFS);
+}
 
 /// Poison-tolerant lock (repo rule).
 fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
@@ -85,11 +105,12 @@ async fn accept_loop(
         };
         let bonds = Arc::clone(bonds);
         tokio::spawn(async move {
-            // S21 P1 perf: the carrier leg is bulk+latency mixed; Nagle on
-            // the public-internet carrier turns each 16 KiB frame's tail
-            // segment into a delayed-ACK stall (~40 ms per frame on Linux
-            // peers) — TCP_NODELAY, matching the client side.
-            let _ = conn.set_nodelay(true);
+            // S21 P1 perf + S23: the carrier leg is bulk+latency mixed;
+            // Nagle on the public-internet carrier turns each 16 KiB
+            // frame's tail segment into a delayed-ACK stall (~40 ms per
+            // frame on Linux peers), and the WAN buffers (see
+            // TUNED_SOCKET_BUFS) keep a full-BDP window in flight.
+            tune_wan_stream(&conn);
             if let Err(e) = handle_conn(bonds, conn).await {
                 tracing::debug!("bond carrier connection ended: {e}");
             }
@@ -269,20 +290,14 @@ async fn session_pump(
 /// Resolve + dial a target (no dependency on a resolver crate: tokio's
 /// `ToSocketAddrs` handles domains via getaddrinfo).
 async fn dial_target(target: &TargetAddr) -> anyhow::Result<TcpStream> {
-    match &target.host {
-        Host::Domain(d) => {
-            let stream = TcpStream::connect((d.as_str(), target.port)).await?;
-            // S21 P1 perf: Nagle here stalls the session pump's 16 KiB
-            // writes behind the target's delayed ACK.
-            let _ = stream.set_nodelay(true);
-            Ok(stream)
-        }
-        Host::Ip(ip) => {
-            let stream = TcpStream::connect(SocketAddr::new(*ip, target.port)).await?;
-            let _ = stream.set_nodelay(true);
-            Ok(stream)
-        }
-    }
+    let stream = match &target.host {
+        // S21 P1 perf (Nagle stalls the session pump's 16 KiB writes
+        // behind the target's delayed ACK) + S23 buffer sizing.
+        Host::Domain(d) => TcpStream::connect((d.as_str(), target.port)).await?,
+        Host::Ip(ip) => TcpStream::connect(SocketAddr::new(*ip, target.port)).await?,
+    };
+    tune_wan_stream(&stream);
+    Ok(stream)
 }
 
 /// Periodic GC: drop bonds whose carriers are all dead for BOND_IDLE_GC.
@@ -325,4 +340,31 @@ mod tests {
             .expect("serve task")
             .expect("serve returns Ok on shutdown");
     }
+
+    /// S23: the WAN tuning applies without error to a real connected
+    /// stream (loopback pair). The actual buffer sizes are OS policy —
+    /// this proves the setsockopt plumbing, not kernel clamps.
+    #[tokio::test]
+    async fn tune_wan_stream_applies_to_a_live_stream() {
+        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
+        let addr = listener.local_addr().unwrap();
+        let server = tokio::spawn(async move {
+            let (stream, _) = listener.accept().await.unwrap();
+            stream
+        });
+        let client = TcpStream::connect(addr).await.unwrap();
+        let accepted = server.await.unwrap();
+        // Neither side may fail (failures are swallowed by design — so
+        // assert the OBSERVABLE nodelay, which socket2 reads back).
+        tune_wan_stream(&client);
+        tune_wan_stream(&accepted);
+        assert!(
+            accepted.nodelay().unwrap_or(false),
+            "tuning includes TCP_NODELAY"
+        );
+        // The requested buffer landed (or was clamped UP — never zero).
+        let sock = socket2::SockRef::from(&accepted);
+        assert!(sock.recv_buffer_size().unwrap_or(0) >= 128 * 1024);
+        assert!(sock.send_buffer_size().unwrap_or(0) >= 128 * 1024);
+    }
 }
diff --git a/docs/research/2026-11-27-stability-changelog-survey.md b/docs/research/2026-11-27-stability-changelog-survey.md
new file mode 100644
index 0000000..ba956c5
--- /dev/null
+++ b/docs/research/2026-11-27-stability-changelog-survey.md
@@ -0,0 +1,86 @@
+# Stability & changelog survey (S23, 2026-11-27)
+
+Scope: the user's ask — "check for any improvements that can be made to
+the connection in terms of speed and stability, and to the UI by
+checking the changelogs of similar applications and implementing
+anything that has worked for them." This is the second changelog pass
+(first: 2026-11-21, `2026-11-21-ui-changelog-survey.md`; the bonding
+literature lives in `2026-11-21-bonding-performance-survey.md`). Sources
+below were read on 2026-11-27 from the projects' own release pages.
+
+## Sources reviewed
+
+- **v2rayN** 7.25.6 (2026-10-11) — the import-compatibility reference:
+  https://github.com/2dust/v2rayN/releases/tag/7.25.6
+  plus the release-history summaries at v2rayn.org mirrors ("computer
+  base rebuilt to launch faster and run more steadily", "global
+  Fragment control", "a rare crash during server switching repaired",
+  "real delay test").
+- **Clash Verge Rev** v2.5.10 (2026-11-03):
+  https://github.com/clash-verge-rev/clash-verge-rev/releases/tag/v2.5.10
+- **Hiddify** app changelog + core feature pages (Balancer semantics,
+  connection-info home screen).
+- **Speedify** release-notes pages (17: redundant-mode streaming
+  enhancements; bonding-mode overview; the socket/buffer-oriented
+  engineering posts behind them).
+- **Windows engineering references** for the os-error-50 root cause:
+  learn.microsoft.com System Error Codes (ERROR_NOT_SUPPORTED = 50),
+  the CreateProcess-with-explorer error-code-50 Stack Overflow case,
+  and the subprocess handle-inheritance writeups that pin it to
+  GUI-subsystem parents (null STD_INPUT_HANDLE → DuplicateHandle fails).
+
+## Adopted (shipped in this branch)
+
+| Finding (source, verbatim where it matters) | What shipped |
+|---|---|
+| v2rayN 7.25.6: "日志和临时文件自动清理时间默认为 7 天" — automatic log/temp-file cleanup, 7-day default | `diagnostics::sweep_old_logs` — 7-day retention, newest-40 floor, live diagnostics file protected, best-effort, runs at every `RealService::with` construction with an INFO line |
+| Window geometry persistence — a standing convention in every desktop client (v2rayN guiNConfig, Clash Verge, VS Code) | `SettingsView.window_w/window_h` (serde-default 1080×720, D8); debounced save after `Resized` (2 quiet ticks), clamped to the 880×580 window minimum; restored via `iced::window::resize` on load; reset-to-defaults restores stock |
+| Speedify-style socket-buffer engineering on WAN legs (their bonding-mode posts discuss per-path window sizing; the MPTCP literature says the same: window-limited paths cap aggregate throughput) | `daemon::tune_wan_stream` — 1 MiB SO_SNDBUF/SO_RCVBUF + existing TCP_NODELAY on the merge daemon's carrier-accept and target-dial legs (the WAN sockets MultiTunnel owns; loopback engine hops intentionally left on OS defaults). Rationale: BDP of a 150 Mbps × 50 ms carrier ≈ 0.94 MiB |
+| The os-error-50 report itself (user, this session) + the documented Windows cause | `mt-app::shell_open` — ShellExecuteW primary + NUL-stdio explorer fallback; the regression write-up lives in the module docs |
+| Hiddify / v2rayN settings-reset affordances ("How to Reset HiddifyManager to default" is one of their most-linked docs) | The Settings "Reset" card (two-click confirm; scope caption; profiles/rules untouched) |
+
+## Confirmed parity (checked, nothing to do)
+
+- v2rayN "real delay" latency tests: `test_latency` already builds the
+  real links and probes through them (S20's first-link fix).
+- Hiddify Balancer (round-robin / lowest-delay / member stickiness):
+  MultiTunnel's five merge policies (D23 + S18 BLEST/ECF) are a superset.
+- Hiddify "connection info when connected — active node, speed, IP":
+  Status screen + titlebar rates + tray tooltip (S17/S18/S22).
+- v2rayN 7.25.6 "lock the maximum core version": the engine is
+  self-provisioned and pinned (D14).
+- Clash Verge 2.5.10's headline fixes (GLOBAL policy group, SSH
+  private-key service mode, GEO DB sync, DNS-overwrite toggle showing
+  stale state): all are Clash-specific machinery; the "toggle shows a
+  state that is not real" class was already fixed here in S22
+  (diagnostics restore-at-construction).
+
+## Deliberately NOT adopted (with reasons)
+
+- **Mux settings in the core UI** (v2rayN #10055): the client→engine leg
+  already carries every proxied connection over one local hop per
+  engine; adding mux knobs would expose Xray-internal knobs this
+  project deliberately hides behind the profile model.
+- **MASQUE / new core protocols** (v2rayN 7.25.6): protocol surface
+  growth is a maintenance stream; the VLESS+REALITY + stream-bonding
+  path is this project's scope.
+- **Custom HTTP headers for subscription updates** (v2rayN #10118):
+  MultiTunnel's subscription support is vless-link paste/import today;
+  headers belong to that future feature, not now.
+- **Redundant mode** (Speedify 17's streaming duplicate-ship): sending
+  every frame on ALL carriers doubles bandwidth — the right behavior
+  for video calls, wrong default for a general tunnel. MultiTunnel's
+  failover + quarantine + BLEST already cover failure-driven switching
+  without the constant duplicate cost. Worth revisiting only as an
+  opt-in "redundant streaming" policy (roadmap).
+- **TUN mode** (Clash Verge / FlClash): rejected in the 2026-11-21
+  survey and unchanged — driver-level risk class.
+
+## Standing perf findings (unchanged from the 2026-11-21 surveys)
+
+The remaining big-ticket perf items are the roadmap's scheduling and
+HoL-blocking work (RTT-trend score, per-carrier active window
+probing), not connection-setup knobs: the S21 work already removed
+Nagle stalls and send-path copies, and the client's own legs are
+loopback (the WAN legs belong to the Xray engines and now the tuned
+merge daemon).
diff --git a/src/diagnostics.rs b/src/diagnostics.rs
index d532bbb..d312b1b 100644
--- a/src/diagnostics.rs
+++ b/src/diagnostics.rs
@@ -394,12 +394,17 @@ mod sweep_tests {
     #[test]
     fn missing_dir_and_subdirs_are_not_errors() {
         let dir = scratch("missing").join("nope");
-        assert!(sweep_old_logs(&dir, SystemTime::now(), std::time::Duration::ZERO, 0, None)
-            .is_empty());
+        assert!(
+            sweep_old_logs(&dir, SystemTime::now(), std::time::Duration::ZERO, 0, None).is_empty()
+        );
         // A subdirectory is never touched.
         let real = scratch("subdir");
         std::fs::create_dir_all(real.join("nested")).expect("subdir");
-        let _ = touch(&real, "old.log", Some(std::time::Duration::from_secs(10_000_000)));
+        let _ = touch(
+            &real,
+            "old.log",
+            Some(std::time::Duration::from_secs(10_000_000)),
+        );
         let pruned = sweep_old_logs(
             &real,
             SystemTime::now(),
diff --git a/src/real_service.rs b/src/real_service.rs
index 4da6336..90b5627 100755
--- a/src/real_service.rs
+++ b/src/real_service.rs
@@ -3509,8 +3509,8 @@ mod tests {
         let echo = spawn_echo().await;
         let paths = scratch("sweep");
         std::fs::create_dir_all(&paths.log_dir).expect("log dir");
-        let stale_mtime = std::time::SystemTime::now()
-            - std::time::Duration::from_secs(9 * 24 * 3600);
+        let stale_mtime =
+            std::time::SystemTime::now() - std::time::Duration::from_secs(9 * 24 * 3600);
         let pin = |p: &std::path::Path| {
             std::fs::File::options()
                 .write(true)
@@ -3527,8 +3527,7 @@ mod tests {
             pin(&p);
         }
         for i in 0..2 {
-            std::fs::write(paths.log_dir.join(format!("fresh-{i}.log")), b"x")
-                .expect("write");
+            std::fs::write(paths.log_dir.join(format!("fresh-{i}.log")), b"x").expect("write");
         }
 
         let _svc = RealService::with(paths, Arc::new(FakeFactory { echo, fail: false }));
@@ -3540,7 +3539,10 @@ mod tests {
             .map(|e| e.file_name().to_string_lossy().into_owned())
             .collect();
         assert_eq!(survived.len(), 40, "48 files − 8 beyond the newest-40 cap");
-        assert!(survived.iter().any(|n| n.starts_with("fresh-")), "fresh kept");
+        assert!(
+            survived.iter().any(|n| n.starts_with("fresh-")),
+            "fresh kept"
+        );
         // All 46 stale files share ONE pinned mtime (9 days old); the
         // newest-40 cap counts the 2 fresh files too, so 38 stale files
         // survive and 8 are swept.
```

