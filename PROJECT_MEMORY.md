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

### Commit: e1ec5cc - feat: automatic stale-log cleanup at startup (v2rayN 7.25.6 parity) (Z User, 2026-11-27)
diff --git a/src/diagnostics.rs b/src/diagnostics.rs
index 599a3f7..d532bbb 100644
--- a/src/diagnostics.rs
+++ b/src/diagnostics.rs
@@ -104,6 +104,59 @@ pub(crate) fn is_on() -> bool {
     ENABLED.load(Ordering::Acquire)
 }
 
+/// S23 (v2rayN 7.25.6 parity — "log and temp-file auto cleanup, 7 days
+/// by default"): prune stale files from the log dir.
+///
+/// Policy (all best-effort — cleanup must never break construction):
+/// - regular FILES only (subdirectories are left alone);
+/// - the newest `keep_newest` files are ALWAYS kept (an active burst of
+///   diagnostics sessions never loses its newest material);
+/// - beyond that, files whose mtime is older than `retention` measured
+///   at `now` are deleted;
+/// - the `protected` path (the file the diagnostics sink is writing
+///   right now) is never touched even if it somehow looks stale.
+///
+/// Returns the paths that were deleted (the caller reports the count);
+/// unreadable metadata and failed deletes are silently skipped and
+/// simply do not appear in the list.
+pub(crate) fn sweep_old_logs(
+    dir: &Path,
+    now: SystemTime,
+    retention: std::time::Duration,
+    keep_newest: usize,
+    protected: Option<&Path>,
+) -> Vec<PathBuf> {
+    let Ok(entries) = std::fs::read_dir(dir) else {
+        return Vec::new(); // no dir yet — nothing to sweep (not an error)
+    };
+    // (path, mtime) for the regular files; newest first.
+    let mut files: Vec<(PathBuf, SystemTime)> = entries
+        .flatten()
+        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
+        .filter_map(|e| {
+            let mtime = e.metadata().ok()?.modified().ok()?;
+            Some((e.path(), mtime))
+        })
+        .collect();
+    files.sort_by(|a, b| b.1.cmp(&a.1));
+    let mut pruned = Vec::new();
+    for (i, (path, mtime)) in files.into_iter().enumerate() {
+        if i < keep_newest {
+            continue; // the newest N always survive
+        }
+        if Some(path.as_path()) == protected {
+            continue; // the live diagnostics file is never swept
+        }
+        let Ok(age) = now.duration_since(mtime) else {
+            continue; // clock skew (mtime in the future): keep
+        };
+        if age > retention && std::fs::remove_file(&path).is_ok() {
+            pruned.push(path);
+        }
+    }
+    pruned
+}
+
 /// Append one service-side line (the GUI `LogLine` tee; `kind` tags the
 /// source, e.g. "GUI"). Best-effort — a write failure just disables the
 /// sink (a broken log file must never break the app).
@@ -268,3 +321,94 @@ mod tests {
         assert!(stamp.chars().all(|c| c.is_ascii_digit() || c == '-'));
     }
 }
