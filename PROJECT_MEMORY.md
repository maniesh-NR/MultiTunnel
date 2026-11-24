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
### Commit: e510e43 - backup(s21-release-prep-2): 2026-11-24T20:08:15Z (Z User, 2026-11-24)
diff --git a/QUICKSTART.md b/QUICKSTART.md
index 2662060..7cf41a5 100755
--- a/QUICKSTART.md
+++ b/QUICKSTART.md
@@ -98,6 +98,17 @@ needed for the usual per-user system-proxy writes (expect the SmartScreen
 > DIAGNOSTICS mode that writes one paste-ready support log file
 > (connection lifecycle + engine output, credentials redacted).
 >
+> v0.14: adding a second server and switching to it while connected no
+> longer kills the tunnel until restart (lingering proxy connections are
+> force-closed at shutdown and busy listener binds retry briefly);
+> throughput/stability parity work (TCP_NODELAY on every carrier leg,
+> coalesced ACKs, batched carrier writes, honest per-carrier RTTs, no
+> more probe blackouts on the only link); Settings → Network gained a
+> **QoS FAIR SHARE** toggle — when two sites race for a loaded tunnel,
+> each gets an equal share instead of one download hogging everything
+> (flips live, no reconnect); the Status tab's exit-IP and speed-test
+> cards merged into one compact **Exit & speed** panel.
+>
 > v0.13.1 (Copilot code-review round): kill-switch hardening — the
 > black-hole port is owned BEFORE the proxy points at it, the proxy
 > hand-off is serialized with disconnect, a failed restore keeps the

### Commit: 1a9849f - backup(s21-release-prep): 2026-11-24T20:08:00Z (Z User, 2026-11-24)
diff --git a/Cargo.toml b/Cargo.toml
index e436401..c96f339 100755
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -18,7 +18,7 @@ members = ["crates/*"]
 resolver = "2"
 
 [workspace.package]
-version = "0.13.1"
+version = "0.14.0"
 
 [lib]
 name = "multitunnel"
diff --git a/README.md b/README.md
index beca1eb..580d5db 100755
--- a/README.md
+++ b/README.md
@@ -374,6 +374,31 @@ GUI, no new dependencies):
   color-scheme), re-checked every few seconds while the app runs. A
   survey of the multipath-bonding literature (BLEST, ECF, MP-QUIC…) and
   how it maps onto the scheduler shipped alongside in `docs/research/`.
