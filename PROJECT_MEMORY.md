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

- **Stage**: S19 IMPLEMENTED, gates green, v0.13.0 releasing — the six
  asks shipped: (1) server EDITING (Servers → per-card Edit button →
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
### Commit: 001d4e0 - s19 review fixes: black-hole port OWNED while engaged, KILL SWITCH banner, edit-failure logging, chained diag-path refresh — (1) [blocker fix] the kill switch now BINDS 127.0.0.1:9 itself while engaged (spawn_blackhole accept loop swallows every connection) so a local process cannot pre-bind the black-hole port and turn it into its own proxy; bind failure falls back to the documented dead-port semantics (still fail-closed; Linux dev hosts without CAP_NET_BIND_SERVICE take the fallback); stop_blackhole wired into every proxy-restore path (both disconnect branches, the toggle-off escape hatch, connect step-0 self-heal) + owns/releases verified by test; (2) [major fix] Status shows a 'KILL SWITCH ACTIVE — traffic is blocked' error-container banner exactly when kill_switch && Failed (pure gate kill_switch_banner_visible, unit-tested) — the give_up state was previously only visible in the Logs tab; (3) [minor fix] a failed server edit ALSO lands in the Logs tab ('WARN server edit failed: …') — the dialog error was easy to dismiss and lose; (4) [minor fix] DiagnosticsToggled chains save→then(path refresh) instead of a batch (the batch could observe the pre-toggle path); (5) review findings assessed as non-issues (documented in the transcript): the speed-test 'race' (guard exists at handler entry), 'unbounded read' (per-read 5s stall + 30s budget + 64MiB cap), 'memory growth' (bytes counted, never accumulated), diagnostics mutex 'race' (no await under the lock), chunked-encoding skew (we measure received wire bytes; framing overhead ~1e-4), update_profile field coverage (non-edited fields cannot change — the capability check covers the result). Review transcript: /tmp/s19-review-transcript.md (3-round discussion; Copilot channels unavailable — no account entitlement, models.github.ai proxy-blocked; z-ai strongest model used per the fallback instruction). TESTS: 466/0 (+3); gates fmt/clippy host+windows-gnu clean; rig: Edit modal open/cancel re-verified after the changes (nonwhite 1.000→0.949 pattern + VLM field read) (Z User, 2026-11-21)
diff --git a/crates/mt-app/src/app.rs b/crates/mt-app/src/app.rs
index fc23329..4cb0442 100755
--- a/crates/mt-app/src/app.rs
+++ b/crates/mt-app/src/app.rs
@@ -642,6 +642,13 @@ pub fn app_version() -> &'static str {
     env!("CARGO_PKG_VERSION")
 }
 
