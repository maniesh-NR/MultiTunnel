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

- **Stage**: S22 CLOSED — v0.15.0 released (tag on 21fd59f). The user's
  five asks shipped: (1) Exit & speed panel split into TWO columns —
  exit-IP outputs LEFT (heading, IP, location/ISP chips, verdict card),
  speed-test outputs RIGHT (heading, Mbps, volume chips, download
  note), quiet placeholders while idle; pure view change, D24-safe.
  (2) UPLOAD SPEED TEST: "Test upload speed" button appears ONLY after
  the download test concluded, result in the right column;
  speed_test_upload_url setting (serde default cloudflare __up, D8),
  RealService::speed_test_upload POSTs 25 MiB through the same tunnel
  selection (S20 profile_id rule; clock after connect; 30 s/5 s stall/
  64 MiB caps; partial stands; status guard only on clean completion);
  Settings → Advanced "upload test URL" field; diag header redacts it.
  (3) PER-LINK ADAPTER: `via <adapter>` dim line UNDER each link row —
  mt_service::adapter_of_link_name parses "<tag> [adapter]" (dedup
  " #N" stripped; "Wi-Fi Direct"/"Ethernet 2" verbatim), populated in
  link_status_of + carrier_snapshot map + mock; survives util::short
  truncation. (4) OPEN LOG FOLDER FIXED: root cause = the diag-file
  gate (diag_path None after restart → "turn diagnostics on first");
  new MtService::log_dir() (always resolvable) + handler
  create_dir_all + opens the folder itself via open_folder_command
  (explorer/open/xdg-open; spawn-result reported; Settings status +
  Logs-tab WARN, never silent); fetched at boot in BOTH init paths;
  ADJACENT: diagnostics persisted ON re-enables the sink AT
  CONSTRUCTION (RealService::with) — the toggle no longer lies after a
  restart. (5) ICON FILL: make_logo_assets.py re-crops emblem/lockup to
  their ALPHA bboxes (dead ~71px left band dropped), margins
  0.04→0.02/0.10→0.03; measured fills 93.8–96.9%w × 75.0–76.6%h (was
  75–81%w × 61–64%h — tray glyph ~24% larger); ICO invariants held
  (4 entries 16/32/48/256, biWidth=32/biHeight=64, logo_64 16384 B,
  logo_24 2304 B, deterministic, banner/logo/color_wheel untouched).
  RIG sf=1.0+1.5 (mock): split layout, download→upload button→result,
  exit verdict left, via-lines, log-folder click honest-failure
  surfaced (no xdg-open under Xvfb; Windows spawns explorer — spawn
  command unit-pinned). LESSONS: gui_drive clicks on buttons that
  APPEARED after a reflow need a pointer jiggle (no motion event →
  stale hover); content appended BELOW a scrollable's initial viewport
  mimics a stale frame — damage-instrumented the vendored compositor
  (reverted byte-identical) to prove the rows were just below the
  fold; scroll y=400 is ABOVE the viewport (403.2), y=500 works. TESTS
  497/0 (+7). GATES all green (ziglang wheel incomplete AGAIN →
  reinstalled 0.14.1 mid-gates; disk hit 99% → rm -rf target/debug +
  windows target between phases). CI: ubuntu+windows test legs hit the
  DOCUMENTED mt-bond runner flake (domain_targets 60 s deadline, both
  OSes, same commit — logs archived s22-ci-*.log) → failed-jobs rerun
  per protocol.

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
### Commit: 21fd59f - backup(s22-release-prep): 2026-11-25T19:24:03Z (Z User, 2026-11-25)
diff --git a/Cargo.lock b/Cargo.lock
index c32d817..8b3e2b9 100755
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1962,7 +1962,7 @@ dependencies = [
 
 [[package]]
 name = "mt-app"
-version = "0.14.0"
+version = "0.15.0"
 dependencies = [
  "async-trait",
  "iced",
@@ -2073,7 +2073,7 @@ dependencies = [
 
 [[package]]
 name = "multitunnel"
-version = "0.14.0"
+version = "0.15.0"
 dependencies = [
  "anyhow",
  "async-trait",
diff --git a/Cargo.toml b/Cargo.toml
index c96f339..e386a49 100755
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -18,7 +18,7 @@ members = ["crates/*"]
 resolver = "2"
 
 [workspace.package]
-version = "0.14.0"
+version = "0.15.0"
 
 [lib]
 name = "multitunnel"
diff --git a/QUICKSTART.md b/QUICKSTART.md
index 7cf41a5..94e9118 100755
--- a/QUICKSTART.md
+++ b/QUICKSTART.md
@@ -109,6 +109,15 @@ needed for the usual per-user system-proxy writes (expect the SmartScreen
 > (flips live, no reconnect); the Status tab's exit-IP and speed-test
 > cards merged into one compact **Exit & speed** panel.
 >
+> v0.15: the Exit & speed panel splits into two columns — exit-IP
+> outputs on the LEFT, speed-test outputs on the RIGHT; after a download
+> test concludes a **Test upload speed** button appears (result in the
+> right column); every link shows **which adapter it goes through**
+> (`via <adapter>`); Settings → Diagnostics' **Open log folder** works
+> whether the mode is on or off (and a diagnostics toggle persisted ON
+> writes again after a restart); the app icon fills its tile — the
+> glyph grew ~24% in the tray.
+>
 > v0.13.1 (Copilot code-review round): kill-switch hardening — the
 > black-hole port is owned BEFORE the proxy points at it, the proxy
 > hand-off is serialized with disconnect, a failed restore keeps the
diff --git a/README.md b/README.md
index 580d5db..e3ef43d 100755
--- a/README.md
+++ b/README.md
@@ -442,6 +442,28 @@ GUI, no new dependencies):
   response head independently of the payload limit, and the Edit dialog
   rejects port 0 and structurally invalid REALITY fields (empty SNI,
   malformed public key / short ID).
+- **v0.15.0 — the Exit & speed panel reads like a form**: the panel's
+  two halves are now two COLUMNS — every exit-IP output (address,
+  location/ISP chips, single-hop verdict) sits on the LEFT and every
+  speed-test output (download rate, volume/duration chips, per-adapter
+  spread note) sits on the RIGHT, each under its own small heading.
+  After a download test concludes, a **Test upload speed** button
+  appears in the right column: it POSTs a 25 MB payload through the
+  tunnel (URL configurable in Advanced settings) and reports the send
+  rate, volume and duration beside the download result. Every link in
+  the Links card now shows **which adapter it goes through** — a
+  `via <adapter>` line under the link (parsed from the engine's
+  `"profile [adapter]"` link names, dedup ordinals stripped, so the
+  adapter survives even when a long profile tag truncates the name).
+  Settings → Diagnostics' **Open log folder** works in every state: it
+  opens the log folder itself (created on demand) instead of the
+  diagnostics file's parent, so it no longer reports "turn diagnostics
+  on first" — and a diagnostics toggle persisted ON now starts writing
+  again after an app restart (the sink used to start only on an
+  off→on transition). The app icon (window, taskbar, tray, every ICO
+  size) was regenerated so the glyph fills the tile — the mark grew
+  ~24% in the tray after a dead gradient band was cropped out of the
+  source and the margins tightened (93–97% width, 75–77% height fill).
 - **Quick Settings on right-click**: the tray menu is built fresh from
   live state — Connect/Disconnect (state-reflecting), the server list
   (radio-checked on the active profile), Open MultiTunnel and Exit.

### Commit: 51e0144 - backup(s22-rig-verified): 2026-11-25T19:10:03Z (Z User, 2026-11-25)
diff --git a/crates/mt-app/src/app.rs b/crates/mt-app/src/app.rs
index 9f0a41d..f1b0892 100755
--- a/crates/mt-app/src/app.rs
+++ b/crates/mt-app/src/app.rs
@@ -815,6 +815,7 @@ impl App {
             app.load_settings(),
             app.load_adapters(),
             app.fetch_diag_path(),
+            app.fetch_log_dir(),
         ];
         (app, iced::Task::batch(tasks))
     }
@@ -908,6 +909,13 @@ impl App {
         )
     }
 
+    /// S22: the log directory for the "Open log folder" button (fetched
+    /// once at boot; the button opens it in every state).
+    fn fetch_log_dir(&self) -> iced::Task<Message> {
+        let svc = Arc::clone(&self.service);
+        iced::Task::perform(async move { svc.log_dir().await }, Message::LogDirLoaded)
+    }
+
     fn load_adapters(&self) -> iced::Task<Message> {
         let svc = Arc::clone(&self.service);
         iced::Task::perform(async move { svc.adapters().await }, Message::AdaptersLoaded)

### Commit: 6fc0708 - backup(s22-log-folder-fix): 2026-11-25T18:43:12Z (Z User, 2026-11-25)
diff --git a/src/real_service.rs b/src/real_service.rs
index 7fa6545..8243b99 100755
--- a/src/real_service.rs
+++ b/src/real_service.rs
@@ -3388,7 +3388,7 @@ mod tests {
                                     }
                                 })
                                 .unwrap_or(0);
-                            drained = head.len() - split as usize;
+                            drained = head.len() - split;
                             break;
                         }
                     }

( PROJECT_MEMORY.md's own diff is elided here — the durable
  sections above are hand-maintained; see harness/memory/LEDGER.md
  for the full stage history. )
```