+
+/// S23: the stale-log sweep (v2rayN 7.25.6 parity).
+#[cfg(test)]
+mod sweep_tests {
+    use super::*;
+    use std::fs::FileTimes;
+
+    fn scratch(tag: &str) -> PathBuf {
+        let d = std::env::temp_dir().join(format!(
+            "mt-sweep-{}-{}-{}",
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
+    /// A file with a pinned mtime (std FileTimes, stable since 1.75).
+    fn touch(dir: &Path, name: &str, age: Option<std::time::Duration>) -> PathBuf {
+        let p = dir.join(name);
+        std::fs::write(&p, b"log line\n").expect("write file");
+        if let Some(age) = age {
+            let mtime = SystemTime::now() - age;
+            let f = std::fs::File::options()
+                .write(true)
+                .open(&p)
+                .expect("open for times");
+            f.set_times(FileTimes::new().set_modified(mtime))
+                .expect("set mtime");
+        }
+        p
+    }
+
+    #[test]
+    fn old_files_pruned_new_and_protected_kept() {
+        let dir = scratch("old");
+        let now = SystemTime::now();
+        let retention = std::time::Duration::from_secs(7 * 24 * 3600);
+        let stale = touch(&dir, "stale.log", Some(retention * 2));
+        let fresh = touch(&dir, "fresh.log", Some(std::time::Duration::from_secs(60)));
+        let ancient_but_kept = touch(&dir, "kept-protected.log", Some(retention * 4));
+
+        let pruned = sweep_old_logs(&dir, now, retention, 1, Some(&ancient_but_kept));
+        assert_eq!(pruned, vec![stale], "only the stale unprotected file");
+        assert!(fresh.exists(), "fresh file survives");
+        assert!(ancient_but_kept.exists(), "the protected file survives");
+        std::fs::remove_dir_all(&dir).ok();
+    }
+
+    #[test]
+    fn the_newest_n_always_survive_regardless_of_age() {
+        let dir = scratch("newest");
+        let now = SystemTime::now();
+        let retention = std::time::Duration::from_secs(3600);
+        // Three ancient files with distinct ages: `a` newest (10×), `b`
+        // middle (11×), `c` oldest (12×). keep_newest=2 → `a` and `b`
+        // survive, only the oldest `c` is swept.
+        let a = touch(&dir, "a.log", Some(retention * 10));
+        let b = touch(&dir, "b.log", Some(retention * 11));
+        let c = touch(&dir, "c.log", Some(retention * 12));
+        let pruned = sweep_old_logs(&dir, now, retention, 2, None);
+        assert_eq!(pruned, vec![c], "only the oldest of the three");
+        assert!(a.exists() && b.exists(), "the newest two survive");
+        std::fs::remove_dir_all(&dir).ok();
+    }
+
+    #[test]
+    fn missing_dir_and_subdirs_are_not_errors() {
+        let dir = scratch("missing").join("nope");
+        assert!(sweep_old_logs(&dir, SystemTime::now(), std::time::Duration::ZERO, 0, None)
+            .is_empty());
+        // A subdirectory is never touched.
+        let real = scratch("subdir");
+        std::fs::create_dir_all(real.join("nested")).expect("subdir");
+        let _ = touch(&real, "old.log", Some(std::time::Duration::from_secs(10_000_000)));
+        let pruned = sweep_old_logs(
+            &real,
+            SystemTime::now(),
+            std::time::Duration::from_secs(60),
+            0,
+            None,
+        );
+        assert_eq!(pruned.len(), 1, "only the file, never the dir");
+        assert!(real.join("nested").is_dir(), "subdirectory untouched");
+        std::fs::remove_dir_all(&real).ok();
+    }
+}
diff --git a/src/real_service.rs b/src/real_service.rs
index 78fb8be..4da6336 100755
--- a/src/real_service.rs
+++ b/src/real_service.rs
@@ -149,6 +149,12 @@ const BIND_WAIT: Duration = Duration::from_secs(8);
 const BIND_POLL: Duration = Duration::from_millis(10);
 /// `ServiceEvent` broadcast buffer (slow GUI receivers lag, never block).
 const EVENT_BUFFER: usize = 256;
+/// S23 (v2rayN 7.25.6 parity): stale-log retention — files in the log
+/// dir older than this are pruned at service construction.
+const LOG_RETENTION_SECS: u64 = 7 * 24 * 3600;
+/// S23: the newest N log files always survive the sweep (an active
+/// diagnostics burst never loses its newest material).
+const LOG_SWEEP_KEEP_NEWEST: usize = 40;
 /// The flow the engine pins (mt-engine hardcodes vision in its config).
 const ENGINE_FLOW: &str = "xtls-rprx-vision";
 /// S19: overall budget for one exit-IP probe (unchanged; the temp-tunnel
@@ -910,6 +916,27 @@ impl RealService {
                 }
             }
         }
+        // S23 (v2rayN 7.25.6 parity): stale-log hygiene at every service
+        // construction — files older than 7 days are pruned, the newest
+        // 40 always survive, and the file the (possibly just restored)
+        // diagnostics sink is writing is never touched. Best-effort: a
+        // failed sweep is logged and forgotten (never fatal).
+        {
+            let pruned = crate::diagnostics::sweep_old_logs(
+                &svc.core.paths.log_dir,
+                SystemTime::now(),
+                std::time::Duration::from_secs(LOG_RETENTION_SECS),
+                LOG_SWEEP_KEEP_NEWEST,
+                crate::diagnostics::path().as_deref(),
+            );
+            if !pruned.is_empty() {
+                svc.core.emit(ServiceEvent::LogLine(format!(
+                    "INFO  log cleanup: removed {} stale file(s) older than {} days",
+                    pruned.len(),
+                    LOG_RETENTION_SECS / (24 * 3600)
+                )));
+            }
+        }
         svc
     }
 