+/// S19 (review fix): should the Status screen show the KILL SWITCH banner?
+/// Pure gate (unit-tested): the switch is armed AND the tunnel FAILED —
+/// the give_up path engages the black hole exactly in that state.
+pub fn kill_switch_banner_visible(state: ServiceState, kill_switch: bool) -> bool {
+    kill_switch && state == ServiceState::Failed
+}
+
 impl App {
     /// Bare constructor with empty state (service loads fill it).
     pub fn new(service: Arc<dyn MtService>, flags: Flags) -> Self {
@@ -1682,6 +1689,9 @@ impl App {
                     return self.load_profiles();
                 }
                 Err(e) => {
+                    // Review fix: surface the failure in the Logs tab too —
+                    // the dialog error is easy to dismiss and lose.
+                    util::push_log(&mut self.logs, format!("WARN  server edit failed: {e}"));
                     if let Some(d) = self.edit.as_mut() {
                         d.saving = false;
                         d.error = Some(e);
@@ -1713,10 +1723,17 @@ impl App {
             }
             Message::DiagnosticsToggled(v) => {
                 self.settings.diagnostics = v;
+                // Review fix: CHAIN the path refresh after the save — a
+                // batch could observe the pre-toggle path (stale caption).
                 let save = self.save_settings(self.settings.clone());
-                // The service creates/closes the file during the save;
-                // refresh the displayed path right after.
-                return iced::Task::batch([save, self.fetch_diag_path()]);
+                let svc = Arc::clone(&self.service);
+                return save.then(move |_| {
+                    let svc = Arc::clone(&svc);
+                    iced::Task::perform(
+                        async move { svc.diagnostics_path().await },
+                        Message::DiagPath,
+                    )
+                });
             }
             Message::DiagPath(p) => self.diag_path = p,
             Message::OpenLogsFolder => {
@@ -3274,4 +3291,39 @@ mod tests {
         drop(app.update(Message::DiagPath(Some("/tmp/x.log".into()))));
         assert_eq!(app.diag_path.as_deref(), Some("/tmp/x.log"));
     }
+
+    /// S19 (review fix): the KILL SWITCH banner gate — visible exactly
+    /// when the switch is armed AND the tunnel FAILED (the give_up state).
+    #[test]
+    fn kill_switch_banner_gate() {
+        use mt_service::ServiceState as St;
+        assert!(crate::app::kill_switch_banner_visible(St::Failed, true));
+        assert!(!crate::app::kill_switch_banner_visible(St::Failed, false));
+        assert!(!crate::app::kill_switch_banner_visible(St::Connected, true));
+        assert!(!crate::app::kill_switch_banner_visible(
+            St::Disconnected,
+            true
+        ));
+        assert!(!crate::app::kill_switch_banner_visible(
+            St::Connecting,
+            true
+        ));
+    }
+
+    /// S19 (review fix): a failed server edit ALSO lands in the Logs tab.
+    #[cfg(feature = "mock")]
+    #[tokio::test]
+    async fn edit_save_failure_is_logged() {
+        let service = Arc::new(crate::mock::MockService::new());
+        let mut app = App::init_async(service, Flags::default()).await;
+        let id = app.profiles[0].id.clone();
+        let details = app.service.profile_details(&id).await.expect("details");
+        drop(app.update(Message::EditLoaded(id, Ok(details))));
+        drop(app.update(Message::EditSaved(Err("boom".into()))));
+        assert!(app
+            .logs
+            .iter()
+            .any(|l| l.contains("server edit failed: boom")));
+        assert!(app.edit.is_some(), "dialog stays open with the error");
+    }
 }
diff --git a/crates/mt-app/src/screens.rs b/crates/mt-app/src/screens.rs
index f967f77..1a2d355 100755
--- a/crates/mt-app/src/screens.rs
+++ b/crates/mt-app/src/screens.rs
@@ -802,6 +802,32 @@ pub fn status(app: &App) -> Element<'_> {
 
     // --- ONE page scrollable below the frame (S13) ---------------------
     let mut below = column![].spacing(12).width(Length::Fill);
+    // S19 (review fix): a KILL SWITCH banner when the tunnel FAILED with
+    // the switch armed — the state give_up() leaves the machine in.
+    if crate::app::kill_switch_banner_visible(
+        snap.as_ref()
+            .map_or(ServiceState::Disconnected, |s| s.state),
+        app.settings.kill_switch,
+    ) {
+        below = below.push(
+            container(
+                column![
+                    text("KILL SWITCH ACTIVE — traffic is blocked")
+                        .size(ts.title_s.size)
+                        .color(s.on_error_container),
+                    text(
+                        "the tunnel died on its own, so system traffic is being blocked \n                         (fail-closed) instead of leaking outside the tunnel. Connect again \n                         to resume, or turn the kill switch off in Settings → Network.",
+                    )
+                    .size(ts.body_s.size)
+                    .color(s.on_error_container),
+                ]
+                .spacing(4),
+            )
+            .padding(14)
+            .style(t3::card(s, s.error_container))
+            .width(Length::Fill),
+        );
+    }
     below = below.push(exit_card);
     below = below.push(speed_card);
     below = below.push(bottom_card);
diff --git a/src/real_service.rs b/src/real_service.rs
index 4e42ff5..65e2703 100755
--- a/src/real_service.rs
+++ b/src/real_service.rs
@@ -166,8 +166,15 @@ const SPEED_TEST_STALL: Duration = Duration::from_secs(5);
 /// pointed here, WinINET consumers get instant connection-refused
 /// instead of silently going DIRECT after a spontaneous tunnel death
 /// (fail-closed). Reuses `sysproxy::set` so the ORIGINAL pre-tunnel
-/// snapshot stays intact for the eventual restore.
+/// snapshot stays intact for the eventual restore. While engaged the
+/// app also BINDS the port (see [`spawn_blackhole`]) so a local process
+/// cannot pre-bind it and turn the black hole into its own proxy.
 const KILL_SWITCH_BLACKHOLE_PROXY: &str = "127.0.0.1:9";
+/// S19 (review fix): the port [`KILL_SWITCH_BLACKHOLE_PROXY`] names.
+/// Windows does not reserve ports < 1024 for admins, so an unprivileged
+/// app can own it; on hosts where the bind fails the dead-port semantics
+/// (instant connection-refused) still hold.
+const KILL_SWITCH_BLACKHOLE_PORT: u16 = 9;
 
 /// Filesystem locations the service works with (injected in tests).
 #[derive(Debug, Clone)]
@@ -762,6 +769,11 @@ struct Core {
     /// Per-link `(up, down)` byte totals accrued from `ConnClosed` events.
     bytes: Mutex<HashMap<String, (u64, u64)>>,
     sysproxy_set: AtomicBool,
+    /// S19 (review fix): the kill switch's black-hole listener — while
+    /// engaged we OWN [`KILL_SWITCH_BLACKHOLE_PORT`] so a local process
+    /// cannot pre-bind it and turn the black hole into its own proxy.
+    /// Accept loop drops every connection; the watch sender stops it.
+    blackhole: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
     disconnect_requested: AtomicBool,
     restarts: AtomicU32,
 }
@@ -855,6 +867,7 @@ impl RealService {
                 session: Mutex::new(None),
                 bytes: Mutex::new(HashMap::new()),
                 sysproxy_set: AtomicBool::new(false),
+                blackhole: Mutex::new(None),
                 disconnect_requested: AtomicBool::new(false),
                 restarts: AtomicU32::new(0),
             }),
@@ -903,8 +916,10 @@ impl RealService {
         // 0. Stale sysproxy self-heal (v0.2 connect step 0 / D5,
         //    S6-REV-C P1-3b): a previous run that died without disconnecting
         //    may have left the registry pointing at a dead local listener.
-        //    Best-effort — a failure never blocks connecting.
+        //    Best-effort — a failure never blocks connecting. This also
+        //    clears any KILL SWITCH black-hole from a previous session.
         let _ = crate::sysproxy::restore(&self.core.paths.state_dir);
+        stop_blackhole(&self.core);
 
         let settings = SettingsFile::load(&self.core.paths.settings_file());
         // S9: refresh the engine log level BEFORE links are built.
@@ -1266,6 +1281,56 @@ async fn pump(
     }
 }
 
+/// S19 (review fix): bind + OWN the black-hole port while the kill
+/// switch is engaged. The accept loop drops every connection the moment
+/// it lands — nothing legible ever flows. A bind failure is non-fatal:
+/// the unbound port still refuses connections (dead-port semantics, still
+/// fail-closed); only the local pre-bind guarantee is lost. On Windows an
+/// unprivileged process may bind port 9; dev hosts without that right
+/// simply take the fallback.
+async fn spawn_blackhole(core: &Core) {
+    let addr = SocketAddr::from(([127, 0, 0, 1], KILL_SWITCH_BLACKHOLE_PORT));
+    let listener = match tokio::net::TcpListener::bind(addr).await {
+        Ok(l) => l,
+        Err(e) => {
+            tracing::warn!(
+                "kill switch: could not own the black-hole port {addr} ({e}) \
+                 — dead-port fallback (still fail-closed)"
+            );
+            return;
+        }
+    };
+    let (tx, mut rx) = tokio::sync::watch::channel(false);
+    tokio::spawn(async move {
+        loop {
+            tokio::select! {
+                changed = rx.changed() => {
+                    if changed.is_err() || *rx.borrow() {
+                        break;
+                    }
+                }
+                accepted = listener.accept() => {
+                    if accepted.is_err() {
+                        break;
+                    }
+                    // The accepted socket drops here BY DESIGN — the
+                    // connection is swallowed, never read, never answered.
+                }
+            }
+        }
+    });
+    *lock(&core.blackhole) = Some(tx);
+}
+
+/// S19 (review fix): stop the black-hole listener (every proxy-restore
+/// path: explicit disconnect, the kill-switch toggle-off escape hatch,
+/// and the connect step-0 self-heal).
+fn stop_blackhole(core: &Core) {
+    if let Some(tx) = lock(&core.blackhole).take() {
+        let _ = tx.send(true);
+    }
+}
+
 /// D5 fail-open give-up (v0.2 supervisor semantics for restart exhaustion):
 /// retire the session, mark `Failed`, and stop the server. The system
 /// proxy hand-off depends on the S19 kill switch:
@@ -1301,6 +1366,10 @@ async fn give_up(core: &Core, shutdown: &tokio::sync::watch::Sender<bool>) {
                 &core.paths.state_dir,
             ) {
                 Ok(()) => {
+                    // Review fix: OWN the port so a local process cannot
+                    // pre-bind it and turn the black hole into a proxy of
+                    // its own (the accept loop swallows connections).
+                    spawn_blackhole(core).await;
                     core.emit(ServiceEvent::LogLine(
                         "KILL SWITCH engaged: the tunnel died on its own — system traffic is \
                          now BLOCKED (fail-closed) instead of leaking direct. Connect again \
@@ -1548,6 +1617,7 @@ impl MtService for RealService {
                 if let Err(e) = crate::sysproxy::restore(&self.core.paths.state_dir) {
                     tracing::warn!("system proxy restore failed: {e}");
                 }
+                stop_blackhole(&self.core);
                 self.core
                     .emit(ServiceEvent::LogLine("system proxy restored".into()));
             }
@@ -1565,6 +1635,7 @@ impl MtService for RealService {
             if let Err(e) = crate::sysproxy::restore(&self.core.paths.state_dir) {
                 tracing::warn!("system proxy restore failed: {e}");
             }
+            stop_blackhole(&self.core);
         }
         self.core.bytes_reset();
         self.core.set_state(ServiceState::Disconnected);
@@ -1789,6 +1860,7 @@ impl MtService for RealService {
             && self.core.sysproxy_set.swap(false, Ordering::SeqCst)
         {
             let _ = crate::sysproxy::restore(&self.core.paths.state_dir);
+            stop_blackhole(&self.core);
             self.core.emit(ServiceEvent::LogLine(
                 "kill switch disabled — system proxy restored".into(),
             ));
@@ -3027,6 +3099,44 @@ mod tests {
         assert!(saw_restore, "the restore is announced in the GUI log");
     }
 
+    /// S19 (review fix): spawn_blackhole OWNS the port while engaged (a
+    /// local process cannot pre-bind it) and stop_blackhole releases it —
+    /// the accept loop swallows connections (an open TCP dial to it lands
+    /// and closes immediately instead of being answered by anyone else).
+    /// On hosts where binding port 9 is not permitted the spawn is a
+    /// documented no-op fallback (dev/linux root-only port).
+    #[tokio::test]
+    async fn blackhole_listener_owns_and_releases_the_port() {
+        let exit = spawn_exit_echo("127.0.0.1").await;
+        let paths = scratch("blackhole");
+        write_settings(&paths, exit.port(), false);
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: exit,
+                fail: false,
+            }),
+        );
+        // Engage.
+        spawn_blackhole(&svc.core).await;
+        let guard_taken = svc.core.blackhole.lock().unwrap().is_some();
+        if guard_taken {
+            // The port answers to US: a connect lands (accepted + dropped).
+            let dial = tokio::net::TcpStream::connect("127.0.0.1:9").await;
+            assert!(dial.is_ok(), "our own listener owns the port");
+            // Release: the next connect is refused again.
+            stop_blackhole(&svc.core);
+            tokio::time::sleep(Duration::from_millis(150)).await;
+            let refused = tokio::net::TcpStream::connect("127.0.0.1:9").await;
+            assert!(refused.is_err(), "port released after stop");
+        } else {
+            // Documented fallback host (e.g. Linux without CAP_NET_BIND_SERVICE):
+            // dead-port semantics — nothing listens, connects are refused.
+            let refused = tokio::net::TcpStream::connect("127.0.0.1:9").await;
+            assert!(refused.is_err(), "fallback keeps the port dead");
+        }
+    }
+
     #[tokio::test]
     async fn stream_bond_mode_connects_and_carries_traffic() {
         let (daemon, _guard) = spawn_merge_daemon().await;

### Commit: 14e3e79 - s19: server editing, offline exit-IP + speed test, diagnostics mode, kill switch — (1) SERVER EDITING: Servers tab per-card Edit button opens a full-window modal (dim backdrop cancels on click; S16 overlay discipline); fields: name, remark, address, port, UUID + REALITY set (SNI/fingerprint/pbk/sid/spiderX, reality profiles only); mt-service ProfileDetails round-trips profile_details→update_profile; ProfileStore::update replaces BY ID in place (id, list position, active preserved; NO dedupe on explicit edit); RealService validates primitives (empty name/address/identity, port 0-65535) + capability-regression check (an edit that would break a convertible profile is rejected with the reason; a never-convertible profile edits freely); editing the active server while connected logs 'reconnect to apply'. (2) EXIT IP WHILE OFFLINE: exit_probe prefers the LIVE session, falls back to temp_link_for() — a one-shot factory link through the active profile (same engine path as test_latency); ExitInfo.via_temp_tunnel flags the path for the GUI chip ('temporary probe tunnel' vs 'live tunnel'); shared http_get()/parse_exit_info() helpers; EXIT_PROBE_MAX_BYTES 64 KiB cap. (3) SPEED TEST: Status card 'Run speed test'; speed_test(Option<id>) rides the live session when the target is the active profile, else the temp tunnel; downloads settings.speed_test_url (default http://speed.cloudflare.com/__down?bytes=25000000 — plain HTTP verified 200/25MB; the client→server leg is REALITY-encrypted) with per-read 5 s stall + 30 s budget + 64 MiB caps; the clock starts AFTER connect+tunnel setup (engine spawn never pollutes the rate); HTTP status ≥400 rejected; SpeedTestInfo{mbps, mbytes, seconds, profile_tag, via_temp_tunnel}; URL validated in set_settings like exit_probe_url. (4) DIAGNOSTICS MODE: SettingsView.diagnostics toggle; src/diagnostics.rs — one paste-ready <log_dir>/diagnostics-YYYYMMDD-HHMMSS.log with a redacted header (version/OS/settings summary/profile endpoints — NEVER uuid/password/pbk, unit-tested); GUI LogLine tee (Core::emit) + a hand-rolled process-global tracing::Subscriber (the e2e PrintSubscriber pattern; no tracing-subscriber dep) captures engine child output + supervisor decisions; engine log level forced debug for the session while ON; diagnostics_path() surfaces the file; Settings card shows the path + 'Open log folder' (explorer/xdg-open); off keeps the file on disk; crate-test lock (tokio Mutex) guards the process-global sink. (5) KILL SWITCH: SettingsView.kill_switch; give_up() fail-CLOSED interception — the system proxy is re-pointed at 127.0.0.1:9 (KILL_SWITCH_BLACKHOLE_PROXY, discard port) via sysproxy::set (keep-first: the original pre-tunnel snapshot survives for the true restore) + 'KILL SWITCH engaged' GUI log; sysproxy_set stays true; explicit disconnect() now ALSO restores proxy state when there is no session (the post-give-up black-hole release); toggling the switch OFF while disconnected is an escape hatch; socks/http-listener consumers were already fail-closed (listeners die with the session; proxied conns error, never direct). (6) Settings: kill-switch toggler + honest fail-open/fail-closed caption (Network card), speed-test URL field (Advanced), Diagnostics card; Status exit/speed buttons gate on has_active (not connected); mt-app EditDraft/EditField + messages; MockService implements all four new methods + offline exit probe (via_temp_tunnel simulated). TESTS: 463/0 (+16: 2 mt-links store.update, 3 mt-service s19 migrations, 2 diagnostics, exit_probe offline/no-active, 2 speed test, 2 update_profile, diag-mode file+engine-debug+redaction, kill-switch release, 3 mt-app edit/speed/toggles). Gates: fmt/clippy host+windows-gnu -D warnings clean; vendored iced 6/0; mt-app --smoke 0; rig sf=1.0+1.5 VLM-verified (52.0 Mbps + 'temporary probe tunnel' chips, offline exit IP, Edit modal fields, rename→save→card updated, Cancel + backdrop dismissal, kill switch + diagnostics toggles ON); zigbuild PE32+ ×4 + ELF ×2; version 0.13.0; README v0.13 section + QUICKSTART note (Z User, 2026-11-21)
diff --git a/Cargo.lock b/Cargo.lock
index cc65bd2..ca35e7c 100755
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1962,7 +1962,7 @@ dependencies = [
 
 [[package]]
 name = "mt-app"
-version = "0.12.0"
+version = "0.13.0"
 dependencies = [
  "async-trait",
  "iced",
@@ -2073,7 +2073,7 @@ dependencies = [
 
 [[package]]
 name = "multitunnel"
-version = "0.12.0"
+version = "0.13.0"
 dependencies = [
  "anyhow",
  "async-trait",
diff --git a/Cargo.toml b/Cargo.toml
index 058ccd7..c40fea9 100755
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -18,7 +18,7 @@ members = ["crates/*"]
 resolver = "2"
 
 [workspace.package]
-version = "0.12.0"
+version = "0.13.0"
 
 [lib]
 name = "multitunnel"
diff --git a/QUICKSTART.md b/QUICKSTART.md
index 19e1b1c..fbe19d9 100755
--- a/QUICKSTART.md
+++ b/QUICKSTART.md
@@ -88,6 +88,15 @@ needed for the usual per-user system-proxy writes (expect the SmartScreen
 > totals, live speeds), the tray/taskbar icons are transparent with a
 > bigger mark, and Settings gained a full accent COLOR WHEEL plus a
 > System / Light / Dark appearance mode (System follows the OS).
+>
+> v0.13: servers can be EDITED after the fact (Servers → Edit: name,
+> remark, address, port, UUID and the REALITY fields), the exit-IP check
+> and a new Speed test work even while DISCONNECTED (they spawn a
+> temporary probe tunnel through the active server), Settings gained a
+> KILL SWITCH (fail-closed: if the tunnel dies on its own, the system
+> proxy blocks traffic instead of silently going direct) and a
+> DIAGNOSTICS mode that writes one paste-ready support log file
+> (connection lifecycle + engine output, credentials redacted).
 
 **SMOKE TEST — this is the first time the GUI binary runs on real Windows.**
 The development sandbox is Linux; the window has been cross-compiled and
diff --git a/README.md b/README.md
index 3c78aaa..69d7e74 100755
--- a/README.md
+++ b/README.md
@@ -374,6 +374,30 @@ GUI, no new dependencies):
   color-scheme), re-checked every few seconds while the app runs. A
   survey of the multipath-bonding literature (BLEST, ECF, MP-QUIC…) and
   how it maps onto the scheduler shipped alongside in `docs/research/`.
+- **Server editing, offline probes, kill switch, diagnostics (v0.13)**:
+  every server card gained an **Edit** button — name, remark, address,
+  port, UUID and the REALITY fields (SNI, fingerprint, public key, short
+  ID, spiderX) are editable after import; the id, the active selection and
+  the list position are preserved, and an edit that would break a profile
+  that used to connect is rejected with the reason. The Status tab's
+  **Check exit IP** and the new **Speed test** work even while
+  DISCONNECTED: both spawn a temporary probe tunnel through the active
+  server (the same engine path a latency test uses) and tear it down
+  after — the card shows whether the answer came through the live tunnel
+  or a temporary one. The speed test downloads a payload through the
+  server (URL configurable in Advanced settings) and reports Mbps, volume
+  and duration; tunnel setup is excluded from the rate. Settings →
+  Network gained a **kill switch**: when the tunnel dies on its own while
+  MultiTunnel owns the system proxy, the proxy is re-pointed at a dead
+  loopback port so traffic FAILS instead of silently leaking direct
+  (fail-closed); connecting again — or toggling the switch off, or a
+  manual disconnect — restores normal operation. Settings → Diagnostics
+  writes one **paste-ready support log** (`diagnostics-<timestamp>.log`
+  in the log folder): connection lifecycle events, engine output (log
+  level forced to `debug` for the session), link health and a redacted
+  settings/profile summary — UUIDs, passwords and public keys never
+  appear in it. Turn it on, reproduce a connection issue, then paste the
+  file into a support chat.
 - **Quick Settings on right-click**: the tray menu is built fresh from
   live state — Connect/Disconnect (state-reflecting), the server list
   (radio-checked on the active profile), Open MultiTunnel and Exit.
diff --git a/crates/mt-app/src/app.rs b/crates/mt-app/src/app.rs
index 11f42bd..fc23329 100755
--- a/crates/mt-app/src/app.rs
+++ b/crates/mt-app/src/app.rs
@@ -368,6 +368,33 @@ pub enum Message {
     TitleMenu(TitleMenuItem),
     /// "always on top" toggled (window menu or Settings).
     AlwaysOnTopToggled(bool),
+    // --- S19: server editing, speed test, kill switch, diagnostics ---
+    /// Open the Edit dialog for a server (fetch the editable material).
+    EditServer(String),
+    /// `profile_details` finished (id + the prefill, or an error).
+    EditLoaded(String, Result<mt_service::ProfileDetails, String>),
+    /// One Edit-dialog field changed.
+    EditField(EditField, String),
+    /// Save the Edit dialog's draft (`update_profile`).
+    EditSave,
+    /// `update_profile` finished.
+    EditSaved(Result<mt_service::ProfileView, String>),
+    /// Close the Edit dialog without saving.
+    EditCancel,
+    /// Run the speed test through the active profile.
+    SpeedTest,
+    /// The speed test finished.
+    SpeedTested(Result<mt_service::SpeedTestInfo, String>),
+    /// Kill switch toggled (Settings → Network).
+    KillSwitchToggled(bool),
+    /// Diagnostics mode toggled (Settings → Diagnostics).
+    DiagnosticsToggled(bool),
+    /// The diagnostics file path arrived (init / after a toggle).
+    DiagPath(Option<String>),
+    /// Speed-test URL changed (Settings → Advanced).
+    SpeedUrlChanged(String),
+    /// Open the log folder in the OS file manager.
+    OpenLogsFolder,
 }
 
 /// S16: one entry of the titlebar right-click window menu.
@@ -379,6 +406,95 @@ pub enum TitleMenuItem {
     Close,
 }
 
+/// S19: one editable field of the server-Edit dialog (indexes into
+/// [`EditDraft`]'s string fields; the update() arm writes exactly one).
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum EditField {
+    Tag,
+    Remark,
+    Address,
+    Port,
+    Identity,
+    Sni,
+    Fingerprint,
+    PublicKey,
+    ShortId,
+    SpiderX,
+}
+
+/// S19: the Edit dialog's draft state (all text fields are strings; the
+/// service validates + converts on save).
+#[derive(Debug, Clone)]
+pub struct EditDraft {
+    pub id: String,
+    pub tag: String,
+    pub remark: String,
+    /// Read-only protocol label ("vless" …).
+    pub protocol: String,
+    pub address: String,
+    pub port: String,
+    pub identity: String,
+    /// Read-only security label ("reality" …); the REALITY-only fields
+    /// below render only when this is "reality".
+    pub security: String,
+    pub sni: String,
+    pub fingerprint: String,
+    pub public_key: String,
+    pub short_id: String,
+    pub spider_x: String,
+    /// A save is in flight (disables the Save button).
+    pub saving: bool,
+    /// Last save error (shown in the dialog; keeps it open).
+    pub error: Option<String>,
+}
+
+impl EditDraft {
+    /// Prefill from the service's `profile_details` payload.
+    pub fn from_details(d: mt_service::ProfileDetails) -> Self {
+        Self {
+            id: d.id,
+            tag: d.tag,
+            remark: d.remark,
+            protocol: d.protocol,
+            address: d.address,
+            port: d.port,
+            identity: d.identity,
+            security: d.security,
+            sni: d.sni,
+            fingerprint: d.fingerprint,
+            public_key: d.public_key,
+            short_id: d.short_id,
+            spider_x: d.spider_x,
+            saving: false,
+            error: None,
+        }
+    }
+
+    /// The draft as a save payload (the service re-validates).
+    pub fn to_details(&self) -> mt_service::ProfileDetails {
+        mt_service::ProfileDetails {
+            id: self.id.clone(),
+            tag: self.tag.clone(),
+            remark: self.remark.clone(),
+            protocol: self.protocol.clone(),
+            address: self.address.clone(),
+            port: self.port.clone(),
+            identity: self.identity.clone(),
+            security: self.security.clone(),
+            sni: self.sni.clone(),
+            fingerprint: self.fingerprint.clone(),
+            public_key: self.public_key.clone(),
+            short_id: self.short_id.clone(),
+            spider_x: self.spider_x.clone(),
+        }
+    }
+
+    /// TRUE iff the security-layer fields apply (REALITY profiles only).
+    pub fn is_reality(&self) -> bool {
+        self.security.eq_ignore_ascii_case("reality")
+    }
+}
+
 /// Valid routing rule kinds (matches the mt-net router matcher).
 pub const RULE_KINDS: [&str; 4] = ["domainSuffix", "domainKeyword", "ipCidr", "any"];
 /// Valid routing rule actions (matches the mt-net router actions).
@@ -507,6 +623,17 @@ pub struct App {
     /// S18: last cursor position inside the wheel (local coords) — the
     /// press picks the color under the cursor without needing a move.
     pub wheel_cursor: Option<iced::Point>,
+    /// S19: the open server-Edit dialog (None = closed). Holds the draft
+    /// fields + the save-in-flight flag; opens via `Message::EditServer`.
+    pub edit: Option<EditDraft>,
+    /// S19: last speed-test result for the Status screen (None = never
+    /// run).
+    pub speed: Option<Result<mt_service::SpeedTestInfo, String>>,
+    /// S19: a speed test is in flight (drives the card's button state).
+    pub speed_testing: bool,
+    /// S19: the diagnostics log file currently being written (Settings
+    /// screen; None when the mode is off).
+    pub diag_path: Option<String>,
 }
 
 /// S17: the release version shown next to the app name in the titlebar.
@@ -539,6 +666,9 @@ impl App {
             exit_probe_url: "http://ip-api.com/json".to_string(),
             close_to_tray: false,
             always_on_top: false,
+            kill_switch: false,
+            diagnostics: false,
+            speed_test_url: "http://speed.cloudflare.com/__down?bytes=25000000".to_string(),
         };
         // S18: detect the OS appearance once so the System mode (the
         // stock default) starts honest.
@@ -592,6 +722,11 @@ impl App {
             sys_poll: 0,
             wheel_dragging: false,
             wheel_cursor: None,
+            // S19
+            edit: None,
+            speed: None,
+            speed_testing: false,
+            diag_path: None,
         }
     }
 
@@ -621,6 +756,7 @@ impl App {
             app.load_rules(),
             app.load_settings(),
             app.load_adapters(),
+            app.fetch_diag_path(),
         ];
         (app, iced::Task::batch(tasks))
     }
@@ -645,6 +781,7 @@ impl App {
         app.rules_saved = app.rules.clone();
         app.settings = app.service.settings().await;
         app.seed_text = app.settings.theme_seed.to_string();
+        app.diag_path = app.service.diagnostics_path().await;
         // S18: resolve light/dark through the picked mode + OS preference.
         app.sys_dark = crate::theme::system_prefers_dark();
         app.theme.rederive(
@@ -702,6 +839,16 @@ impl App {
         iced::Task::perform(async move { svc.settings().await }, Message::SettingsLoaded)
     }
 
+    /// S19: refresh the diagnostics file path shown on the Settings screen
+    /// (the service creates the file when the mode turns on).
+    fn fetch_diag_path(&self) -> iced::Task<Message> {
+        let svc = Arc::clone(&self.service);
+        iced::Task::perform(
+            async move { svc.diagnostics_path().await },
+            Message::DiagPath,
+        )
+    }
+
     fn load_adapters(&self) -> iced::Task<Message> {
         let svc = Arc::clone(&self.service);
         iced::Task::perform(async move { svc.adapters().await }, Message::AdaptersLoaded)
@@ -1466,6 +1613,150 @@ impl App {
                 self.settings.exit_probe_url = t;
                 return self.save_settings(self.settings.clone());
             }
+            Message::SpeedUrlChanged(t) => {
+                self.settings.speed_test_url = t;
+                return self.save_settings(self.settings.clone());
+            }
+            // --- S19: server editing, speed test, kill switch, diagnostics ---
+            Message::EditServer(id) => {
+                let svc = Arc::clone(&self.service);
+                let id_for_res = id.clone();
+                return iced::Task::perform(
+                    async move {
+                        let res = svc.profile_details(&id_for_res).await;
+                        (id_for_res, res)
+                    },
+                    |(id, res)| Message::EditLoaded(id, res),
+                );
+            }
+            Message::EditLoaded(_id, res) => match res {
+                Ok(details) => {
+                    // Only open when no dialog is up (a stale fetch must
+                    // never clobber the draft the user is typing into).
+                    if self.edit.is_none() {
+                        self.edit = Some(EditDraft::from_details(details));
+                        self.notice = None;
+                    }
+                }
+                Err(e) => self.notice = Some(format!("cannot edit server: {e}")),
+            },
+            Message::EditField(field, value) => {
+                if let Some(d) = self.edit.as_mut() {
+                    match field {
+                        EditField::Tag => d.tag = value,
+                        EditField::Remark => d.remark = value,
+                        EditField::Address => d.address = value,
+                        EditField::Port => d.port = value,
+                        EditField::Identity => d.identity = value,
+                        EditField::Sni => d.sni = value,
+                        EditField::Fingerprint => d.fingerprint = value,
+                        EditField::PublicKey => d.public_key = value,
+                        EditField::ShortId => d.short_id = value,
+                        EditField::SpiderX => d.spider_x = value,
+                    }
+                }
+            }
+            Message::EditSave => {
+                let Some(draft) = self.edit.clone() else {
+                    return iced::Task::none();
+                };
+                if draft.saving {
+                    return iced::Task::none();
+                }
+                if let Some(d) = self.edit.as_mut() {
+                    d.saving = true;
+                    d.error = None;
+                }
+                let svc = Arc::clone(&self.service);
+                let id = draft.id.clone();
+                let payload = draft.to_details();
+                return iced::Task::perform(
+                    async move { svc.update_profile(&id, payload).await },
+                    Message::EditSaved,
+                );
+            }
+            Message::EditSaved(res) => match res {
+                Ok(pv) => {
+                    self.edit = None;
+                    self.notice = Some(format!("saved server \"{}\"", pv.tag));
+                    return self.load_profiles();
+                }
+                Err(e) => {
+                    if let Some(d) = self.edit.as_mut() {
+                        d.saving = false;
+                        d.error = Some(e);
+                    }
+                }
+            },
+            Message::EditCancel => self.edit = None,
+            Message::SpeedTest => {
+                if self.speed_testing {
+                    return iced::Task::none();
+                }
+                self.speed_testing = true;
+                let svc = Arc::clone(&self.service);
+                return iced::Task::perform(
+                    async move { svc.speed_test(None).await },
+                    Message::SpeedTested,
+                );
+            }
+            Message::SpeedTested(res) => {
+                self.speed_testing = false;
+                if let Err(e) = &res {
+                    util::push_log(&mut self.logs, format!("WARN  speed test: {e}"));
+                }
+                self.speed = Some(res);
+            }
+            Message::KillSwitchToggled(v) => {
+                self.settings.kill_switch = v;
+                return self.save_settings(self.settings.clone());
+            }
+            Message::DiagnosticsToggled(v) => {
+                self.settings.diagnostics = v;
+                let save = self.save_settings(self.settings.clone());
+                // The service creates/closes the file during the save;
+                // refresh the displayed path right after.
+                return iced::Task::batch([save, self.fetch_diag_path()]);
+            }
+            Message::DiagPath(p) => self.diag_path = p,
+            Message::OpenLogsFolder => {
+                // The diagnostics file's folder — the button lives on the
+                // Diagnostics card and is only meaningful while the file
+                // exists (diagnostics ON).
+                let Some(diag) = self.diag_path.clone() else {
+                    self.settings_status = Some(
+                        "turn diagnostics on first — the log file is created when it starts"
+                            .to_string(),
+                    );
+                    return iced::Task::none();
+                };
+                let path = std::path::Path::new(&diag)
+                    .parent()
+                    .map(std::path::Path::to_path_buf)
+                    .unwrap_or_else(|| std::path::PathBuf::from(&diag));
+                let opened = if path.exists() {
+                    #[cfg(windows)]
+                    {
+                        std::process::Command::new("explorer")
+                            .arg(&path)
+                            .spawn()
+                            .is_ok()
+                    }
+                    #[cfg(not(windows))]
+                    {
+                        std::process::Command::new("xdg-open")
+                            .arg(&path)
+                            .spawn()
+                            .is_ok()
+                    }
+                } else {
+                    false
+                };
+                if !opened {
+                    self.settings_status =
+                        Some(format!("could not open the log folder: {}", path.display()));
+                }
+            }
             // --- S15: title bar / tray / close-to-tray (D25) ---
             Message::WindowEvent(id, event) => {
                 if self.window_id.is_none() {
@@ -1747,6 +2038,10 @@ impl App {
         if self.title_menu.is_some() {
             stack = stack.push(screens::title_menu_overlay(self));
         }
+        // S19: the server Edit dialog — same full-window layer discipline.
+        if self.edit.is_some() {
+            stack = stack.push(screens::edit_overlay(self));
+        }
         stack.into()
     }
 
@@ -2854,4 +3149,129 @@ mod tests {
         app.service.set_settings(s).await.expect("save");
         assert!(app.service.settings().await.always_on_top);
     }
+
+    // --- S19: server editing, speed test, kill switch, diagnostics ---------
+
+    /// S19: the Edit dialog flow — open (fetch), type into fields, save
+    /// (update_profile), close; a failing save keeps the dialog open with
+    /// the error in it.
+    #[cfg(feature = "mock")]
+    #[tokio::test]
+    async fn edit_dialog_open_type_save_close() {
+        let service = Arc::new(crate::mock::MockService::new());
+        let mut app = App::init_async(service, Flags::default()).await;
+        let id = app.profiles[0].id.clone();
+        assert!(app.edit.is_none());
+
+        // Open: the fetch completes into a draft.
+        drop(app.update(Message::EditServer(id.clone())));
+        let details = app.service.profile_details(&id).await.expect("details");
+        drop(app.update(Message::EditLoaded(id.clone(), Ok(details.clone()))));
+        let draft = app.edit.as_ref().expect("dialog open");
+        assert_eq!(draft.id, id);
+        assert_eq!(draft.tag, details.tag);
+        assert!(
+            draft.is_reality(),
+            "the mock's seed vless profile is REALITY"
+        );
+
+        // Type: rename + retag the port.
+        drop(app.update(Message::EditField(EditField::Tag, "edited".into())));
+        drop(app.update(Message::EditField(EditField::Port, "8443".into())));
+        drop(app.update(Message::EditField(EditField::Sni, "new.sni.example".into())));
+        let draft = app.edit.as_ref().expect("still open");
+        assert_eq!(draft.tag, "edited");
+        assert_eq!(draft.port, "8443");
+        assert_eq!(draft.sni, "new.sni.example");
+
+        // Save: the service applies the edit; the dialog closes and the
+        // notice names the saved server.
+        let res = app
+            .service
+            .update_profile(&id, app.edit.as_ref().unwrap().to_details())
+            .await
+            .expect("save");
+        drop(app.update(Message::EditSaved(Ok(res))));
+        assert!(app.edit.is_none(), "closed after a good save");
+        assert!(app.notice.as_deref().unwrap_or("").contains("edited"));
+
+        // A failing save keeps the dialog open with the error shown.
+        drop(app.update(Message::EditServer(id.clone())));
+        let details = app.service.profile_details(&id).await.expect("details");
+        drop(app.update(Message::EditLoaded(id.clone(), Ok(details))));
+        drop(app.update(Message::EditSaved(Err(
+            "port must be a number 0-65535".into(),
+        ))));
+        let draft = app.edit.as_ref().expect("still open on error");
+        assert_eq!(
+            draft.error.as_deref(),
+            Some("port must be a number 0-65535")
+        );
+        assert!(!draft.saving);
+        drop(app.update(Message::EditCancel));
+        assert!(app.edit.is_none(), "cancel closes");
+    }
+
+    /// S19: the speed-test card state machine — in-flight guard, result
+    /// lands, errors land too.
+    #[cfg(feature = "mock")]
+    #[tokio::test]
+    async fn speed_test_message_flow() {
+        let service = Arc::new(crate::mock::MockService::new());
+        let mut app = App::init_async(service, Flags::default()).await;
+        assert!(app.speed.is_none());
+        drop(app.update(Message::SpeedTest));
+        assert!(app.speed_testing);
+        let res = app.service.speed_test(None).await;
+        drop(app.update(Message::SpeedTested(res)));
+        assert!(!app.speed_testing);
+        let info = app
+            .speed
+            .as_ref()
+            .expect("result present")
+            .as_ref()
+            .expect("ok");
+        assert!(info.mbps > 0.0);
+        // Errors surface in the result and the log buffer.
+        drop(app.update(Message::SpeedTest));
+        drop(app.update(Message::SpeedTested(Err("no data received".into()))));
+        assert!(app.speed.as_ref().expect("error result present").is_err());
+        assert!(app.logs.iter().any(|l| l.contains("speed test")));
+    }
+
+    /// S19: kill switch + diagnostics + speed-URL toggles update the draft
+    /// settings (the save task is runtime-driven; persistence through the
+    /// service is proven directly like the S15/S16 twins) and the view
+    /// modal only exists while a dialog is open.
+    #[cfg(feature = "mock")]
+    #[tokio::test]
+    async fn s19_settings_toggles_and_modal_gating() {
+        let service = Arc::new(crate::mock::MockService::new());
+        let mut app = App::init_async(service, Flags::default()).await;
+        assert!(!app.settings.kill_switch);
+        drop(app.update(Message::KillSwitchToggled(true)));
+        assert!(app.settings.kill_switch);
+        assert!(!app.settings.diagnostics);
+        drop(app.update(Message::DiagnosticsToggled(true)));
+        assert!(app.settings.diagnostics);
+        drop(app.update(Message::SpeedUrlChanged("http://x.example/f".into())));
+        assert_eq!(app.settings.speed_test_url, "http://x.example/f");
+
+        // The modal layer only renders while the draft exists (view()
+        // gates on edit.is_some()).
+        drop(app.view());
+        assert!(app.edit.is_none());
+        let id = app.profiles[0].id.clone();
+        let details = app.service.profile_details(&id).await.expect("details");
+        drop(app.update(Message::EditLoaded(id, Ok(details))));
+        assert!(app.edit.is_some());
+        drop(app.view()); // builds fine with the dialog up (no panic)
+
+        // The mock keeps no diagnostics file: DiagPath(None) keeps the
+        // Settings caption honest.
+        drop(app.update(Message::DiagPath(None)));
+        assert!(app.diag_path.is_none());
+        drop(app.update(Message::DiagPath(Some("/tmp/x.log".into()))));
+        assert_eq!(app.diag_path.as_deref(), Some("/tmp/x.log"));
+    }
 }
diff --git a/crates/mt-app/src/lib.rs b/crates/mt-app/src/lib.rs
index f998ed8..c370121 100755
--- a/crates/mt-app/src/lib.rs
+++ b/crates/mt-app/src/lib.rs
@@ -63,8 +63,8 @@ pub mod tray_win;
 pub mod mock;
 
 pub use app::{
-    run_app, run_smoke, AdapterRow, App, Flags, LogFilter, Message, ParsedPreview, RuleRow, Screen,
-    SmokeReport,
+    run_app, run_smoke, AdapterRow, App, EditDraft, EditField, Flags, LogFilter, Message,
+    ParsedPreview, RuleRow, Screen, SmokeReport,
 };
 pub use theme::MtTheme;
 
diff --git a/crates/mt-app/src/mock.rs b/crates/mt-app/src/mock.rs
index 538b295..54fb875 100755
--- a/crates/mt-app/src/mock.rs
+++ b/crates/mt-app/src/mock.rs
@@ -107,6 +107,9 @@ impl MockService {
             exit_probe_url: "http://ip-api.com/json".to_string(),
             close_to_tray: false,
             always_on_top: false,
+            kill_switch: false,
+            diagnostics: false,
+            speed_test_url: "http://speed.cloudflare.com/__down?bytes=25000000".to_string(),
         };
         let inner = Inner {
             state: ServiceState::Disconnected,
@@ -697,14 +700,146 @@ impl MtService for MockService {
         self.tx.subscribe()
     }
 
+    async fn profile_details(&self, id: &str) -> Result<mt_service::ProfileDetails, String> {
+        let inner = self.inner.lock().await;
+        let p = inner
+            .profiles
+            .iter()
+            .find(|p| p.id == id)
+            .ok_or_else(|| format!("no profile matches {id:?}"))?;
+        let (security, sni, fingerprint, public_key, short_id, spider_x) = match &p.security {
+            mt_links::Security::Reality {
+                sni,
+                fingerprint,
+                public_key,
+                short_id,
+                spider_x,
+            } => (
+                "reality",
+                sni.clone(),
+                fingerprint.clone(),
+                public_key.clone(),
+                short_id.clone(),
+                spider_x.clone().unwrap_or_default(),
+            ),
+            mt_links::Security::Tls { sni, .. } => (
+                "tls",
+                sni.clone().unwrap_or_default(),
+                String::new(),
+                String::new(),
+                String::new(),
+                String::new(),
+            ),
+            mt_links::Security::None => (
+                "none",
+                String::new(),
+                String::new(),
+                String::new(),
+                String::new(),
+                String::new(),
+            ),
+        };
+        Ok(mt_service::ProfileDetails {
+            id: p.id.clone(),
+            tag: p.tag.clone(),
+            remark: p.remark.clone(),
+            protocol: format!("{:?}", p.protocol).to_ascii_lowercase(),
+            address: p.address.clone(),
+            port: p.port.to_string(),
+            identity: p.identity.clone(),
+            security: security.to_string(),
+            sni,
+            fingerprint,
+            public_key,
+            short_id,
+            spider_x,
+        })
+    }
+
+    async fn update_profile(
+        &self,
+        id: &str,
+        edit: mt_service::ProfileDetails,
+    ) -> Result<mt_service::ProfileView, String> {
+        if edit.tag.trim().is_empty() {
+            return Err("name cannot be empty".to_string());
+        }
+        edit.port
+            .trim()
+            .parse::<u16>()
+            .map_err(|_| format!("port must be a number 0-65535 ({:?})", edit.port))?;
+        if edit.identity.trim().is_empty() {
+            return Err("identity (UUID/password) cannot be empty".to_string());
+        }
+        let mut inner = self.inner.lock().await;
+        let active = inner.active.clone();
+        let p = inner
+            .profiles
+            .iter_mut()
+            .find(|p| p.id == id)
+            .ok_or_else(|| format!("no profile matches {id:?}"))?;
+        p.tag = edit.tag.trim().to_string();
+        p.remark = edit.remark.trim().to_string();
+        p.address = edit.address.trim().to_string();
+        p.port = edit.port.trim().parse().expect("validated above");
+        p.identity = edit.identity.trim().to_string();
+        if let mt_links::Security::Reality {
+            sni,
+            fingerprint,
+            public_key,
+            short_id,
+            spider_x,
+        } = &mut p.security
+        {
+            *sni = edit.sni.trim().to_string();
+            *fingerprint = edit.fingerprint.trim().to_string();
+            *public_key = edit.public_key.trim().to_string();
+            *short_id = edit.short_id.trim().to_string();
+            let spider = edit.spider_x.trim().to_string();
+            *spider_x = (!spider.is_empty()).then_some(spider);
+        }
+        let view = view_of(p, &active);
+        drop(inner);
+        self.log("INFO", "profile edited (mock)");
+        Ok(view)
+    }
+
+    async fn speed_test(&self, _id: Option<String>) -> Result<mt_service::SpeedTestInfo, String> {
+        // Simulated throughput (no real network in mock builds).
+        tokio::time::sleep(Duration::from_millis(800)).await;
+        let (mbps, secs, tag, live) = {
+            let mut g = self.inner.lock().await;
+            let mbps = 40.0 + MockService::rng_below(&mut g, 90) as f64;
+            let live = matches!(g.state, ServiceState::Connected);
+            let tag = g
+                .active
+                .as_deref()
+                .and_then(|a| g.profiles.iter().find(|p| p.id == a))
+                .map(|p| p.tag.clone())
+                .unwrap_or_else(|| "mock".to_string());
+            (mbps, 0.8, tag, live)
+        };
+        self.log(
+            "INFO",
+            &format!("speed test finished ({mbps:.0} Mbps simulated)"),
+        );
+        Ok(mt_service::SpeedTestInfo {
+            mbps,
+            mbytes: mbps * secs / 8.0,
+            seconds: secs,
+            profile_tag: tag,
+            via_temp_tunnel: !live,
+        })
+    }
+
     async fn exit_probe(&self) -> Result<ExitInfo, String> {
         // Simulated identity (RFC 5737 documentation range — clearly fake,
         // no real network in mock builds). The exit IS the seed server:
-        // single hop, matching production's happy path.
+        // single hop, matching production's happy path. S19: works while
+        // DISCONNECTED too — via_temp_tunnel marks the simulated
+        // temporary-probe path (production spawns a real one).
         let inner = self.inner.lock().await;
-        if inner.state != ServiceState::Connected {
-            return Err("not connected".to_string());
-        }
+        let live = matches!(inner.state, ServiceState::Connected);
         let endpoint = inner
             .profiles
             .iter()
@@ -719,8 +854,14 @@ impl MtService for MockService {
             asn: "AS64512 Example Telecom".to_string(),
             server_endpoint: endpoint,
             direct_exit: true,
+            via_temp_tunnel: !live,
         })
     }
+
+    async fn diagnostics_path(&self) -> Option<String> {
+        // The mock keeps no files; the Settings screen shows its caption.
+        None
+    }
 }
 
 #[cfg(test)]
diff --git a/crates/mt-app/src/screens.rs b/crates/mt-app/src/screens.rs
index 5e41daf..f967f77 100755
--- a/crates/mt-app/src/screens.rs
+++ b/crates/mt-app/src/screens.rs
@@ -14,8 +14,8 @@ use iced::{Length, Padding};
 use mt_service::{ProfileView, ServiceState, ThemeMode};
 
 use crate::app::{
-    App, LatencyState, LogFilter, Message, RuleRow, Screen, TitleMenuItem, BOND_MODES, LOG_LEVELS,
-    MERGE_POLICIES, RATE_HISTORY_LEN, RULE_ACTIONS, RULE_KINDS,
+    App, EditField, LatencyState, LogFilter, Message, RuleRow, Screen, TitleMenuItem, BOND_MODES,
+    LOG_LEVELS, MERGE_POLICIES, RATE_HISTORY_LEN, RULE_ACTIONS, RULE_KINDS,
 };
 use crate::graph::{RateGraph, RateGraphStyle};
 use crate::theme as t3;
@@ -468,14 +468,20 @@ pub fn status(app: &App) -> Element<'_> {
         .height(Length::Fixed(STATUS_GRAPH_HEIGHT));
     let _ = util::fmt_bps; // rate labels live in graph.rs; kept for chips/tests
 
-    // --- exit probe card (D18) ---------------------------------------------
+    // --- exit probe card (D18; S19 works offline too) ----------------------
+    let has_active = app.profiles.iter().any(|p| p.active);
     let mut exit_col = column![
         text("Exit identity")
             .size(ts.title_m.size)
             .color(s.on_surface),
-        text("checks which IP the internet sees when the tunnel is up")
-            .size(ts.body_s.size)
-            .color(s.on_surface_variant),
+        text(if has_active {
+            "checks which IP the internet sees — connected (rides the live tunnel) or not \
+             (spawns a temporary probe tunnel)"
+        } else {
+            "checks which IP the internet sees — pick an active server first"
+        })
+        .size(ts.body_s.size)
+        .color(s.on_surface_variant),
     ]
     .spacing(6);
     if app.exit_probing {
@@ -487,7 +493,7 @@ pub fn status(app: &App) -> Element<'_> {
     } else {
         exit_col = exit_col.push(m3_button(
             "Check exit IP",
-            connected,
+            has_active,
             ButtonKind::Tonal,
             Message::ExitProbe,
             app,
@@ -534,6 +540,21 @@ pub fn status(app: &App) -> Element<'_> {
                                 s.secondary_container,
                                 s.on_secondary_container
                             ),
+                            if info.via_temp_tunnel {
+                                chip(
+                                    "temporary probe tunnel",
+                                    app,
+                                    s.surface_container_high,
+                                    s.on_surface_variant,
+                                )
+                            } else {
+                                chip(
+                                    "live tunnel",
+                                    app,
+                                    s.tertiary_container,
+                                    s.on_tertiary_container,
+                                )
+                            },
                         ]
                         .spacing(8)
                         .align_y(iced::Alignment::Center),
@@ -558,6 +579,98 @@ pub fn status(app: &App) -> Element<'_> {
         .style(t3::card(s, s.surface_container_low))
         .width(Length::Fill);
 
+    // --- speed test card (S19) ---------------------------------------------
+    let mut speed_col = column![
+        text("Speed test").size(ts.title_m.size).color(s.on_surface),
+        text(if has_active {
+            "downloads a test payload through the active server — the live tunnel when \
+             connected, a temporary probe tunnel otherwise (single carrier; setup is \
+             excluded from the rate)"
+        } else {
+            "downloads a test payload through the active server — pick an active server first"
+        })
+        .size(ts.body_s.size)
+        .color(s.on_surface_variant),
+    ]
+    .spacing(6);
+    if app.speed_testing {
+        speed_col = speed_col.push(
+            text("testing… (up to 30 s)")
+                .size(ts.body_m.size)
+                .color(s.on_surface_variant),
+        );
+    } else {
+        speed_col = speed_col.push(m3_button(
+            "Run speed test",
+            has_active,
+            ButtonKind::Tonal,
+            Message::SpeedTest,
+            app,
+        ));
+    }
+    if let Some(res) = &app.speed {
+        match res {
+            Ok(info) => {
+                speed_col = speed_col
+                    .push(
+                        row![
+                            text(format!("{:.1} Mbps", info.mbps))
+                                .size(ts.headline_s.size)
+                                .color(s.on_surface),
+                            horizontal_space().width(Length::Fill),
+                            chip(
+                                format!("{:.1} MB in {:.1} s", info.mbytes, info.seconds),
+                                app,
+                                s.secondary_container,
+                                s.on_secondary_container
+                            ),
+                            chip(
+                                util::short(&info.profile_tag, 24),
+                                app,
+                                s.surface_container_high,
+                                s.on_surface_variant
+                            ),
+                            if info.via_temp_tunnel {
+                                chip(
+                                    "temporary probe tunnel",
+                                    app,
+                                    s.surface_container_high,
+                                    s.on_surface_variant,
+                                )
+                            } else {
+                                chip(
+                                    "live tunnel",
+                                    app,
+                                    s.tertiary_container,
+                                    s.on_tertiary_container,
+                                )
+                            },
+                        ]
+                        .spacing(8)
+                        .align_y(iced::Alignment::Center),
+                    )
+                    .push(
+                        text(
+                            "measures the DOWNLOAD path through this server; the engine's \
+                             per-adapter graphs show the live spread while connected",
+                        )
+                        .size(ts.label_m.size)
+                        .color(s.on_surface_variant),
+                    );
+            }
+            Err(e) => {
+                speed_col = speed_col.push(
+                    text(format!("speed test failed: {e}"))
+                        .size(ts.body_s.size)
+                        .color(s.error),
+                );
+            }
+        }
+    }
+    let speed_card = container(speed_col.spacing(8).padding(16))
+        .style(t3::card(s, s.surface_container_low))
+        .width(Length::Fill);
+
     // --- links (scrollable) -------------------------------------------------
     let mut links_col = column![].spacing(8);
     match snap.as_ref().map(|x| x.links.as_slice()) {
@@ -690,6 +803,7 @@ pub fn status(app: &App) -> Element<'_> {
     // --- ONE page scrollable below the frame (S13) ---------------------
     let mut below = column![].spacing(12).width(Length::Fill);
     below = below.push(exit_card);
+    below = below.push(speed_card);
     below = below.push(bottom_card);
     let page_scroll = scrollable(container(below).width(Length::Fill))
         .width(Length::Fill)
@@ -1017,6 +1131,13 @@ fn profile_card<'a>(app: &'a App, p: &'a ProfileView) -> Element<'a> {
             Message::SetActive(p.id.clone()),
             app
         ),
+        m3_button(
+            "Edit",
+            true,
+            ButtonKind::Text,
+            Message::EditServer(p.id.clone()),
+            app
+        ),
         m3_button(
             "Test latency",
             true,
@@ -1058,6 +1179,209 @@ fn profile_card<'a>(app: &'a App, p: &'a ProfileView) -> Element<'a> {
         .into()
 }
 
+// ---------------------------------------------------------------------------
+// S19: server Edit dialog (full-window modal)
+// ---------------------------------------------------------------------------
+
+/// One `[label | text field]` row of the edit form.
+fn edit_field_row<'a>(
+    app: &'a App,
+    label: &'a str,
+    value: &'a str,
+    field: EditField,
+    placeholder: &'a str,
+    secret: bool,
+) -> iced::widget::Row<'a, Message> {
+    let s = app.theme.scheme;
+    let ts = app.theme.ts;
+    let mut input = text_input(placeholder, value)
+        .on_input(move |t| Message::EditField(field, t))
+        .padding(6)
+        .width(Length::Fixed(260.0))
+        .style(t3::text_field(s));
+    if secret {
+        input = input.on_submit(Message::EditSave).size(ts.body_s.size); // credentials read a touch smaller
+    } else {
+        input = input.on_submit(Message::EditSave);
+    }
+    row![
+        text(label)
+            .size(app.theme.ts.body_m.size)
+            .color(s.on_surface_variant)
+            .width(Length::Fixed(150.0)),
+        input,
+    ]
+    .spacing(10)
+    .align_y(iced::Alignment::Center)
+}
+
+/// S19: the server Edit dialog — a full-window modal layer (the S16
+/// title-menu overlay discipline): a dim backdrop that cancels on click,
+/// with a centered card carrying the editable fields. REALITY-only fields
+/// render only for REALITY profiles (the service ignores them otherwise).
+pub fn edit_overlay(app: &App) -> Element<'_> {
+    let s = app.theme.scheme;
+    let ts = app.theme.ts;
+    let Some(d) = &app.edit else {
+        // Not open (the caller gates on edit.is_some(); render nothing
+        // clickable either way).
+        return container(vertical_space().height(Length::Shrink))
+            .width(Length::Fill)
+            .height(Length::Fill)
+            .into();
+    };
+
+    let mut form = column![
+        text(format!("Edit server — {} · {}", d.protocol, d.security))
+            .size(ts.title_m.size)
+            .color(s.on_surface),
+        edit_field_row(app, "name", &d.tag, EditField::Tag, "server name", false),
+        edit_field_row(
+            app,
+            "remark",
+            &d.remark,
+            EditField::Remark,
+            "free-form note",
+            false
+        ),
+        edit_field_row(
+            app,
+            "address",
+            &d.address,
+            EditField::Address,
+            "host or IP",
+            false
+        ),
+        edit_field_row(app, "port", &d.port, EditField::Port, "1-65535", false),
+        edit_field_row(
+            app,
+            "identity (UUID)",
+            &d.identity,
+            EditField::Identity,
+            "vless UUID",
+            true
+        ),
+    ]
+    .spacing(10);
+
+    if d.is_reality() {
+        form = form.push(
+            text("REALITY security (vless+reality+tcp)")
+                .size(ts.label_l.size)
+                .color(s.on_surface_variant),
+        );
+        form = form.push(edit_field_row(
+            app,
+            "SNI",
+            &d.sni,
+            EditField::Sni,
+            "server name indication",
+            false,
+        ));
+        form = form.push(edit_field_row(
+            app,
+            "fingerprint",
+            &d.fingerprint,
+            EditField::Fingerprint,
+            "uTLS fingerprint (chrome…)",
+            false,
+        ));
+        form = form.push(edit_field_row(
+            app,
+            "public key",
+            &d.public_key,
+            EditField::PublicKey,
+            "pbk",
+            true,
+        ));
+        form = form.push(edit_field_row(
+            app,
+            "short ID",
+            &d.short_id,
+            EditField::ShortId,
+            "sid",
+            false,
+        ));
+        form = form.push(edit_field_row(
+            app,
+            "spiderX",
+            &d.spider_x,
+            EditField::SpiderX,
+            "empty = /",
+            false,
+        ));
+    } else {
+        form = form.push(
+            text("security fields apply to REALITY profiles only")
+                .size(ts.label_m.size)
+                .color(s.on_surface_variant),
+        );
+    }
+
+    if let Some(err) = &d.error {
+        form = form.push(
+            container(
+                text(format!("save failed: {err}"))
+                    .size(ts.body_s.size)
+                    .color(s.on_error_container),
+            )
+            .padding(10)
+            .style(t3::card(s, s.error_container))
+            .width(Length::Fill),
+        );
+    }
+
+    form = form.push(
+        row![
+            m3_button(
+                if d.saving { "saving…" } else { "Save" },
+                !d.saving,
+                ButtonKind::Filled,
+                Message::EditSave,
+                app
+            ),
+            m3_button("Cancel", true, ButtonKind::Text, Message::EditCancel, app),
+            text("editing the ACTIVE server applies on the next reconnect")
+                .size(ts.label_m.size)
+                .color(s.on_surface_variant),
+        ]
+        .spacing(8)
+        .align_y(iced::Alignment::Center),
+    );
+
+    // Scrollable so the form never overflows short windows.
+    let card = container(
+        scrollable(column![form].spacing(4).width(Length::Fill))
+            .height(Length::Fill)
+            .width(Length::Fill),
+    )
+    .padding(20)
+    .style(t3::card(s, s.surface_container_low))
+    .width(Length::Fixed(520.0))
+    .height(Length::Fill)
+    .max_height(560.0);
+
+    // Dim backdrop: click cancels; the card centers.
+    let backdrop = mouse_area(
+        container(card)
+            .width(Length::Fill)
+            .height(Length::Fill)
+            .align_x(iced::alignment::Horizontal::Center)
+            .align_y(iced::alignment::Vertical::Center)
+            .padding(24)
+            .style(move |_t| iced::widget::container::Style {
+                background: Some(iced::Background::Color(iced::Color::from_rgba(
+                    0.0, 0.0, 0.0, 0.45,
+                ))),
+                ..Default::default()
+            }),
+    )
+    .on_press(Message::EditCancel)
+    .interaction(iced::mouse::Interaction::Pointer);
+
+    backdrop.into()
+}
+
 // ---------------------------------------------------------------------------
 // Routing
 // ---------------------------------------------------------------------------
@@ -1659,6 +1983,22 @@ pub fn settings(app: &App) -> Element<'_> {
             .size(28.0)
             .on_toggle(Message::SystemProxyToggled)
             .style(t3::switch(s)),
+        toggler(app.settings.kill_switch)
+            .label("kill switch (block traffic if the tunnel dies)")
+            .size(28.0)
+            .on_toggle(Message::KillSwitchToggled)
+            .style(t3::switch(s)),
+        text(
+            "on: when the tunnel dies on its own, the system proxy is pointed at a \
+             dead local port — apps FAIL instead of silently leaking outside the \
+             tunnel (fail-closed). Connect again to resume, or toggle this off to \
+             restore direct traffic. A MANUAL disconnect always restores. Guards \
+             system-proxy traffic; apps using the SOCKS/HTTP port already fail \
+             closed while the tunnel is down. off (default): the proxy is \
+             restored on a spontaneous death and traffic goes direct (fail-open).",
+        )
+        .size(ts.body_s.size)
+        .color(s.on_surface_variant),
         row![
             text("merge policy")
                 .size(ts.body_m.size)
@@ -1802,6 +2142,19 @@ pub fn settings(app: &App) -> Element<'_> {
         ]
         .spacing(10)
         .align_y(iced::Alignment::Center),
+        row![
+            text("speed test URL")
+                .size(ts.body_m.size)
+                .color(s.on_surface_variant)
+                .width(Length::Fixed(210.0)),
+            text_input("http://host/path", &app.settings.speed_test_url)
+                .on_input(Message::SpeedUrlChanged)
+                .padding(6)
+                .width(Length::Fixed(240.0))
+                .style(t3::text_field(s)),
+        ]
+        .spacing(10)
+        .align_y(iced::Alignment::Center),
         text("stream mode splits every connection's bytes across the bonded adapters and merges them on the server (needs the mt-merge-server daemon and an Xray finalRules allow for the merge port on the server — re-run server/setup-xray-reality.sh; falls back to per-connection when unreachable)")
             .size(ts.body_s.size)
             .color(s.on_surface_variant),
@@ -1853,6 +2206,44 @@ pub fn settings(app: &App) -> Element<'_> {
     ]
     .spacing(10);
 
+    // Diagnostics card (S19) ------------------------------------------------
+    let diag_caption = match &app.diag_path {
+        Some(p) => format!("currently writing: {p}"),
+        None => "the file is created when the mode turns on (its path also lands \
+                 in the Logs tab)"
+            .to_string(),
+    };
+    let diagnostics_card = column![
+        text("Diagnostics")
+            .size(ts.title_m.size)
+            .color(s.on_surface),
+        toggler(app.settings.diagnostics)
+            .label("diagnostics mode (verbose support log)")
+            .size(28.0)
+            .on_toggle(Message::DiagnosticsToggled)
+            .style(t3::switch(s)),
+        text(
+            "on: one timestamped file per session in the log folder collects \
+             connection lifecycle events, engine output (log level forced to \
+             debug), link health and a redacted settings/profile summary — \
+             paste it into a chat for connection-issue analysis. off: the file \
+             stops growing but stays on disk.",
+        )
+        .size(ts.body_s.size)
+        .color(s.on_surface_variant),
+        text(diag_caption)
+            .size(ts.body_s.size)
+            .color(s.on_surface_variant),
+        m3_button(
+            "Open log folder",
+            true,
+            ButtonKind::Outlined,
+            Message::OpenLogsFolder,
+            app
+        ),
+    ]
+    .spacing(10);
+
     let mut col = column![
         container(appearance)
             .padding(16)
@@ -1874,6 +2265,10 @@ pub fn settings(app: &App) -> Element<'_> {
             .padding(16)
             .style(t3::card(s, s.surface_container_low))
             .width(Length::Fill),
+        container(diagnostics_card)
+            .padding(16)
+            .style(t3::card(s, s.surface_container_low))
+            .width(Length::Fill),
     ]
     .spacing(12);
 
diff --git a/crates/mt-app/src/theme.rs b/crates/mt-app/src/theme.rs
index 34468bf..e3c33ec 100755
--- a/crates/mt-app/src/theme.rs
+++ b/crates/mt-app/src/theme.rs
@@ -690,6 +690,9 @@ mod tests {
             exit_probe_url: String::new(),
             close_to_tray: false,
             always_on_top: false,
+            kill_switch: false,
+            diagnostics: false,
+            speed_test_url: String::new(),
         });
         assert_eq!(t.palette.seed(), 0x4285F4);
         assert!(!t.dark);
diff --git a/crates/mt-links/src/store.rs b/crates/mt-links/src/store.rs
index 0f089fa..680ab8d 100755
--- a/crates/mt-links/src/store.rs
+++ b/crates/mt-links/src/store.rs
@@ -6,6 +6,10 @@
 //!   shape (`version` / `active` / `profiles`);
 //! - `upsert` dedupes by `(protocol, address, port, identity)` and keeps the
 //!   EXISTING id so references stay stable; returns the effective id;
+//! - `update` (S19) replaces one profile BY ID in place — the id, the
+//!   list position and the `active` pointer are all preserved, and NO
+//!   dedupe runs (an explicit edit may legitimately collide with another
+//!   entry; merging is the user's call, not ours);
 //! - `remove` clears `active` when it pointed at the removed id;
 //! - `set_active(Some(id))` is rejected (false, no change) for unknown ids,
 //!   `set_active(None)` always succeeds.
@@ -100,6 +104,22 @@ impl ProfileStore {
         removed
     }
 
+    /// S19: replace one profile by id, IN PLACE (same list position, same
+    /// id, `active` untouched). The replacement's `id` field is ignored —
+    /// the stored id wins, exactly like `upsert` keeps the existing id on
+    /// a metadata refresh. Returns false (no change) for an unknown id.
+    /// No dedupe: an explicit edit that collides with another profile's
+    /// `(protocol, address, port, identity)` is the user's choice; a later
+    /// re-import of the same link still merges via `upsert`.
+    pub fn update(&mut self, id: &str, mut replacement: Profile) -> bool {
+        let Some(slot) = self.profiles.iter_mut().find(|p| p.id == id) else {
+            return false;
+        };
+        replacement.id = id.to_string();
+        *slot = replacement;
+        true
+    }
+
     /// Set the active profile by id (or clear it). An unknown id is
     /// rejected: returns false and changes nothing.
     pub fn set_active(&mut self, id: Option<&str>) -> bool {
@@ -334,6 +354,73 @@ mod tests {
         assert_eq!(store.list().len(), 1);
     }
 
+    #[test]
+    fn update_replaces_in_place_keeping_id_position_and_active() {
+        let mut store = ProfileStore::default();
+        let id_a = store.upsert(profile(
+            "id-a",
+            "tag-a",
+            "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
+        ));
+        store.upsert(profile(
+            "id-b",
+            "tag-b",
+            "11111111-2222-3333-4444-555555555555",
+        ));
+        assert!(store.set_active(Some(&id_a)), "set active");
+
+        // Edit A: new address + tag; a FRESH uuid in the id field must be
+        // ignored (the stored id wins).
+        let mut edited = profile(
+            "fresh-uuid",
+            "edited-a",
+            "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
+        );
+        edited.address = "198.51.100.7".to_string();
+        edited.port = 8443;
+        assert!(store.update(&id_a, edited));
+
+        assert_eq!(store.list().len(), 2, "no append on edit");
+        assert_eq!(store.list()[0].id, id_a, "id preserved");
+        assert_eq!(store.list()[0].tag, "edited-a");
+        assert_eq!(store.list()[0].address, "198.51.100.7");
+        assert_eq!(store.list()[0].port, 8443);
+        assert_eq!(store.active.as_deref(), Some(id_a.as_str()), "active kept");
+
+        // Unknown id: rejected, nothing changes.
+        assert!(!store.update("no-such-id", profile("x", "x", "x")));
+        assert_eq!(store.list().len(), 2);
+    }
+
+    #[test]
+    fn update_colliding_with_another_profile_is_kept() {
+        // An explicit edit may duplicate another entry's dedupe key — the
+        // store keeps BOTH (merging is the user's call, not ours).
+        let mut store = ProfileStore::default();
+        let id_a = store.upsert(profile(
+            "id-a",
+            "tag-a",
+            "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
+        ));
+        store.upsert(profile(
+            "id-b",
+            "tag-b",
+            "11111111-2222-3333-4444-555555555555",
+        ));
+        let mut edited = profile(
+            "id-a",
+            "tag-a-clone",
+            "11111111-2222-3333-4444-555555555555",
+        );
+        edited.address = "203.0.113.10".to_string();
+        assert!(store.update(&id_a, edited));
+        assert_eq!(store.list().len(), 2, "no silent merge on explicit edit");
+        assert_eq!(
+            store.list()[0].identity,
+            "11111111-2222-3333-4444-555555555555"
+        );
+    }
+
     #[test]
     fn set_active_unknown_id_is_rejected_without_change() {
         let mut store = ProfileStore::default();
diff --git a/crates/mt-service/src/lib.rs b/crates/mt-service/src/lib.rs
index 24c343d..321af51 100755
--- a/crates/mt-service/src/lib.rs
+++ b/crates/mt-service/src/lib.rs
@@ -153,6 +153,25 @@ pub struct SettingsView {
     /// files → false (same D8 version-free migration).
     #[serde(default)]
     pub always_on_top: bool,
+    /// S19: fail-closed on spontaneous tunnel death — when the tunnel
+    /// dies on its own while the app owns the system proxy, the proxy is
+    /// repointed at a dead local port (traffic FAILS) instead of being
+    /// restored (traffic would silently go direct). A manual disconnect
+    /// always restores. Absent in files written by v0.12 and older →
+    /// false (D8 version-free migration by serde default).
+    #[serde(default)]
+    pub kill_switch: bool,
+    /// S19: diagnostics mode — while on, the service writes one
+    /// self-contained log file (connection lifecycle, engine output,
+    /// link health; credentials redacted) the user can paste into a
+    /// support chat. Absent → false (D8).
+    #[serde(default)]
+    pub diagnostics: bool,
+    /// S19: speed-test download URL (Status screen). Plain `http://` by
+    /// design — the client→server leg rides the REALITY tunnel, so the
+    /// payload needs no TLS of its own (D18's zero-TLS rule).
+    #[serde(default = "default_speed_test_url")]
+    pub speed_test_url: String,
 }
 
 /// S18: how the app resolves light vs dark appearance. Serialized as
@@ -239,6 +258,9 @@ fn default_probe_target() -> String {
 fn default_exit_probe_url() -> String {
     "http://ip-api.com/json".to_string()
 }
+fn default_speed_test_url() -> String {
+    "http://speed.cloudflare.com/__down?bytes=25000000".to_string()
+}
 
 /// Live events pushed to the GUI.
 #[derive(Debug, Clone, Serialize, Deserialize)]
@@ -266,6 +288,22 @@ pub trait MtService: Send + Sync {
     async fn set_active_profile(&self, id: &str) -> Result<(), String>;
     async fn test_latency(&self, id: &str) -> Result<u32, String>;
 
+    /// S19: the full editable material of one stored profile (the Edit
+    /// dialog's prefill). Errors are user-facing.
+    async fn profile_details(&self, id: &str) -> Result<ProfileDetails, String>;
+
+    /// S19: apply edits to a stored profile. The id and the active
+    /// selection are preserved; a live session keeps running on the old
+    /// links until the next reconnect (the service logs a reminder when
+    /// the edited profile is the active one).
+    async fn update_profile(&self, id: &str, edit: ProfileDetails) -> Result<ProfileView, String>;
+
+    /// S19: download a speed-test payload through the tunnel and report
+    /// throughput — through the LIVE session when the target is the
+    /// active profile, through a temporary probe tunnel otherwise.
+    /// `None` targets the active profile.
+    async fn speed_test(&self, id: Option<String>) -> Result<SpeedTestInfo, String>;
+
     async fn routing_rules(&self) -> Vec<RuleView>;
     async fn set_routing_rules(&self, rules: Vec<RuleView>) -> Result<(), String>;
 
@@ -286,8 +324,15 @@ pub trait MtService: Send + Sync {
     fn events(&self) -> broadcast::Receiver<ServiceEvent>;
 
     /// Probe the tunnel's EXIT identity (Status screen, D18): rides the
-    /// LIVE session, so it errors ("not connected") when disconnected.
+    /// LIVE session when one is up; S19 falls back to a temporary probe
+    /// tunnel through the active profile when the app is disconnected
+    /// (same exit identity, no session left behind).
     async fn exit_probe(&self) -> Result<ExitInfo, String>;
+
+    /// S19: the diagnostics log file currently being written (Settings
+    /// screen), if diagnostics mode is on. The file stays on disk after
+    /// the mode is turned off.
+    async fn diagnostics_path(&self) -> Option<String>;
 }
 
 /// Result of the Status-screen exit probe (D18): which IP the tunnel
@@ -310,6 +355,65 @@ pub struct ExitInfo {
     /// hop); false means an extra egress hop exists (e.g. a VPN on the
     /// server VM's default route).
     pub direct_exit: bool,
+    /// S19: true when the probe rode a TEMPORARY tunnel spawned for the
+    /// check (the app was disconnected); false when it rode the live
+    /// session. Same exit identity either way — the flag only tells the
+    /// GUI which caption to show.
+    #[serde(default)]
+    pub via_temp_tunnel: bool,
+}
+
+/// S19: the full editable material of one server profile (Servers screen
+/// → Edit dialog). `profile_details` produces the prefill; `update_profile`
+/// consumes the edited copy. Carries credential fields by necessity — it
+/// exists only for the one GUI edit round-trip and is never logged.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct ProfileDetails {
+    pub id: String,
+    /// Display name (derived from the link on import; freely editable).
+    pub tag: String,
+    /// Free-form note shown under the name ("" = hidden).
+    pub remark: String,
+    /// Protocol label — "vless" | "vmess" | "trojan" | "ss" (read-only:
+    /// re-protocoling is a re-import, not an edit).
+    pub protocol: String,
+    /// Server host (domain or IP literal).
+    pub address: String,
+    /// Server port (decimal string — the dialog is a text field).
+    pub port: String,
+    /// UUID (vless/vmess) or password (trojan/ss).
+    pub identity: String,
+    /// Security label — "reality" | "tls" | "none" (read-only).
+    pub security: String,
+    /// REALITY SNI ("" for non-reality profiles).
+    pub sni: String,
+    /// REALITY uTLS fingerprint, e.g. "chrome" ("" for non-reality).
+    pub fingerprint: String,
+    /// REALITY public key ("" for non-reality).
+    pub public_key: String,
+    /// REALITY short ID ("" for non-reality).
+    pub short_id: String,
+    /// REALITY spiderX path ("" = default "/" on save).
+    pub spider_x: String,
+}
+
+/// S19: one speed-test run's outcome (Status screen → Speed test card).
+#[derive(Debug, Clone, PartialEq)]
+pub struct SpeedTestInfo {
+    /// Measured download throughput, megabits per second.
+    pub mbps: f64,
+    /// Megabytes actually received (1 MiB units, one decimal of
+    /// precision in the GUI).
+    pub mbytes: f64,
+    /// Wall-clock seconds of the measured transfer (excludes tunnel
+    /// setup — the clock starts once the stream is connected).
+    pub seconds: f64,
+    /// The profile that was measured (tag).
+    pub profile_tag: String,
+    /// true = measured through a temporary probe tunnel (the app was
+    /// disconnected / another profile was picked); false = through the
+    /// live session.
+    pub via_temp_tunnel: bool,
 }
 
 // ---------------------------------------------------------------------------
@@ -778,6 +882,9 @@ mod close_to_tray_migration_tests {
                 exit_probe_url: String::new(),
                 close_to_tray: false,
                 always_on_top: false,
+                kill_switch: false,
+                diagnostics: false,
+                speed_test_url: default_speed_test_url(),
             }
         }
     }
@@ -875,3 +982,51 @@ mod theme_mode_migration_tests {
         assert_eq!(ThemeMode::parse("", true), ThemeMode::Dark);
     }
 }
+
+#[cfg(test)]
+mod s19_migration_tests {
+    use super::*;
+
+    /// S19: settings.json written by v0.12 and older (no `kill_switch`,
+    /// `diagnostics` or `speed_test_url` keys) loads with the safe
+    /// defaults — kill switch off, diagnostics off, the stock speed-test
+    /// URL (D8 version-free migration via serde defaults).
+    #[test]
+    fn old_settings_without_s19_keys_load_with_defaults() {
+        let json = r#"{
+            "theme_seed": 42,
+            "dark_mode": false,
+            "system_proxy": true,
+            "merge_policy": "balanced",
+            "autostart": false,
+            "close_to_tray": true,
+            "always_on_top": false,
+            "log_level": "info",
+            "probe_target": "connectivitycheck.gstatic.com:80",
+            "exit_probe_url": "http://ip-api.com/json"
+        }"#;
+        let v: SettingsView = serde_json::from_str(json).expect("old settings load");
+        assert!(!v.kill_switch);
+        assert!(!v.diagnostics);
+        assert_eq!(v.speed_test_url, default_speed_test_url());
+        assert!(v.close_to_tray, "S15 field intact");
+        assert!(!v.always_on_top, "S16 field intact");
+    }
+
+    /// The S19 fields round-trip through JSON verbatim (an explicit
+    /// non-default choice persists).
+    #[test]
+    fn s19_fields_roundtrip() {
+        let mut v = SettingsView::default_settings_for_tests();
+        v.kill_switch = true;
+        v.diagnostics = true;
+        v.speed_test_url = "http://127.0.0.1:9/down".to_string();
+        let json = serde_json::to_string(&v).expect("serialize");
+        assert!(json.contains("\"kill_switch\":true"));
+        assert!(json.contains("\"diagnostics\":true"));
+        let back: SettingsView = serde_json::from_str(&json).expect("reload");
+        assert!(back.kill_switch);
+        assert!(back.diagnostics);
+        assert_eq!(back.speed_test_url, "http://127.0.0.1:9/down");
+    }
+}
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index 5526006..829d397 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "7203362 2026-11-21T08:27:44Z",
+  "as_of": "4888fec 2026-11-21T08:27:47Z",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 264,
-  "edge_count": 273,
+  "node_count": 265,
+  "edge_count": 274,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -575,6 +575,26 @@
         }
       ]
     },
+    {
+      "id": "commit:4888fec",
+      "type": "commit",
+      "title": "backup(s18-memory-node): 2026-11-21T08:27:47Z",
+      "tags": [
+        "2026-11-21"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "4888fec"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s18-memory-node): 2026-11-21T08:27:47Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:48a06a0",
       "type": "commit",
@@ -7691,6 +7711,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s2)"
     },
+    {
+      "src": "commit:4888fec",
+      "dst": "ledger:s18",
+      "rel": "part_of",
+      "evidence": "subject stage label (s18)"
+    },
     {
       "src": "commit:48a06a0",
       "dst": "ledger:s1",
diff --git a/src/diagnostics.rs b/src/diagnostics.rs
new file mode 100644
index 0000000..599a3f7
--- /dev/null
+++ b/src/diagnostics.rs
@@ -0,0 +1,270 @@
+//! S19: diagnostics mode — one self-contained, paste-ready log file.
+//!
+//! While diagnostics mode is ON the service appends EVERYTHING a support
+//! conversation needs to a single file under the log dir:
+//!
+//! - a redacted header (version, OS, settings summary, profile endpoints —
+//!   NEVER a UUID/password/public key value);
+//! - every GUI-visible log line (`ServiceEvent::LogLine` tee — connection
+//!   lifecycle, carrier verdicts, sysproxy operations);
+//! - every `tracing` event the process emits (engine child stdout/stderr
+//!   forwarded by mt-engine, supervisor decisions, warnings) via a
+//!   hand-rolled process-global [`tracing::Subscriber`] — no
+//!   `tracing-subscriber` dependency (zero-dependency rule; the e2e rig's
+//!   `PrintSubscriber` is the pattern).
+//!
+//! Design notes:
+//! - The sink is a PROCESS-GLOBAL static: the tracing subscriber can only
+//!   be installed once per process, so the file handle it writes to must be
+//!   reachable from it. `RealService` instances (and tests) share it.
+//! - Toggling the mode off just clears the handle — the file stays on disk
+//!   for the user to paste; a later re-enable starts a NEW timestamped
+//!   file (fresh context per support conversation).
+//! - Writes are line-buffered by construction (`File` is unbuffered; one
+//!   `writeln!` per event) so a crash never eats the tail of the log.
+//! - `enable`/`disable`/`line` are cheap and never panic: a diagnostics
+//!   sink failure must degrade to "no diagnostics", never to a dead app
+//!   (the GUI-never-dies-invisibly rule, D15).
+
+use std::fs::{File, OpenOptions};
+use std::io::Write as _;
+use std::path::{Path, PathBuf};
+use std::sync::atomic::{AtomicBool, Ordering};
+use std::sync::{LazyLock, Mutex, OnceLock};
+use std::time::{SystemTime, UNIX_EPOCH};
+
+/// The one sink the whole process (service + subscriber) writes through.
+static SINK: LazyLock<Mutex<Option<Sink>>> = LazyLock::new(|| Mutex::new(None));
+/// The global subscriber is installed at most once per process.
+static SUBSCRIBER: OnceLock<()> = OnceLock::new();
+/// Fast-path check: events skip formatting entirely when off.
+static ENABLED: AtomicBool = AtomicBool::new(false);
+/// Crate-visible lock for tests that flip the (process-global) sink: the
+/// crate's tests run in parallel inside one binary, so every test that
+/// enables/disables diagnostics holds this to keep windows from
+/// interleaving (see the real_service S19 tests too). Tokio's mutex —
+/// guards may be held across `.await` (the clippy rule); LazyLock because
+/// its `new` is not const.
+#[cfg(test)]
+pub(crate) static TEST_LOCK: LazyLock<tokio::sync::Mutex<()>> =
+    LazyLock::new(|| tokio::sync::Mutex::new(()));
+
+struct Sink {
+    path: PathBuf,
+    file: File,
+}
+
+/// Turn diagnostics ON: create `<log_dir>/diagnostics-<timestamp>.log`,
+/// write the caller-prepared redacted header, install the process-global
+/// subscriber (first enable only). Any previous sink is closed first
+/// (toggles are idempotent-ish: re-enable = fresh file).
+/// Returns the file path on success.
+pub(crate) fn enable(log_dir: &Path, header: &str) -> Result<PathBuf, String> {
+    install_subscriber();
+    std::fs::create_dir_all(log_dir).map_err(|e| format!("create {}: {e}", log_dir.display()))?;
+    let path = log_dir.join(format!("diagnostics-{}.log", timestamp_for_filename()));
+    let mut file = OpenOptions::new()
+        .create(true)
+        .append(true)
+        .open(&path)
+        .map_err(|e| format!("open {}: {e}", path.display()))?;
+    let _ = writeln!(file, "{header}");
+    let _ = writeln!(file, "{} [DIAG] diagnostics mode started", fmt_hms(now()));
+    *SINK.lock().unwrap_or_else(|p| p.into_inner()) = Some(Sink {
+        path: path.clone(),
+        file,
+    });
+    ENABLED.store(true, Ordering::Release);
+    Ok(path)
+}
+
+/// Turn diagnostics OFF: close the sink, keep the file. Returns the path
+/// of the file that was being written (None when the mode was already off).
+pub(crate) fn disable() -> Option<PathBuf> {
+    ENABLED.store(false, Ordering::Release);
+    let mut sink = SINK.lock().unwrap_or_else(|p| p.into_inner()).take()?;
+    let _ = writeln!(
+        sink.file,
+        "{} [DIAG] diagnostics mode stopped",
+        fmt_hms(now())
+    );
+    Some(sink.path)
+}
+
+/// The file currently being written, if the mode is on.
+pub(crate) fn path() -> Option<PathBuf> {
+    SINK.lock()
+        .unwrap_or_else(|p| p.into_inner())
+        .as_ref()
+        .map(|s| s.path.clone())
+}
+
+/// Is the mode on right now? (The engine log level override asks this.)
+pub(crate) fn is_on() -> bool {
+    ENABLED.load(Ordering::Acquire)
+}
+
+/// Append one service-side line (the GUI `LogLine` tee; `kind` tags the
+/// source, e.g. "GUI"). Best-effort — a write failure just disables the
+/// sink (a broken log file must never break the app).
+pub(crate) fn line(kind: &str, msg: &str) {
+    let mut guard = SINK.lock().unwrap_or_else(|p| p.into_inner());
+    let Some(sink) = guard.as_mut() else {
+        return;
+    };
+    let wrote = writeln!(sink.file, "{} [{kind}] {msg}", fmt_hms(now()));
+    if wrote.is_err() {
+        *guard = None;
+        ENABLED.store(false, Ordering::Release);
+    }
+}
+
+fn install_subscriber() {
+    let _ = SUBSCRIBER.set(());
+    // set_global_default fails when another subscriber exists (e.g. a test
+    // rig's) — then tracing output simply stays wherever it was going and
+    // only the LogLine tee reaches the file. Acceptable, never fatal.
+    let _ = tracing::subscriber::set_global_default(DiagSubscriber);
+}
+
+/// Hand-rolled subscriber (the e2e `PrintSubscriber` pattern): formats
+/// every event as `<time> [LEVEL] <target> <fields>` into the sink.
+struct DiagSubscriber;
+
+impl tracing::Subscriber for DiagSubscriber {
+    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
+        // The cheap path: everything is offered to `event`, which checks
+        // the fast-path flag itself (mirrors the e2e PrintSubscriber
+        // discipline — no filtering logic duplicated here).
+        true
+    }
+    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
+        tracing::span::Id::from_u64(1)
+    }
+    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
+    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
+    fn event(&self, event: &tracing::Event<'_>) {
+        if !ENABLED.load(Ordering::Acquire) {
+            return;
+        }
+        struct V(String);
+        impl tracing::field::Visit for V {
+            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
+                use std::fmt::Write as _;
+                let _ = write!(self.0, " {}={:?}", field.name(), value);
+            }
+        }
+        let mut v = V(String::new());
+        event.record(&mut v);
+        line(
+            &format!("LEVEL:{}", event.metadata().level()),
+            &format!("[{}]{}", event.metadata().target(), v.0),
+        );
+    }
+    fn enter(&self, _span: &tracing::span::Id) {}
+    fn exit(&self, _span: &tracing::span::Id) {}
+}
+
+fn now() -> SystemTime {
+    SystemTime::now()
+}
+
+/// `HH:MM:SS.mmm` UTC (the file's own header carries the full date).
+fn fmt_hms(t: SystemTime) -> String {
+    let ms = t
+        .duration_since(UNIX_EPOCH)
+        .map(|d| d.as_millis())
+        .unwrap_or(0);
+    let secs = ms / 1000;
+    let ms_part = ms % 1000;
+    format!(
+        "{:02}:{:02}:{:02}.{:03}",
+        (secs / 3600) % 24,
+        (secs / 60) % 60,
+        secs % 60,
+        ms_part
+    )
+}
+
+/// `YYYYMMDD-HHMMSS` UTC for the filename (sortable, no spaces).
+fn timestamp_for_filename() -> String {
+    let secs = now()
+        .duration_since(UNIX_EPOCH)
+        .map(|d| d.as_secs())
+        .unwrap_or(0);
+    let days = secs.div_euclid(86_400) as i64;
+    let rem = secs.rem_euclid(86_400);
+    // Howard Hinnant's civil_from_days (the fmt_utc sibling in
+    // real_service; duplicated here to keep the module self-contained).
+    let z = days + 719_468;
+    let era = z.div_euclid(146_097);
+    let doe = z.rem_euclid(146_097);
+    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
+    let y = yoe + era * 400;
+    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
+    let mp = (5 * doy + 2) / 153;
+    let d = doy - (153 * mp + 2) / 5 + 1;
+    let m = if mp < 10 { mp + 3 } else { mp - 9 };
+    let y = if m <= 2 { y + 1 } else { y };
+    format!(
+        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
+        rem / 3600,
+        (rem / 60) % 60,
+        rem % 60
+    )
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    fn scratch(tag: &str) -> PathBuf {
+        let d = std::env::temp_dir().join(format!(
+            "mt-diag-{}-{}-{}",
+            tag,
+            std::process::id(),
+            SystemTime::now()
+                .duration_since(UNIX_EPOCH)
+                .map(|d| d.as_nanos())
+                .unwrap_or(0)
+        ));
+        std::fs::create_dir_all(&d).expect("scratch dir");
+        d
+    }
+
+    #[tokio::test]
+    async fn enable_writes_header_and_lines_disable_keeps_file() {
+        let _guard = TEST_LOCK.lock().await;
+        let _ = disable(); // a previous test may have left it on
+        let dir = scratch("enable");
+        let diag_file = enable(&dir, "HEADER-LINE v0.0.0-test").expect("enable");
+        assert!(diag_file.to_string_lossy().contains("diagnostics-"));
+        line("GUI", "hello diagnostics");
+        // A tracing event must land too (enable installs the subscriber —
+        // or it was already installed; either way the sink receives it).
+        tracing::warn!(target: "mt_test_diag", "traced through");
+        let closed = disable().expect("disable returns the path");
+        assert_eq!(closed, diag_file);
+        assert!(path().is_none(), "no sink after disable");
+        let text = std::fs::read_to_string(&diag_file).expect("file kept on disk");
+        assert!(text.contains("HEADER-LINE v0.0.0-test"));
+        assert!(text.contains("diagnostics mode started"));
+        assert!(text.contains("[GUI] hello diagnostics"));
+        assert!(
+            text.contains("traced through"),
+            "tracing events must reach the sink: {text}"
+        );
+        assert!(text.contains("diagnostics mode stopped"));
+        // Off-state writes are dropped, never panic.
+        line("GUI", "after close: dropped silently");
+        std::fs::remove_dir_all(&dir).ok();
+    }
+
+    #[test]
+    fn filename_is_sortable_utc() {
+        let stamp = timestamp_for_filename();
+        assert_eq!(stamp.len(), 15, "YYYYMMDD-HHMMSS: {stamp}");
+        assert_eq!(stamp.as_bytes()[8], b'-');
+        assert!(stamp.chars().all(|c| c.is_ascii_digit() || c == '-'));
+    }
+}
diff --git a/src/lib.rs b/src/lib.rs
index 1b59093..5bd80ff 100755
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -23,6 +23,7 @@
 
 pub mod config;
 pub mod core_proc;
