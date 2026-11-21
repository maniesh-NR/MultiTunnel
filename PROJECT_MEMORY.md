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
### Commit: 666bc16 - backup(s19-ci-hardening): 2026-11-21T14:58:33Z (Z User, 2026-11-21)
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index ae089af..14882ff 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "001d4e0 2026-11-21T13:50:09Z",
+  "as_of": "702293b 2026-11-21T14:58:33Z",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 270,
-  "edge_count": 277,
+  "node_count": 273,
+  "edge_count": 279,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -535,6 +535,26 @@
         }
       ]
     },
+    {
+      "id": "commit:3da17ad",
+      "type": "commit",
+      "title": "backup(s19-review-fixes): 2026-11-21T13:51:08Z",
+      "tags": [
+        "2026-11-21"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "3da17ad"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s19-review-fixes): 2026-11-21T13:51:08Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:40ee65c",
       "type": "commit",
@@ -1015,6 +1035,26 @@
         }
       ]
     },
+    {
+      "id": "commit:702293b",
+      "type": "commit",
+      "title": "s19: runner-flake hardening — mt-bond integration tests carrier_death_mid_stream + domain_targets_dial wrapped in hard deadlines (90 s / 60 s). Evidence: today's ubuntu CI leg failed on carrier_death (the same-commit PR + release runs green, local 466/0 repeatedly — the documented S17/S18 runner flake, third occurrence) and the rerun leg then WEDGED 54 minutes inside domain_targets_dial AFTER carrier_death passed (log captured: harness/memory/agent-reports/s19-ci-wedge-job-136868435499.log — 'running 6 tests', five ok, domain_targets 'over 60 seconds', cargo + integration orphan killed at cancel). A deadline converts the silent hourly hang into a loud one-minute failure with the LEDGER pointer in the message; zero behavior change to the code under test (bodies untouched inside the timeout wrapper). Local: mt-bond 32+6/0, clippy clean.",
+      "tags": [
+        "2026-11-21"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "702293b"
+        }
+      ],
+      "passages": [
+        {
+          "text": "s19: runner-flake hardening — mt-bond integration tests carrier_death_mid_stream + domain_targets_dial wrapped in hard deadlines (90 s / 60 s). Evidence: today's ubuntu CI leg failed on carrier_death (the same-commit PR + release runs green, local 466/0 repeatedly — the documented S17/S18 runner flake, third occurrence) and the rerun leg then WEDGED 54 minutes inside domain_targets_dial AFTER carrier_death passed (log captured: harness/memory/agent-reports/s19-ci-wedge-job-136868435499.log — 'running 6 tests', five ok, domain_targets 'over 60 seconds', cargo + integration orphan killed at cancel). A deadline converts the silent hourly hang into a loud one-minute failure with the LEDGER pointer in the message; zero behavior change to the code under test (bodies untouched inside the timeout wrapper). Local: mt-bond 32+6/0, clippy clean.",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:7073762",
       "type": "commit",
@@ -5512,6 +5552,26 @@
         }
       ]
     },