+- **P0 reconnect fix + performance parity work + QoS + merged panel
+  (v0.14)**: adding a second server and switching to it while connected
+  no longer kills the tunnel until restart — in-flight proxy connections
+  are now tracked and force-closed when the drain grace expires (on
+  Windows their sockets used to pin the 10808/10809 local endpoints and
+  the next connect landed `ADDRINUSE` → `Failed`), the listeners retry a
+  busy bind for a few seconds instead of failing instantly, engine
+  config/log files carry an endpoint-identity hash so two profiles
+  sharing a tag can no longer overwrite each other's Xray config, and
+  profiles.json edits are serialized. Throughput/stability parity work:
+  TCP_NODELAY on every carrier/target leg (Nagle + delayed-ACK used to
+  stall each 16 KiB bonding frame behind a 40 ms ACK), cumulative ACKs
+  coalesced (8 frames / 20 ms, flushed by CLOSE and the 1 s heartbeat),
+  the carrier writer batches co-queued frames into one vectored write,
+  the send path drops two of its five per-frame copies, PONGs echo on
+  the carrier they arrived on (per-carrier RTT samples were
+  cross-contaminated), a probe can no longer quarantine the ONLY link
+  into a blackout (fail-open dial instead), and the background prober
+  now uses the user-configured probe target. Settings → Network gained
+  a **QoS fair share** toggle: connections are grouped by destination
+  and, when ≥2 groups race for a loaded tunnel, each gets an equal
+  share — one site/download can no longer dominate all bandwidth
+  (applies live, no reconnect; a single group or an idle tunnel runs at
+  full speed). The Status tab's exit-IP and speed-test cards merged into
+  one compact **Exit & speed** panel with both triggers side by side.
 - **Server editing, offline probes, kill switch, diagnostics (v0.13)**:
   every server card gained an **Edit** button — name, remark, address,
   port, UUID and the REALITY fields (SNI, fingerprint, public key, short

### Commit: 685e95c - backup(s21-features-qos-merged-panel): 2026-11-24T20:07:19Z (Z User, 2026-11-24)
diff --git a/crates/mt-app/src/app.rs b/crates/mt-app/src/app.rs
index 4cb0442..849c6f8 100755
--- a/crates/mt-app/src/app.rs
+++ b/crates/mt-app/src/app.rs
@@ -387,6 +387,8 @@ pub enum Message {
     SpeedTested(Result<mt_service::SpeedTestInfo, String>),
     /// Kill switch toggled (Settings → Network).
     KillSwitchToggled(bool),
+    /// S21 (feature o): the fair-share bandwidth scheduler toggle.
+    QosToggled(bool),
     /// Diagnostics mode toggled (Settings → Diagnostics).
     DiagnosticsToggled(bool),
     /// The diagnostics file path arrived (init / after a toggle).
@@ -676,6 +678,7 @@ impl App {
             kill_switch: false,
             diagnostics: false,
             speed_test_url: "http://speed.cloudflare.com/__down?bytes=25000000".to_string(),
+            qos_fair: false,
         };
         // S18: detect the OS appearance once so the System mode (the
         // stock default) starts honest.
@@ -1721,6 +1724,11 @@ impl App {
                 self.settings.kill_switch = v;
                 return self.save_settings(self.settings.clone());
             }
+            // S21 (feature o): flips live in the service (no reconnect).
+            Message::QosToggled(v) => {
+                self.settings.qos_fair = v;
+                return self.save_settings(self.settings.clone());
+            }
             Message::DiagnosticsToggled(v) => {
                 self.settings.diagnostics = v;
                 // Review fix: CHAIN the path refresh after the save — a
@@ -3256,6 +3264,21 @@ mod tests {
         assert!(app.logs.iter().any(|l| l.contains("speed test")));
     }
 
+    /// S21 (feature o): the QoS toggle updates the draft settings exactly
+    /// like the kill-switch twin (persistence through the service is
+    /// proven directly in real_service's qos_toggle test).
+    #[cfg(feature = "mock")]
+    #[tokio::test]
+    async fn s21_qos_toggle_updates_draft() {
+        let service = Arc::new(crate::mock::MockService::new());
+        let mut app = App::init_async(service, Flags::default()).await;
+        assert!(!app.settings.qos_fair);
+        drop(app.update(Message::QosToggled(true)));
+        assert!(app.settings.qos_fair);
+        drop(app.update(Message::QosToggled(false)));
+        assert!(!app.settings.qos_fair);
+    }
+
     /// S19: kill switch + diagnostics + speed-URL toggles update the draft
     /// settings (the save task is runtime-driven; persistence through the
     /// service is proven directly like the S15/S16 twins) and the view
diff --git a/crates/mt-app/src/mock.rs b/crates/mt-app/src/mock.rs
index 54fb875..38443a5 100755
--- a/crates/mt-app/src/mock.rs
+++ b/crates/mt-app/src/mock.rs
@@ -110,6 +110,7 @@ impl MockService {
             kill_switch: false,
             diagnostics: false,
             speed_test_url: "http://speed.cloudflare.com/__down?bytes=25000000".to_string(),
+            qos_fair: false,
         };
         let inner = Inner {
             state: ServiceState::Disconnected,
diff --git a/crates/mt-app/src/screens.rs b/crates/mt-app/src/screens.rs
index 1a2d355..1ab8b5e 100755
--- a/crates/mt-app/src/screens.rs
+++ b/crates/mt-app/src/screens.rs
@@ -468,37 +468,75 @@ pub fn status(app: &App) -> Element<'_> {
         .height(Length::Fixed(STATUS_GRAPH_HEIGHT));
     let _ = util::fmt_bps; // rate labels live in graph.rs; kept for chips/tests
 
-    // --- exit probe card (D18; S19 works offline too) ----------------------
+    // --- merged exit & speed panel (S21, user ask p) -------------------------
+    // The two separate cards each carried a title + subtitle + button +
+    // result rows with lots of air between them; ONE panel shares the
+    // header, puts both triggers side by side and stacks the result rows
+    // compactly. All state/messages are unchanged — a pure view merge
+    // inside the page scrollable (D24: nothing growable in the frame).
     let has_active = app.profiles.iter().any(|p| p.active);
-    let mut exit_col = column![
-        text("Exit identity")
-            .size(ts.title_m.size)
-            .color(s.on_surface),
-        text(if has_active {
-            "checks which IP the internet sees — connected (rides the live tunnel) or not \
-             (spawns a temporary probe tunnel)"
-        } else {
-            "checks which IP the internet sees — pick an active server first"
-        })
-        .size(ts.body_s.size)
-        .color(s.on_surface_variant),
-    ]
-    .spacing(6);
-    if app.exit_probing {
-        exit_col = exit_col.push(
+    // Both triggers live in ONE row (S21 merged panel). Explicit Element
+    // annotations: the two branches build different widget types and the
+    // row needs a single erased type.
+    let exit_ctl: Element = if app.exit_probing {
+        container(
             text("probing…")
                 .size(ts.body_m.size)
                 .color(s.on_surface_variant),
-        );
+        )
+        .width(Length::Fill)
+        .into()
     } else {
-        exit_col = exit_col.push(m3_button(
+        container(m3_button(
             "Check exit IP",
-            has_active,
+            has_active && !app.speed_testing,
             ButtonKind::Tonal,
             Message::ExitProbe,
             app,
-        ));
-    }
+        ))
+        .width(Length::Fill)
+        .into()
+    };
+    let speed_ctl: Element = if app.speed_testing {
+        container(
+            text("testing… (up to 30 s)")
+                .size(ts.body_m.size)
+                .color(s.on_surface_variant),
+        )
+        .width(Length::Fill)
+        .into()
+    } else {
+        container(m3_button(
+            "Run speed test",
+            has_active && !app.exit_probing,
+            ButtonKind::Tonal,
+            Message::SpeedTest,
+            app,
+        ))
+        .width(Length::Fill)
+        .into()
+    };
+    let mut diag_col = column![
+        text("Exit & speed")
+            .size(ts.title_m.size)
+            .color(s.on_surface),
+        text(if has_active {
+            "check which IP the internet sees and measure the download rate — the live \
+             tunnel when connected, a temporary probe tunnel otherwise (setup is \
+             excluded from the rate)"
+        } else {
+            "check which IP the internet sees and measure the download rate — pick an \
+             active server first"
+        })
+        .size(ts.body_s.size)
+        .color(s.on_surface_variant),
+    ]
+    .spacing(6)
+    .push(
+        row![exit_ctl, speed_ctl]
+            .spacing(12)
+            .align_y(iced::Alignment::Center),
+    );
     if let Some(res) = &app.exit {
         match res {
             Ok(info) => {
@@ -521,9 +559,12 @@ pub fn status(app: &App) -> Element<'_> {
                         s.on_error_container,
                     )
                 };
-                exit_col = exit_col
+                diag_col = diag_col
                     .push(
                         row![
+                            text("exit")
+                                .size(ts.label_m.size)
+                                .color(s.on_surface_variant),
                             text(info.exit_ip.clone())
                                 .size(ts.headline_s.size)
                                 .color(s.on_surface),
@@ -567,53 +608,23 @@ pub fn status(app: &App) -> Element<'_> {
                     );
             }
             Err(e) => {
-                exit_col = exit_col.push(
-                    text(format!("probe failed: {e}"))
+                diag_col = diag_col.push(
+                    text(format!("exit probe failed: {e}"))
                         .size(ts.body_s.size)
                         .color(s.error),
                 );
             }
         }
     }
-    let exit_card = container(exit_col.spacing(8).padding(16))
-        .style(t3::card(s, s.surface_container_low))
-        .width(Length::Fill);
-
-    // --- speed test card (S19) ---------------------------------------------
-    let mut speed_col = column![
-        text("Speed test").size(ts.title_m.size).color(s.on_surface),
-        text(if has_active {
-            "downloads a test payload through the active server — the live tunnel when \
-             connected, a temporary probe tunnel otherwise (single carrier; setup is \
-             excluded from the rate)"
-        } else {
-            "downloads a test payload through the active server — pick an active server first"
-        })
-        .size(ts.body_s.size)
-        .color(s.on_surface_variant),
-    ]
-    .spacing(6);
-    if app.speed_testing {
-        speed_col = speed_col.push(
-            text("testing… (up to 30 s)")
-                .size(ts.body_m.size)
-                .color(s.on_surface_variant),
-        );
-    } else {
-        speed_col = speed_col.push(m3_button(
-            "Run speed test",
-            has_active,
-            ButtonKind::Tonal,
-            Message::SpeedTest,
-            app,
-        ));
-    }
     if let Some(res) = &app.speed {
         match res {
             Ok(info) => {
-                speed_col = speed_col
+                diag_col = diag_col
                     .push(
                         row![
+                            text("speed")
+                                .size(ts.label_m.size)
+                                .color(s.on_surface_variant),
                             text(format!("{:.1} Mbps", info.mbps))
                                 .size(ts.headline_s.size)
                                 .color(s.on_surface),
@@ -659,7 +670,7 @@ pub fn status(app: &App) -> Element<'_> {
                     );
             }
             Err(e) => {
-                speed_col = speed_col.push(
+                diag_col = diag_col.push(
                     text(format!("speed test failed: {e}"))
                         .size(ts.body_s.size)
                         .color(s.error),
@@ -667,7 +678,7 @@ pub fn status(app: &App) -> Element<'_> {
             }
         }
     }
-    let speed_card = container(speed_col.spacing(8).padding(16))
+    let diag_card = container(diag_col.spacing(8).padding(16))
         .style(t3::card(s, s.surface_container_low))
         .width(Length::Fill);
 
@@ -828,8 +839,7 @@ pub fn status(app: &App) -> Element<'_> {
             .width(Length::Fill),
         );
     }
-    below = below.push(exit_card);
-    below = below.push(speed_card);
+    below = below.push(diag_card);
     below = below.push(bottom_card);
     let page_scroll = scrollable(container(below).width(Length::Fill))
         .width(Length::Fill)
@@ -2025,6 +2035,24 @@ pub fn settings(app: &App) -> Element<'_> {
         )
         .size(ts.body_s.size)
         .color(s.on_surface_variant),
+        // S21 (feature o): the fair-share bandwidth scheduler.
+        toggler(app.settings.qos_fair)
+            .label("QoS fair share (stop one app hogging all bandwidth)")
+            .size(28.0)
+            .on_toggle(Message::QosToggled)
+            .style(t3::switch(s)),
+        text(
+            "on: connections are grouped by the site they talk to; when two or \
+             more groups race for a loaded tunnel, each gets an equal share — \
+             a single download cannot starve everything else. One group alone \
+             (or an idle tunnel) runs at full speed. Applies immediately, no \
+             reconnect. off (default): every connection competes freely and \
+             one busy site can take all of it. Note: grouping is per \
+             destination — a download manager spreading over many servers \
+             counts as several groups.",
+        )
+        .size(ts.body_s.size)
+        .color(s.on_surface_variant),
         row![
             text("merge policy")
                 .size(ts.body_m.size)
diff --git a/crates/mt-app/src/theme.rs b/crates/mt-app/src/theme.rs
index e3c33ec..719c205 100755
--- a/crates/mt-app/src/theme.rs
+++ b/crates/mt-app/src/theme.rs
@@ -693,6 +693,7 @@ mod tests {
             kill_switch: false,
             diagnostics: false,
             speed_test_url: String::new(),
+            qos_fair: false,
         });
         assert_eq!(t.palette.seed(), 0x4285F4);
         assert!(!t.dark);
diff --git a/crates/mt-net/src/lib.rs b/crates/mt-net/src/lib.rs
index c41ad63..764831a 100755
--- a/crates/mt-net/src/lib.rs
+++ b/crates/mt-net/src/lib.rs
@@ -183,6 +183,7 @@ mod error;
 mod http;
 mod merge;
 mod outbound;
+mod qos;
 mod router;
 mod server;
 mod shutdown;
@@ -196,6 +197,7 @@ pub use merge::{
     CAPACITY_FLOOR_BPS, CONN_RESERVE_BPS, SATURATION_FRACTION,
 };
 pub use outbound::DirectOutbound;
+pub use qos::QosScheduler;
 pub use router::Router;
 pub use server::ProxyServer;
 pub use socks5::Socks5Outbound;
diff --git a/crates/mt-net/src/qos.rs b/crates/mt-net/src/qos.rs
new file mode 100644
index 0000000..61a2e5b
--- /dev/null
+++ b/crates/mt-net/src/qos.rs
@@ -0,0 +1,278 @@
+//! Per-destination fair-share bandwidth scheduler (S21, user ask o):
+//! "a QoS toggle that can be turned on to prevent a single application
+//! from dominating all the bandwidth".
+//!
+//! DESIGN (pick one and document it — the mt-net discipline):
+//! - Every proxied connection is one FLOW; flows are grouped by their
+//!   TARGET (`host:port`). A browser's many connections to one site form
+//!   one group; a download manager hitting one CDN also forms one group —
+//!   exactly the "one application hogs everything" shape the toggle is
+//!   about.
+//! - The 1 s ticker measures the tunnel's aggregate up+down rate and, when
+//!   ≥2 groups are active AND the aggregate is above
+//!   [`QOS_CONTENTION_FLOOR`], refills every group's token bucket with
+//!   `aggregate / groups` bytes. A greedy group is then throttled at each
+//!   read (its pump sleeps until the next refill) while everyone else
+//!   keeps their full share. Below the floor — or with a single active
+//!   group — buckets are UNLIMITED: one app alone never pays a
+//!   throughput tax, and idle tunnels are never shaped.
+//! - The pump acquires budget BEFORE each read (the read is capped by
+//!   `PUMP_BUF`), so over-reserve is bounded by one buffer and the OS
+//!   socket buffers; writes back-pressure naturally behind reads.
+//! - Live toggle: the enabled flag is an AtomicBool the service flips on
+//!   the settings toggle WITHOUT rebuilding the session (no reconnect —
+//!   unlike the diagnostics toggle, nothing here needs a data-plane
+//!   rebuild).
+//!
+//! NOT an app-level classifier: local flows arrive from 127.0.0.1 with
+//! per-connection ephemeral ports, so per-PROCESS attribution is not
+//! available at this layer; per-target grouping is the honest proxy-level
+//! approximation (documented in the Settings copy).
+
+use std::collections::HashMap;
+use std::sync::atomic::{AtomicBool, Ordering};
+use std::sync::Mutex;
+use std::time::Duration;
+
+/// Aggregate rate below which no shaping happens (bytes/s): shaping only
+/// engages when the tunnel is actually loaded — a 1 Mbps tunnel cannot
+/// "hog" anything.
+pub const QOS_CONTENTION_FLOOR: u64 = 2 * 1024 * 1024;
+/// How long a throttled pump sleeps between acquire attempts (the next
+/// refill lands within one 1 s tick).
+const QOS_RETRY: Duration = Duration::from_millis(20);
+/// How long a group with no active flows stays in the table (ticks it was
+/// last seen with a live flow).
+const GROUP_STALE_TICKS: u32 = 3;
+
+#[derive(Default)]
+struct GroupState {
+    /// Token budget for the CURRENT tick (bytes this group may still read).
+    tokens: u64,
+    /// Live flows in this group (the pump registers/unregisters).
+    flows: usize,
+    /// Ticks since the last refill where the group had no flows.
+    idle_ticks: u32,
+}
+
+/// Fair-share scheduler shared by the proxy pumps (see module docs).
+/// Cheap in the hot path: one AtomicBool load + a short mutex critical
+/// section per ≤16 KiB read.
+#[derive(Default)]
+pub struct QosScheduler {
+    enabled: AtomicBool,
+    groups: Mutex<HashMap<String, GroupState>>,
+}
+
+impl QosScheduler {
+    pub fn new(enabled: bool) -> Self {
+        Self {
+            enabled: AtomicBool::new(enabled),
+            groups: Mutex::new(HashMap::new()),
+        }
+    }
+
+    /// Whether shaping is currently ON.
+    pub fn enabled(&self) -> bool {
+        self.enabled.load(Ordering::SeqCst)
+    }
+
+    /// Live-toggle the scheduler (Settings toggle; no reconnect needed).
+    /// Turning it OFF also unblocks every waiting pump (they re-check the
+    /// flag each retry) and clears the group table.
+    pub fn set_enabled(&self, on: bool) {
+        self.enabled.store(on, Ordering::SeqCst);
+        if !on {
+            if let Ok(mut g) = self.groups.lock() {
+                g.clear();
+            }
+        }
+    }
+
+    /// A flow starts (tunnel entry).
+    pub fn register(&self, group: &str) {
+        if let Ok(mut g) = self.groups.lock() {
+            g.entry(group.to_string()).or_default().flows += 1;
+        }
+    }
+
+    /// A flow ends (tunnel exit).
+    pub fn unregister(&self, group: &str) {
+        if let Ok(mut g) = self.groups.lock() {
+            if let Some(state) = g.get_mut(group) {
+                state.flows = state.flows.saturating_sub(1);
+            }
+        }
+    }
+
+    /// 1 s tick: refill buckets from the measured aggregate rate. Called
+    /// from the proxy ticker with the window's up+down bytes/s.
+    pub fn tick(&self, up_bps: u64, down_bps: u64) {
+        let aggregate = up_bps.saturating_add(down_bps);
+        let Ok(mut g) = self.groups.lock() else {
+            return;
+        };
+        g.retain(|_, s| {
+            s.flows > 0 || {
+                s.idle_ticks += 1;
+                s.idle_ticks < GROUP_STALE_TICKS
+            }
+        });
+        let active: usize = g.values().filter(|s| s.flows > 0).count();
+        // Shaping engages ONLY under real contention: ≥2 groups racing for
+        // an aggregate above the floor. Otherwise everything is unlimited.
+        if active < 2 || aggregate < QOS_CONTENTION_FLOOR {
+            for s in g.values_mut() {
+                s.tokens = u64::MAX;
+            }
+            return;
+        }
+        let quantum = aggregate / active as u64;
+        for s in g.values_mut() {
+            s.tokens = quantum;
+        }
+    }
+
+    /// The pump's read gate: wait until this group has budget for one
+    /// buffer-sized read (or shaping is off / not contended). Blocks at
+    /// most until the next 1 s refill.
+    pub async fn acquire(&self, group: &str, budget: u64) {
+        loop {
+            if !self.enabled() {
+                return;
+            }
+            {
+                let Ok(mut g) = self.groups.lock() else {
+                    return; // poisoned lock: fail open, never stall traffic
+                };
+                let state = g.entry(group.to_string()).or_default();
+                if state.tokens >= budget {
+                    state.tokens -= budget;
+                    return;
+                }
+            }
+            tokio::time::sleep(QOS_RETRY).await;
+        }
+    }
+
+    /// Test/observability hook: the tokens currently available to a group.
+    #[cfg(test)]
+    pub(crate) fn tokens_of(&self, group: &str) -> u64 {
+        self.groups
+            .lock()
+            .map(|g| g.get(group).map(|s| s.tokens).unwrap_or(0))
+            .unwrap_or(0)
+    }
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+    use std::sync::Arc;
+
+    fn groups_active(s: &QosScheduler) -> usize {
+        s.groups
+            .lock()
+            .unwrap()
+            .values()
+            .filter(|g| g.flows > 0)
+            .count()
+    }
+
+    #[tokio::test]
+    async fn single_group_is_never_shaped() {
+        let s = QosScheduler::new(true);
+        s.register("a.example:443");
+        s.register("a.example:443");
+        assert_eq!(groups_active(&s), 1);
+        s.tick(100 * 1024 * 1024, 0); // 100 Mbps, 1 group
+                                      // Unlimited tokens: big acquires never block.
+        tokio::time::timeout(
+            Duration::from_millis(100),
+            s.acquire("a.example:443", 1024 * 1024),
+        )
+        .await
+        .expect("unlimited group must not block");
+    }
+
+    #[tokio::test]
+    async fn disabled_scheduler_never_blocks() {
+        let s = QosScheduler::new(false);
+        s.set_enabled(false);
+        s.register("a:1");
+        s.register("b:1");
+        s.tick(1024 * 1024 * 1024, 0);
+        tokio::time::timeout(Duration::from_millis(50), s.acquire("a:1", 1 << 30))
+            .await
+            .expect("disabled scheduler must not block");
+    }
+
+    #[tokio::test]
+    async fn contended_groups_get_a_fair_quantum() {
+        let s = QosScheduler::new(true);
+        s.register("greedy:443");
+        s.register("small:443");
+        // 80 MiB/s aggregate, 2 groups → 40 MiB/s each per tick.
+        let agg = 80 * 1024 * 1024;
+        s.tick(agg, 0);
+        let quantum = agg / 2;
+        assert!(
+            s.tokens_of("greedy:443") == quantum,
+            "each contended group gets aggregate/groups"
+        );
+        // The greedy group exhausts its quantum, then must WAIT for the
+        // next refill (bounded: below one tick + retry cadence).
+        let started = std::time::Instant::now();
+        // Acquire the full quantum in one 1 MiB step: the NEXT acquire of
+        // another quantum-sized budget must block (not resolve instantly).
+        tokio::time::timeout(Duration::from_millis(200), s.acquire("greedy:443", quantum))
+            .await
+            .expect("initial quantum is available");
+        let blocked = tokio::time::timeout(Duration::from_millis(60), async {
+            s.acquire("greedy:443", quantum).await;
+        })
+        .await;
+        assert!(blocked.is_err(), "exhausted group must block (no budget)");
+        // The small group keeps its own full quantum meanwhile.
+        tokio::time::timeout(Duration::from_millis(50), s.acquire("small:443", quantum))
+            .await
+            .expect("the other group still has its share");
+        assert!(started.elapsed() < Duration::from_secs(2));
+    }
+
+    #[tokio::test]
+    async fn idle_flow_groups_expire() {
+        let s = QosScheduler::new(true);
+        s.register("gone:80");
+        s.unregister("gone:80");
+        for _ in 0..GROUP_STALE_TICKS + 1 {
+            s.tick(0, 0);
+        }
+        assert!(
+            s.groups.lock().unwrap().is_empty(),
+            "flow-less groups are garbage-collected"
+        );
+    }
+
+    #[tokio::test]
+    async fn set_enabled_false_unblocks_waiters() {
+        let s = Arc::new(QosScheduler::new(true));
+        s.register("a:1");
+        s.register("b:1");
+        s.tick(80 * 1024 * 1024, 0);
+        let quantum = 40 * 1024 * 1024;
+        tokio::time::timeout(Duration::from_millis(200), s.acquire("a:1", quantum))
+            .await
+            .expect("first quantum ok");
+        let waiter = {
+            let s2 = Arc::clone(&s);
+            tokio::spawn(async move { s2.acquire("a:1", quantum).await })
+        };
+        tokio::time::sleep(Duration::from_millis(60)).await;
+        s.set_enabled(false); // live toggle must unblock the pump
+        tokio::time::timeout(Duration::from_secs(2), waiter)
+            .await
+            .expect("join")
+            .expect("waiter resolves after disable");
+    }
+}
diff --git a/crates/mt-net/src/server.rs b/crates/mt-net/src/server.rs
index 7d57ab9..b7e0cc2 100755
--- a/crates/mt-net/src/server.rs
+++ b/crates/mt-net/src/server.rs
@@ -68,6 +68,7 @@ use tokio::time::timeout;
 
 use crate::merge::{prober_loop, Aggregator};
 use crate::outbound::DirectOutbound;
+use crate::qos::QosScheduler;
 use crate::router::Router;
 use crate::shutdown::wait_for_shutdown;
 use crate::socks5::{self, rep};
@@ -190,6 +191,10 @@ pub struct ProxyServer {
     direct: Arc<dyn Outbound>,
     events: broadcast::Sender<ProxyEvent>,
     shared: Arc<Shared>,
+    /// S21 (feature o): optional fair-share scheduler. `None` = disabled
+    /// (all stock builders/tests); production attaches one when the
+    /// Settings toggle is on, and can live-flip the flag mid-session.
+    qos: Option<Arc<QosScheduler>>,
 }
 
 impl ProxyServer {
@@ -215,9 +220,23 @@ impl ProxyServer {
             direct: Arc::new(DirectOutbound::new()),
             events,
             shared: Arc::new(Shared::default()),
+            qos: None,
         }
     }
 
+    /// S21 (feature o): attach the fair-share scheduler (per-target group
+    /// shaping; see [`QosScheduler`]).
+    pub fn with_qos(mut self, qos: Arc<QosScheduler>) -> Self {
+        self.qos = Some(qos);
+        self
+    }
+
+    /// The attached fair-share scheduler, if any (the service live-flips
+    /// its enabled flag on the Settings toggle).
+    pub fn qos(&self) -> Option<Arc<QosScheduler>> {
+        self.qos.clone()
+    }
+
     /// Override the background probe interval.
     pub fn with_probe_interval(mut self, interval: Duration) -> Self {
         self.probe_interval = interval;
@@ -293,6 +312,7 @@ impl ProxyServer {
         background.push(tokio::spawn(tick_loop(
             Arc::clone(&self.shared),
             Arc::clone(&self.aggregator),
+            self.qos.clone(),
             self.events.clone(),
             shutdown.clone(),
         )));
@@ -609,6 +629,15 @@ impl ProxyServer {
         let down_count = Arc::new(AtomicU64::new(0));
         // S11: the per-link counter (up+down combined) the pumps bump.
         let link_counter = link_idx.and_then(|i| self.aggregator.link_bytes(i));
+        // S21 (feature o): flows group by TARGET for fair sharing — one
+        // application = one group regardless of how many connections it
+        // opens. Registered/unregistered around the pump lifetime so the
+        // ticker's contention math counts live groups only.
+        let group = target.to_string();
+        let qos = self.qos.as_ref().map(|q| {
+            q.register(&group);
+            (Arc::clone(q), group.clone())
+        });
         let (client_rd, client_wr) = tokio::io::split(client);
         let (up_rd, up_wr) = tokio::io::split(upstream);
         let pump_up = tokio::spawn(pump(
@@ -618,6 +647,7 @@ impl ProxyServer {
             Arc::clone(&up_count),
             Arc::clone(&self.shared.up_total),
             link_counter.clone(),
+            qos.clone(),
         ));
         let pump_down = tokio::spawn(pump(
             up_rd,
@@ -626,6 +656,7 @@ impl ProxyServer {
             Arc::clone(&down_count),
             Arc::clone(&self.shared.down_total),
             link_counter,
+            qos,
         ));
         // S21 P0 fix: the pumps are DETACHED children of this task — if a
         // shutdown force-close aborts THIS task, dropping their JoinHandles
@@ -643,6 +674,9 @@ impl ProxyServer {
         }
         let up = up_count.load(Ordering::Relaxed);
         let down = down_count.load(Ordering::Relaxed);
+        if let Some(q) = &self.qos {
+            q.unregister(&group);
+        }
         let dur_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
         tracing::debug!(target = %target, link, up, down, dur_ms, "connection closed");
         if let Some(idx) = link_idx {
@@ -668,6 +702,7 @@ async fn pump<R, W>(
     conn: Arc<AtomicU64>,
     global: Arc<AtomicU64>,
     per_link: Option<Arc<AtomicU64>>,
+    qos: Option<(Arc<QosScheduler>, String)>,
 ) -> std::io::Result<()>
 where
     R: tokio::io::AsyncRead + Unpin + Send + 'static,
@@ -679,6 +714,12 @@ where
     }
     let mut buf = vec![0u8; PUMP_BUF];
     loop {
+        // S21 (feature o): the fair-share gate — blocks this pump until
+        // the flow's target group has budget for one read. Cheap when
+        // shaping is off or uncontended (one atomic load + map lookup).
+        if let Some((sched, group)) = &qos {
+            sched.acquire(group, PUMP_BUF as u64).await;
+        }
         let n = rd.read(&mut buf).await?;
         if n == 0 {
             let _ = wr.shutdown().await;
@@ -769,6 +810,7 @@ async fn accept_loop(server: Arc<ProxyServer>, listener: TcpListener, kind: Inbo
 async fn tick_loop(
     shared: Arc<Shared>,
     aggregator: Arc<Aggregator>,
+    qos: Option<Arc<QosScheduler>>,
     events: broadcast::Sender<ProxyEvent>,
     mut shutdown: watch::Receiver<bool>,
 ) {
@@ -787,13 +829,17 @@ async fn tick_loop(
         last_tick = now;
         let up = shared.up_total.load(Ordering::Relaxed);
         let down = shared.down_total.load(Ordering::Relaxed);
-        let ev = ProxyEvent::Tick {
-            up_bps: up.saturating_sub(last_up),
-            down_bps: down.saturating_sub(last_down),
-        };
+        let up_bps = up.saturating_sub(last_up);
+        let down_bps = down.saturating_sub(last_down);
+        let ev = ProxyEvent::Tick { up_bps, down_bps };
         last_up = up;
         last_down = down;
         aggregator.tick_loads(dt).await;
+        // S21 (feature o): the fair-share refill runs on the same 1 s
+        // cadence, off the measured aggregate (no extra sampling).
+        if let Some(q) = &qos {
+            q.tick(up_bps, down_bps);
+        }
         let _ = events.send(ev);
     }
 }
diff --git a/crates/mt-service/src/lib.rs b/crates/mt-service/src/lib.rs
index 321af51..c850187 100755
--- a/crates/mt-service/src/lib.rs
+++ b/crates/mt-service/src/lib.rs
@@ -172,6 +172,14 @@ pub struct SettingsView {
     /// payload needs no TLS of its own (D18's zero-TLS rule).
     #[serde(default = "default_speed_test_url")]
     pub speed_test_url: String,
+    /// S21 (feature o): fair-share bandwidth scheduler ("QoS") — when ON,
+    /// the proxy groups flows by TARGET and, under contention (≥2 groups
+    /// racing for an aggregate above the floor), caps each group at its
+    /// `aggregate / groups` share, so no single application/site can
+    /// dominate all the bandwidth. Flips live (no reconnect). Absent in
+    /// older files → false (D8 version-free migration by serde default).
+    #[serde(default)]
+    pub qos_fair: bool,
 }
 
 /// S18: how the app resolves light vs dark appearance. Serialized as
@@ -885,6 +893,7 @@ mod close_to_tray_migration_tests {
                 kill_switch: false,
                 diagnostics: false,
                 speed_test_url: default_speed_test_url(),
+                qos_fair: false,
             }
         }
     }
@@ -1029,4 +1038,27 @@ mod s19_migration_tests {
         assert!(back.diagnostics);
         assert_eq!(back.speed_test_url, "http://127.0.0.1:9/down");
     }
+
+    /// S21 (feature o): settings.json written by v0.13 and older (no
+    /// `qos_fair` key) loads with the scheduler OFF (D8 version-free
+    /// migration via serde default), and an explicit choice round-trips.
+    #[test]
+    fn s21_qos_fair_defaults_and_roundtrip() {
+        let json = r#"{
+            "theme_seed": 42,
+            "dark_mode": false,
+            "system_proxy": true,
+            "merge_policy": "balanced",
+            "autostart": false,
+            "log_level": "info"
+        }"#;
+        let v: SettingsView = serde_json::from_str(json).expect("old settings load");
+        assert!(!v.qos_fair, "absent key → scheduler off");
+        let mut v = SettingsView::default_settings_for_tests();
+        v.qos_fair = true;
+        let json = serde_json::to_string(&v).expect("serialize");
+        assert!(json.contains("\"qos_fair\":true"));
+        let back: SettingsView = serde_json::from_str(&json).expect("reload");
+        assert!(back.qos_fair);
+    }
 }
diff --git a/src/real_service.rs b/src/real_service.rs
index 3e222ff..b8a3bab 100755
--- a/src/real_service.rs
+++ b/src/real_service.rs
@@ -609,6 +609,7 @@ impl Default for SettingsFile {
                 kill_switch: false,
                 diagnostics: false,
                 speed_test_url: default_speed_test_url(),
+                qos_fair: false,
             },
             // Stock rules = the LAN/private bypass (S6-REV-C P1-4): a fresh
             // settings.json must carry private-CIDR → direct rules, mirroring
@@ -1112,7 +1113,13 @@ impl RealService {
             };
         let server = Arc::new(
             ProxyServer::new(socks, http, links.clone(), router, policy)
-                .with_probe_target(probe_target.clone()),
+                .with_probe_target(probe_target.clone())
+                // S21 (feature o): the fair-share scheduler is ALWAYS
+                // attached (initialized from settings) so the Settings
+                // toggle can flip it live without rebuilding the session.
+                .with_qos(Arc::new(mt_net::QosScheduler::new(
+                    settings.settings.qos_fair,
+                ))),
         );
         let proxy_events = server.subscribe();
 
@@ -1987,6 +1994,10 @@ impl MtService for RealService {
         // on them AFTER the (successful) persist below.
         let diagnostics_changed = file.settings.diagnostics != settings.diagnostics;
         let kill_switch_disabled = file.settings.kill_switch && !settings.kill_switch;
+        // S21 (feature o): the QoS fair-share scheduler flips LIVE on the
+        // attached session — no reconnect (nothing in the data plane
+        // changes; the scheduler's enabled flag is atomic).
+        let qos_changed = file.settings.qos_fair != settings.qos_fair;
         // S20 (Copilot round 2): the engines only pick up the debug log
         // level at SPAWN time, so enabling diagnostics while CONNECTED
         // must reconnect — otherwise the file silently misses the engine
@@ -2057,6 +2068,28 @@ impl MtService for RealService {
             self.disconnect().await;
             self.connect().await;
         }
+        // S21 (feature o): live QoS flip — applies to the attached session
+        // immediately (the pump gate re-checks the flag per read) and to
+        // every future session through settings.json.
+        if qos_changed {
+            let sched = lock(&self.core.session)
+                .as_ref()
+                .and_then(|s| s.server.qos());
+            match sched {
+                Some(q) => {
+                    q.set_enabled(settings.qos_fair);
+                    self.core.emit(ServiceEvent::LogLine(if settings.qos_fair {
+                        "INFO  QoS fair share ON — no single site/application can hog the bandwidth anymore".into()
+                    } else {
+                        "INFO  QoS fair share OFF".into()
+                    }));
+                }
+                None => {
+                    // No live session: the flag persists; the next connect
+                    // attaches a scheduler initialized from settings.json.
+                }
+            }
+        }
         // S19: turning the kill switch off is an explicit escape hatch for
         // an engaged black-hole (the give_up path keeps the proxy blocked
         // while the switch stays on). S20 (Copilot round 2): under the
@@ -2908,6 +2941,7 @@ mod tests {
                 kill_switch: false,
                 diagnostics: false,
                 speed_test_url: default_speed_test_url(),
+                qos_fair: false,
             },
             rules: Vec::new(),
             socks_port: 0,
@@ -4036,6 +4070,51 @@ mod tests {
         assert_eq!(&buf, b"ping");
     }
 
+    /// S21 (feature o): the QoS toggle persists, flips the LIVE session's
+    /// scheduler WITHOUT a reconnect, and is visible to a fresh service
+    /// instance (settings.json round-trip).
+    #[tokio::test]
+    async fn qos_toggle_flips_live_without_reconnect_and_persists() {
+        let echo = spawn_echo().await;
+        let paths = scratch("qos");
+        write_settings(&paths, echo.port(), false);
+        let svc = RealService::with(paths.clone(), Arc::new(FakeFactory { echo, fail: false }));
+        let view = svc.import_link(VLESS_REALITY).await.expect("import");
+        svc.set_active_profile(&view.tag).await.expect("activate");
+        svc.connect().await;
+        assert_eq!(svc.status().await.state, ServiceState::Connected);
+
+        // Flip ON: persisted, live scheduler enabled, session untouched.
+        let mut settings = svc.settings().await;
+        settings.qos_fair = true;
+        svc.set_settings(settings).await.expect("save");
+        assert!(svc.settings().await.qos_fair, "persisted");
+        let sched = lock(&svc.core.session)
+            .as_ref()
+            .and_then(|s| s.server.qos())
+            .expect("the live session carries the scheduler");
+        assert!(sched.enabled(), "live flip applies without a rebuild");
+        assert_eq!(
+            svc.status().await.state,
+            ServiceState::Connected,
+            "the QoS toggle must NOT reconnect"
+        );
+
+        // Flip OFF: live disable.
+        let mut settings = svc.settings().await;
+        settings.qos_fair = false;
+        svc.set_settings(settings).await.expect("save");
+        assert!(!svc.settings().await.qos_fair);
+        assert!(!sched.enabled(), "live disable applies");
+
+        // Flip ON again and prove persistence across instances.
+        let mut settings = svc.settings().await;
+        settings.qos_fair = true;
+        svc.set_settings(settings).await.expect("save");
+        let svc2 = RealService::with(paths, Arc::new(FakeFactory { echo, fail: false }));
+        assert!(svc2.settings().await.qos_fair, "survives a restart");
+    }
+
     #[tokio::test]
     async fn factory_failure_reports_failed_with_no_leftover_server() {
         let paths = scratch("fail");
@@ -4092,6 +4171,7 @@ mod tests {
             kill_switch: false,
             diagnostics: false, // never true here: the sink is process-global
             speed_test_url: "http://speed.cloudflare.com/__down?bytes=25000000".to_string(),
+            qos_fair: true, // round-trips a non-default value
         };
         svc.set_settings(view.clone()).await.expect("save settings");
         assert_eq!(svc.settings().await, view);

( PROJECT_MEMORY.md's own diff is elided here — the durable
  sections above are hand-maintained; see harness/memory/LEDGER.md
  for the full stage history. )
```