+pub mod diagnostics;
 pub mod elevate;
 pub mod error;
 pub mod fsutil;
diff --git a/src/real_service.rs b/src/real_service.rs
index be86059..4e42ff5 100755
--- a/src/real_service.rs
+++ b/src/real_service.rs
@@ -124,8 +124,8 @@ use mt_net::{
 };
 use mt_platform::NetAdapter;
 use mt_service::{
-    AdapterView, ExitInfo, LinkStatus, MtService, ProfileView, RuleView, ServiceEvent,
-    ServiceState, SettingsView, StatusSnapshot,
+    AdapterView, ExitInfo, LinkStatus, MtService, ProfileDetails, ProfileView, RuleView,
+    ServiceEvent, ServiceState, SettingsView, SpeedTestInfo, StatusSnapshot,
 };
 use tokio::sync::broadcast;
 
@@ -148,6 +148,26 @@ const BIND_POLL: Duration = Duration::from_millis(10);
 const EVENT_BUFFER: usize = 256;
 /// The flow the engine pins (mt-engine hardcodes vision in its config).
 const ENGINE_FLOW: &str = "xtls-rprx-vision";
+/// S19: overall budget for one exit-IP probe (unchanged; the temp-tunnel
+/// variant also pays engine spawn inside this budget).
+const EXIT_PROBE_BUDGET: Duration = Duration::from_secs(12);
+/// S19: exit probe body cap (the identity document is tiny).
+const EXIT_PROBE_MAX_BYTES: u64 = 64 * 1024;
+/// S19: overall budget for one speed test (transfer time only; the
+/// temp-tunnel setup also pays inside this window when offline).
+const SPEED_TEST_BUDGET: Duration = Duration::from_secs(30);
+/// S19: stop reading after this many bytes even if the server has more.
+const SPEED_TEST_MAX_BYTES: u64 = 64 * 1024 * 1024;
+/// S19: per-read stall limit inside a speed test — a server that trickles
+/// slower than this yields the partial measurement instead of hanging.
+const SPEED_TEST_STALL: Duration = Duration::from_secs(5);
+/// S19: the kill switch's black-hole proxy target — a loopback port
+/// nothing legitimately listens on (discard). With the system proxy
+/// pointed here, WinINET consumers get instant connection-refused
+/// instead of silently going DIRECT after a spontaneous tunnel death
+/// (fail-closed). Reuses `sysproxy::set` so the ORIGINAL pre-tunnel
+/// snapshot stays intact for the eventual restore.
+const KILL_SWITCH_BLACKHOLE_PROXY: &str = "127.0.0.1:9";
 
 /// Filesystem locations the service works with (injected in tests).
 #[derive(Debug, Clone)]