@@ -3472,6 +3499,55 @@ mod tests {
         assert!(paths.log_dir.ends_with("logs"), "{dir}");
     }
 
+    /// S23 (v2rayN 7.25.6 parity): service construction prunes stale
+    /// log files (7-day retention) while fresh ones survive. The sweep
+    /// keeps the newest 40 files unconditionally, so the fixture needs
+    /// MORE than 40 files for anything to become eligible.
+    #[tokio::test]
+    async fn s23_construction_sweeps_stale_logs() {
+        use std::fs::FileTimes;
+        let echo = spawn_echo().await;
+        let paths = scratch("sweep");
+        std::fs::create_dir_all(&paths.log_dir).expect("log dir");
+        let stale_mtime = std::time::SystemTime::now()
+            - std::time::Duration::from_secs(9 * 24 * 3600);
+        let pin = |p: &std::path::Path| {
+            std::fs::File::options()
+                .write(true)
+                .open(p)
+                .expect("open")
+                .set_times(FileTimes::new().set_modified(stale_mtime))
+                .expect("mtime");
+        };
+        // 46 nine-day-old files (all eligible, only the oldest 6 exceed
+        // the newest-40 cap) + 2 fresh files.
+        for i in 0..46 {
+            let p = paths.log_dir.join(format!("engine-stale-{i:03}.log"));
+            std::fs::write(&p, b"x").expect("write");
+            pin(&p);
+        }
+        for i in 0..2 {
+            std::fs::write(paths.log_dir.join(format!("fresh-{i}.log")), b"x")
+                .expect("write");
+        }
+
+        let _svc = RealService::with(paths, Arc::new(FakeFactory { echo, fail: false }));
+        // Count what survived directly from the log dir.
+        let log_dir = std::path::PathBuf::from(_svc.log_dir().await);
+        let survived: Vec<String> = std::fs::read_dir(&log_dir)
+            .expect("log dir readable")
+            .flatten()
+            .map(|e| e.file_name().to_string_lossy().into_owned())
+            .collect();
+        assert_eq!(survived.len(), 40, "48 files − 8 beyond the newest-40 cap");
+        assert!(survived.iter().any(|n| n.starts_with("fresh-")), "fresh kept");
+        // All 46 stale files share ONE pinned mtime (9 days old); the
+        // newest-40 cap counts the 2 fresh files too, so 38 stale files
+        // survive and 8 are swept.
+        let stale_left = survived.iter().filter(|n| n.contains("stale")).count();
+        assert_eq!(stale_left, 38, "38 stale + 2 fresh = the kept 40");
+    }
+
     /// S22: a diagnostics toggle persisted ON writes again after a
     /// restart — the sink starts AT CONSTRUCTION, not only on an off→on
     /// transition (the old gap made the Settings toggle lie and the
```