+    {
+      "id": "release:v0.13.0",
+      "type": "release",
+      "title": "Release v0.13.0 (tagged 3da17ad)",
+      "tags": [
+        "v0.13.0"
+      ],
+      "sources": [
+        {
+          "file": "git:tags/v0.13.0",
+          "anchor": "3da17ad"
+        }
+      ],
+      "passages": [
+        {
+          "text": "Release v0.13.0: tag points at commit 3da17ad.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "release:v0.2.0",
       "type": "release",
@@ -7884,6 +7944,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s2)"
     },
+    {
+      "src": "commit:3da17ad",
+      "dst": "ledger:s19",
+      "rel": "part_of",
+      "evidence": "subject stage label (s19)"
+    },
     {
       "src": "commit:40ee65c",
       "dst": "ledger:s5",
@@ -9066,6 +9132,12 @@
       "rel": "built_from",
       "evidence": "git tag v0.12.0"
     },
+    {
+      "src": "release:v0.13.0",
+      "dst": "commit:3da17ad",
+      "rel": "built_from",
+      "evidence": "git tag v0.13.0"
+    },
     {
       "src": "release:v0.2.0",
       "dst": "commit:06881a6",

### Commit: 702293b - s19: runner-flake hardening — mt-bond integration tests carrier_death_mid_stream + domain_targets_dial wrapped in hard deadlines (90 s / 60 s). Evidence: today's ubuntu CI leg failed on carrier_death (the same-commit PR + release runs green, local 466/0 repeatedly — the documented S17/S18 runner flake, third occurrence) and the rerun leg then WEDGED 54 minutes inside domain_targets_dial AFTER carrier_death passed (log captured: harness/memory/agent-reports/s19-ci-wedge-job-136868435499.log — 'running 6 tests', five ok, domain_targets 'over 60 seconds', cargo + integration orphan killed at cancel). A deadline converts the silent hourly hang into a loud one-minute failure with the LEDGER pointer in the message; zero behavior change to the code under test (bodies untouched inside the timeout wrapper). Local: mt-bond 32+6/0, clippy clean. (Z User, 2026-11-21)
diff --git a/crates/mt-bond/tests/integration.rs b/crates/mt-bond/tests/integration.rs
index f823e4b..31f0513 100755
--- a/crates/mt-bond/tests/integration.rs
+++ b/crates/mt-bond/tests/integration.rs
@@ -253,66 +253,74 @@ async fn carrier_death_mid_stream_fails_over_without_data_loss() {
         }
     }
 
-    let (daemon_addr, _daemon) = spawn_daemon().await;
-    let echo = spawn_echo().await;
-
-    let held: Arc<std::sync::Mutex<Vec<std::net::TcpStream>>> =
-        Arc::new(std::sync::Mutex::new(Vec::new()));
-    let wifi: Arc<dyn Outbound> = Arc::new(HoldingLink {
-        name: "Wi-Fi",
-        daemon: daemon_addr,
-        held: Arc::clone(&held),
-    });
-    let lte: Arc<dyn Outbound> = Arc::new(FakeEngineLink {
-        name: "LTE",
-        daemon: daemon_addr,
-        dials: Arc::new(AtomicUsize::new(0)),
-        fail_next: Arc::new(AtomicBool::new(false)),
-    });
-    let bond = BondOutbound::new(vec![wifi, lte], merge_target(daemon_addr));
-    bond.ensure_carriers(Duration::from_secs(5))
-        .await
-        .expect("carriers established");
-
-    let payload = blob(512 * 1024, 0x5A);
-    let mut stream = bond.connect(&tcp_target(echo)).await.expect("connect");
-
-    // Write the first half, let it spread over both carriers, then kill
-    // every Wi-Fi carrier socket (FIN → both ends mark the carrier dead).
-    let half = payload.len() / 2;
-    stream
-        .write_all(&payload[..half])
-        .await
-        .expect("first half");
-    tokio::time::sleep(Duration::from_millis(250)).await;
-    {
-        let mut sockets = held.lock().unwrap();
-        assert!(!sockets.is_empty(), "Wi-Fi carrier socket held");
-        for s in sockets.drain(..) {
-            let _ = s.shutdown(StdShutdown::Both);
+    // Runner-flake hardening (S19, LEDGER 2026-11-21): this test failed
+    // or wedged on ubuntu runners three documented times (green locally,
+    // green in sibling runs of the SAME commit). A hard deadline turns a
+    // silent 54-minute wedge into a loud one-minute failure.
+    tokio::time::timeout(Duration::from_secs(90), async {
+        let (daemon_addr, _daemon) = spawn_daemon().await;
+        let echo = spawn_echo().await;
+
+        let held: Arc<std::sync::Mutex<Vec<std::net::TcpStream>>> =
+            Arc::new(std::sync::Mutex::new(Vec::new()));
+        let wifi: Arc<dyn Outbound> = Arc::new(HoldingLink {
+            name: "Wi-Fi",
+            daemon: daemon_addr,
+            held: Arc::clone(&held),
+        });
+        let lte: Arc<dyn Outbound> = Arc::new(FakeEngineLink {
+            name: "LTE",
+            daemon: daemon_addr,
+            dials: Arc::new(AtomicUsize::new(0)),
+            fail_next: Arc::new(AtomicBool::new(false)),
+        });
+        let bond = BondOutbound::new(vec![wifi, lte], merge_target(daemon_addr));
+        bond.ensure_carriers(Duration::from_secs(5))
+            .await
+            .expect("carriers established");
+
+        let payload = blob(512 * 1024, 0x5A);
+        let mut stream = bond.connect(&tcp_target(echo)).await.expect("connect");
+
+        // Write the first half, let it spread over both carriers, then kill
+        // every Wi-Fi carrier socket (FIN → both ends mark the carrier dead).
+        let half = payload.len() / 2;
+        stream
+            .write_all(&payload[..half])
+            .await
+            .expect("first half");
+        tokio::time::sleep(Duration::from_millis(250)).await;
+        {
+            let mut sockets = held.lock().unwrap();
+            assert!(!sockets.is_empty(), "Wi-Fi carrier socket held");
+            for s in sockets.drain(..) {
+                let _ = s.shutdown(StdShutdown::Both);
+            }
         }
-    }
-    // The write side must still work through the surviving carrier.
-    stream
-        .write_all(&payload[half..])
-        .await
-        .expect("second half");
-    stream.shutdown().await.expect("shutdown");
-
-    let mut received = Vec::with_capacity(payload.len());
-    let _ = stream.read_to_end(&mut received).await;
-    assert_eq!(
-        received.len(),
-        payload.len(),
-        "failover must not lose bytes (got {}/{})",
-        received.len(),
-        payload.len()
-    );
-    assert_eq!(received, payload, "failover integrity");
-    // The respawn must have re-dialed the Wi-Fi carrier.
-    let snap = bond.carrier_snapshot();
-    assert_eq!(snap.len(), 2, "both slots still present");
-    bond.shutdown();
+        // The write side must still work through the surviving carrier.
+        stream
+            .write_all(&payload[half..])
+            .await
+            .expect("second half");
+        stream.shutdown().await.expect("shutdown");
+
+        let mut received = Vec::with_capacity(payload.len());
+        let _ = stream.read_to_end(&mut received).await;
+        assert_eq!(
+            received.len(),
+            payload.len(),
+            "failover must not lose bytes (got {}/{})",
+            received.len(),
+            payload.len()
+        );
+        assert_eq!(received, payload, "failover integrity");
+        // The respawn must have re-dialed the Wi-Fi carrier.
+        let snap = bond.carrier_snapshot();
+        assert_eq!(snap.len(), 2, "both slots still present");
+        bond.shutdown();
+    })
+    .await
+    .expect("deadline: carrier_death wedged (documented runner flake — see LEDGER)");
 }
 
 #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
@@ -345,26 +353,33 @@ async fn refused_target_surfaces_as_connection_reset() {
 
 #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
 async fn domain_targets_dial_through_the_daemon() {
-    let (daemon_addr, _daemon) = spawn_daemon().await;
-    let echo = spawn_echo().await;
-    let (links, _a, _b) = bond_links(daemon_addr);
-    let bond = BondOutbound::new(links, merge_target(daemon_addr));
-    bond.ensure_carriers(Duration::from_secs(5))
-        .await
-        .expect("carriers established");
-
-    // localhost:port resolves inside the daemon's dial_target.
-    let target = TargetAddr {
-        host: Host::Domain("localhost".to_string()),
-        port: echo.port(),
-    };
-    let mut stream = bond.connect(&target).await.expect("connect");
-    stream.write_all(b"hello bond").await.expect("write");
-    stream.shutdown().await.expect("shutdown");
-    let mut received = Vec::new();
-    stream.read_to_end(&mut received).await.expect("read");
-    assert_eq!(received, b"hello bond");
-    bond.shutdown();
+    // Runner-flake hardening (S19, LEDGER 2026-11-21): wedged >54 min on
+    // an ubuntu runner while green locally and in the sibling run of the
+    // same commit. Deadline: loud failure instead of an hourly hang.
+    tokio::time::timeout(Duration::from_secs(60), async {
+        let (daemon_addr, _daemon) = spawn_daemon().await;
+        let echo = spawn_echo().await;
+        let (links, _a, _b) = bond_links(daemon_addr);
+        let bond = BondOutbound::new(links, merge_target(daemon_addr));
+        bond.ensure_carriers(Duration::from_secs(5))
+            .await
+            .expect("carriers established");
+
+        // localhost:port resolves inside the daemon's dial_target.
+        let target = TargetAddr {
+            host: Host::Domain("localhost".to_string()),
+            port: echo.port(),
+        };
+        let mut stream = bond.connect(&target).await.expect("connect");
+        stream.write_all(b"hello bond").await.expect("write");
+        stream.shutdown().await.expect("shutdown");
+        let mut received = Vec::new();
+        stream.read_to_end(&mut received).await.expect("read");
+        assert_eq!(received, b"hello bond");
+        bond.shutdown();
+    })
+    .await
+    .expect("deadline: domain_targets wedged (documented runner flake — see LEDGER)");
 }
 
 #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
diff --git a/harness/memory/agent-reports/s19-ci-wedge-job-136868435499.log b/harness/memory/agent-reports/s19-ci-wedge-job-136868435499.log
new file mode 100644
index 0000000..572db2e
--- /dev/null
+++ b/harness/memory/agent-reports/s19-ci-wedge-job-136868435499.log
@@ -0,0 +1,529 @@
+﻿2026-11-21T13:59:51.9101908Z Current runner version: '2.338.2'
+2026-11-21T13:59:51.9127233Z ##[group]Runner Image Provisioner
+2026-11-21T13:59:51.9128207Z Hosted Compute Agent
+2026-11-21T13:59:51.9128804Z Version: 20261111.635
+2026-11-21T13:59:51.9129537Z Commit: b8a594ea514ff44cadb5342986c68b1eac6e22ed
+2026-11-21T13:59:51.9130320Z Build Date: 2026-11-11T01:49:01Z
+2026-11-21T13:59:51.9130988Z Worker ID: {5974ec8c-a4ac-4e84-97a7-db3d7e816918}
+2026-11-21T13:59:51.9131879Z Region: westus2
+2026-11-21T13:59:51.9132533Z Cloud: Azure
+2026-11-21T13:59:51.9133114Z ##[endgroup]
+2026-11-21T13:59:51.9134651Z ##[group]Operating System
+2026-11-21T13:59:51.9135294Z Ubuntu
+2026-11-21T13:59:51.9136149Z 26.04.1
+2026-11-21T13:59:51.9136682Z LTS
+2026-11-21T13:59:51.9137181Z ##[endgroup]
+2026-11-21T13:59:51.9137890Z ##[group]Runner Image
+2026-11-21T13:59:51.9138489Z Image: ubuntu-26.04
+2026-11-21T13:59:51.9139097Z Version: 20261116.202.1
+2026-11-21T13:59:51.9140357Z Included Software: https://github.com/actions/runner-images/blob/ubuntu26/20261116.202/images/ubuntu/Ubuntu2604-Readme.md
+2026-11-21T13:59:51.9142042Z Image Release: https://github.com/actions/runner-images/releases/tag/ubuntu26%2F20261116.202
+2026-11-21T13:59:51.9143027Z ##[endgroup]
+2026-11-21T13:59:51.9144229Z ##[group]GITHUB_TOKEN Permissions
+2026-11-21T13:59:51.9146424Z Contents: read
+2026-11-21T13:59:51.9147531Z Metadata: read
+2026-11-21T13:59:51.9148107Z Packages: read
+2026-11-21T13:59:51.9148649Z ##[endgroup]
+2026-11-21T13:59:51.9151122Z Secret source: Actions
+2026-11-21T13:59:51.9152113Z Cache mode: write
+2026-11-21T13:59:51.9153128Z Prepare workflow directory
+2026-11-21T13:59:51.9550369Z Prepare all required actions
+2026-11-21T13:59:51.9605641Z Getting action download info
+2026-11-21T13:59:52.2827935Z Download action repository 'actions/checkout@v4' (SHA:11d5960a326750d5838078e36cf38b85af677262)
+2026-11-21T13:59:52.3963785Z Download action repository 'dtolnay/rust-toolchain@stable' (SHA:39008827e6b4319a349e9a42dfc90579dd25f1ca)
+2026-11-21T13:59:52.5479947Z Download action repository 'Swatinem/rust-cache@v2' (SHA:d1527c8f67ef9af357290475d4af4b0fab8f12e0)
+2026-11-21T13:59:52.9003106Z Complete job name: test (ubuntu-latest)
+2026-11-21T13:59:52.9889035Z ##[group]Run actions/checkout@v4
+2026-11-21T13:59:52.9889925Z with:
+2026-11-21T13:59:52.9890382Z   repository: maniesh-NR/MultiTunnel
+2026-11-21T13:59:52.9894366Z   token: ***
+2026-11-21T13:59:52.9894795Z   ssh-strict: true
+2026-11-21T13:59:52.9895239Z   ssh-user: git
+2026-11-21T13:59:52.9895681Z   persist-credentials: true
+2026-11-21T13:59:52.9896338Z   clean: true
+2026-11-21T13:59:52.9896787Z   sparse-checkout-cone-mode: true
+2026-11-21T13:59:52.9897299Z   fetch-depth: 1
+2026-11-21T13:59:52.9897722Z   fetch-tags: false
+2026-11-21T13:59:52.9898155Z   show-progress: true
+2026-11-21T13:59:52.9898594Z   lfs: false
+2026-11-21T13:59:52.9899011Z   submodules: false
+2026-11-21T13:59:52.9899452Z   set-safe-directory: true
+2026-11-21T13:59:52.9899946Z   allow-unsafe-pr-checkout: false
+2026-11-21T13:59:52.9900714Z ##[endgroup]
+2026-11-21T13:59:53.2529280Z Syncing repository: maniesh-NR/MultiTunnel
+2026-11-21T13:59:53.2531823Z ##[group]Getting Git version info
+2026-11-21T13:59:53.2533133Z Working directory is '/home/runner/work/MultiTunnel/MultiTunnel'
+2026-11-21T13:59:53.2534904Z [command]/usr/bin/git version
+2026-11-21T13:59:53.2554037Z git version 2.56.1
+2026-11-21T13:59:53.2578370Z ##[endgroup]
+2026-11-21T13:59:53.2603065Z Temporarily overriding HOME='/home/runner/work/_temp/44a49762-d021-4870-9952-e0cc49575af3' before making global git config changes
+2026-11-21T13:59:53.2605400Z Adding repository directory to the temporary git global config as a safe directory
+2026-11-21T13:59:53.2609618Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/MultiTunnel/MultiTunnel
+2026-11-21T13:59:53.2675253Z Deleting the contents of '/home/runner/work/MultiTunnel/MultiTunnel'
+2026-11-21T13:59:53.2680408Z ##[group]Initializing the repository
+2026-11-21T13:59:53.2685446Z [command]/usr/bin/git init /home/runner/work/MultiTunnel/MultiTunnel
+2026-11-21T13:59:53.2793164Z hint: Using 'master' as the name for the initial branch. This default branch name
+2026-11-21T13:59:53.2795517Z hint: will change to "main" in Git 3.0. To configure the initial branch name
+2026-11-21T13:59:53.2797920Z hint: to use in all of your new repositories, which will suppress this warning,
+2026-11-21T13:59:53.2799623Z hint: call:
+2026-11-21T13:59:53.2800686Z hint:
+2026-11-21T13:59:53.2801972Z hint: 	git config --global init.defaultBranch <name>
+2026-11-21T13:59:53.2803261Z hint:
+2026-11-21T13:59:53.2804491Z hint: Names commonly chosen instead of 'master' are 'main', 'trunk' and
+2026-11-21T13:59:53.2806561Z hint: 'development'. The just-created branch can be renamed via this command:
+2026-11-21T13:59:53.2808040Z hint:
+2026-11-21T13:59:53.2809092Z hint: 	git branch -m <name>
+2026-11-21T13:59:53.2810344Z hint:
+2026-11-21T13:59:53.2811566Z hint: Disable this message with "git config set advice.defaultBranchName false"
+2026-11-21T13:59:53.2813601Z Initialized empty Git repository in /home/runner/work/MultiTunnel/MultiTunnel/.git/
+2026-11-21T13:59:53.2818331Z [command]/usr/bin/git remote add origin https://github.com/maniesh-NR/MultiTunnel
+2026-11-21T13:59:53.2874085Z ##[endgroup]
+2026-11-21T13:59:53.2875520Z ##[group]Disabling automatic garbage collection
+2026-11-21T13:59:53.2877586Z [command]/usr/bin/git config --local gc.auto 0
+2026-11-21T13:59:53.2915630Z ##[endgroup]
+2026-11-21T13:59:53.2917053Z ##[group]Setting up auth
+2026-11-21T13:59:53.2923400Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
+2026-11-21T13:59:53.2959465Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
+2026-11-21T13:59:53.3440662Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
+2026-11-21T13:59:53.3476289Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
+2026-11-21T13:59:53.3757802Z [command]/usr/bin/git config --local --name-only --get-regexp ^includeIf\.gitdir:
+2026-11-21T13:59:53.3793402Z [command]/usr/bin/git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url
+2026-11-21T13:59:53.4073404Z [command]/usr/bin/git config --local http.https://github.com/.extraheader AUTHORIZATION: basic ***
+2026-11-21T13:59:53.4112751Z ##[endgroup]
+2026-11-21T13:59:53.4113551Z ##[group]Fetching the repository
+2026-11-21T13:59:53.4124514Z [command]/usr/bin/git -c protocol.version=2 fetch --no-tags --prune --no-recurse-submodules --depth=1 origin +3da17ad7bf461b6c581a940d57ac2163a85f6bc8:refs/remotes/origin/main
+2026-11-21T13:59:53.8778849Z From https://github.com/maniesh-NR/MultiTunnel
+2026-11-21T13:59:53.8780422Z  * [new ref]         3da17ad7bf461b6c581a940d57ac2163a85f6bc8 -> origin/main
+2026-11-21T13:59:53.8787934Z ##[endgroup]
+2026-11-21T13:59:53.8790043Z ##[group]Determining the checkout info
+2026-11-21T13:59:53.8792233Z ##[endgroup]
+2026-11-21T13:59:53.8796424Z [command]/usr/bin/git sparse-checkout disable
+2026-11-21T13:59:53.8853159Z [command]/usr/bin/git config --local --unset-all extensions.worktreeConfig
+2026-11-21T13:59:53.8887982Z ##[group]Checking out the ref
+2026-11-21T13:59:53.8891373Z [command]/usr/bin/git checkout --progress --force -B main refs/remotes/origin/main
+2026-11-21T13:59:53.9253872Z Switched to a new branch 'main'
+2026-11-21T13:59:53.9259462Z branch 'main' set up to track 'origin/main'.
+2026-11-21T13:59:53.9268296Z ##[endgroup]
+2026-11-21T13:59:53.9319232Z [command]/usr/bin/git log -1 --format=%H
+2026-11-21T13:59:53.9344964Z 3da17ad7bf461b6c581a940d57ac2163a85f6bc8
+2026-11-21T13:59:53.9877950Z ##[group]Run dtolnay/rust-toolchain@stable
+2026-11-21T13:59:53.9878677Z with:
+2026-11-21T13:59:53.9879131Z   toolchain: stable
+2026-11-21T13:59:53.9879611Z ##[endgroup]
+2026-11-21T13:59:53.9968306Z ##[start-action display=Parse toolchain version;id=__dtolnay_rust-toolchain.parse]
+2026-11-21T13:59:54.0041161Z ##[group]Run if [[ -z $toolchain ]]; then
+2026-11-21T13:59:54.0041918Z [36;1mif [[ -z $toolchain ]]; then[0m
+2026-11-21T13:59:54.0042972Z [36;1m  # GitHub does not enforce `required: true` inputs itself. https://github.com/actions/runner/issues/1070[0m
+2026-11-21T13:59:54.0044057Z [36;1m  echo "'toolchain' is a required input" >&2[0m
+2026-11-21T13:59:54.0044702Z [36;1m  exit 1[0m
+2026-11-21T13:59:54.0045421Z [36;1melif [[ $toolchain =~ ^stable' '[0-9]+' '(year|month|week|day)s?' 'ago$ ]]; then[0m
+2026-11-21T13:59:54.0046664Z [36;1m  if [[ Linux == macOS ]]; then[0m
+2026-11-21T13:59:54.0047791Z [36;1m    echo "toolchain=1.$((($(date -v-$(sed 's/stable \([0-9]*\) \(.\).*/\1\2/' <<< $toolchain) +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
+2026-11-21T13:59:54.0048802Z [36;1m  else[0m
+2026-11-21T13:59:54.0049653Z [36;1m    echo "toolchain=1.$((($(date --date "${toolchain#stable }" +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
+2026-11-21T13:59:54.0050591Z [36;1m  fi[0m
+2026-11-21T13:59:54.0051257Z [36;1melif [[ $toolchain =~ ^stable' 'minus' '[0-9]+' 'releases?$ ]]; then[0m
+2026-11-21T13:59:54.0052322Z [36;1m  echo "toolchain=1.$((($(date +%s)/60/60/24-16569)/7/6-${toolchain//[^0-9]/}))" >> $GITHUB_OUTPUT[0m
+2026-11-21T13:59:54.0053259Z [36;1melif [[ $toolchain =~ ^1\.[0-9]+$ ]]; then[0m
+2026-11-21T13:59:54.0054305Z [36;1m  echo "toolchain=1.$((i=${toolchain#1.}, c=($(date +%s)/60/60/24-16569)/7/6, i+9*i*(10*i<=c)+90*i*(100*i<=c)))" >> $GITHUB_OUTPUT[0m
+2026-11-21T13:59:54.0055289Z [36;1melse[0m
+2026-11-21T13:59:54.0055927Z [36;1m  delimiter=EOF[0m
+2026-11-21T13:59:54.0056607Z [36;1m  while [[ $toolchain == *$delimiter* ]]; do delimiter+=_; done[0m
+2026-11-21T13:59:54.0057614Z [36;1m  printf 'toolchain<<%s\n%s\n%s\n' "$delimiter" "$toolchain" "$delimiter" >> $GITHUB_OUTPUT[0m
+2026-11-21T13:59:54.0058474Z [36;1mfi[0m
+2026-11-21T13:59:54.0096368Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-21T13:59:54.0097146Z env:
+2026-11-21T13:59:54.0097610Z   toolchain: stable
+2026-11-21T13:59:54.0098092Z ##[endgroup]
+2026-11-21T13:59:54.0228627Z ##[end-action id=__dtolnay_rust-toolchain.parse;outcome=success;conclusion=success;duration_ms=25]
+2026-11-21T13:59:54.0235059Z ##[start-action display=Set $CARGO_HOME;id=__dtolnay_rust-toolchain.__run]
+2026-11-21T13:59:54.0288120Z ##[group]Run echo CARGO_HOME=${CARGO_HOME:-"$HOME/.cargo"} >> $GITHUB_ENV
+2026-11-21T13:59:54.0289137Z [36;1mecho CARGO_HOME=${CARGO_HOME:-"$HOME/.cargo"} >> $GITHUB_ENV[0m
+2026-11-21T13:59:54.0321027Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-21T13:59:54.0321759Z ##[endgroup]
+2026-11-21T13:59:54.0392137Z ##[end-action id=__dtolnay_rust-toolchain.__run;outcome=success;conclusion=success;duration_ms=15]
+2026-11-21T13:59:54.0396520Z ##[start-action display=Install rustup if needed;id=__dtolnay_rust-toolchain.__run_2]
+2026-11-21T13:59:54.0435329Z ##[group]Run if ! command -v rustup &>/dev/null; then
+2026-11-21T13:59:54.0436500Z [36;1mif ! command -v rustup &>/dev/null; then[0m
+2026-11-21T13:59:54.0437947Z [36;1m  curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused --location --silent --show-error --fail https://sh.rustup.rs | sh -s -- --default-toolchain none -y[0m
+2026-11-21T13:59:54.0439363Z [36;1m  echo "$CARGO_HOME/bin" >> $GITHUB_PATH[0m
+2026-11-21T13:59:54.0439997Z [36;1mfi[0m
+2026-11-21T13:59:54.0470860Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-21T13:59:54.0471592Z env:
+2026-11-21T13:59:54.0472056Z   CARGO_HOME: /home/runner/.cargo
+2026-11-21T13:59:54.0472631Z ##[endgroup]
+2026-11-21T13:59:54.0537448Z ##[end-action id=__dtolnay_rust-toolchain.__run_2;outcome=success;conclusion=success;duration_ms=13]
+2026-11-21T13:59:54.0541534Z ##[start-action display=Install rustup if needed on windows;id=__dtolnay_rust-toolchain.__run_3]
+2026-11-21T13:59:54.0551991Z ##[end-action id=__dtolnay_rust-toolchain.__run_3;outcome=skipped;conclusion=skipped;duration_ms=0]
+2026-11-21T13:59:54.0568878Z ##[start-action display=rustup toolchain install stable;id=__dtolnay_rust-toolchain.__run_4]
+2026-11-21T13:59:54.0597765Z ##[group]Run set -f  # split targets and components on commas and whitespace, without glob expansion
+2026-11-21T13:59:54.0599062Z [36;1mset -f  # split targets and components on commas and whitespace, without glob expansion[0m
+2026-11-21T13:59:54.0599975Z [36;1mflags=()[0m
+2026-11-21T13:59:54.0600573Z [36;1mfor t in ${targets//,/ }; do flags+=(--target "$t"); done[0m
+2026-11-21T13:59:54.0601432Z [36;1mfor c in ${components//,/ }; do flags+=(--component "$c"); done[0m
+2026-11-21T13:59:54.0602445Z [36;1mif [[ $toolchain == nightly && -n $components ]]; then flags+=(--allow-downgrade); fi[0m
+2026-11-21T13:59:54.0603340Z [36;1mlog=$(mktemp)[0m
+2026-11-21T13:59:54.0603884Z [36;1mtrap 'rm -f "$log"' EXIT[0m
+2026-11-21T13:59:54.0604477Z [36;1mfor attempt in 1 2 3 4 5; do[0m
+2026-11-21T13:59:54.0605614Z [36;1m  if rustup toolchain install "$toolchain" "${flags[@]}" --profile minimal --no-self-update --force-non-host 2>&1 | tee "$log"; then[0m
+2026-11-21T13:59:54.0607261Z [36;1m    break[0m
+2026-11-21T13:59:54.0607750Z [36;1m  else[0m
+2026-11-21T13:59:54.0608219Z [36;1m    status=$?[0m
+2026-11-21T13:59:54.0608723Z [36;1m  fi[0m
+2026-11-21T13:59:54.0609345Z [36;1m  if [[ $attempt == 5 ]] || ! grep -Fq 'checksum failed for ' "$log" \[0m
+2026-11-21T13:59:54.0610430Z [36;1m      || ! grep -Fq 'this is likely due to an ongoing update of the official release server' "$log"; then[0m
+2026-11-21T13:59:54.0611372Z [36;1m    exit "$status"[0m
+2026-11-21T13:59:54.0611885Z [36;1m  fi[0m
+2026-11-21T13:59:54.0612639Z [36;1m  echo "Retrying rustup toolchain install in 30 seconds (retry $attempt of 4)" >&2[0m
+2026-11-21T13:59:54.0613514Z [36;1m  sleep 30[0m
+2026-11-21T13:59:54.0613992Z [36;1mdone[0m
+2026-11-21T13:59:54.0648052Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-21T13:59:54.0649260Z env:
+2026-11-21T13:59:54.0650043Z   CARGO_HOME: /home/runner/.cargo
+2026-11-21T13:59:54.0651019Z   toolchain: stable
+2026-11-21T13:59:54.0651827Z   targets: 
+2026-11-21T13:59:54.0652589Z   components: 
+2026-11-21T13:59:54.0653404Z   RUSTUP_PERMIT_COPY_RENAME: 1
+2026-11-21T13:59:54.0654325Z ##[endgroup]
+2026-11-21T13:59:54.2457592Z info: syncing channel updates for stable-x86_64-unknown-linux-gnu
+2026-11-21T13:59:54.3279146Z 
+2026-11-21T13:59:54.3362073Z   stable-x86_64-unknown-linux-gnu unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-21T13:59:54.3363423Z 
+2026-11-21T13:59:54.3418786Z ##[end-action id=__dtolnay_rust-toolchain.__run_4;outcome=success;conclusion=success;duration_ms=284]
+2026-11-21T13:59:54.3427122Z ##[start-action display=rustup default stable;id=__dtolnay_rust-toolchain.__run_5]
+2026-11-21T13:59:54.3456668Z ##[group]Run rustup default "$toolchain" --force-non-host
+2026-11-21T13:59:54.3457467Z [36;1mrustup default "$toolchain" --force-non-host[0m
+2026-11-21T13:59:54.3490949Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-21T13:59:54.3491683Z env:
+2026-11-21T13:59:54.3492129Z   CARGO_HOME: /home/runner/.cargo
+2026-11-21T13:59:54.3492686Z   toolchain: stable
+2026-11-21T13:59:54.3493144Z ##[endgroup]
+2026-11-21T13:59:54.3593774Z info: using existing install for stable-x86_64-unknown-linux-gnu
+2026-11-21T13:59:54.3602178Z info: default toolchain set to stable-x86_64-unknown-linux-gnu
+2026-11-21T13:59:54.3603218Z 
+2026-11-21T13:59:54.3680615Z   stable-x86_64-unknown-linux-gnu unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-21T13:59:54.3681324Z 
+2026-11-21T13:59:54.3702271Z ##[end-action id=__dtolnay_rust-toolchain.__run_5;outcome=success;conclusion=success;duration_ms=27]
+2026-11-21T13:59:54.3708823Z ##[start-action display=Create cachekey;id=__dtolnay_rust-toolchain.rustc-version]
+2026-11-21T13:59:54.3739537Z ##[group]Run DATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')
+2026-11-21T13:59:54.3741392Z [36;1mDATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')[0m
+2026-11-21T13:59:54.3742714Z [36;1mHASH=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-hash: //p')[0m
+2026-11-21T13:59:54.3743815Z [36;1mecho "cachekey=$(echo $DATE$HASH | head -c12)" >> $GITHUB_OUTPUT[0m
+2026-11-21T13:59:54.3777652Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-21T13:59:54.3778373Z env:
+2026-11-21T13:59:54.3778828Z   CARGO_HOME: /home/runner/.cargo
+2026-11-21T13:59:54.3779405Z   toolchain: stable
+2026-11-21T13:59:54.3779875Z ##[endgroup]
+2026-11-21T13:59:54.4159375Z ##[end-action id=__dtolnay_rust-toolchain.rustc-version;outcome=success;conclusion=success;duration_ms=44]
+2026-11-21T13:59:54.4164013Z ##[start-action display=Disable incremental compilation;id=__dtolnay_rust-toolchain.__run_6]
+2026-11-21T13:59:54.4194746Z ##[group]Run if [ -z "${CARGO_INCREMENTAL+set}" ]; then
+2026-11-21T13:59:54.4195546Z [36;1mif [ -z "${CARGO_INCREMENTAL+set}" ]; then[0m
+2026-11-21T13:59:54.4196556Z [36;1m  echo CARGO_INCREMENTAL=0 >> $GITHUB_ENV[0m
+2026-11-21T13:59:54.4197195Z [36;1mfi[0m
+2026-11-21T13:59:54.4230590Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-21T13:59:54.4231318Z env:
+2026-11-21T13:59:54.4231776Z   CARGO_HOME: /home/runner/.cargo
+2026-11-21T13:59:54.4232324Z ##[endgroup]
+2026-11-21T13:59:54.4297450Z ##[end-action id=__dtolnay_rust-toolchain.__run_6;outcome=success;conclusion=success;duration_ms=13]
+2026-11-21T13:59:54.4301427Z ##[start-action display=Enable colors in Cargo output;id=__dtolnay_rust-toolchain.__run_7]
+2026-11-21T13:59:54.4330389Z ##[group]Run if [ -z "${CARGO_TERM_COLOR+set}" ]; then
+2026-11-21T13:59:54.4353092Z [36;1mif [ -z "${CARGO_TERM_COLOR+set}" ]; then[0m
+2026-11-21T13:59:54.4353994Z [36;1m  echo CARGO_TERM_COLOR=always >> $GITHUB_ENV[0m
+2026-11-21T13:59:54.4354668Z [36;1mfi[0m
+2026-11-21T13:59:54.4385513Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-21T13:59:54.4386522Z env:
+2026-11-21T13:59:54.4387028Z   CARGO_HOME: /home/runner/.cargo
+2026-11-21T13:59:54.4387613Z   CARGO_INCREMENTAL: 0
+2026-11-21T13:59:54.4388112Z ##[endgroup]
+2026-11-21T13:59:54.4452682Z ##[end-action id=__dtolnay_rust-toolchain.__run_7;outcome=success;conclusion=success;duration_ms=14]
+2026-11-21T13:59:54.4458392Z ##[start-action display=Enable Cargo sparse registry;id=__dtolnay_rust-toolchain.__run_8]
+2026-11-21T13:59:54.4489057Z ##[group]Run # implemented in 1.66, stabilized in 1.68, made default in 1.70
+2026-11-21T13:59:54.4490087Z [36;1m# implemented in 1.66, stabilized in 1.68, made default in 1.70[0m
+2026-11-21T13:59:54.4491463Z [36;1mif [ -z "${CARGO_REGISTRIES_CRATES_IO_PROTOCOL+set}" -o -f "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol ]; then[0m
+2026-11-21T13:59:54.4492905Z [36;1m  if rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[89]\.'; then[0m
+2026-11-21T13:59:54.4494076Z [36;1m    touch "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
+2026-11-21T13:59:54.4495179Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse >> $GITHUB_ENV[0m
+2026-11-21T13:59:54.4496354Z [36;1m  elif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[67]\.'; then[0m
+2026-11-21T13:59:54.4497534Z [36;1m    touch "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
+2026-11-21T13:59:54.4498583Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git >> $GITHUB_ENV[0m
+2026-11-21T13:59:54.4499329Z [36;1m  fi[0m
+2026-11-21T13:59:54.4499769Z [36;1mfi[0m
+2026-11-21T13:59:54.4529654Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-21T13:59:54.4530365Z env:
+2026-11-21T13:59:54.4530986Z   CARGO_HOME: /home/runner/.cargo
+2026-11-21T13:59:54.4531573Z   CARGO_INCREMENTAL: 0
+2026-11-21T13:59:54.4532092Z   CARGO_TERM_COLOR: always
+2026-11-21T13:59:54.4532607Z   toolchain: stable
+2026-11-21T13:59:54.4533072Z ##[endgroup]
+2026-11-21T13:59:54.4869026Z ##[end-action id=__dtolnay_rust-toolchain.__run_8;outcome=success;conclusion=success;duration_ms=40]
+2026-11-21T13:59:54.4875654Z ##[start-action display=Work around spurious network errors in curl 8.0;id=__dtolnay_rust-toolchain.__run_9]
+2026-11-21T13:59:54.4906534Z ##[group]Run # https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation
+2026-11-21T13:59:54.4907953Z [36;1m# https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation[0m
+2026-11-21T13:59:54.4909198Z [36;1mif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.7[01]\.'; then[0m
+2026-11-21T13:59:54.4910179Z [36;1m  echo CARGO_HTTP_MULTIPLEXING=false >> $GITHUB_ENV[0m
+2026-11-21T13:59:54.4910874Z [36;1mfi[0m
+2026-11-21T13:59:54.4944623Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-21T13:59:54.4945360Z env:
+2026-11-21T13:59:54.4945971Z   CARGO_HOME: /home/runner/.cargo
+2026-11-21T13:59:54.4946561Z   CARGO_INCREMENTAL: 0
+2026-11-21T13:59:54.4947087Z   CARGO_TERM_COLOR: always
+2026-11-21T13:59:54.4947612Z   toolchain: stable
+2026-11-21T13:59:54.4948091Z ##[endgroup]
+2026-11-21T13:59:54.5143362Z ##[end-action id=__dtolnay_rust-toolchain.__run_9;outcome=success;conclusion=success;duration_ms=26]
+2026-11-21T13:59:54.5150867Z ##[start-action display=rustc +stable --version --verbose;id=__dtolnay_rust-toolchain.__run_10]
+2026-11-21T13:59:54.5180575Z ##[group]Run rustc +"$toolchain" --version --verbose
+2026-11-21T13:59:54.5181323Z [36;1mrustc +"$toolchain" --version --verbose[0m
+2026-11-21T13:59:54.5214049Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-21T13:59:54.5214765Z env:
+2026-11-21T13:59:54.5215222Z   CARGO_HOME: /home/runner/.cargo
+2026-11-21T13:59:54.5216075Z   CARGO_INCREMENTAL: 0
+2026-11-21T13:59:54.5216614Z   CARGO_TERM_COLOR: always
+2026-11-21T13:59:54.5217125Z   toolchain: stable
+2026-11-21T13:59:54.5217588Z ##[endgroup]
+2026-11-21T13:59:54.5382949Z rustc 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-21T13:59:54.5384190Z binary: rustc
+2026-11-21T13:59:54.5385240Z commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a
+2026-11-21T13:59:54.5386512Z commit-date: 2026-11-09
+2026-11-21T13:59:54.5387165Z host: x86_64-unknown-linux-gnu
+2026-11-21T13:59:54.5387723Z release: 1.100.0
+2026-11-21T13:59:54.5388192Z LLVM version: 23.1.5
+2026-11-21T13:59:54.5409078Z ##[end-action id=__dtolnay_rust-toolchain.__run_10;outcome=success;conclusion=success;duration_ms=25]
+2026-11-21T13:59:54.5567528Z ##[group]Run Swatinem/rust-cache@v2
+2026-11-21T13:59:54.5568187Z with:
+2026-11-21T13:59:54.5568654Z   prefix-key: v0-rust
+2026-11-21T13:59:54.5569167Z   add-job-id-key: true
+2026-11-21T13:59:54.5569703Z   add-rust-environment-hash-key: true
+2026-11-21T13:59:54.5570303Z   cache-targets: true
+2026-11-21T13:59:54.5570811Z   cache-all-crates: false
+2026-11-21T13:59:54.5571353Z   cache-workspace-crates: false
+2026-11-21T13:59:54.5571892Z   save-if: true
+2026-11-21T13:59:54.5572373Z   cache-provider: github
+2026-11-21T13:59:54.5572914Z   cache-bin: true
+2026-11-21T13:59:54.5573388Z   lookup-only: false
+2026-11-21T13:59:54.5573869Z   cmd-format: {0}
+2026-11-21T13:59:54.5574319Z env:
+2026-11-21T13:59:54.5574768Z   CARGO_HOME: /home/runner/.cargo
+2026-11-21T13:59:54.5575326Z   CARGO_INCREMENTAL: 0
+2026-11-21T13:59:54.5575985Z   CARGO_TERM_COLOR: always
+2026-11-21T13:59:54.5576508Z ##[endgroup]
+2026-11-21T13:59:55.0108599Z ##[group]Cache Configuration
+2026-11-21T13:59:55.0111062Z Cache Provider:
+2026-11-21T13:59:55.0112738Z     github
+2026-11-21T13:59:55.0114249Z Workspaces:
+2026-11-21T13:59:55.0116119Z     /home/runner/work/MultiTunnel/MultiTunnel
+2026-11-21T13:59:55.0118263Z Cache Paths:
+2026-11-21T13:59:55.0119861Z     /home/runner/.cargo/bin
+2026-11-21T13:59:55.0121673Z     /home/runner/.cargo/.crates.toml
+2026-11-21T13:59:55.0123947Z     /home/runner/.cargo/.crates2.json
+2026-11-21T13:59:55.0126552Z     /home/runner/.cargo/registry
+2026-11-21T13:59:55.0128981Z     /home/runner/.cargo/git
+2026-11-21T13:59:55.0131178Z     /home/runner/work/MultiTunnel/MultiTunnel/target
+2026-11-21T13:59:55.0133751Z Restore Key:
+2026-11-21T13:59:55.0135436Z     v0-rust-test-Linux-x64-2fd77104
+2026-11-21T13:59:55.0137524Z Cache Key:
+2026-11-21T13:59:55.0139231Z     v0-rust-test-Linux-x64-2fd77104-d412efeb
+2026-11-21T13:59:55.0141249Z .. Prefix:
+2026-11-21T13:59:55.0142835Z   - v0-rust-test-Linux-x64
+2026-11-21T13:59:55.0145124Z .. Environment considered:
+2026-11-21T13:59:55.0147116Z   - Rust Versions:
+2026-11-21T13:59:55.0149139Z     - 1.100.0 x86_64-unknown-linux-gnu 0ef1bc386059596dada8fb65b50425aab2ea637a
+2026-11-21T13:59:55.0151336Z   - CARGO_HOME
+2026-11-21T13:59:55.0152663Z   - CARGO_INCREMENTAL
+2026-11-21T13:59:55.0154070Z   - CARGO_TERM_COLOR
+2026-11-21T13:59:55.0155536Z .. Lockfiles considered:
+2026-11-21T13:59:55.0158025Z   - /home/runner/work/MultiTunnel/MultiTunnel/Cargo.lock
+2026-11-21T13:59:55.0160269Z   - /home/runner/work/MultiTunnel/MultiTunnel/Cargo.toml
+2026-11-21T13:59:55.0162566Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-app/Cargo.toml
+2026-11-21T13:59:55.0166286Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond/Cargo.toml
+2026-11-21T13:59:55.0169408Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-engine/Cargo.toml
+2026-11-21T13:59:55.0173274Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-links/Cargo.toml
+2026-11-21T13:59:55.0177363Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-m3/Cargo.toml
+2026-11-21T13:59:55.0181179Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net/Cargo.toml
+2026-11-21T13:59:55.0185130Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-platform/Cargo.toml
+2026-11-21T13:59:55.0189998Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-service/Cargo.toml
+2026-11-21T13:59:55.0194472Z ##[endgroup]
+2026-11-21T13:59:55.0195421Z 
+2026-11-21T13:59:55.0196389Z ... Restoring cache ...
+2026-11-21T13:59:55.2493362Z Cache hit for: v0-rust-test-Linux-x64-2fd77104-d412efeb
+2026-11-21T13:59:56.5191220Z Received 29360128 of 520715129 (5.6%), 27.9 MBs/sec
+2026-11-21T13:59:57.5196234Z Received 159383552 of 520715129 (30.6%), 75.9 MBs/sec
+2026-11-21T13:59:58.5195465Z Received 343932928 of 520715129 (66.1%), 109.2 MBs/sec
+2026-11-21T13:59:59.5257363Z Received 516520825 of 520715129 (99.2%), 123.1 MBs/sec
+2026-11-21T14:00:00.1923815Z Received 520715129 of 520715129 (100.0%), 106.2 MBs/sec
+2026-11-21T14:00:00.1926277Z Cache Size: ~497 MB (520715129 B)
+2026-11-21T14:00:00.1967092Z [command]/usr/bin/tar -xf /home/runner/work/_temp/13814c37-18ae-40d0-8dc1-0f049a9a69a8/cache.tzst -P -C /home/runner/work/MultiTunnel/MultiTunnel --use-compress-program unzstd
+2026-11-21T14:00:04.0604499Z Cache restored successfully
+2026-11-21T14:00:04.0984772Z Restored from cache key "v0-rust-test-Linux-x64-2fd77104-d412efeb" full match: true.
+2026-11-21T14:00:04.1461411Z ##[group]Run cargo test --workspace
+2026-11-21T14:00:04.1461795Z [36;1mcargo test --workspace[0m
+2026-11-21T14:00:04.1497462Z shell: /usr/bin/bash -e {0}
+2026-11-21T14:00:04.1497747Z env:
+2026-11-21T14:00:04.1497962Z   CARGO_HOME: /home/runner/.cargo
+2026-11-21T14:00:04.1498272Z   CARGO_INCREMENTAL: 0
+2026-11-21T14:00:04.1498517Z   CARGO_TERM_COLOR: always
+2026-11-21T14:00:04.1498774Z   CACHE_ON_FAILURE: false
+2026-11-21T14:00:04.1499012Z ##[endgroup]
+2026-11-21T14:00:05.1877368Z [1m[92m    Updating[0m crates.io index
+2026-11-21T14:00:07.2863197Z [1m[92m   Compiling[0m mt-net v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net)
+2026-11-21T14:00:07.2949053Z [1m[92m   Compiling[0m iced_graphics v0.13.0 (/home/runner/work/MultiTunnel/MultiTunnel/vendor/iced_graphics)
+2026-11-21T14:00:07.3029880Z [1m[92m   Compiling[0m mt-links v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-links)
+2026-11-21T14:00:07.3031796Z [1m[92m   Compiling[0m mt-service v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-service)
+2026-11-21T14:00:08.1588342Z [1m[92m   Compiling[0m mt-platform v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-platform)
+2026-11-21T14:00:08.4967821Z [1m[92m   Compiling[0m iced_tiny_skia v0.13.0 (/home/runner/work/MultiTunnel/MultiTunnel/vendor/iced_tiny_skia)
+2026-11-21T14:00:08.9505538Z [1m[92m   Compiling[0m iced_winit v0.13.0
+2026-11-21T14:00:09.0377104Z [1m[92m   Compiling[0m iced_renderer v0.13.0
+2026-11-21T14:00:09.0957543Z [1m[92m   Compiling[0m mt-engine v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-engine)
+2026-11-21T14:00:09.1757601Z [1m[92m   Compiling[0m iced_widget v0.13.4
+2026-11-21T14:00:09.6324367Z [1m[92m   Compiling[0m mt-bond v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond)
+2026-11-21T14:00:11.5284509Z [1m[92m   Compiling[0m iced v0.13.1
+2026-11-21T14:00:12.4991102Z [1m[92m   Compiling[0m mt-m3 v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-m3)
+2026-11-21T14:00:12.9256464Z [1m[92m   Compiling[0m mt-app v0.13.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-app)
+2026-11-21T14:00:15.7207524Z [1m[92m   Compiling[0m multitunnel v0.13.0 (/home/runner/work/MultiTunnel/MultiTunnel)
+2026-11-21T14:00:28.3370650Z [1m[92m    Finished[0m `test` profile [unoptimized + debuginfo] target(s) in 24.10s
+2026-11-21T14:00:28.4052621Z [1m[92m     Running[0m unittests src/lib.rs (target/debug/build/mt-app/9e15a8999771d192/out/mt_app-9e15a8999771d192)
+2026-11-21T14:00:28.4081265Z 
+2026-11-21T14:00:28.4081831Z running 63 tests
+2026-11-21T14:00:28.4101278Z test app::tests::app_version_is_release_triplet ... ok
+2026-11-21T14:00:28.5069341Z test app::tests::adapter_rate_sampling_deltas_and_resets ... ok
+2026-11-21T14:00:28.5088496Z test app::tests::close_action_decision ... ok
+2026-11-21T14:00:28.5116223Z test app::tests::close_to_tray_settings_roundtrip ... ok
+2026-11-21T14:00:28.5186755Z test app::tests::always_on_top_menu_item_roundtrips ... ok
+2026-11-21T14:00:28.5256735Z test app::tests::begin_exit_is_idempotent_without_tray ... ok
+2026-11-21T14:00:28.5288425Z test app::tests::fmt_helpers ... ok
+2026-11-21T14:00:28.5296683Z test app::tests::kill_switch_banner_gate ... ok
+2026-11-21T14:00:28.5298056Z test app::tests::link_matches_adapter_shapes ... ok
+2026-11-21T14:00:28.5303187Z test app::tests::log_filter_matches ... ok
+2026-11-21T14:00:28.5304508Z test app::tests::adapters_screen_toggle_move_apply_revert ... ok
+2026-11-21T14:00:28.5305537Z test app::tests::edit_dialog_open_type_save_close ... ok
+2026-11-21T14:00:28.5306837Z test app::tests::logs_scroll_id_stable ... ok
+2026-11-21T14:00:28.5307757Z test app::tests::rule_editor_add_edit_remove ... ok
+2026-11-21T14:00:28.5357189Z test app::tests::logs_autoscroll_default_and_toggle ... ok
+2026-11-21T14:00:28.5387116Z test app::tests::seed_parse ... ok
+2026-11-21T14:00:28.5416693Z test app::tests::edit_save_failure_is_logged ... ok
+2026-11-21T14:00:28.5421925Z test app::tests::smoke_flag_parse ... ok
+2026-11-21T14:00:28.5466964Z test app::tests::logs_autoscroll_gated_on_screen ... ok
+2026-11-21T14:00:28.5489366Z test app::tests::sort_adapter_rows_selected_first_in_saved_order ... ok
+2026-11-21T14:00:28.5558980Z test app::tests::session_totals_accumulate_and_reset ... ok
+2026-11-21T14:00:28.5559995Z test app::tests::s19_settings_toggles_and_modal_gating ... ok
+2026-11-21T14:00:28.5673433Z test app::tests::title_menu_anchor_clamps ... ok
+2026-11-21T14:00:28.5674351Z test app::tests::titlebar_double_click_window ... ok
+2026-11-21T14:00:28.5974706Z test app::tests::theme_reroutes_on_settings_messages ... ok
+2026-11-21T14:00:28.6244292Z test app::tests::wheel_drag_updates_seed_live_and_persists_on_release ... ok
+2026-11-21T14:00:28.6245482Z test graph::tests::local_bounds_strips_widget_offset_regression ... ok
+2026-11-21T14:00:28.6246635Z test graph::tests::nice_ceiling_covers_and_rounds ... ok
+2026-11-21T14:00:28.6247692Z test graph::tests::points_stay_inside_bounds_and_right_align ... ok
+2026-11-21T14:00:28.6248545Z test graph::tests::rates_format_human ... ok
+2026-11-21T14:00:28.6249310Z test mock::tests::fmt_utc_known_values ... ok
+2026-11-21T14:00:28.6296386Z test app::tests::smoke_run_drives_app_with_mock ... ok
+2026-11-21T14:00:28.6913080Z test mock::tests::mock_latency_test ... ok
+2026-11-21T14:00:28.6918374Z test mock::tests::mock_profiles_import_remove ... ok
+2026-11-21T14:00:28.6922037Z test mock::tests::mock_settings_and_rules ... ok
+2026-11-21T14:00:28.6978397Z test theme::tests::button_states_use_state_layers ... ok
+2026-11-21T14:00:28.6979346Z test theme::tests::hsv_conversions_roundtrip_primaries ... ok
+2026-11-21T14:00:28.7036094Z test theme::tests::switch_styles_differ_by_state ... ok
+2026-11-21T14:00:28.7037045Z test theme::tests::system_prefers_dark_env_override_wins ... ok
+2026-11-21T14:00:28.7091605Z test theme::tests::text_color_from_m3 ... ok
+2026-11-21T14:00:28.7303513Z test theme::tests::theme_derivation_seed_changes_scheme ... ok
+2026-11-21T14:00:28.7412674Z test theme::tests::theme_from_settings_and_rederive ... ok
+2026-11-21T14:00:28.7413595Z test theme::tests::wheel_asset_dimensions ... ok
+2026-11-21T14:00:28.7414581Z test theme::tests::wheel_marker_roundtrips_picked_points ... ok
+2026-11-21T14:00:28.7416114Z test theme::tests::wheel_seed_clamps_outside_points_to_the_rim ... ok
+2026-11-21T14:00:28.7417126Z test theme::tests::wheel_seed_matches_cardinal_hues ... ok
+2026-11-21T14:00:28.7418276Z test tray::tests::ico_embedded_asset_parses_with_32_entry ... ok
+2026-11-21T14:00:28.7419221Z test tray::tests::ico_embedded_mono_asset_parses_with_32_entry ... ok
+2026-11-21T14:00:28.7420016Z test tray::tests::ico_rejects_bad_directories ... ok
+2026-11-21T14:00:28.7420882Z test tray::tests::ids_roundtrip_and_skip_zero ... ok
+2026-11-21T14:00:28.7421926Z test tray::tests::menu_connected_shows_disconnect_and_servers ... ok
+2026-11-21T14:00:28.7422765Z test tray::tests::menu_disconnected_shows_connect ... ok
+2026-11-21T14:00:28.7423607Z test tray::tests::menu_failed_is_disconnected_shape_with_honest_header ... ok
+2026-11-21T14:00:28.7424461Z test tray::tests::menu_transition_disables_the_toggle ... ok
+2026-11-21T14:00:28.7425182Z test tray::tests::model_from_snapshot_states ... ok
+2026-11-21T14:00:28.7426254Z test tray::tests::tip_array_never_overruns_or_splits_surrogates ... ok
+2026-11-21T14:00:28.7427202Z test tray::tests::to_wide_is_nul_terminated ... ok
+2026-11-21T14:00:28.7428312Z test tray::tests::tooltip_live_formats_speeds_and_totals ... ok
+2026-11-21T14:00:28.7429296Z test tray::tests::tooltip_states ... ok
+2026-11-21T14:00:28.7430130Z test tray::tests::tray_icon_follows_connection_state ... ok
+2026-11-21T14:00:29.3725074Z test app::tests::speed_test_message_flow ... ok
+2026-11-21T14:00:29.4847659Z test app::tests::update_loop_screens_and_rules ... ok
+2026-11-21T14:00:29.5268457Z test mock::tests::mock_connect_disconnect_lifecycle ... ok
+2026-11-21T14:00:29.5268881Z 
+2026-11-21T14:00:29.5269268Z test result: ok. 63 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.12s
+2026-11-21T14:00:29.5270230Z 
+2026-11-21T14:00:29.5276426Z [1m[92m     Running[0m unittests src/main.rs (target/debug/build/mt-app/d0f8c07fd7733fee/out/mt_app-d0f8c07fd7733fee)
+2026-11-21T14:00:29.5295297Z 
+2026-11-21T14:00:29.5295494Z running 0 tests
+2026-11-21T14:00:29.5295919Z 
+2026-11-21T14:00:29.5296388Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
+2026-11-21T14:00:29.5296986Z 
+2026-11-21T14:00:29.5301765Z [1m[92m     Running[0m unittests src/lib.rs (target/debug/build/mt-bond/47daf16dc832a37b/out/mt_bond-47daf16dc832a37b)
+2026-11-21T14:00:29.5314117Z 
+2026-11-21T14:00:29.5314369Z running 32 tests
+2026-11-21T14:00:29.5323178Z test endpoint::scheduling_tests::balanced_blest_guard_never_bypasses_the_score_leader ... ok
+2026-11-21T14:00:29.5324188Z test endpoint::scheduling_tests::balanced_blest_guard_skips_slow_draining_tied_carrier ... ok
+2026-11-21T14:00:29.5325515Z test endpoint::scheduling_tests::balanced_capacity_aware_bypasses_the_saturated_adapter ... ok
+2026-11-21T14:00:29.5327464Z test endpoint::scheduling_tests::balanced_picks_the_least_backlogged_carrier ... ok
+2026-11-21T14:00:29.5328855Z test endpoint::scheduling_tests::carrier_score_is_relative_load_with_probe_floor ... ok
+2026-11-21T14:00:29.5330483Z test endpoint::scheduling_tests::ecf_completion_is_rtt_plus_drain_time ... ok
+2026-11-21T14:00:29.5332486Z test endpoint::scheduling_tests::balanced_ties_rotate ... ok
+2026-11-21T14:00:29.5333808Z test endpoint::scheduling_tests::balanced_spreads_proportionally_to_reported_capacity ... ok
+2026-11-21T14:00:29.5335257Z test endpoint::scheduling_tests::lowest_latency_ecf_prefers_earliest_completion ... ok
+2026-11-21T14:00:29.5339741Z test endpoint::scheduling_tests::failover_takes_the_first_live_carrier ... ok
+2026-11-21T14:00:29.5341186Z test endpoint::scheduling_tests::lowest_latency_prefers_sampled_rtt_then_known_over_unknown ... ok
+2026-11-21T14:00:29.5342169Z test endpoint::scheduling_tests::mixed_reports_probe_the_unreported_carrier ... ok
+2026-11-21T14:00:29.5343289Z test endpoint::scheduling_tests::priority_fills_in_order_until_the_queue_backs_up ... ok
+2026-11-21T14:00:29.5344076Z test endpoint::scheduling_tests::priority_spills_when_relative_load_exceeds_ratio ... ok
+2026-11-21T14:00:29.5344976Z test endpoint::scheduling_tests::round_robin_cycles_the_cursor ... ok
+2026-11-21T14:00:29.5346186Z test frame::tests::header_rejects_bad_magic_version_kind_and_size ... ok
+2026-11-21T14:00:29.5347082Z test frame::tests::empty_and_max_payloads_roundtrip ... ok
+2026-11-21T14:00:29.5347695Z test frame::tests::read_write_frame_over_duplex ... ok
+2026-11-21T14:00:29.5348041Z test frame::tests::roundtrip_all_kinds ... ok
+2026-11-21T14:00:29.5348715Z test frame::tests::target_rejects_truncated_and_garbage ... ok
+2026-11-21T14:00:29.5349409Z test frame::tests::target_roundtrip_domain_ipv4_ipv6 ... ok
+2026-11-21T14:00:29.5350155Z test outbound::tests::ensure_carriers_fails_cleanly_with_no_links ... ok
+2026-11-21T14:00:29.5350920Z test reorder::tests::cap_zero_still_delivers_in_order ... ok
+2026-11-21T14:00:29.5351644Z test outbound::tests::ensure_carriers_reports_all_dead_links ... ok
+2026-11-21T14:00:29.5352397Z test reorder::tests::duplicates_and_replays_are_idempotent ... ok
+2026-11-21T14:00:29.5352941Z test reorder::tests::gap_buffers_then_flushes ... ok
+2026-11-21T14:00:29.5353305Z test reorder::tests::in_order_delivery ... ok
+2026-11-21T14:00:29.5353704Z test reorder::tests::long_out_of_order_run_reorders_completely ... ok
+2026-11-21T14:00:29.5354100Z test reorder::tests::overflow_is_an_error ... ok
+2026-11-21T14:00:29.5354479Z test reorder::tests::would_overflow_mirrors_the_insert_boundary ... ok
+2026-11-21T14:00:29.5354857Z test tests::constants_are_coherent ... ok
+2026-11-21T14:00:29.7335079Z test daemon::tests::serve_binds_and_shuts_down ... ok
+2026-11-21T14:00:29.7335645Z 
+2026-11-21T14:00:29.7336460Z test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
+2026-11-21T14:00:29.7337220Z 
+2026-11-21T14:00:29.7340537Z [1m[92m     Running[0m unittests src/bin/mt_merge_server.rs (target/debug/build/mt-bond/9c1ad43eeb883f2c/out/mt_merge_server-9c1ad43eeb883f2c)
+2026-11-21T14:00:29.7356287Z 
+2026-11-21T14:00:29.7356540Z running 0 tests
+2026-11-21T14:00:29.7356881Z 
+2026-11-21T14:00:29.7357513Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
+2026-11-21T14:00:29.7358149Z 
+2026-11-21T14:00:29.7359087Z [1m[92m     Running[0m tests/integration.rs (target/debug/build/mt-bond/1488f9338504bb39/out/integration-1488f9338504bb39)
+2026-11-21T14:00:29.7374024Z 
+2026-11-21T14:00:29.7374289Z running 6 tests
+2026-11-21T14:00:29.8780243Z test echo_roundtrip_1mib_across_two_carriers ... ok
+2026-11-21T14:00:29.9784986Z test one_dead_link_still_yields_a_working_bond ... ok
+2026-11-21T14:00:30.0311288Z test refused_target_surfaces_as_connection_reset ... ok
+2026-11-21T14:00:30.0936901Z test carrier_death_mid_stream_fails_over_without_data_loss ... ok
+2026-11-21T14:00:32.8678488Z test concurrent_sessions_multiplex_over_the_same_carriers ... ok
+2026-11-21T14:01:29.7377486Z test domain_targets_dial_through_the_daemon has been running for over 60 seconds
+2026-11-21T14:54:22.0623119Z ##[error]The operation was canceled.
+2026-11-21T14:54:22.0774095Z Post job cleanup.
+2026-11-21T14:54:22.1630015Z [command]/usr/bin/git version
+2026-11-21T14:54:22.1676681Z git version 2.56.1
+2026-11-21T14:54:22.1745352Z Temporarily overriding HOME='/home/runner/work/_temp/a44677aa-6bbd-4c6e-acbe-6c87ee9cc037' before making global git config changes
+2026-11-21T14:54:22.1746938Z Adding repository directory to the temporary git global config as a safe directory
+2026-11-21T14:54:22.1751299Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/MultiTunnel/MultiTunnel
+2026-11-21T14:54:22.1791088Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
+2026-11-21T14:54:22.1828403Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
+2026-11-21T14:54:22.2140537Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
+2026-11-21T14:54:22.2186556Z http.https://github.com/.extraheader
+2026-11-21T14:54:22.2191047Z [command]/usr/bin/git config --local --unset-all http.https://github.com/.extraheader
+2026-11-21T14:54:22.2223724Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
+2026-11-21T14:54:22.2567344Z [command]/usr/bin/git config --local --name-only --get-regexp ^includeIf\.gitdir:
+2026-11-21T14:54:22.2611228Z [command]/usr/bin/git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url
+2026-11-21T14:54:22.3089973Z Cleaning up orphan processes
+2026-11-21T14:54:22.3299738Z Terminate orphan process: pid (2988) (cargo)
+2026-11-21T14:54:22.3429766Z Terminate orphan process: pid (3776) (integration-1488f9338504bb39)
+2026-11-21T14:54:22.3551795Z ##[warning]Node.js 20 is deprecated. The following actions target Node.js 20 but are being forced to run on Node.js 24: actions/checkout@v4. For more information see: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/

### Commit: 3da17ad - backup(s19-review-fixes): 2026-11-21T13:51:08Z (Z User, 2026-11-21)
diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
index ea28030..bcb7935 100755
--- a/harness/memory/LEDGER.md
+++ b/harness/memory/LEDGER.md
@@ -214,3 +214,4 @@
 2026-11-21T07:30Z | s18 | VERIFIED: 447/0 workspace tests (+14 over S17: 3 mt-service mode migration, 2 theme wheel/system-seam, 2 app wheel-drag/mode-messages, 5 mt-bond BLEST/ECF, 2 tooltip); gates fmt/clippy host+windows-gnu (after clippy::explicit_counter_loop fix in the registry read) clean; vendored iced 6/0; zigbuild PE32+ ×4 + ELF ×2; mt-app --smoke 0. RIG sf=1.0: Settings → wheel click at 3 o'clock (hue 90) → seed chip purple(106,85,160)→GREEN(145,239,50), System-button fill purple→green, marker disc+halo+ring at the pick point; Dark click → page lum 236→37; Light click → 235 with green accent preserved. RIG env B (MT_THEME_SYSTEM_DARK=1): boots DARK (lum 27), System filled light-purple in dark scheme. RIG sf=1.5: same flow green chip + dark(36)/light(236) — HiDPI transform holds. Titlebar emblem lum spread 114 (light) / 212 (dark) — the boosted transparent mark reads on both.
 2026-11-21T07:17Z | s18 | v0.12.0 RELEASED: push attempt-1 REJECTED by GitHub — PROJECT_MEMORY.md had crossed the 100 MB per-file hard limit (the update_memory.py self-embedding recursion: each ritual run committed its own previous section into the file, so the next diff contained it — 81 MB at S17 close → 134 MB after this session's two runs; detected via the LFS-error push message, NOTHING reached origin). FIX: update_memory.py now excludes PROJECT_MEMORY.md's own diff (pathspec :(exclude) + an elision note), local history rewritten pre-push (reset --soft to origin/main, regenerated the section → 77 KB, re-committed s18 + re-tagged v0.12.0 — the oversized blobs never left the sandbox and are now unreachable). Attempt-2 push OK → release run 44879546925 SUCCESS on the first try (no cold-cache stall this time) + ci 44879546922 running. Assets verified by download+unzip: multitunnel-v0.12.0-windows-x64.zip 6,430,469 B integrity OK (multitunnel.exe PE32+ console + multitunnel-tray.exe PE32+ GUI + README v0.9–v0.12 section + QUICKSTART v0.12 note + server scripts + LICENSE + THIRD_PARTY_NOTICES + config.example) and mt-merge-server {x86_64 1,826,272 B, aarch64 1,860,808 B} ELFs.
 2026-11-21T08:30Z | s18 | OPS: the ci run on the s18 commit (44879546922) — lint/cross-check/test(ubuntu) all green; the windows test leg sat 69 min inside cargo test with no live logs fetchable mid-run → CANCELLED per the S11/S17 ops protocol. Verdict: 100%-cold rust-cache, not a wedge — the s18 commit changed Cargo.lock (version bumps + windows-sys feature), no windows job with that lock had ever completed, so Swatinem/rust-cache had nothing to restore and the leg compiled ~450 crates from zero (the exact S17 attempt-1 signature, cancelled at 68 min). Same-commit green proof on a REAL windows-latest runner exists: release run 44879546925's build-windows leg ran fmt + clippy + cargo test + packaging and PASSED (its zip is the verified v0.12.0 artifact). No rerun: the next ci.yml run would face the same cold cache (the release workflow's cache is keyed to a different workflow file); the S13 precedent (slow-cold cancelled, green proof elsewhere, no rerun) applies. The NEXT stage's ci will find the cache the release job saved for ITS key only — expect one more cold windows leg on the next Cargo.lock change.
+2026-11-21T14:20Z | s19 | IMPLEMENTED (six asks, v0.13.0): (1) server editing — Servers per-card Edit → full-window modal (S16 overlay discipline; dim backdrop cancels), mt-service ProfileDetails round-trip, ProfileStore::update by id (id/position/active preserved, no dedupe on explicit edit), primitive + capability-regression validation (a never-convertible profile edits freely); (2) exit-IP while OFFLINE — temp_link_for() one-shot factory link through the active profile, ExitInfo.via_temp_tunnel chip; (3) speed test — live session for the active profile else temp tunnel, settings.speed_test_url (cloudflare __down 25MB default, plain-HTTP verified), clock starts after connect, 5s stall/30s budget/64MiB caps, ≥400 rejected, SpeedTestInfo chips; (4) diagnostics mode — src/diagnostics.rs redacted paste-ready file (header + LogLine tee + hand-rolled global tracing subscriber, no tracing-subscriber dep, engine level forced debug, redaction test-pinned), Settings card + Open log folder; (5) kill switch — give_up() fail-closed at 127.0.0.1:9, review fix: app BINDS the port while engaged (spawn_blackhole swallows connections; dead-port fallback documented), stop_blackhole on every restore path (both disconnect branches, toggle-off, connect step-0), explicit disconnect restores with no session, Status "KILL SWITCH ACTIVE" banner (Failed+armed, pure gate tested); (6) code review — Copilot unavailable (no entitlement → reviewer silently dropped; models.github.ai proxy-intercepted "OK"), 3-round discussion-based review on the strongest reachable model instead, 4 valid findings fixed (port pre-bind ownership, banner, edit-failure logging, chained diag-path refresh), 7 assessed non-issues documented (transcript /tmp/s19-review-transcript.md, copied to harness/memory/agent-reports/s19-review-transcript.md); review PR #1 (base v0.12-line) left open for an entitled-machine Copilot pass. 466/0 (+19); gates fmt/clippy host+windows-gnu; iced 6/0; smoke 0; rig sf=1.0+1.5 (52.0 Mbps chips, offline exit-IP chip, modal fields, rename→save, Cancel+backdrop, kill-switch + diagnostics toggles, banner gate unit-tested); PE32+ ×4 (multitunnel 19.2MB, mt-app 14.9MB, tray 10.1MB, merge-server 2.4MB) + ELF ×2. LESSONS: z-ai CLI --thinking broken; window is 1080x720 (clicks beyond x=1080 miss); locate-then-click per scroll position; ENOSPC — stage binaries before wiping target trees.
diff --git a/harness/memory/STATE.md b/harness/memory/STATE.md
index e2b89a7..74ef545 100755
--- a/harness/memory/STATE.md
+++ b/harness/memory/STATE.md
@@ -1,44 +1,73 @@
-# STATE — rewritten at the end of every stage. Last update: S18 (tray tooltip/icon rework + color wheel + theme modes + research-driven scheduler refinements, v0.12.0)
+# STATE — rewritten at the end of every stage. Last update: S19 (server editing + offline probes + speed test + diagnostics + kill switch, v0.13.0)
 
 ## Current position
-- Stage: S18 IMPLEMENTED + all local gates green, releasing v0.12.0.
-- The user's six asks shipped (D28): (1) 3-line tray tooltip —
-  `MultiTunnel v0.12.0` / `Total ↓x ↑y` / `↓x/s ↑y/s` (CRLF, ≤70 NIF_TIP
-  units worst case); (2) ALL runtime icons (tray ICOs, window/taskbar
-  logo_64, titlebar logo_24) transparent-background + hue-preserving
-  contrast boost + tighter margin (0.08→0.04); (3) Settings accent COLOR
-  WHEEL (256px HSV disc asset + inverse hit-test/marker math in theme.rs,
-  live re-theme during drag, persist on release); (4) System/Light/Dark
-  mode with OS detection (env seam → Windows AppsUseLightTheme →
-  gsettings/gtk) re-polled every 5 s in System mode, legacy dark_mode
-  kept as the forward-compat shadow; (5) docs/research/ bonding survey —
-  BLEST tie-guard + ECF completion scheduling IMPLEMENTED in mt-bond
-  (reconciled with S14: the guard only narrows the min-score tied set),
-  redundant-mode/FEC/reinjection/delay-signal roadmap documented; (6)
-  docs/research/ UI changelog survey (CVR/v2rayN/FlClash/Hiddify/
-  Speedify) with a prioritized adopt-next list.
-- 447/0 workspace tests; fmt/clippy host+windows-gnu; vendored iced 6/0;
-  zigbuild PE32+ ×4 + ELF ×2; rig verified sf=1.0+1.5 (wheel pick
-  purple→green end-to-end, mode buttons light↔dark, system env boot,
-  emblem legibility both modes).
-- Sandbox recycled AGAIN before the stage (registry/rustup/zig/xkblibs
-  gone, tools/zig tarball truncated): full D2 bootstrap re-run, env.sh
-  rewritten; disk hit 100% twice — stale /tmp/my-project snapshots +
-  mt-target/debug cleaned.
+- Stage: S19 IMPLEMENTED, all local gates green, review fixes in,
+  releasing v0.13.0.
+- The user's six asks shipped: (1) SERVER EDITING — Servers tab per-card
+  Edit button → full-window modal (name/remark/address/port/UUID +
+  REALITY fields for reality profiles); mt-service ProfileDetails
+  round-trip (profile_details → update_profile); ProfileStore::update
+  replaces BY ID in place (id/position/active preserved, no dedupe on
+  explicit edit); RealService validates primitives + a
+  capability-regression check (an edit that would break a convertible
+  profile is rejected with the reason). (2) EXIT IP WHILE OFFLINE —
+  exit_probe prefers the live session, falls back to
+  temp_link_for() (the same factory path test_latency uses);
+  ExitInfo.via_temp_tunnel drives a "temporary probe tunnel" chip.
+  (3) SPEED TEST — Status card; speed_test(None) rides the live session
+  for the active profile (else the temp tunnel); downloads
+  settings.speed_test_url (default
+  http://speed.cloudflare.com/__down?bytes=25000000, plain-HTTP verified);
+  clock starts after connect (engine spawn excluded); 5 s stall / 30 s
+  budget / 64 MiB caps; ≥400 rejected. (4) DIAGNOSTICS MODE —
+  src/diagnostics.rs: one redacted paste-ready
+  <log_dir>/diagnostics-YYYYMMDD-HHMMSS.log (header + GUI LogLine tee +
+  ALL tracing events via a hand-rolled global subscriber; engine level
+  forced debug; UUID/pbk never written — test-pinned); Settings card
+  with the path + "Open log folder". (5) KILL SWITCH — give_up()
+  fail-CLOSED: system proxy re-pointed at 127.0.0.1:9 (review fix: the
+  app BINDS the port itself while engaged — spawn_blackhole accept loop
+  swallows connections — so a local process cannot pre-bind it); every
+  restore path releases it; explicit disconnect restores even with no
+  session; Status banner "KILL SWITCH ACTIVE" when Failed + armed.
+  (6) CODE REVIEW — Copilot channels unavailable from the sandbox (no
+  account entitlement → reviewer requests silently dropped;
+  models.github.ai proxy-intercepted): a 3-round discussion-based review
+  ran on the strongest reachable model instead (z-ai chat), 4 findings
+  fixed (above), the rest assessed + documented in the transcript;
+  review PR #1 left open (base v0.12-line) for a Copilot pass from an
+  entitled machine.
+- 466/0 workspace tests (+19 vs v0.12); fmt/clippy host+windows-gnu;
+  vendored iced 6/0; smoke 0; rig sf=1.0+1.5 VLM-verified (52.0 Mbps
+  chip row, offline exit-IP chip, modal fields + rename→save→card
+  updated, Cancel + backdrop dismissal, kill switch + diagnostics
+  toggles); PE32+ ×4 + ELF ×2; version 0.13.0.
 
 ## Next actions
-1. Tag v0.12.0 → release.yml; watch the run; verify assets by
-   download+unzip (PE32+ ×2 in zip, ELFs, README/QUICKSTART carried).
-2. User-side Windows verification: 3-line hover tooltip, icon size/
-   transparency on light AND dark taskbars, wheel + System mode against
-   the real OS theme flip (Settings → Personalization → Color).
-3. Backlog candidates from the research docs when asked: redundant bond
-   mode (calls), FEC parity frames, reinjection, per-link live status
-   badges, exit-IP auto-refresh, empty states.
+1. Tag v0.13.0 → release.yml; watch the run; verify assets by
+   download+unzip (PE32+ in zip, ELFs, README/QUICKSTART carried).
+2. User-side Windows verification: Edit dialog against a real profile
+   (save + reconnect-apply), speed test + offline exit IP against the
+   real server, diagnostics file paste-readiness (turn on, connect,
+   reproduce an issue, paste), kill switch behavior after killing the
+   VM side.
+3. Review PR #1: request Copilot from an entitled machine
+   (`gh pr edit 1 --add-reviewer Copilot`) if the account gains a
+   Copilot seat; otherwise close it after the release.
 
 ## Watch items (unchanged unless noted)
-- Disk: /tmp/mt-target release trees ~4.5G; clean debug trees after gate
-  runs; the 100%-full rootfs manifests as linker "signal 7" or ENOSPC
-  temp-dir panics (hit BOTH this stage).
-- Background processes DO NOT survive between sandbox tool calls — long
-  builds must run in the foreground (setsid/nohup both reaped).
+- Disk: 9.9G rootfs — the release zigbuild + debug trees TOGETHER do not
+  fit; clean /tmp/mt-target/debug before release builds (hit ENOSPC +
+  ld signal 7 twice this stage). Stage release binaries in
+  ../download/v0.13.0-staging/ before wiping target trees.
+- Sandbox recycles cost: registry src/ wiped (rm -rf
+  $CARGO_HOME/registry/src re-extracts), ziglang lib/std wiped
+  (pip reinstall ziglang==0.14.1), zig symlink missing (recreate
+  tools/zig/zig → pypkgs/ziglang/zig), /tmp/xkblibs re-extract +
+  libxkbcommon-x11.so + libxcb-xkb1 symlinks, python-xlib pip install.
+- `z-ai chat --thinking` is BROKEN in the current CLI (instant exit 1)
+  — plain mode works; VLM `z-ai vision` works.
+- rig coordinate discipline: derive control coords from a screenshot of
+  the SAME scroll position before clicking (scroll-then-locate-then-
+  click in separate runs); the app window is 1080x720 — clicks beyond
+  x≈1080 land outside it.
diff --git a/harness/memory/agent-reports/s19-review-transcript.md b/harness/memory/agent-reports/s19-review-transcript.md
new file mode 100644
index 0000000..23fa01f
--- /dev/null
+++ b/harness/memory/agent-reports/s19-review-transcript.md
@@ -0,0 +1,269 @@
+# S19 code review discussion (v0.13.0)
+
+Model: z-ai CLI strongest chat model (Copilot unavailable from sandbox —
+no account entitlement for PR review; models.github.ai proxy-blocked)
+Diff: 4888fec..14e3e79 (core 63.8 KB + GUI 60.3 KB chunks)
+
+---
+
+## Round 1 — service core
+
+```
+# MultiTunnel S19 / v0.13.0 Review: Service Core Changes
+
+## Summary of Changes
+This diff adds several significant features to the MultiTunnel service core:
+1. Server editing functionality with profile details round-trip
+2. Exit-IP probe through a temporary tunnel when disconnected
+3. In-app speed test through the tunnel
+4. Diagnostics mode with global logging
+5. Kill switch functionality for fail-closed behavior
+
+Let's examine each area in detail.
+
+## Security Review
+
+### [major] Potential credential leakage in diagnostics header
+In `real_service.rs`, the `build_diag_header` method includes profile endpoints but correctly redacts sensitive fields. However, the `diagnostics.rs` module's `line` function appends all GUI log lines to the diagnostics file. While the header is redacted, any log line emitted by the application that might contain sensitive information (like connection errors with partial URLs) could be captured.
+
+**Recommendation**: Ensure all log messages are scrubbed of sensitive information before being passed to `diagnostics::line`.
+
+### [blocker] Kill switch black hole proxy could be manipulated
+The kill switch uses `127.0.0.1:9` as a black hole proxy, but this address could potentially be manipulated by a local attacker who could set up a service on that port to intercept traffic.
+
+**Recommendation**: Use a non-standard, high-numbered port that's unlikely to be used by other services, or implement a proper network namespace isolation for the kill switch behavior.
+
+## Correctness Review
+
+### [major] Race condition in diagnostics enable/disable
+The `diagnostics.rs` module uses a global static `SINK` protected by a `Mutex`. However, the `enable` and `disable` functions don't properly handle the case where multiple threads might try to enable/disable diagnostics simultaneously. The `AtomicBool` flag `ENABLED` is used for fast-path checks, but the file operations aren't atomic.
+
+**Recommendation**: Use a `tokio::sync::Mutex` instead of `std::sync::Mutex` to properly handle async operations and ensure thread safety.
+
+### [major] Potential deadlock in speed test
+In `real_service.rs`, the `speed_test` function uses `tokio::time::timeout` for the connection establishment, but then performs an unbounded read without any timeout after that. If the server stops sending data after the initial response, the test could hang indefinitely.
+
+**Recommendation**: Add a timeout for the entire download operation, not just the connection phase.
+
+### [minor] Memory usage in speed test
+The `speed_test` function reads data into a growing buffer without any size limits beyond the `SPEED_TEST_MAX_BYTES` constant. This could lead to high memory usage if the server sends more data than expected.
+
+**Recommendation**: Implement a fixed-size buffer with a sliding window approach to limit memory usage while still measuring throughput accurately.
+
+## Lifecycle Races
+
+### [major] Session state inconsistency in give_up
+The `give_up` function in `real_service.rs` has complex logic around the system proxy state. When the kill switch is enabled, it sets the proxy to the black hole but doesn't clear `sysproxy_set`. This could lead to inconsistent state if the disconnect sequence is interrupted.
+
+**Recommendation**: Ensure that the `sysproxy_set` flag accurately reflects the actual proxy state in all code paths.
+
+### [minor] Temporary link cleanup
+The `temp_link_for` function creates a temporary link for offline probes but doesn't have any explicit cleanup mechanism. While the engines are designed to die with the returned Arcs, there's no guarantee that this will happen in all error cases.
+
+**Recommendation**: Add explicit cleanup for temporary links in error cases, or ensure that the engine lifecycle is properly managed.
+
+## API Ergonomics
+
+### [minor] Profile update validation
+The `update_profile` function has good validation for required fields, but the error messages could be more specific. For example, the port validation just says "port must be a number 0-65535" without indicating which field failed validation.
+
+**Recommendation**: Include the field name in error messages to improve debugging experience.
+
+### [nit] Speed test URL validation
+The speed test URL validation only checks for HTTP URLs, which is correct given the zero-dependency rule, but the error message could be more explicit about why HTTPS isn't supported.
+
+**Recommendation**: Update the error message to mention that only HTTP URLs are supported due to the zero-dependency architecture.
+
+## Honest-UI Review
+
+### [major] Missing user feedback for kill switch
+When the kill switch engages due to a spontaneous tunnel death, the user gets a log message, but there's no UI indication that their traffic is being blocked. This could lead to confusion if the user isn't monitoring the logs.
+
+**Recommendation**: Add a UI indicator (like a red banner or status icon) when the kill switch is active.
+
+### [minor] Speed test progress indication
+The speed test runs without any progress indication in the UI. Users might think the app is frozen if the test takes a long time.
+
+**Recommendation**: Add a progress indicator or status updates during the speed test.
+
+## Overall Assessment
+
+The changes are well-structured and implement the requested features correctly. The security aspects are mostly handled well, with proper redaction in the diagnostics mode. The kill switch implementation is a good addition for fail-closed behavior.
+
+The main concerns are around race conditions in the diagnostics module and potential deadlocks in the speed test. These should be addressed before release.
+
+## Summary of Findings
+
+1. [major] Race condition in diagnostics enable/disable due to improper mutex usage
+2. [major] Potential deadlock in speed test due to unbounded read after connection
+3. [major] Missing UI feedback when kill switch is active
+4. [blocker] Kill switch black hole proxy could be manipulated by local attacker
+5. [minor] Memory usage in speed test could be optimized
+6. [minor] Profile update error messages could be more specific
+7. [nit] Speed test URL validation error message could be more explicit
+
+The code is generally well-written and follows the project's invariants, but the identified issues should be addressed before release.
+```
+
+---
+
+## Round 2 — GUI + contract
+
+```
+# MultiTunnel S19 / v0.13.0 Review: Service Contract and GUI Changes
+
+## Summary of Changes
+This diff adds several significant features to the MultiTunnel service contract and GUI:
+1. Server editing functionality with profile details round-trip
+2. Exit-IP probe through a temporary tunnel when disconnected
+3. In-app speed test through the tunnel
+4. Diagnostics mode with global logging
+5. Kill switch functionality for fail-closed behavior
+
+Let's examine each area in detail.
+
+## Security Review
+
+### [major] Potential credential exposure in server editing
+The `EditDraft` struct in `app.rs` holds all profile fields including sensitive credentials like identity (UUID/password) in plain text. While the service correctly validates and updates the profile, there's no explicit sanitization when displaying error messages in the UI.
+
+**Recommendation**: Ensure error messages from `update_profile` are sanitized to never include credential values, even in partial form.
+
+### [major] No input sanitization in server editing
+The `edit_field_row` function in `screens.rs` directly accepts user input for all fields without sanitization. While the service validates on save, malicious input could potentially cause UI issues or unexpected behavior.
+
+**Recommendation**: Add basic input sanitization for special characters, especially in fields like tag, remark, and address that might be displayed in the UI.
+
+## Correctness Review
+
+### [blocker] Race condition in speed test state
+In `app.rs`, the `SpeedTest` message handler doesn't check if a speed test is already in progress before starting a new one. While there's a `speed_testing` flag, it's only checked after the message is processed, allowing multiple speed tests to be queued.
+
+**Recommendation**: Move the `speed_testing` check to the beginning of the `SpeedTest` message handler to prevent race conditions.
+
+### [major] Inconsistent error handling in server editing
+The `EditSaved` message handler in `app.rs` only shows errors in the UI but doesn't log them. This makes troubleshooting difficult for users and support.
+
+**Recommendation**: Log errors when saving a profile fails, in addition to showing them in the UI.
+
+### [minor] No validation for speed test URL
+The `SpeedUrlChanged` message handler in `app.rs` accepts any string as the speed test URL without validation. This could lead to unexpected behavior if an invalid URL is set.
+
+**Recommendation**: Add validation for the speed test URL to ensure it's a valid HTTP URL.
+
+## Lifecycle Races
+
+### [major] Potential race in diagnostics toggle
+The `DiagnosticsToggled` message handler in `app.rs` saves settings and then fetches the diagnostics path in a batch task. If the service fails to save settings, the diagnostics path might still be fetched, leading to inconsistent state.
+
+**Recommendation**: Only fetch the diagnostics path after the settings are successfully saved.
+
+### [minor] No cleanup for temporary speed test
+The speed test creates a temporary tunnel when disconnected, but there's no explicit cleanup mechanism if the test fails or is cancelled.
+
+**Recommendation**: Ensure proper cleanup of temporary tunnels in all error cases.
+
+## API Ergonomics
+
+### [major] Profile update error messages lack context
+The `update_profile` function in `mock.rs` returns generic error messages like "port must be a number 0-65535" without indicating which field failed validation.
+
+**Recommendation**: Include the field name in error messages to help users identify which field needs correction.
+
+### [minor] Speed test URL field label is unclear
+The settings screen in `screens.rs` labels the speed test URL field as "speed test URL" without explaining that it should be an HTTP URL.
+
+**Recommendation**: Update the label to "HTTP speed test URL" to clarify the expected format.
+
+## Honest-UI Review
+
+### [major] No feedback for kill switch activation
+When the kill switch engages due to a spontaneous tunnel death, the UI doesn't indicate that traffic is being blocked. This could confuse users who aren't monitoring the logs.
+
+**Recommendation**: Add a visual indicator (like a red banner or status icon) when the kill switch is active.
+
+### [minor] Speed test progress is unclear
+The speed test shows "testing… (up to 30 s)" but doesn't provide any indication of actual progress during the test.
+
+**Recommendation**: Add a progress bar or percentage indicator during the speed test.
+
+### [minor] No confirmation for diagnostics file deletion
+The diagnostics file remains on disk even after diagnostics mode is turned off, but there's no UI indication of this or option to delete the file.
+
+**Recommendation**: Add a button to delete the diagnostics file when diagnostics mode is turned off.
+
+## Summary of Findings
+
+1. [blocker] Race condition in speed test state handling
+2. [major] Potential credential exposure in server editing error messages
+3. [major] No feedback for kill switch activation in UI
+4. [major] Inconsistent error handling in server editing (missing logs)
+5. [major] Profile update error messages lack context
+6. [major] Potential race in diagnostics toggle
+7. [minor] No input sanitization in server editing
+8. [minor] No validation for speed test URL
+9. [minor] Speed test progress is unclear
+10. [minor] No confirmation for diagnostics file deletion
+
+The implementation of the new features is generally solid, but there are several important issues around error handling, race conditions, and user feedback that should be addressed before release. The server editing functionality is well-designed but needs better error context and sanitization. The kill switch and diagnostics features are good additions but need better UI feedback.
+```
+
+---
+
+## Round 3 — follow-up discussion
+
+```
+# MultiTunnel S19 / v0.13.0 Final Review
+
+## Follow-up Questions Answered
+
+### 1. Kill Switch Race Concerns
+The kill switch implementation appears sound from a consistency perspective. `give_up()` reads the settings atomically from the file, and `set_settings` persists atomically. The `connect_flow` also reads settings at connect time, so there's a potential race where a settings change could occur between connect and give_up, but this would be a normal race condition that doesn't violate fail-safe behavior.
+
+**Assessment**: No blocker issue here. The fail-closed vs fail-open decision is made consistently within each operation based on the current settings at that time.
+
+### 2. Diagnostics Subscriber Implementation
+The global subscriber with an atomic flag and mutex-protected file writing introduces some overhead on every tracing event, but it's acceptable for a diagnostics feature that's typically disabled. The atomic flag provides a fast path when diagnostics are disabled, and the mutex only contends when multiple threads are logging simultaneously with diagnostics enabled.
+
+**Assessment**: This is a reasonable implementation for a debugging feature. No correctness issues identified.
+
+### 3. Profile Update Capability Regression
+The capability-regression check compares endpoints, but there are indeed potential profile corruption scenarios that wouldn't be caught by endpoint comparison alone. Changes to `spiderX`, `flow`, or transport settings could break a profile even if the endpoint remains the same.
+
+**Assessment**: [major] The validation should be expanded to check all critical configuration fields, not just the endpoint. This could lead to profiles that appear valid but fail to connect.
+
+### 4. Speed Test Byte Accounting
+The head-peeling approach that counts body bytes as `head.len() - split` is vulnerable to chunked encoding and malformed responses. If a response uses chunked transfer encoding, the actual data length won't match the content length header, and the measurement would be incorrect.
+
+**Assessment**: [major] The speed test should handle chunked encoding properly by either disabling it in the request or correctly parsing chunked responses to get accurate byte counts.
+
+### 5. Top 3 Changes Before Shipping
+
+1. [blocker] Fix the speed test race condition - Multiple concurrent tests could cause undefined behavior
+2. [major] Expand profile validation to check all critical fields, not just endpoints
+3. [major] Fix speed test to handle chunked encoding correctly for accurate measurements
+
+## Final Assessment
+
+The implementation is generally solid with well-designed features, but there are several important issues that need addressing before release:
+
+### Critical Issues:
+1. [blocker] Speed test race condition in `app.rs` - The `SpeedTest` handler must check `speed_testing` flag before proceeding
+2. [major] Profile validation is incomplete - Should check all critical fields, not just endpoints
+3. [major] Speed test byte accounting is flawed - Doesn't handle chunked encoding correctly
+
+### Important Issues:
+4. [major] Potential credential exposure in error messages from server editing
+5. [major] No UI feedback when kill switch activates
+6. [major] Inconsistent error handling in server editing (missing logs)
+
+### Minor Issues:
+7. [minor] No input sanitization in server editing UI
+8. [minor] No validation for speed test URL
+9. [minor] Speed test progress is unclear
+10. [minor] No confirmation for diagnostics file deletion
+
+The server editing functionality is well-designed but needs better error context and sanitization. The kill switch and diagnostics features are good additions but need better UI feedback. The speed test implementation has critical correctness issues that must be fixed.
+
+Overall, the changes are ready for release once the critical and major issues are addressed, particularly the race condition in speed testing and the incomplete profile validation.
+```
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index 829d397..ae089af 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "4888fec 2026-11-21T08:27:47Z",
+  "as_of": "001d4e0 2026-11-21T13:50:09Z",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 265,
-  "edge_count": 274,
+  "node_count": 270,
+  "edge_count": 277,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -75,6 +75,26 @@
         }
       ]
     },
+    {
+      "id": "commit:001d4e0",
+      "type": "commit",
+      "title": "s19 review fixes: black-hole port OWNED while engaged, KILL SWITCH banner, edit-failure logging, chained diag-path refresh — (1) [blocker fix] the kill switch now BINDS 127.0.0.1:9 itself while engaged (spawn_blackhole accept loop swallows every connection) so a local process cannot pre-bind the black-hole port and turn it into its own proxy; bind failure falls back to the documented dead-port semantics (still fail-closed; Linux dev hosts without CAP_NET_BIND_SERVICE take the fallback); stop_blackhole wired into every proxy-restore path (both disconnect branches, the toggle-off escape hatch, connect step-0 self-heal) + owns/releases verified by test; (2) [major fix] Status shows a 'KILL SWITCH ACTIVE — traffic is blocked' error-container banner exactly when kill_switch && Failed (pure gate kill_switch_banner_visible, unit-tested) — the give_up state was previously only visible in the Logs tab; (3) [minor fix] a failed server edit ALSO lands in the Logs tab ('WARN server edit failed: …') — the dialog error was easy to dismiss and lose; (4) [minor fix] DiagnosticsToggled chains save→then(path refresh) instead of a batch (the batch could observe the pre-toggle path); (5) review findings assessed as non-issues (documented in the transcript): the speed-test 'race' (guard exists at handler entry), 'unbounded read' (per-read 5s stall + 30s budget + 64MiB cap), 'memory growth' (bytes counted, never accumulated), diagnostics mutex 'race' (no await under the lock), chunked-encoding skew (we measure received wire bytes; framing overhead ~1e-4), update_profile field coverage ",
+      "tags": [
+        "2026-11-21"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "001d4e0"
+        }
+      ],
+      "passages": [
+        {
+          "text": "s19 review fixes: black-hole port OWNED while engaged, KILL SWITCH banner, edit-failure logging, chained diag-path refresh — (1) [blocker fix] the kill switch now BINDS 127.0.0.1:9 itself while engaged (spawn_blackhole accept loop swallows every connection) so a local process cannot pre-bind the black-hole port and turn it into its own proxy; bind failure falls back to the documented dead-port semantics (still fail-closed; Linux dev hosts without CAP_NET_BIND_SERVICE take the fallback); stop_blackhole wired into every proxy-restore path (both disconnect branches, the toggle-off escape hatch, connect step-0 self-heal) + owns/releases verified by test; (2) [major fix] Status shows a 'KILL SWITCH ACTIVE — traffic is blocked' error-container banner exactly when kill_switch && Failed (pure gate kill_switch_banner_visible, unit-tested) — the give_up state was previously only visible in the Logs tab; (3) [minor fix] a failed server edit ALSO lands in the Logs tab ('WARN server edit failed: …') — the dialog error was easy to dismiss and lose; (4) [minor fix] DiagnosticsToggled chains save→then(path refresh) instead of a batch (the batch could observe the pre-toggle path); (5) review findings assessed as non-issues (documented in the transcript): the speed-test 'race' (guard exists at handler entry), 'unbounded read' (per-read 5s stall + 30s budget + 64MiB cap), 'memory growth' (bytes counted, never accumulated), diagnostics mutex 'race' (no await under the lock), chunked-encoding skew (we measure received wire bytes; framing overhead ~1e-4), update_profile field coverage ",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:02c8759",
       "type": "commit",
@@ -255,6 +275,26 @@
         }
       ]
     },
+    {
+      "id": "commit:14e3e79",
+      "type": "commit",
+      "title": "s19: server editing, offline exit-IP + speed test, diagnostics mode, kill switch — (1) SERVER EDITING: Servers tab per-card Edit button opens a full-window modal (dim backdrop cancels on click; S16 overlay discipline); fields: name, remark, address, port, UUID + REALITY set (SNI/fingerprint/pbk/sid/spiderX, reality profiles only); mt-service ProfileDetails round-trips profile_details→update_profile; ProfileStore::update replaces BY ID in place (id, list position, active preserved; NO dedupe on explicit edit); RealService validates primitives (empty name/address/identity, port 0-65535) + capability-regression check (an edit that would break a convertible profile is rejected with the reason; a never-convertible profile edits freely); editing the active server while connected logs 'reconnect to apply'. (2) EXIT IP WHILE OFFLINE: exit_probe prefers the LIVE session, falls back to temp_link_for() — a one-shot factory link through the active profile (same engine path as test_latency); ExitInfo.via_temp_tunnel flags the path for the GUI chip ('temporary probe tunnel' vs 'live tunnel'); shared http_get()/parse_exit_info() helpers; EXIT_PROBE_MAX_BYTES 64 KiB cap.",
+      "tags": [
+        "2026-11-21"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "14e3e79"
+        }
+      ],
+      "passages": [
+        {
+          "text": "s19: server editing, offline exit-IP + speed test, diagnostics mode, kill switch — (1) SERVER EDITING: Servers tab per-card Edit button opens a full-window modal (dim backdrop cancels on click; S16 overlay discipline); fields: name, remark, address, port, UUID + REALITY set (SNI/fingerprint/pbk/sid/spiderX, reality profiles only); mt-service ProfileDetails round-trips profile_details→update_profile; ProfileStore::update replaces BY ID in place (id, list position, active preserved; NO dedupe on explicit edit); RealService validates primitives (empty name/address/identity, port 0-65535) + capability-regression check (an edit that would break a convertible profile is rejected with the reason; a never-convertible profile edits freely); editing the active server while connected logs 'reconnect to apply'. (2) EXIT IP WHILE OFFLINE: exit_probe prefers the LIVE session, falls back to temp_link_for() — a one-shot factory link through the active profile (same engine path as test_latency); ExitInfo.via_temp_tunnel flags the path for the GUI chip ('temporary probe tunnel' vs 'live tunnel'); shared http_get()/parse_exit_info() helpers; EXIT_PROBE_MAX_BYTES 64 KiB cap.",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:166fcf6",
       "type": "commit",
@@ -2055,6 +2095,26 @@
         }
       ]
     },
+    {
+      "id": "commit:fac1a42",
+      "type": "commit",
+      "title": "backup(s19-impl): 2026-11-21T13:22:22Z [no changes]",
+      "tags": [
+        "2026-11-21"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "fac1a42"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s19-impl): 2026-11-21T13:22:22Z [no changes]",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:fe613ab",
       "type": "commit",
@@ -4458,6 +4518,27 @@
         }
       ]
     },
+    {
+      "id": "ledger:s19",
+      "type": "ledger",
+      "title": "Stage s19 ledger",
+      "tags": [
+        "s19",
+        "append-only"
+      ],
+      "sources": [
+        {
+          "file": "harness/memory/LEDGER.md",
+          "anchor": "s19"
+        }
+      ],
+      "passages": [
+        {
+          "text": "2026-11-21T14:20Z | IMPLEMENTED (six asks, v0.13.0): (1) server editing — Servers per-card Edit → full-window modal (S16 overlay discipline; dim backdrop cancels), mt-service ProfileDetails round-trip, ProfileStore::update by id (id/position/active preserved, no dedupe on explicit edit), primitive + capability-regression validation (a never-convertible profile edits freely); (2) exit-IP while OFFLINE — temp_link_for() one-shot factory link through the active profile, ExitInfo.via_temp_tunnel chip; (3) speed test — live session for the active profile else temp tunnel, settings.speed_test_url (cloudflare __down 25MB default, plain-HTTP verified), clock starts after connect, 5s stall/30s budget/64MiB caps, ≥400 rejected, SpeedTestInfo chips; (4) diagnostics mode — src/diagnostics.rs redacted paste-ready file (header + LogLine tee + hand-rolled global tracing subscriber, no tracing-subscriber dep, engine level forced debug, redaction test-pinned), Settings card + Open log folder; (5) kill switch — give_up() fail-closed at 127.0.0.1:9, review fix: app BINDS the port while engaged (spawn_blackhole swallows connections; dead-port fallback documented), stop_blackhole on every restore path (both disconnect branches, toggle-off, connect step-0), explicit disconnect restores with no session, Status \"KILL SWITCH ACTIVE\" banner (Failed+armed, pure gate tested); (6) code review — Copilot unavailable (no entitlement → reviewer silently dropped; models.github.ai proxy-intercepted \"OK\"), 3-round discussion-based review on the strongest reachable model instead, 4 valid findings fixed (port ",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "ledger:s2",
       "type": "ledger",
@@ -5258,7 +5339,7 @@
       ],
       "passages": [
         {
-          "text": "- **Stage**: IDLE — S18 CLOSED, v0.12.0 released and verified (release run 44879546925 first-try SUCCESS; the ci windows leg was a 69-min cold-cache cancel with same-commit green proof from the release's own build-windows — see LEDGER 08:30Z). The six asks shipped (D28): 3-line tray tooltip (name+version / totals / speeds, CRLF), transparent-background contrast-boosted LARGER runtime icons (tray ICOs + logo_64 + logo_24), Settings accent COLOR WHEEL (HSV disc asset + inverse marker math, live re-theme, persist on release), System/Light/Dark mode (OS detection chain: MT_THEME_SYSTEM_DARK env seam → Win32 AppsUseLightTheme → gsettings/gtk; 5 s repoll; legacy dark_mode kept as the forward-compat shadow), mt-bond BLEST tie-guard + ECF completion scheduling (papers survey: docs/research/), UI changelog survey (docs/research/). 447/0 tests; rig verified sf=1.0+1.5. - **Prior stages**: S18 (v0.12.0, D28: 3-line tray tooltip; transparent contrast-boosted larger icons everywhere; Settings accent COLOR WHEEL; System/Light/Dark mode with OS detection; mt-bond BLEST tie-guard + ECF scheduling per the docs/research/ papers survey; UI changelog survey), S17 (v0.11.0, titlebar version + live speeds, logs floating jump buttons + autoscroll).",
+          "text": "- **Stage**: S19 IMPLEMENTED, gates green, v0.13.0 releasing — the six asks shipped: (1) server EDITING (Servers → per-card Edit button → full-window modal; mt-service ProfileDetails round-trip via profile_details/update_profile; ProfileStore::update replaces BY ID in place — id/position/active preserved, no dedupe on explicit edits; primitive + capability-regression validation; editing the active server logs \"reconnect to apply\"); (2) exit-IP while OFFLINE (exit_probe falls back to temp_link_for — a one-shot factory link through the active profile, same engine path as test_latency; ExitInfo.via_temp_tunnel → \"temporary probe tunnel\" chip); (3) SPEED TEST (Status card; rides the live session for the active profile else the temp tunnel; settings.speed_test_url default http://speed.cloudflare.com/__down?bytes=25000000; clock starts AFTER connect so engine spawn never pollutes the rate; 5 s stall / 30 s budget / 64 MiB caps; HTTP ≥400 rejected); (4) DIAGNOSTICS MODE (SettingsView.diagnostics; src/diagnostics.rs writes one redacted paste-ready <log_dir>/diagnostics-*.log — header + GUI LogLine tee + ALL tracing events via a hand-rolled process-global subscriber (the e2e PrintSubscriber pattern, no tracing-subscriber dep); engine log level forced debug while ON; redaction test-pinned; Settings card + Open log folder); (5) KILL SWITCH (SettingsView.kill_switch; give_up() is fail-CLOSED: system proxy → 127.0.0.1:9 via sysproxy::set (keep-first snapshot survives for the true restore); REVIEW FIX: the app BINDS port 9 itself while engaged (spawn_blackhole accept loop swallows ",
           "kind": "body"
         }
       ]
@@ -5731,6 +5812,122 @@
         }
       ]
     },
+    {
+      "id": "report:s19-review-transcript",
+      "type": "report",
+      "title": "S19 code review discussion (v0.13.0)",
+      "tags": [
+        "agent-reports"
+      ],
+      "sources": [
+        {
+          "file": "harness/memory/agent-reports/s19-review-transcript.md",
+          "anchor": ""
+        }
+      ],
+      "passages": [
+        {
+          "text": "# S19 code review discussion (v0.13.0)",
+          "kind": "title"
+        },
+        {
+          "text": "Model: z-ai CLI strongest chat model (Copilot unavailable from sandbox — no account entitlement for PR review; models.github.ai proxy-blocked) Diff: 4888fec..14e3e79 (core 63.8 KB + GUI 60.3 KB chunks)",
+          "kind": "body"
+        },
+        {
+          "text": "---",
+          "kind": "body"
+        },
+        {
+          "text": "## Round 1 — service core",
+          "kind": "title"
+        },
+        {
+          "text": "## Summary of Changes This diff adds several significant features to the MultiTunnel service core: 1. Server editing functionality with profile details round-trip 2. Exit-IP probe through a temporary tunnel when disconnected 3. In-app speed test through the tunnel 4. Diagnostics mode with global logging 5. Kill switch functionality for fail-closed behavior",
+          "kind": "title"
+        },
+        {
+          "text": "Let's examine each area in detail.",
+          "kind": "body"
+        },
+        {
+          "text": "## Security Review",
+          "kind": "title"
+        },
+        {
+          "text": "### [major] Potential credential leakage in diagnostics header In `real_service.rs`, the `build_diag_header` method includes profile endpoints but correctly redacts sensitive fields. However, the `diagnostics.rs` module's `line` function appends all GUI log lines to the diagnostics file. While the header is redacted, any log line emitted by the application that might contain sensitive information (like connection errors with partial URLs) could be captured.",
+          "kind": "title"
+        },
+        {
+          "text": "**Recommendation**: Ensure all log messages are scrubbed of sensitive information before being passed to `diagnostics::line`.",
+          "kind": "body"
+        },
+        {
+          "text": "### [blocker] Kill switch black hole proxy could be manipulated The kill switch uses `127.0.0.1:9` as a black hole proxy, but this address could potentially be manipulated by a local attacker who could set up a service on that port to intercept traffic.",
+          "kind": "title"
+        },
+        {
+          "text": "**Recommendation**: Use a non-standard, high-numbered port that's unlikely to be used by other services, or implement a proper network namespace isolation for the kill switch behavior.",
+          "kind": "body"
+        },
+        {
+          "text": "## Correctness Review",
+          "kind": "title"
+        },
+        {
+          "text": "### [major] Race condition in diagnostics enable/disable The `diagnostics.rs` module uses a global static `SINK` protected by a `Mutex`. However, the `enable` and `disable` functions don't properly handle the case where multiple threads might try to enable/disable diagnostics simultaneously. The `AtomicBool` flag `ENABLED` is used for fast-path checks, but the file operations aren't atomic.",
+          "kind": "title"
+        },
+        {
+          "text": "**Recommendation**: Use a `tokio::sync::Mutex` instead of `std::sync::Mutex` to properly handle async operations and ensure thread safety.",
+          "kind": "body"
+        },
+        {
+          "text": "### [major] Potential deadlock in speed test In `real_service.rs`, the `speed_test` function uses `tokio::time::timeout` for the connection establishment, but then performs an unbounded read without any timeout after that. If the server stops sending data after the initial response, the test could hang indefinitely.",
+          "kind": "title"
+        },
+        {
+          "text": "**Recommendation**: Add a timeout for the entire download operation, not just the connection phase.",
+          "kind": "body"
+        },
+        {
+          "text": "### [minor] Memory usage in speed test The `speed_test` function reads data into a growing buffer without any size limits beyond the `SPEED_TEST_MAX_BYTES` constant. This could lead to high memory usage if the server sends more data than expected.",
+          "kind": "title"
+        },
+        {
+          "text": "**Recommendation**: Implement a fixed-size buffer with a sliding window approach to limit memory usage while still measuring throughput accurately.",
+          "kind": "body"
+        },
+        {
+          "text": "## Lifecycle Races",
+          "kind": "title"
+        },
+        {
+          "text": "### [major] Session state inconsistency in give_up The `give_up` function in `real_service.rs` has complex logic around the system proxy state. When the kill switch is enabled, it sets the proxy to the black hole but doesn't clear `sysproxy_set`. This could lead to inconsistent state if the disconnect sequence is interrupted.",
+          "kind": "title"
+        },
+        {
+          "text": "**Recommendation**: Ensure that the `sysproxy_set` flag accurately reflects the actual proxy state in all code paths.",
+          "kind": "body"
+        },
+        {
+          "text": "### [minor] Temporary link cleanup The `temp_link_for` function creates a temporary link for offline probes but doesn't have any explicit cleanup mechanism. While the engines are designed to die with the returned Arcs, there's no guarantee that this will happen in all error cases.",
+          "kind": "title"
+        },
+        {
+          "text": "**Recommendation**: Add explicit cleanup for temporary links in error cases, or ensure that the engine lifecycle is properly managed.",
+          "kind": "body"
+        },
+        {
+          "text": "## API Ergonomics",
+          "kind": "title"
+        },
+        {
+          "text": "### [minor] Profile update validation The `update_profile` function has good validation for required fields, but the error messages could be more specific. For example, the port validation just says \"port must be a number 0-65535\" without indicating which field failed validation.",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "report:s2-rev-b",
       "type": "report",
@@ -7318,7 +7515,7 @@
     {
       "id": "state:current",
       "type": "state",
-      "title": "STATE — current position (last update: S18 (tray tooltip/icon rework + color wheel + theme modes + research-driven scheduler refinements, v0.12.0)",
+      "title": "STATE — current position (last update: S19 (server editing + offline probes + speed test + diagnostics + kill switch, v0.13.0)",
       "tags": [
         "durable"
       ],
@@ -7330,19 +7527,19 @@
       ],
       "passages": [
         {
-          "text": "# STATE — rewritten at the end of every stage. Last update: S18 (tray tooltip/icon rework + color wheel + theme modes + research-driven scheduler refinements, v0.12.0)",
+          "text": "# STATE — rewritten at the end of every stage. Last update: S19 (server editing + offline probes + speed test + diagnostics + kill switch, v0.13.0)",
           "kind": "title"
         },
         {
-          "text": "## Current position - Stage: S18 IMPLEMENTED + all local gates green, releasing v0.12.0. - The user's six asks shipped (D28): (1) 3-line tray tooltip — `MultiTunnel v0.12.0` / `Total ↓x ↑y` / `↓x/s ↑y/s` (CRLF, ≤70 NIF_TIP units worst case); (2) ALL runtime icons (tray ICOs, window/taskbar logo_64, titlebar logo_24) transparent-background + hue-preserving contrast boost + tighter margin (0.08→0.04); (3) Settings accent COLOR WHEEL (256px HSV disc asset + inverse hit-test/marker math in theme.rs, live re-theme during drag, persist on release); (4) System/Light/Dark mode with OS detection (env seam → Windows AppsUseLightTheme → gsettings/gtk) re-polled every 5 s in System mode, legacy dark_mode kept as the forward-compat shadow; (5) docs/research/ bonding survey — BLEST tie-guard + ECF completion scheduling IMPLEMENTED in mt-bond (reconciled with S14: the guard only narrows the min-score tied set), redundant-mode/FEC/reinjection/delay-signal roadmap documented; (6) docs/research/ UI changelog survey (CVR/v2rayN/FlClash/Hiddify/ Speedify) with a prioritized adopt-next list. - 447/0 workspace tests; fmt/clippy host+windows-gnu; vendored iced 6/0; zigbuild PE32+ ×4 + ELF ×2; rig verified sf=1.0+1.5 (wheel pick purple→green end-to-end, mode buttons light↔dark, system env boot, emblem legibility both modes). - Sandbox recycled AGAIN before the stage (registry/rustup/zig/xkblibs gone, tools/zig tarball truncated): full D2 bootstrap re-run, env.sh rewritten; disk hit 100% twice — stale /tmp/my-project snapshots + mt-target/debug cleaned.",
+          "text": "## Current position - Stage: S19 IMPLEMENTED, all local gates green, review fixes in, releasing v0.13.0. - The user's six asks shipped: (1) SERVER EDITING — Servers tab per-card Edit button → full-window modal (name/remark/address/port/UUID + REALITY fields for reality profiles); mt-service ProfileDetails round-trip (profile_details → update_profile); ProfileStore::update replaces BY ID in place (id/position/active preserved, no dedupe on explicit edit); RealService validates primitives + a capability-regression check (an edit that would break a convertible profile is rejected with the reason). (2) EXIT IP WHILE OFFLINE — exit_probe prefers the live session, falls back to temp_link_for() (the same factory path test_latency uses); ExitInfo.via_temp_tunnel drives a \"temporary probe tunnel\" chip. (3) SPEED TEST — Status card; speed_test(None) rides the live session for the active profile (else the temp tunnel); downloads settings.speed_test_url (default http://speed.cloudflare.com/__down?bytes=25000000, plain-HTTP verified); clock starts after connect (engine spawn excluded); 5 s stall / 30 s budget / 64 MiB caps; ≥400 rejected. (4) DIAGNOSTICS MODE — src/diagnostics.rs: one redacted paste-ready <log_dir>/diagnostics-YYYYMMDD-HHMMSS.log (header + GUI LogLine tee + ALL tracing events via a hand-rolled global subscriber; engine level forced debug; UUID/pbk never written — test-pinned); Settings card with the path + \"Open log folder\".",
           "kind": "title"
         },
         {
-          "text": "## Next actions 1. Tag v0.12.0 → release.yml; watch the run; verify assets by download+unzip (PE32+ ×2 in zip, ELFs, README/QUICKSTART carried). 2. User-side Windows verification: 3-line hover tooltip, icon size/ transparency on light AND dark taskbars, wheel + System mode against the real OS theme flip (Settings → Personalization → Color). 3. Backlog candidates from the research docs when asked: redundant bond mode (calls), FEC parity frames, reinjection, per-link live status badges, exit-IP auto-refresh, empty states.",
+          "text": "## Next actions 1. Tag v0.13.0 → release.yml; watch the run; verify assets by download+unzip (PE32+ in zip, ELFs, README/QUICKSTART carried). 2. User-side Windows verification: Edit dialog against a real profile (save + reconnect-apply), speed test + offline exit IP against the real server, diagnostics file paste-readiness (turn on, connect, reproduce an issue, paste), kill switch behavior after killing the VM side. 3. Review PR #1: request Copilot from an entitled machine (`gh pr edit 1 --add-reviewer Copilot`) if the account gains a Copilot seat; otherwise close it after the release.",
           "kind": "title"
         },
         {
-          "text": "## Watch items (unchanged unless noted) - Disk: /tmp/mt-target release trees ~4.5G; clean debug trees after gate runs; the 100%-full rootfs manifests as linker \"signal 7\" or ENOSPC temp-dir panics (hit BOTH this stage). - Background processes DO NOT survive between sandbox tool calls — long builds must run in the foreground (setsid/nohup both reaped).",
+          "text": "## Watch items (unchanged unless noted) - Disk: 9.9G rootfs — the release zigbuild + debug trees TOGETHER do not fit; clean /tmp/mt-target/debug before release builds (hit ENOSPC + ld signal 7 twice this stage). Stage release binaries in ../download/v0.13.0-staging/ before wiping target trees. - Sandbox recycles cost: registry src/ wiped (rm -rf $CARGO_HOME/registry/src re-extracts), ziglang lib/std wiped (pip reinstall ziglang==0.14.1), zig symlink missing (recreate tools/zig/zig → pypkgs/ziglang/zig), /tmp/xkblibs re-extract + libxkbcommon-x11.so + libxcb-xkb1 symlinks, python-xlib pip install. - `z-ai chat --thinking` is BROKEN in the current CLI (instant exit 1) — plain mode works; VLM `z-ai vision` works. - rig coordinate discipline: derive control coords from a screenshot of the SAME scroll position before clicking (scroll-then-locate-then- click in separate runs); the app window is 1080x720 — clicks beyond x≈1080 land outside it.",
           "kind": "title"
         }
       ]
@@ -8065,6 +8262,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s6-review)"
     },
+    {
+      "src": "commit:fac1a42",
+      "dst": "ledger:s19",
+      "rel": "part_of",
+      "evidence": "subject stage label (s19)"
+    },
     {
       "src": "commit:fe613ab",
       "dst": "ledger:s2-close",
@@ -8635,6 +8838,12 @@
       "rel": "released_in",
       "evidence": "2026-11-21T07:17Z | v0.12.0 RELEASED: push attempt-1 REJECTED by GitHub — PROJECT_MEMORY.md had crossed the 100 MB per-file hard limit (the update_memory.py self-embedding recursion: each ritual run committed its own previous section into the file, so the next diff contained it — 81 MB at S17 close → 134 MB after this session's two runs; detected via the LFS-error push message, NOTHING reached origin). FIX: update_memory.py now excludes PROJECT_MEMORY.md's own diff (pathspec :(exclude) + an elision note), local history rewritten pre-push (reset --soft to origin/main, regenerated the section → 77 KB, re-committed s18 + re-tagged v0.12.0 — the oversized blobs never left the sandbox and are now unreachable). Attempt-2 push OK → release run 44879546925 SUCCESS on the first try (no cold-cache stall this time) + ci 44879546922 running. Assets verified by download+unzip: multitunnel-v0.12.0-windows-x64.zip 6,430,469 B integrity OK (multitunnel.exe PE32+ console + multitunnel-tray.exe PE32+ GUI + README v0.9–v0.12 section + QUICKSTART v0.12 note + server scripts + LICENSE + THIRD_PARTY_NOTICES + config.example) and mt-merge-server {x86_64 1,826,272 B, aarch64 1,860,808 B} ELFs."
     },
+    {
+      "src": "ledger:s19",
+      "dst": "project:multitunnel",
+      "rel": "part_of",
+      "evidence": "project ledger"
+    },
     {
       "src": "ledger:s2",
       "dst": "project:multitunnel",
@@ -8947,6 +9156,12 @@
       "rel": "reports_on",
       "evidence": "filename stage prefix"
     },
+    {
+      "src": "report:s19-review-transcript",
+      "dst": "ledger:s19",
+      "rel": "reports_on",
+      "evidence": "filename stage prefix"
+    },
     {
       "src": "report:s2-rev-b",
       "dst": "ledger:s2",

( PROJECT_MEMORY.md's own diff is elided here — the durable
  sections above are hand-maintained; see harness/memory/LEDGER.md
  for the full stage history. )
```