@@ -570,6 +590,9 @@ impl Default for SettingsFile {
                 exit_probe_url: "http://ip-api.com/json".to_string(),
                 close_to_tray: false,
                 always_on_top: false,
+                kill_switch: false,
+                diagnostics: false,
+                speed_test_url: default_speed_test_url(),
             },
             // Stock rules = the LAN/private bypass (S6-REV-C P1-4): a fresh
             // settings.json must carry private-CIDR → direct rules, mirroring
@@ -588,6 +611,12 @@ impl Default for SettingsFile {
 /// round-robin; D23 = 2: stock round-robin → balanced).
 const SETTINGS_VERSION: u32 = 2;
 
+/// S19: the default speed-test URL (mirrors the mt-service serde default
+/// so a fresh SettingsFile and a freshly-parsed one agree).
+fn default_speed_test_url() -> String {
+    "http://speed.cloudflare.com/__down?bytes=25000000".to_string()
+}
+
 impl SettingsFile {
     fn load(path: &Path) -> Self {
         let mut file = match std::fs::read_to_string(path) {
@@ -748,6 +777,11 @@ impl Core {
     }
 
     fn emit(&self, ev: ServiceEvent) {
+        // S19: every GUI-visible line doubles into the diagnostics file
+        // (no-op when the mode is off).
+        if let ServiceEvent::LogLine(line) = &ev {
+            crate::diagnostics::line("GUI", line);
+        }
         let _ = self.events.send(ev);
     }
 
@@ -874,7 +908,16 @@ impl RealService {
 
         let settings = SettingsFile::load(&self.core.paths.settings_file());
         // S9: refresh the engine log level BEFORE links are built.
-        *wlock(&self.core.engine_log_level) = Some(settings.settings.log_level.clone());
+        // S19: diagnostics mode forces `debug` for the session (the whole
+        // point is capturing enough signal for a support conversation).
+        if crate::diagnostics::is_on() {
+            *wlock(&self.core.engine_log_level) = Some("debug".to_string());
+            self.core.emit(ServiceEvent::LogLine(
+                "INFO  diagnostics: engine log level forced to debug for this session".into(),
+            ));
+        } else {
+            *wlock(&self.core.engine_log_level) = Some(settings.settings.log_level.clone());
+        }
         let store = match self.core.load_store() {
             Ok(s) => s,
             Err(e) => {
@@ -1224,13 +1267,60 @@ async fn pump(
 }
 
 /// D5 fail-open give-up (v0.2 supervisor semantics for restart exhaustion):
-/// restore the system proxy, retire the session, mark `Failed`, and stop
-/// the server. Best-effort throughout — the user's browsing keeps working.
+/// retire the session, mark `Failed`, and stop the server. The system
+/// proxy hand-off depends on the S19 kill switch:
+///
+/// - **kill switch OFF** (the pre-S19 D5 behavior): the system proxy is
+///   RESTORED — the user's browsing keeps working, traffic silently goes
+///   direct (fail-open by design).
+/// - **kill switch ON**: the proxy is re-pointed at
+///   [`KILL_SWITCH_BLACKHOLE_PROXY`] — a dead loopback port. WinINET
+///   consumers then FAIL instead of leaking outside the tunnel
+///   (fail-closed). The original pre-tunnel snapshot stays on disk
+///   (keep-first), so a later connect's step-0 self-heal or an explicit
+///   disconnect still restores the true pre-tunnel state. Apps that use
+///   the SOCKS/HTTP listeners directly are fail-closed either way: the
+///   listeners die with the session and proxied connections were already
+///   refused when all links were down.
+///
+/// Best-effort throughout — the app never dies invisibly (D15).
 async fn give_up(core: &Core, shutdown: &tokio::sync::watch::Sender<bool>) {
-    tracing::warn!("supervisor: all tunnel links exhausted — giving up (fail-open)");
-    if core.sysproxy_set.swap(false, Ordering::SeqCst) {
-        if let Err(e) = crate::sysproxy::restore(&core.paths.state_dir) {
-            tracing::warn!("system proxy restore failed: {e}");
+    tracing::warn!("supervisor: all tunnel links exhausted — giving up");
+    if core.sysproxy_set.load(Ordering::SeqCst) {
+        let kill_switch = SettingsFile::load(&core.paths.settings_file())
+            .settings
+            .kill_switch;
+        if kill_switch {
+            // Keep sysproxy_set TRUE: we still own the proxy state (the
+            // black hole IS ours); the next explicit disconnect or connect
+            // self-heal restores it.
+            let override_list = crate::config::default_proxy_override();
+            match crate::sysproxy::set(
+                KILL_SWITCH_BLACKHOLE_PROXY,
+                &override_list,
+                &core.paths.state_dir,
+            ) {
+                Ok(()) => {
+                    core.emit(ServiceEvent::LogLine(
+                        "KILL SWITCH engaged: the tunnel died on its own — system traffic is \
+                         now BLOCKED (fail-closed) instead of leaking direct. Connect again \
+                         to resume, or toggle the kill switch off in Settings."
+                            .into(),
+                    ));
+                }
+                Err(e) => {
+                    // The registry still points at OUR dead local listener
+                    // — still fail-closed, just less tidy.
+                    core.emit(ServiceEvent::LogLine(format!(
+                        "KILL SWITCH: could not engage the black-hole proxy ({e}) — the stale \
+                         proxy entry keeps blocking traffic (fail-closed)"
+                    )));
+                }
+            }
+        } else if core.sysproxy_set.swap(false, Ordering::SeqCst) {
+            if let Err(e) = crate::sysproxy::restore(&core.paths.state_dir) {
+                tracing::warn!("system proxy restore failed: {e}");
+            }
         }
     }
     // Retire the session so status/profiles views stop reporting the dead
@@ -1238,7 +1328,7 @@ async fn give_up(core: &Core, shutdown: &tokio::sync::watch::Sender<bool>) {
     // the last server Arc → run() exits).
     drop(lock(&core.session).take());
     core.emit(ServiceEvent::LogLine(
-        "all tunnel links down — disconnected (system proxy restored)".into(),
+        "all tunnel links down — disconnected".into(),
     ));
     core.set_state(ServiceState::Failed);
     let _ = shutdown.send(true);
@@ -1317,9 +1407,6 @@ impl Core {
     }
 }
 
-/// Overall budget for one exit-IP probe (D18).
-const EXIT_PROBE_BUDGET: Duration = Duration::from_secs(12);
-
 /// Parse a plain-HTTP probe URL into `(host, port, path)` (D18). Only
 /// `http://` URLs are accepted — by design (no TLS stack in the tree).
 pub fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
@@ -1453,6 +1540,17 @@ impl MtService for RealService {
             if self.core.get_state() == ServiceState::Connecting {
                 self.core.disconnect_requested.store(true, Ordering::SeqCst);
             }
+            // S19: an explicit disconnect must ALSO release a kill-switch
+            // black-hole (or any proxy state we still own from a session
+            // that already died) — the user asked to stop tunneling, not
+            // to keep the network blocked.
+            if self.core.sysproxy_set.swap(false, Ordering::SeqCst) {
+                if let Err(e) = crate::sysproxy::restore(&self.core.paths.state_dir) {
+                    tracing::warn!("system proxy restore failed: {e}");
+                }
+                self.core
+                    .emit(ServiceEvent::LogLine("system proxy restored".into()));
+            }
             return;
         };
         // S9: stop the bond's carriers/sweeper before the engines drop.
@@ -1616,6 +1714,13 @@ impl MtService for RealService {
                 settings.exit_probe_url
             ));
         }
+        // S19: the speed-test URL lives under the same plain-HTTP rule.
+        if parse_http_url(&settings.speed_test_url).is_none() {
+            return Err(format!(
+                "speed test URL must be plain http:// ({:?})",
+                settings.speed_test_url
+            ));
+        }
         if settings.probe_target.split(':').count() != 2 {
             return Err(format!(
                 "probe target must be host:port ({:?})",
@@ -1634,6 +1739,10 @@ impl MtService for RealService {
         // Module note 15: `bonded_adapters` belongs to the Adapters screen
         // (`set_bonded_adapters`); a possibly-stale GUI settings copy must
         // never clobber a fresh selection — persist the server-side value.
+        // S19: remember the diagnostics + kill-switch transitions to act
+        // on them AFTER the (successful) persist below.
+        let diagnostics_changed = file.settings.diagnostics != settings.diagnostics;
+        let kill_switch_disabled = file.settings.kill_switch && !settings.kill_switch;
         let mut settings = settings;
         settings.bonded_adapters = file.settings.bonded_adapters.clone();
         file.settings = settings.clone();
@@ -1647,6 +1756,43 @@ impl MtService for RealService {
             self.disconnect().await;
             self.connect().await;
         }
+        // S19: diagnostics mode transitions (only after a successful save —
+        // a rejected settings change never touches the log file).
+        if diagnostics_changed {
+            if settings.diagnostics {
+                let header = self.build_diag_header(&settings);
+                match crate::diagnostics::enable(&self.core.paths.log_dir, &header) {
+                    Ok(p) => {
+                        self.core.emit(ServiceEvent::LogLine(format!(
+                            "INFO  diagnostics mode ON — writing {}",
+                            p.display()
+                        )));
+                    }
+                    Err(e) => {
+                        self.core.emit(ServiceEvent::LogLine(format!(
+                            "WARN  diagnostics mode could not start: {e}"
+                        )));
+                    }
+                }
+            } else if let Some(p) = crate::diagnostics::disable() {
+                self.core.emit(ServiceEvent::LogLine(format!(
+                    "INFO  diagnostics mode OFF — file kept at {}",
+                    p.display()
+                )));
+            }
+        }
+        // S19: turning the kill switch off is an explicit escape hatch for
+        // an engaged black-hole (the give_up path keeps the proxy blocked
+        // while the switch stays on).
+        if kill_switch_disabled
+            && self.core.get_state() != ServiceState::Connected
+            && self.core.sysproxy_set.swap(false, Ordering::SeqCst)
+        {
+            let _ = crate::sysproxy::restore(&self.core.paths.state_dir);
+            self.core.emit(ServiceEvent::LogLine(
+                "kill switch disabled — system proxy restored".into(),
+            ));
+        }
         if autostart_changed {
             let exe = std::env::current_exe()
                 .map_err(|e| Error(format!("resolve executable: {e}")))
@@ -1723,16 +1869,17 @@ impl MtService for RealService {
     }
 
     async fn exit_probe(&self) -> Result<ExitInfo, String> {
-        // 1. Must ride the LIVE session (the whole point is the tunnel's
-        //    egress as the internet sees it, D18).
+        // 1. Prefer the LIVE session (the whole point is the tunnel's
+        //    egress as the internet sees it, D18). S19: with no live
+        //    session, fall back to a TEMPORARY probe tunnel through the
+        //    active profile — same exit identity, works while the app is
+        //    disconnected, no session left behind.
         let socks_addr = {
             let guard = lock(&self.core.session);
-            let Some(s) = guard.as_ref() else {
-                return Err("not connected".to_string());
-            };
-            s.server
-                .socks_local_addr()
-                .ok_or_else(|| "no local listener".to_string())?
+            match guard.as_ref() {
+                Some(s) => s.server.socks_local_addr(),
+                None => None,
+            }
         };
         let store = self.core.load_store_lossy();
         let Some(profile) = store
@@ -1741,15 +1888,15 @@ impl MtService for RealService {
             .and_then(|id| store.profiles.iter().find(|p| p.id == id))
             .cloned()
         else {
-            return Err("no active profile".to_string());
+            return Err("no active profile — pick a server first".to_string());
         };
         let settings = SettingsFile::load(&self.core.paths.settings_file());
         let url = settings.settings.exit_probe_url;
         let (host, port, path) =
             parse_http_url(&url).ok_or_else(|| format!("invalid exit probe URL {url:?}"))?;
-        // 2. SOCKS CONNECT through the live local listener, then a plain
-        //    HTTP/1.1 GET (no TLS stack — zero-dep rule; the client→server
-        //    leg is REALITY-encrypted and the payload is only an IP echo).
+        // 2. Plain HTTP/1.1 GET through the tunnel (no TLS stack —
+        //    zero-dep rule; the client→server leg is REALITY-encrypted
+        //    and the payload is only an IP echo).
         let target = TargetAddr {
             host: if let Ok(ip) = host.parse() {
                 Host::Ip(ip)
@@ -1759,62 +1906,489 @@ impl MtService for RealService {
             port,
         };
         let probe = async {
-            let outbound = mt_net::Socks5Outbound::new(socks_addr);
-            let mut stream = outbound.connect(&target).await?;
-            let req = format!(
-                "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: MultiTunnel/0.5\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
-            );
-            use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
-            stream.write_all(req.as_bytes()).await?;
-            let mut buf = Vec::new();
-            stream.read_to_end(&mut buf).await?;
-            Ok::<Vec<u8>, anyhow::Error>(buf)
+            let body = if let Some(socks) = socks_addr {
+                let outbound = mt_net::Socks5Outbound::new(socks);
+                let mut stream = outbound.connect(&target).await?;
+                http_get(&mut stream, &host, &path, EXIT_PROBE_MAX_BYTES).await?
+            } else {
+                let link = temp_link_for(&self.core, &profile)
+                    .await
+                    .map_err(anyhow::Error::msg)?;
+                let mut stream = link.connect(&target).await?;
+                http_get(&mut stream, &host, &path, EXIT_PROBE_MAX_BYTES).await?
+            };
+            Ok::<Vec<u8>, anyhow::Error>(body)
         };
         let body = tokio::time::timeout(EXIT_PROBE_BUDGET, probe)
             .await
             .map_err(|_| "exit probe timed out".to_string())?
             .map_err(|e| format!("exit probe failed: {e}"))?;
-        // 3. Body = everything after the blank line.
-        let text = String::from_utf8_lossy(&body);
-        let payload = text
-            .split_once("\r\n\r\n")
-            .map(|(_, b)| b)
-            .unwrap_or(text.as_ref());
-        let json: serde_json::Value = serde_json::from_str(payload.trim_start_matches('\u{feff}'))
-            .map_err(|e| format!("exit probe response not JSON: {e}"))?;
-        let field = |k: &str| {
-            json.get(k)
-                .and_then(|v| v.as_str())
-                .unwrap_or("")
-                .to_string()
-        };
-        let exit_ip = field("query");
-        if exit_ip.is_empty() {
-            return Err("exit probe response carried no IP".to_string());
-        }
+        // 3. Parse the identity document.
+        let exit = parse_exit_info(&body)?;
         // 4. Direct-exit verdict: the exit IP should BE the profile server.
         //    A domain profile is resolved locally for the comparison.
         let server_endpoint = format!("{}:{}", profile.address, profile.port);
-        let direct = exit_ip == profile.address
+        let direct = exit.exit_ip == profile.address
             || std::net::ToSocketAddrs::to_socket_addrs(&server_endpoint)
-                .map(|addrs| addrs.map(|a| a.ip().to_string()).any(|ip| ip == exit_ip))
+                .map(|addrs| {
+                    addrs
+                        .map(|a| a.ip().to_string())
+                        .any(|ip| ip == exit.exit_ip)
+                })
                 .unwrap_or(false);
         Ok(ExitInfo {
-            exit_ip,
-            country: field("country"),
-            city: field("city"),
-            isp: field("isp"),
-            asn: field("as"),
+            exit_ip: exit.exit_ip,
+            country: exit.country,
+            city: exit.city,
+            isp: exit.isp,
+            asn: exit.asn,
             server_endpoint,
             direct_exit: direct,
+            via_temp_tunnel: socks_addr.is_none(),
         })
     }
 
+    async fn speed_test(&self, id: Option<String>) -> Result<SpeedTestInfo, String> {
+        // 1. Resolve the target profile: explicit id (or tag), else the
+        //    active one.
+        let store = self.core.load_store().map_err(|e| e.to_string())?;
+        let profile = match id {
+            Some(id) => find_profile(&store, &id)
+                .cloned()
+                .ok_or_else(|| format!("no profile matches {id:?}"))?,
+            None => store
+                .active
+                .as_deref()
+                .and_then(|a| store.profiles.iter().find(|p| p.id == a))
+                .cloned()
+                .ok_or_else(|| "no active profile — pick a server first".to_string())?,
+        };
+        let settings = SettingsFile::load(&self.core.paths.settings_file());
+        let url = settings.settings.speed_test_url;
+        let (host, port, path) =
+            parse_http_url(&url).ok_or_else(|| format!("invalid speed test URL {url:?}"))?;
+        let target = TargetAddr {
+            host: if let Ok(ip) = host.parse() {
+                Host::Ip(ip)
+            } else {
+                Host::Domain(host.clone())
+            },
+            port,
+        };
+        // 2. The LIVE session only counts when it belongs to the target
+        //    (the active profile) — otherwise a temporary tunnel through
+        //    the target profile.
+        let is_active = store.active.as_deref() == Some(profile.id.as_str());
+        let live_socks = if is_active {
+            lock(&self.core.session)
+                .as_ref()
+                .and_then(|s| s.server.socks_local_addr())
+        } else {
+            None
+        };
+        let via_temp = live_socks.is_none();
+        let connect = async {
+            Ok::<_, anyhow::Error>(if let Some(socks) = live_socks {
+                let outbound = mt_net::Socks5Outbound::new(socks);
+                outbound.connect(&target).await?
+            } else {
+                let link = temp_link_for(&self.core, &profile)
+                    .await
+                    .map_err(anyhow::Error::msg)?;
+                link.connect(&target).await?
+            })
+        };
+        // 3. Download: the clock starts AFTER the connect (tunnel setup —
+        //    possibly an engine spawn — must not pollute the rate).
+        let connect = tokio::time::timeout(SPEED_TEST_BUDGET, connect)
+            .await
+            .map_err(|_| "speed test: tunnel setup timed out".to_string())?
+            .map_err(|e| format!("speed test: connect failed: {e}"))?;
+        let mut stream = connect;
+        // The request itself is outside the measurement window too (it is
+        // one small write; servers answer after it lands).
+        {
+            use tokio::io::AsyncWriteExt as _;
+            let req = format!(
+                "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: MultiTunnel/{}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
+                env!("CARGO_PKG_VERSION")
+            );
+            stream
+                .write_all(req.as_bytes())
+                .await
+                .map_err(|e| format!("speed test: request failed: {e}"))?;
+        }
+        use tokio::io::AsyncReadExt as _;
+        let started = Instant::now();
+        let mut head = Vec::new();
+        let mut total: u64 = 0;
+        let mut chunk = vec![0u8; 64 * 1024];
+        let mut first = true;
+        loop {
+            if total >= SPEED_TEST_MAX_BYTES || started.elapsed() >= SPEED_TEST_BUDGET {
+                break;
+            }
+            let read = tokio::time::timeout(SPEED_TEST_STALL, stream.read(&mut chunk)).await;
+            let n = match read {
+                Ok(Ok(0)) => break,
+                Ok(Ok(n)) => n,
+                Ok(Err(e)) => {
+                    if total > 0 {
+                        break; // partial measurement stands
+                    }
+                    return Err(format!("speed test: read failed: {e}"));
+                }
+                Err(_) => break, // stall: keep what we have
+            };
+            // The response head (up to the blank line) is not payload —
+            // peel it off the first read(s).
+            if first && n > 0 {
+                head.extend_from_slice(&chunk[..n]);
+                if let Some(split) = find_head_end(&head) {
+                    // Body bytes = everything past the head end in the
+                    // WHOLE head buffer (the split may sit mid-buffer
+                    // when the head straddled reads).
+                    total += head.len() as u64 - split as u64;
+                    first = false;
+                }
+                // Still inside the head (a slow server dribbling headers):
+                // keep buffering, nothing counts yet.
+            } else {
+                total += n as u64;
+            }
+        }
+        if first && !head.is_empty() {
+            // The server closed before a complete head — nothing measured.
+            return Err("speed test: no usable response (connection closed early)".into());
+        }
+        if total == 0 {
+            return Err("speed test: no data received".into());
+        }
+        // Reject obvious HTTP errors (a captive portal answering 302/407
+        // would otherwise measure as "throughput").
+        let head_text = String::from_utf8_lossy(&head);
+        if let Some(status) = head_text.lines().next() {
+            let code = status.split_whitespace().nth(1).unwrap_or("");
+            if let Ok(code) = code.parse::<u16>() {
+                if code >= 400 {
+                    return Err(format!("speed test: server answered {code} ({status})"));
+                }
+            }
+        }
+        let seconds = started.elapsed().as_secs_f64().max(0.001);
+        let mbps = total as f64 * 8.0 / seconds / 1_000_000.0;
+        Ok(SpeedTestInfo {
+            mbps,
+            mbytes: total as f64 / (1024.0 * 1024.0),
+            seconds,
+            profile_tag: profile.tag.clone(),
+            via_temp_tunnel: via_temp,
+        })
+    }
+
+    async fn profile_details(&self, id: &str) -> Result<ProfileDetails, String> {
+        let store = self.core.load_store().map_err(|e| e.to_string())?;
+        let p = find_profile(&store, id).ok_or_else(|| format!("no profile matches {id:?}"))?;
+        Ok(details_of(p))
+    }
+
+    async fn update_profile(&self, id: &str, edit: ProfileDetails) -> Result<ProfileView, String> {
+        let mut store = self.core.load_store().map_err(|e| e.to_string())?;
+        let target = find_profile(&store, id)
+            .ok_or_else(|| format!("no profile matches {id:?}"))?
+            .id
+            .clone();
+        let original = store
+            .profiles
+            .iter()
+            .find(|p| p.id == target)
+            .cloned()
+            .expect("find_profile just matched");
+        // Apply the edits (trim; validate the primitives the engine will
+        // need — a save that would break the profile fails loud here).
+        let mut updated = original.clone();
+        let tag = edit.tag.trim().to_string();
+        if tag.is_empty() {
+            return Err("name cannot be empty".to_string());
+        }
+        updated.tag = tag;
+        updated.remark = edit.remark.trim().to_string();
+        let address = edit.address.trim().to_string();
+        if address.is_empty() {
+            return Err("server address cannot be empty".to_string());
+        }
+        updated.address = address;
+        updated.port = edit
+            .port
+            .trim()
+            .parse::<u16>()
+            .map_err(|_| format!("port must be a number 0-65535 ({:?})", edit.port))?;
+        let identity = edit.identity.trim().to_string();
+        if identity.is_empty() {
+            return Err("identity (UUID/password) cannot be empty".to_string());
+        }
+        updated.identity = identity;
+        // Security-layer fields only apply to a REALITY profile (the dialog
+        // hides them otherwise; stale values never overwrite the store).
+        if let Security::Reality {
+            sni,
+            fingerprint,
+            public_key,
+            short_id,
+            spider_x,
+        } = &mut updated.security
+        {
+            *sni = edit.sni.trim().to_string();
+            *fingerprint = edit.fingerprint.trim().to_string();
+            *public_key = edit.public_key.trim().to_string();
+            *short_id = edit.short_id.trim().to_string();
+            let spider = edit.spider_x.trim().to_string();
+            *spider_x = (!spider.is_empty()).then_some(spider);
+        }
+        // Capability regression check: if the ORIGINAL could drive the
+        // engine, the edited one must still be able to (fail loud BEFORE
+        // the save; a profile that never could connect may be edited
+        // freely — its inability predates the edit).
+        if endpoint_from_profile(&original).is_ok() {
+            endpoint_from_profile(&updated).map_err(|e| e.to_string())?;
+        }
+        if !store.update(&target, updated.clone()) {
+            return Err(format!("no profile matches {id:?}"));
+        }
+        self.core.save_store(&store).map_err(|e| e.to_string())?;
+        let active = store.active.as_deref() == Some(target.as_str());
+        if active && self.core.get_state() == ServiceState::Connected {
+            self.core.emit(ServiceEvent::LogLine(
+                "INFO  the active server was edited — reconnect to apply".into(),
+            ));
+        }
+        Ok(view_of(&updated, active))
+    }
+
+    async fn diagnostics_path(&self) -> Option<String> {
+        crate::diagnostics::path().map(|p| p.display().to_string())
+    }
+
     fn events(&self) -> broadcast::Receiver<ServiceEvent> {
         self.core.events.subscribe()
     }
 }
 
+/// S19: find the end of the HTTP response head (`\r\n\r\n`), returning the
+/// byte offset where the BODY starts (i.e. the index just past the blank
+/// line). `None` while the head is incomplete.
+fn find_head_end(buf: &[u8]) -> Option<usize> {
+    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
+}
+
+/// S19: plain HTTP/1.1 GET over an established stream, capped: reads to
+/// EOF or `max_bytes` (whichever first). Shared by the exit probe (live +
+/// temp-tunnel variants) — the speed test keeps its own loop because it
+/// needs byte/time accounting the probe does not.
+async fn http_get(
+    stream: &mut mt_net::ProxyStream,
+    host: &str,
+    path: &str,
+    max_bytes: u64,
+) -> anyhow::Result<Vec<u8>> {
+    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
+    let req = format!(
+        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: MultiTunnel/{}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
+        env!("CARGO_PKG_VERSION")
+    );
+    stream.write_all(req.as_bytes()).await?;
+    let mut buf = Vec::new();
+    let mut chunk = vec![0u8; 16 * 1024];
+    loop {
+        if buf.len() as u64 >= max_bytes {
+            break;
+        }
+        let n = stream.read(&mut chunk).await?;
+        if n == 0 {
+            break;
+        }
+        buf.extend_from_slice(&chunk[..n]);
+    }
+    Ok(buf)
+}
+
+/// The parsed exit-identity document (the ip-api.com JSON shape).
+struct ExitDoc {
+    exit_ip: String,
+    country: String,
+    city: String,
+    isp: String,
+    asn: String,
+}
+
+/// S19: body → [`ExitDoc`] (shared by the live and temp-tunnel probe
+/// paths; factored out of the old inline parsing).
+fn parse_exit_info(body: &[u8]) -> Result<ExitDoc, String> {
+    let text = String::from_utf8_lossy(body);
+    let payload = text
+        .split_once("\r\n\r\n")
+        .map(|(_, b)| b)
+        .unwrap_or(text.as_ref());
+    let json: serde_json::Value = serde_json::from_str(payload.trim_start_matches('\u{feff}'))
+        .map_err(|e| format!("exit probe response not JSON: {e}"))?;
+    let field = |k: &str| {
+        json.get(k)
+            .and_then(|v| v.as_str())
+            .unwrap_or("")
+            .to_string()
+    };
+    let exit_ip = field("query");
+    if exit_ip.is_empty() {
+        return Err("exit probe response carried no IP".to_string());
+    }
+    Ok(ExitDoc {
+        exit_ip,
+        country: field("country"),
+        city: field("city"),
+        isp: field("isp"),
+        asn: field("as"),
+    })
+}
+
+/// S19: build a one-shot link for `profile` (offline probes: exit-IP,
+/// speed test). Mirrors `test_latency`'s factory use — engines spawn
+/// lazily and die with the returned Arcs. With several bonded adapters
+/// the FIRST link (the top-priority adapter) carries the probe.
+async fn temp_link_for(core: &Core, profile: &Profile) -> Result<Arc<dyn Outbound>, String> {
+    let settings = SettingsFile::load(&core.paths.settings_file());
+    let bonded = resolve_bonded(
+        &settings.settings.bonded_adapters,
+        &mt_platform::enumerate_adapters(),
+    );
+    let mut links = core
+        .factory
+        .build(profile, &bonded)
+        .await
+        .map_err(|e| e.to_string())?;
+    links
+        .pop()
+        .ok_or_else(|| "link factory produced no links".to_string())
+}
+
+/// S19: [`Profile`] → the Edit dialog's material (credential fields
+/// included by necessity; the dialog is the only consumer).
+fn details_of(p: &Profile) -> ProfileDetails {
+    let (security, sni, fingerprint, public_key, short_id, spider_x) = match &p.security {
+        Security::Reality {
+            sni,
+            fingerprint,
+            public_key,
+            short_id,
+            spider_x,
+        } => (
+            "reality",
+            sni.clone(),
+            fingerprint.clone(),
+            public_key.clone(),
+            short_id.clone(),
+            spider_x.clone().unwrap_or_default(),
+        ),
+        Security::Tls { sni, .. } => (
+            "tls",
+            sni.clone().unwrap_or_default(),
+            String::new(),
+            String::new(),
+            String::new(),
+            String::new(),
+        ),
+        Security::None => (
+            "none",
+            String::new(),
+            String::new(),
+            String::new(),
+            String::new(),
+            String::new(),
+        ),
+    };
+    ProfileDetails {
+        id: p.id.clone(),
+        tag: p.tag.clone(),
+        remark: p.remark.clone(),
+        protocol: protocol_name(p.protocol).to_string(),
+        address: p.address.clone(),
+        port: p.port.to_string(),
+        identity: p.identity.clone(),
+        security: security.to_string(),
+        sni,
+        fingerprint,
+        public_key,
+        short_id,
+        spider_x,
+    }
+}
+
+/// S19: [`Profile`] → the GUI list row (single mapping for profiles(),
+/// import_link() and update_profile()).
+fn view_of(p: &Profile, active: bool) -> ProfileView {
+    ProfileView {
+        active,
+        id: p.id.clone(),
+        tag: p.tag.clone(),
+        protocol: protocol_name(p.protocol).to_string(),
+        endpoint: format!("{}:{}", p.address, p.port),
+        remark: p.remark.clone(),
+    }
+}
+
+impl RealService {
+    /// S19: the redacted diagnostics header — everything a support
+    /// conversation needs, nothing it must not (D9: no uuid, no password,
+    /// no public key values).
+    fn build_diag_header(&self, settings: &SettingsView) -> String {
+        let mut out = String::new();
+        out.push_str(&format!(
+            "MultiTunnel {} diagnostics — {} ({} {})\n",
+            env!("CARGO_PKG_VERSION"),
+            fmt_utc(SystemTime::now()),
+            std::env::consts::OS,
+            std::env::consts::ARCH
+        ));
+        out.push_str("=== settings (no credentials live here) ===\n");
+        out.push_str(&format!(
+            "merge_policy={} bond_mode={} socks_port={} http_port={} allow_lan={} \
+             system_proxy={} kill_switch={} log_level={}\n",
+            settings.merge_policy,
+            settings.bond_mode,
+            settings.socks_port,
+            settings.http_port,
+            settings.allow_lan,
+            settings.system_proxy,
+            settings.kill_switch,
+            settings.log_level
+        ));
+        out.push_str(&format!(
+            "probe_target={} exit_probe_url={} speed_test_url={}\n",
+            settings.probe_target, settings.exit_probe_url, settings.speed_test_url
+        ));
+        let store = self.core.load_store_lossy();
+        out.push_str("=== profiles (endpoints only — ids/secrets redacted) ===\n");
+        for p in &store.profiles {
+            let active = if store.active.as_deref() == Some(p.id.as_str()) {
+                " [active]"
+            } else {
+                ""
+            };
+            out.push_str(&format!(
+                "{} {}/{}:{}{}\n",
+                p.tag,
+                protocol_name(p.protocol),
+                p.address,
+                p.port,
+                active
+            ));
+        }
+        out.push_str("=== bonded adapters ===\n");
+        out.push_str(&format!("{:?}\n", settings.bonded_adapters));
+        out.push_str("=== events (GUI log lines + tracing/engine output) ===");
+        out
+    }
+}
+
 /// Find a profile by id first, then by tag (decision #11).
 fn find_profile<'a>(store: &'a ProfileStore, id_or_tag: &str) -> Option<&'a Profile> {
     store
@@ -1981,6 +2555,9 @@ mod tests {
                 exit_probe_url: "http://ip-api.com/json".to_string(),
                 close_to_tray: false,
                 always_on_top: false,
+                kill_switch: false,
+                diagnostics: false,
+                speed_test_url: default_speed_test_url(),
             },
             rules: Vec::new(),
             socks_port: 0,
@@ -2086,8 +2663,11 @@ mod tests {
         let view = svc.import_link(VLESS_REALITY).await.expect("import");
         svc.set_active_profile(&view.tag).await.expect("set active");
 
-        // Not connected → refused.
-        assert_eq!(svc.exit_probe().await.unwrap_err(), "not connected");
+        // S19: DISCONNECTED, the probe now works through a temporary
+        // probe tunnel (the user's ask) instead of erroring.
+        let offline = svc.exit_probe().await.expect("offline probe");
+        assert_eq!(offline.exit_ip, "127.0.0.1");
+        assert!(offline.via_temp_tunnel, "no live session → temp tunnel");
 
         svc.connect().await;
         assert_eq!(svc.status().await.state, ServiceState::Connected);
@@ -2101,6 +2681,7 @@ mod tests {
         assert_eq!(info.isp, "Example Telecom");
         assert!(info.asn.contains("AS64512"));
         assert_eq!(info.server_endpoint, "203.0.113.10:443");
+        assert!(!info.via_temp_tunnel, "live session in use");
         assert!(
             !info.direct_exit,
             "203.0.113.10 profile must NOT match a 127.0.0.1 exit"
@@ -2108,6 +2689,29 @@ mod tests {
         svc.disconnect().await;
     }
 
+    #[tokio::test]
+    async fn exit_probe_without_active_profile_errors() {
+        let exit = spawn_exit_echo("127.0.0.1").await;
+        let paths = scratch("exitnoactive");
+        write_settings(&paths, exit.port(), false);
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: exit,
+                fail: false,
+            }),
+        );
+        // Profile exists but is NOT active.
+        let view = svc.import_link(VLESS_REALITY).await.expect("import");
+        assert_eq!(
+            svc.exit_probe().await.unwrap_err(),
+            "no active profile — pick a server first"
+        );
+        // Sanity: activating makes it work again (temp tunnel).
+        svc.set_active_profile(&view.tag).await.expect("set active");
+        assert!(svc.exit_probe().await.is_ok());
+    }
+
     #[tokio::test]
     async fn exit_probe_direct_exit_matches_loopback_profile() {
         let exit = spawn_exit_echo("127.0.0.1").await;
@@ -2137,6 +2741,292 @@ mod tests {
         svc.disconnect().await;
     }
 
+    /// S19: an HTTP server serving a fixed-size body for speed tests.
+    async fn spawn_speed_server(mib: usize) -> SocketAddr {
+        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
+            .await
+            .expect("bind speed server");
+        let addr = listener.local_addr().expect("speed addr");
+        let body = vec![b'x'; mib * 1024 * 1024];
+        tokio::spawn(async move {
+            loop {
+                let Ok((mut stream, _)) = listener.accept().await else {
+                    break;
+                };
+                let body = body.clone();
+                tokio::spawn(async move {
+                    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
+                    let mut buf = vec![0u8; 2048];
+                    let _ = stream.read(&mut buf).await; // drain the request head
+                    let head = format!(
+                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
+                        body.len()
+                    );
+                    let _ = stream.write_all(head.as_bytes()).await;
+                    let _ = stream.write_all(&body).await;
+                });
+            }
+        });
+        addr
+    }
+
+    #[tokio::test]
+    async fn speed_test_measures_through_temp_tunnel_when_disconnected() {
+        let server = spawn_speed_server(2).await; // 2 MiB on loopback
+        let paths = scratch("speedtest");
+        write_settings(&paths, server.port(), false);
+        let mut file = SettingsFile::load(&paths.settings_file());
+        file.settings.speed_test_url = format!("http://127.0.0.1:{}/down", server.port());
+        file.save(&paths.settings_file()).expect("save settings");
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: server,
+                fail: false,
+            }),
+        );
+        let view = svc.import_link(VLESS_REALITY).await.expect("import");
+        svc.set_active_profile(&view.tag).await.expect("set active");
+
+        // Disconnected → temporary probe tunnel; the clock starts after
+        // the connect, so the measured rate is the loopback body push.
+        let info = svc.speed_test(None).await.expect("speed test");
+        assert!(info.via_temp_tunnel);
+        assert_eq!(info.profile_tag, view.tag);
+        assert!((info.mbytes - 2.0).abs() < 0.05, "body ~2 MiB: {info:?}");
+        assert!(info.seconds > 0.0);
+        assert!(info.mbps > 1.0, "loopback must beat 1 Mbps: {info:?}");
+
+        // No active profile at all → a clear error.
+        svc.remove_profile(&view.id).await.expect("remove");
+        assert_eq!(
+            svc.speed_test(None).await.unwrap_err(),
+            "no active profile — pick a server first"
+        );
+
+        // A bad URL is rejected before any dialing.
+        let mut file = SettingsFile::load(&svc.core.paths.settings_file());
+        file.settings.speed_test_url = "https://no-tls.example/".to_string();
+        file.save(&svc.core.paths.settings_file()).expect("save");
+        let view2 = svc.import_link(VLESS_REALITY).await.expect("re-import");
+        svc.set_active_profile(&view2.tag).await.expect("active");
+        let err = svc.speed_test(None).await.unwrap_err();
+        assert!(err.contains("invalid speed test URL"), "{err}");
+    }
+
+    #[tokio::test]
+    async fn speed_test_rides_live_session_for_active_profile() {
+        let server = spawn_speed_server(1).await;
+        let echo = spawn_echo().await;
+        let paths = scratch("speedlive");
+        // Probe target = echo (so connect() succeeds); speed URL = server.
+        write_settings(&paths, echo.port(), false);
+        let mut file = SettingsFile::load(&paths.settings_file());
+        file.settings.speed_test_url = format!("http://127.0.0.1:{}/down", server.port());
+        file.save(&paths.settings_file()).expect("save settings");
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: server,
+                fail: false,
+            }),
+        );
+        let view = svc.import_link(VLESS_REALITY).await.expect("import");
+        svc.set_active_profile(&view.tag).await.expect("set active");
+        svc.connect().await;
+        assert_eq!(svc.status().await.state, ServiceState::Connected);
+        let info = svc.speed_test(None).await.expect("speed test");
+        assert!(!info.via_temp_tunnel, "active profile + live session");
+        assert!((info.mbytes - 1.0).abs() < 0.05, "1 MiB body: {info:?}");
+        svc.disconnect().await;
+    }
+
+    #[tokio::test]
+    async fn update_profile_edits_fields_and_keeps_id_and_active() {
+        let exit = spawn_exit_echo("127.0.0.1").await;
+        let paths = scratch("editprofile");
+        write_settings(&paths, exit.port(), false);
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: exit,
+                fail: false,
+            }),
+        );
+        let view = svc.import_link(VLESS_REALITY).await.expect("import");
+        svc.set_active_profile(&view.tag).await.expect("set active");
+
+        let mut details = svc.profile_details(&view.id).await.expect("details");
+        assert_eq!(details.protocol, "vless");
+        assert_eq!(details.security, "reality");
+        assert_eq!(details.port, "443");
+        assert_eq!(details.sni, "tunnel.example.org");
+        details.tag = "renamed server".into();
+        details.remark = "my remark".into();
+        details.port = "8443".into();
+        details.sni = "new.example.org".into();
+        let updated = svc.update_profile(&view.id, details).await.expect("save");
+        assert_eq!(updated.tag, "renamed server");
+        assert_eq!(updated.endpoint, "203.0.113.10:8443");
+        assert!(updated.active, "the edited profile stays active");
+
+        // The store reloads with the same id, the edit and the active mark.
+        let store = svc.core.load_store().expect("reload");
+        assert_eq!(store.profiles.len(), 1);
+        assert_eq!(store.profiles[0].id, view.id);
+        assert_eq!(store.profiles[0].tag, "renamed server");
+        assert_eq!(store.profiles[0].port, 8443);
+        assert_eq!(store.active.as_deref(), Some(view.id.as_str()));
+        let Security::Reality { sni, .. } = &store.profiles[0].security else {
+            panic!("still reality");
+        };
+        assert_eq!(sni, "new.example.org");
+
+        // Unknown id → clear error.
+        assert!(svc.profile_details("nope").await.is_err());
+        assert!(svc
+            .update_profile("nope", svc.profile_details(&view.id).await.unwrap())
+            .await
+            .is_err());
+    }
+
+    #[tokio::test]
+    async fn update_profile_rejects_invalid_edits() {
+        let exit = spawn_exit_echo("127.0.0.1").await;
+        let paths = scratch("editinvalid");
+        write_settings(&paths, exit.port(), false);
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: exit,
+                fail: false,
+            }),
+        );
+        let view = svc.import_link(VLESS_REALITY).await.expect("import");
+        svc.set_active_profile(&view.tag).await.expect("set active");
+
+        let bad = |tag: &str, port: &str, identity: &str| ProfileDetails {
+            id: view.id.clone(),
+            tag: tag.to_string(),
+            remark: String::new(),
+            protocol: "vless".to_string(),
+            address: "203.0.113.10".to_string(),
+            port: port.to_string(),
+            identity: identity.to_string(),
+            security: "reality".to_string(),
+            sni: "tunnel.example.org".to_string(),
+            fingerprint: "chrome".to_string(),
+            public_key: "k".to_string(),
+            short_id: "0123abcd".to_string(),
+            spider_x: String::new(),
+        };
+        assert!(svc
+            .update_profile(&view.id, bad("", "443", "u"))
+            .await
+            .unwrap_err()
+            .contains("name"));
+        assert!(svc
+            .update_profile(&view.id, bad("t", "not-a-port", "u"))
+            .await
+            .unwrap_err()
+            .contains("port"));
+        assert!(svc
+            .update_profile(&view.id, bad("t", "443", "  "))
+            .await
+            .unwrap_err()
+            .contains("identity"));
+        // Nothing was saved.
+        let store = svc.core.load_store().expect("reload");
+        assert_eq!(store.profiles[0].tag, view.tag, "store untouched");
+    }
+
+    #[tokio::test]
+    async fn diagnostics_mode_creates_file_and_forces_engine_debug() {
+        // The sink is process-global: hold the crate's diag test lock for
+        // the whole window.
+        let _guard = crate::diagnostics::TEST_LOCK.lock().await;
+        let _ = crate::diagnostics::disable();
+        let exit = spawn_exit_echo("127.0.0.1").await;
+        let paths = scratch("diagmode");
+        write_settings(&paths, exit.port(), false);
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: exit,
+                fail: false,
+            }),
+        );
+        let view = svc.import_link(VLESS_REALITY).await.expect("import");
+        svc.set_active_profile(&view.tag).await.expect("set active");
+
+        // Turn the mode ON through the settings path (the real entry).
+        let mut s = svc.settings().await;
+        s.diagnostics = true;
+        svc.set_settings(s).await.expect("save with diagnostics");
+        let diag = svc.diagnostics_path().await.expect("file created");
+        assert!(diag.contains("diagnostics-"), "{diag}");
+        assert!(std::path::Path::new(&diag).is_file());
+
+        // A connect forces the ENGINE log level to debug for the session.
+        svc.connect().await;
+        assert_eq!(svc.status().await.state, ServiceState::Connected);
+        let level = wlock(&svc.core.engine_log_level).clone();
+        assert_eq!(level.as_deref(), Some("debug"));
+
+        // GUI log lines (the connected notice) land in the file.
+        tokio::time::sleep(Duration::from_millis(50)).await;
+        let text = std::fs::read_to_string(&diag).expect("read diag");
+        assert!(text.contains("MultiTunnel"), "header present");
+        assert!(text.contains("diagnostics mode started"));
+        assert!(text.contains("engine log level forced to debug"));
+        assert!(text.contains("[GUI]"), "GUI lines tee'd");
+        // Redaction: the profile UUID must never appear.
+        assert!(
+            !text.contains("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"),
+            "no credentials in the diagnostics file"
+        );
+        svc.disconnect().await;
+
+        // Off: the path clears, the file stays.
+        let mut s = svc.settings().await;
+        s.diagnostics = false;
+        svc.set_settings(s).await.expect("save without diagnostics");
+        assert!(svc.diagnostics_path().await.is_none());
+        assert!(std::path::Path::new(&diag).is_file(), "file kept");
+    }
+
+    #[tokio::test]
+    async fn disconnect_releases_owned_proxy_state_without_a_session() {
+        // S19: after a kill-switch give-up (or any death that left
+        // sysproxy_set armed with no session), an explicit disconnect must
+        // restore the proxy instead of leaving the black-hole engaged.
+        let exit = spawn_exit_echo("127.0.0.1").await;
+        let paths = scratch("ksrelease");
+        write_settings(&paths, exit.port(), false);
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: exit,
+                fail: false,
+            }),
+        );
+        let mut events = svc.events();
+        // Simulate the post-give-up state: no session, proxy still owned.
+        svc.core.sysproxy_set.store(true, Ordering::SeqCst);
+        assert!(svc.core.session.lock().unwrap().is_none());
+        svc.disconnect().await;
+        assert!(!svc.core.sysproxy_set.load(Ordering::SeqCst));
+        let mut saw_restore = false;
+        while let Ok(ev) = events.try_recv() {
+            if let ServiceEvent::LogLine(l) = ev {
+                if l.contains("system proxy restored") {
+                    saw_restore = true;
+                }
+            }
+        }
+        assert!(saw_restore, "the restore is announced in the GUI log");
+    }
+
     #[tokio::test]
     async fn stream_bond_mode_connects_and_carries_traffic() {
         let (daemon, _guard) = spawn_merge_daemon().await;
@@ -2324,6 +3214,9 @@ mod tests {
             exit_probe_url: "http://ip-api.com/json".to_string(),
             close_to_tray: false,
             always_on_top: false,
+            kill_switch: false,
+            diagnostics: false, // never true here: the sink is process-global
+            speed_test_url: "http://speed.cloudflare.com/__down?bytes=25000000".to_string(),
         };
         svc.set_settings(view.clone()).await.expect("save settings");
         assert_eq!(svc.settings().await, view);

### Commit: 4888fec - backup(s18-memory-node): 2026-11-21T08:27:47Z (Z User, 2026-11-21)
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index 9b16150..5526006 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "9d4111b 2026-11-21T07:17:18Z",
+  "as_of": "7203362 2026-11-21T08:27:44Z",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 263,
-  "edge_count": 272,
+  "node_count": 264,
+  "edge_count": 273,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -975,6 +975,26 @@
         }
       ]
     },
+    {
+      "id": "commit:7203362",
+      "type": "commit",
+      "title": "backup(s18-close-v0.12.0): 2026-11-21T08:27:44Z",
+      "tags": [
+        "2026-11-21"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "7203362"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s18-close-v0.12.0): 2026-11-21T08:27:44Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:723ec6e",
       "type": "commit",
@@ -5218,7 +5238,7 @@
       ],
       "passages": [
         {
-          "text": "- **Stage**: S18 (v0.12.0) — implementation + local gates COMPLETE, release running. The six asks shipped (D28): 3-line tray tooltip (name+version / totals / speeds, CRLF), transparent-background contrast-boosted LARGER runtime icons (tray ICOs + logo_64 + logo_24), Settings accent COLOR WHEEL (HSV disc asset + inverse marker math, live re-theme, persist on release), System/Light/Dark mode (OS detection chain: MT_THEME_SYSTEM_DARK env seam → Win32 AppsUseLightTheme → gsettings/gtk; 5 s repoll; legacy dark_mode kept as the forward-compat shadow), mt-bond BLEST tie-guard + ECF completion scheduling (papers survey: docs/research/), UI changelog survey (docs/research/). 447/0 tests; rig verified sf=1.0+1.5. - **Prior stage**: S17 (v0.11.0, titlebar version + live speeds, logs floating jump buttons + autoscroll). The three asks shipped: release version next to the app name in the titlebar (workspace version inheritance — one-line release bumps; mt-app.exe PE metadata now carries the release version), live global ↓/↑ rates on the strip's right (the tray tooltip's same 1 s sample), Logs tab = fixed toolbar + dedicated viewport with floating top/bottom jump buttons (26px right stand-off — the rig caught the scrollbar grab-zone swallowing flush-corner clicks) + autoscroll toggle (default ON, follows the tail; re-enabling re-anchors). All 8 gates green; rig verified sf=1.0+1.5 (VLM content reads + pixel diffs); 433/0 tests.",
+          "text": "- **Stage**: IDLE — S18 CLOSED, v0.12.0 released and verified (release run 44879546925 first-try SUCCESS; the ci windows leg was a 69-min cold-cache cancel with same-commit green proof from the release's own build-windows — see LEDGER 08:30Z). The six asks shipped (D28): 3-line tray tooltip (name+version / totals / speeds, CRLF), transparent-background contrast-boosted LARGER runtime icons (tray ICOs + logo_64 + logo_24), Settings accent COLOR WHEEL (HSV disc asset + inverse marker math, live re-theme, persist on release), System/Light/Dark mode (OS detection chain: MT_THEME_SYSTEM_DARK env seam → Win32 AppsUseLightTheme → gsettings/gtk; 5 s repoll; legacy dark_mode kept as the forward-compat shadow), mt-bond BLEST tie-guard + ECF completion scheduling (papers survey: docs/research/), UI changelog survey (docs/research/). 447/0 tests; rig verified sf=1.0+1.5. - **Prior stages**: S18 (v0.12.0, D28: 3-line tray tooltip; transparent contrast-boosted larger icons everywhere; Settings accent COLOR WHEEL; System/Light/Dark mode with OS detection; mt-bond BLEST tie-guard + ECF scheduling per the docs/research/ papers survey; UI changelog survey), S17 (v0.11.0, titlebar version + live speeds, logs floating jump buttons + autoscroll).",
           "kind": "body"
         }
       ]
@@ -7761,6 +7781,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s11)"
     },
+    {
+      "src": "commit:7203362",
+      "dst": "ledger:s18",
+      "rel": "part_of",
+      "evidence": "subject stage label (s18)"
+    },
     {
       "src": "commit:723ec6e",
       "dst": "ledger:s11-release",

( PROJECT_MEMORY.md's own diff is elided here — the durable
  sections above are hand-maintained; see harness/memory/LEDGER.md
  for the full stage history. )
```

