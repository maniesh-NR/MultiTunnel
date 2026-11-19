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

- **Stage**: S12 CLOSED, v0.7.0 released (run 44537336705, assets verified).
  All three user reports shipped: A = chart fixes (v0.6.0/v0.6.1), B = stream
  bonding (v0.6.2: Xray finalRules + verified carriers), C = balanced/priority
  scheduling + adapter ordering UI (v0.7.0).
- **Pending — user side**: verify v0.7.0 (balanced default, per-adapter graphs
  even out, priority reorder via ↑/↓); if stream bonding still fails on the
  VM: `sudo bash server/setup-xray-reality.sh` (idempotent finalRules patch)
  and `sudo bash server/setup-merge-daemon.sh`.
- **Backlog (parked)**: subscription auto-update, vmess validation,
  profiles.json locking, engine log rotation, listener max_conns, upstream iced
  migration, S7 native REALITY core.
- **Watch items**: sandbox recycles (stale clone → fetch + rebase before
  building); disk headroom (release zigbuild needs ~5G; clean
  /tmp/mt-target/debug first); GitHub runner wedges (>45 min in cargo test
  while a sibling passed = cancel + rerun-failed-jobs).

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
### Commit: d241e42 - backup(s12-post-release): 2026-11-19T14:09:49Z (Z User, 2026-11-19)
diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
index ac34291..9b1f10c 100755
--- a/harness/memory/LEDGER.md
+++ b/harness/memory/LEDGER.md
@@ -178,3 +178,4 @@
 2026-11-19T13:50Z | s12 | VERIFIED: cargo test --workspace 396/0 (+18: mt-net balanced/priority/pressure/plateau/E2E-through-the-real-pumps, mt-bond pure picker ×6, root from_str/migration chains, mt-app ordering flow + sort primitive); vendored iced 6/0; fmt clean; clippy host + windows-gnu (zig shims) -D warnings clean; gui --smoke exit 0; zigbuild PE32+ ×4 (multitunnel 16.9MB, tray 9.7MB, mt-app 13MB, mt-merge-server 2.35MB) + linux mt-merge-server ELF 1.79MB. RIG (Xvfb+XTEST, pixel-verified — no VLM): the Adapters tab renders 3 bonded rows with #1/#2/#3 chips at x-offsets tracking name length; clicking a disabled ↑ (row 1) is a correct no-op; clicking ↓ swaps rows 1↔2 (chip x-centers 283↔305 — the swap is measurable); the Settings merge-policy pick_list opens a 5-row menu (change bands ×5 below the picker) and selecting row 3 changes the picker text and closes the menu; the Status tab (window resized to 1152 — the bottom card needs height) shows the same 3 priority chips on its adapter rows. NOTE: the iced default window is 1280x720 — at that height the Status adapters section sits below the fold; scroll inside the bottom card or resize taller.
 2026-11-19T13:50Z | s12 | v0.7.0: README (Scheduling & bonding modes rewritten for the five policies + priority ordering + stream-mode scheduler note), QUICKSTART (Steps 4/7), version 0.7.0. Local artifacts staged in sandbox download/: multitunnel-v0.7.0-windows-x64.zip (11,482,022 B, 8 entries, integrity OK) + mt-merge-server-v0.7.0-x86_64-unknown-linux-gnu (1,793,880 B ELF).
 2026-11-19T13:50Z | s12-merge | REBASE NOTE: this session's sandbox had recycled to a pre-S11 snapshot, so the scheduling work landed as ONE commit rebased onto the remote's v0.6.2 stream-bonding fixes (the first push was rejected; fetch showed origin/main had moved — finalRules script patch + verified carriers + tests/e2e_real_xray.rs). Conflicts resolved by MERGING BOTH: pick_carrier is policy-aware AND only considers VERIFIED carriers (usable()); send_bytes filters usable() AND tracks queued bytes; the GUI explainers carry both the priority-order wording and the finalRules/merge-daemon setup note; lib.rs exports VERIFY_PONG_TIMEOUT + PRIORITY_QUEUE_HIGH_WATER. Stage numbering: the remote session took S11 (stream bonding, v0.6.2); this work is S12. Full gates re-run after the rebase.
+2026-11-19T14:10Z | s12-release | v0.7.0 RELEASED. Rebased main (83c9ab8) pushed as backup(s12-close-v0.7.0); tag v0.7.0 → release run 44537336705 SUCCESS (~8 min: build-linux-daemon both ELFs + build-windows fmt/clippy/test/package). CI run 44537322307 on the same commit: lint + test(ubuntu) + cross-check GREEN quickly; test(windows-latest) cold-compiling in cargo test (Cargo.lock version bump → rust-cache invalidated, the documented pattern) — the release run's build-windows PASSED the identical suite on the same commit, which is the same-commit green proof (S9.1 precedent). Assets verified by download+unzip: multitunnel-v0.7.0-windows-x64.zip 6,001,721 B (integrity OK; multitunnel.exe PE32+ console + multitunnel-tray.exe PE32+ GUI; README carries the balanced/priority scheduling docs — 11 mentions; server/setup-xray-reality.sh carries the v0.6.2 finalRules patch — 17 mentions) + mt-merge-server {x86_64 1,805,664 B, aarch64 1,856,480 B} ELFs. Local artifacts re-staged in sandbox download/ post-rebase: multitunnel-v0.7.0-windows-x64.zip (11,502,768 B — the LOCAL zip excludes server/, the CI zip includes it; both valid) + mt-merge-server-v0.7.0-x86_64-unknown-linux-gnu. S12 CLOSED — the user's three asks shipped: (1) even byte distribution (balanced, new default, per-connection AND stream frame scheduling), (2) priority fill-order mode with saturation spill + automatic recovery, (3) adapter ordering UI (Adapters tab ↑/↓ + #n chips, Status chips, five-policy picker).

### Commit: 83c9ab8 - backup(s12-close-v0.7.0): 2026-11-19T13:49:34Z (Z User, 2026-11-19)
diff --git a/crates/mt-bond/src/lib.rs b/crates/mt-bond/src/lib.rs
index 91b46a7..3de15d4 100755
--- a/crates/mt-bond/src/lib.rs
+++ b/crates/mt-bond/src/lib.rs
@@ -33,8 +33,8 @@ pub use daemon::{serve, BOND_IDLE_GC};
 pub use endpoint::{
     CarrierSnapshot, APP_BACKLOG_CAP, CARRIER_RESPAWN_ATTEMPTS, CARRIER_RESPAWN_BACKOFF,
     DIAL_TIMEOUT, MAX_SESSIONS_PER_BOND, PENDING_OPEN_TIMEOUT, PING_INTERVAL, PONG_TIMEOUT,
-    PRIORITY_QUEUE_HIGH_WATER, REORDER_CAP, STALL_TIMEOUT, SWEEP_INTERVAL,
-    VERIFY_PONG_TIMEOUT, WRITER_QUEUE,
+    PRIORITY_QUEUE_HIGH_WATER, REORDER_CAP, STALL_TIMEOUT, SWEEP_INTERVAL, VERIFY_PONG_TIMEOUT,
+    WRITER_QUEUE,
 };
 pub use frame::{
     decode_header, decode_target, encode_target, read_frame, write_frame, CloseReason, Frame, Kind,

### Commit: 6e5502e - backup(s11-close-v0.7.0): 2026-11-19T13:38:15Z (Z User, 2026-11-19)
diff --git a/Cargo.lock b/Cargo.lock
index 6a2af28..184511e 100755
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -2036,7 +2036,7 @@ dependencies = [
 
 [[package]]
 name = "multitunnel"
-version = "0.6.2"
+version = "0.7.0"
 dependencies = [
  "anyhow",
  "async-trait",
diff --git a/Cargo.toml b/Cargo.toml
index 4d11611..d3a00f5 100755
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1,6 +1,6 @@
 [package]
 name = "multitunnel"
-version = "0.6.2"
+version = "0.7.0"
 edition = "2021"
 license = "MIT"
 description = "Windows supervisor that combines an Xray VLESS+REALITY tunnel with net-merge multi-link bonding"
diff --git a/QUICKSTART.md b/QUICKSTART.md
index d01a14b..3711850 100755
--- a/QUICKSTART.md
+++ b/QUICKSTART.md
@@ -102,19 +102,26 @@ from a terminal) is the first stop for clues.
   Status tab (switching while connected reconnects automatically).
   Latency tests run per profile from the Servers screen.
 - The data plane bonds multiple links in-process (merge policy in
-  Settings: **round-robin by default** — new connections alternate across
-  every ticked adapter, so both NICs carry traffic; `failover` keeps
-  everything on the first healthy adapter; `lowest-latency` picks the
-  lowest-RTT adapter per connection). The legacy external net-merge tool
-  is no longer needed; see the appendix if you used it with v0.2.
+  Settings: **balanced by default** — every new connection goes to the
+  adapter carrying the least traffic RIGHT NOW, so a big download stops
+  hogging one NIC and the spread evens out by BYTES; `priority` fills
+  the adapters in the order you set on the Adapters tab — #1 takes
+  everything until its bandwidth saturates, then the overflow spills to
+  #2, and so on; `round-robin` alternates by connection count;
+  `failover` keeps everything on the first healthy adapter;
+  `lowest-latency` picks the lowest-RTT adapter per connection). The
+  legacy external net-merge tool is no longer needed; see the appendix
+  if you used it with v0.2.
 - Every adapter row on the Adapters tab has an arrow (▸) that expands a
   live per-interface traffic graph — and since v0.6.1 the same graphs
   live on the Status tab: press an ADAPTER'S NAME in the bottom card to
   expand its chart. The same two color-coded lines as the Status graph
   (down = primary, up = tertiary), so you can SEE which NIC carries
-  what. Upgrading from v0.5.0? The stock merge policy migrates to
-  round-robin automatically (Settings shows it; pick `failover` there if
-  you really want the old behavior).
+  what. The ORDER of the ticked adapters (reorder with ↑/↓) is the
+  adapter priority used by the `priority` policy. Upgrading from
+  v0.6.x? The stock merge policy migrates to balanced automatically
+  (Settings shows it; pick `round-robin` there if you want the old
+  behavior).
 
 ## Step 5 (optional) — stream bonding: one connection, split across adapters
 
@@ -200,10 +207,11 @@ direct / reject). To import a bypass list like in v2rayN:
    `multitunnel routing import C:\path\to\list.txt`.
 
 Which adapter carries which connection is the merge policy's job
-(Settings -> Network): `round-robin` (default) alternates, and
-`lowest-latency` sends each connection over the link with the best
-smoothed RTT. To split ONE connection across both adapters at the same
-time, use bonding mode `stream` (Step 5).
+(Settings -> Network): `balanced` (default) sends each new connection to
+the least-loaded adapter, and `priority` fills the Adapters-tab order —
+#1 until its bandwidth saturates, then #2. To split ONE connection
+across both adapters at the same time, use bonding mode `stream`
+(Step 5).
 
 ## Appendix — the legacy v0.2 path (config.json + tray)
 
diff --git a/README.md b/README.md
index 77e209f..2eb5eab 100755
--- a/README.md
+++ b/README.md
@@ -39,7 +39,7 @@ whole path is about ten minutes.
 │    ├─ ProxyServer    (mt-net)            │    VLESS +     │  xray (systemd, User=nobody)       │
 │    │    SOCKS :10808 / HTTP :10809       │    REALITY     │    - listens on 0.0.0.0:8443       │
 │    │    Router: private CIDRs → direct   │ ═════════════▶ │    - REALITY borrows the TLS cert   │
-│    │    merge: round-robin (default)     │    TCP :8443   │      of the gateway already on     │
+│    │    merge: balanced (default)        │    TCP :8443   │      of the gateway already on     │
 │    ├─ ManagedEngine  (mt-engine)         │                │      127.0.0.1:443 (serverNames)   │
 │    │    one Xray child per link,         │                │    │    as its camouflage            │
 │    │    kill-on-close job, respawn       │                │    ▼ freedom outbound              │
@@ -56,7 +56,7 @@ contracts in `harness/CONTRACTS-S6.md`):
 | Crate | Role |
 |---|---|
 | `mt-links` | share-link parsing (`vless://` strict REALITY-only), profile store, subscriptions, v0.2 migration |
-| `mt-net` | the data plane: SOCKS5 + HTTP inbounds, routing table, link bonding (round-robin default / failover / lowest-latency), quarantine |
+| `mt-net` | the data plane: SOCKS5 + HTTP inbounds, routing table, link bonding (balanced default / round-robin / priority / failover / lowest-latency), quarantine, live per-link load tracking |
 | `mt-engine` | supervised Xray child processes: respawn with backoff, kill-on-drop + kill-on-close job objects |
 | `mt-m3` | Material You design tokens with real HCT dynamic color (material-colors) |
 | `mt-platform` | engine locator/self-download (SHA256SUMS-verified), autostart, platform dirs |
@@ -69,8 +69,8 @@ because MultiTunnel has set the WinINET system proxy to `127.0.0.1:10809`.
 The router sends private/LAN CIDRs direct and everything else through the
 VLESS+REALITY outbound; because mux is off, every proxied connection is its
 own TCP stream, so a multi-link setup can spread connections across
-uplinks (connection-level bonding, round-robin by default — failover and
-lowest-latency selectable). On the VM,
+uplinks (connection-level bonding, balanced by default — round-robin,
+priority, failover and lowest-latency selectable). On the VM,
 xray terminates REALITY on port 8443 — an unauthenticated handshake is
 proxied to the real gateway, which is what makes the port look like an
 ordinary HTTPS endpoint — and exits through a plain `freedom` outbound.
@@ -156,9 +156,10 @@ stream granularity:
   whose target is the merge daemon on the server (`127.0.0.1:<merge
   port>`, reached through the tunnel). No new ports are exposed.
 - `mt-bond` frames the streams (24-byte headers: HELLO/OPEN/DATA/CLOSE/
-  ACK/PING/PONG), stripes DATA frames round-robin across carriers,
-  reorders per session, and replays unacked frames on a carrier that
-  dies — a Wi-Fi drop mid-download costs zero bytes. Cumulative ACKs
+  ACK/PING/PONG), spreads DATA frames across carriers under the merge
+  policy (balanced = least writer backpressure, priority = user adapter
+  order), reorders per session, and replays unacked frames on a carrier
+  that dies — a Wi-Fi drop mid-download costs zero bytes. Cumulative ACKs
   bound the replay window; TCP-native backpressure provides flow control.
 - `mt-merge-server` (the daemon, installed by
   `server/setup-merge-daemon.sh`) accepts the carriers, re-merges each
@@ -228,22 +229,39 @@ same import is available headless: `multitunnel routing import
 <file>` (plus `multitunnel routing list`). A live session picks new
 rules up on the next reconnect.
 
-## Scheduling & bonding modes (v0.6) — beyond round-robin
+## Scheduling & bonding modes (v0.7) — even spreading and adapter priority
 
 The merge policy picks which bonded adapter carries each NEW connection
-(Advanced tab):
-
-- `round-robin` (default) — connections alternate across every ticked
-  adapter; both NICs carry traffic.
+(Settings → Network), and — in stream mode — how every connection's BYTES
+are striped across the carriers:
+
+- `balanced` (default, v0.7) — each new connection goes to the adapter
+  carrying the least traffic RIGHT NOW (a 1 s byte-rate estimate plus a
+  small per-connection reserve). A fat download stops hogging one NIC and
+  the spread evens out by BYTES, not by connection count. With three or
+  more adapters this is the fix for "one adapter carries almost
+  everything, the last one rarely sees traffic".
+- `round-robin` — connections alternate across every ticked adapter;
+  even by connection count, not by bytes (one big flow still dominates
+  its adapter).
+- `priority` (v0.7) — adapters fill in the priority order you set on the
+  Adapters tab (↑/↓, or tick to append): adapter #1 takes everything
+  until its measured bandwidth saturates (a ramp/plateau detector on its
+  live rate), then the overflow spills to #2, and so on. When #1's load
+  drops, it takes the traffic back automatically.
 - `lowest-latency` — each connection takes the adapter with the best
-  smoothed (EWMA) RTT, so the faster link naturally takes more
-  connections than a blind alternation. (v0.6 also fixes the policy
-  parser: the GUI's hyphenated names were silently falling back to
-  round-robin before.)
-- `failover` — everything rides the first healthy adapter; the second
-  stays idle until it dies.
-
-To use **both adapters for a single connection at the same time**, set
+  smoothed (EWMA) RTT.
+- `failover` — everything rides the first healthy adapter; the others
+  stay idle until it dies.
+
+In stream bonding mode the same policy drives the frame scheduler:
+`balanced` sends each frame down the carrier with the least writer
+backpressure (fast NICs naturally take more), `priority` fills carriers
+in the Adapters-tab order until one backs up. Upgrading from v0.6.x? The
+stock `round-robin` setting migrates to `balanced` once; an explicit
+re-choice of `round-robin` persists.
+
+To use **every adapter for a single connection at the same time**, set
 bonding mode to `stream` (Advanced): the bytes of every connection are
 split across all ticked adapters and re-merged by `mt-merge-server` on
 the tunnel server — see "Stream bonding (v0.5)" above.
diff --git a/crates/mt-app/src/app.rs b/crates/mt-app/src/app.rs
index b9b19d7..652e857 100755
--- a/crates/mt-app/src/app.rs
+++ b/crates/mt-app/src/app.rs
@@ -294,6 +294,10 @@ pub enum Message {
     AdapterToggled(usize),
     /// D20: an adapter's per-interface graph expanded/collapsed.
     AdapterExpanded(usize),
+    /// S11: move a SELECTED adapter row within the priority order
+    /// (index into `adapters`, delta −1/±1; ignored at the block edges
+    /// and for unselected rows).
+    AdapterMoved(usize, i32),
     /// Push the drafted selection to the service.
     AdaptersApply,
     /// set_bonded_adapters finished.
@@ -324,8 +328,15 @@ pub enum Message {
 pub const RULE_KINDS: [&str; 4] = ["domainSuffix", "domainKeyword", "ipCidr", "any"];
 /// Valid routing rule actions (matches the mt-net router actions).
 pub const RULE_ACTIONS: [&str; 3] = ["proxy", "direct", "reject"];
-/// Valid merge policies (matches the mt-net aggregator).
-pub const MERGE_POLICIES: [&str; 3] = ["failover", "round-robin", "lowest-latency"];
+/// Valid merge policies (matches the mt-net aggregator + the real
+/// service's parser; the first entry is the stock default).
+pub const MERGE_POLICIES: [&str; 5] = [
+    "balanced",
+    "round-robin",
+    "priority",
+    "failover",
+    "lowest-latency",
+];
 /// Valid bond modes (S9): whole-connection spreading vs stream bonding.
 pub const BOND_MODES: [&str; 2] = ["per-connection", "stream"];
 /// Valid log levels (engine + service).
@@ -949,6 +960,12 @@ impl App {
                 self.settings = s.clone();
                 self.seed_text = s.theme_seed.to_string();
                 self.theme.rederive(s.theme_seed, s.dark_mode);
+                // S11: the saved bonded order may have arrived after the
+                // adapter rows (the init batch is concurrent) — re-sort
+                // UNLESS a draft is in progress (never reshuffle edits).
+                if !self.adapters_dirty() && !self.adapters.is_empty() {
+                    sort_adapter_rows(&mut self.adapters, &s.bonded_adapters);
+                }
             }
             Message::SeedSwatch(seed) => {
                 self.settings.theme_seed = seed;
@@ -999,7 +1016,9 @@ impl App {
                 // selection (an unsaved draft is discarded — same contract
                 // as the Routing screen's reload). Expansion state follows
                 // the rows (a vanished adapter's graph goes with it).
-                self.adapters = views
+                // S11: rows land in PRIORITY order — selected adapters
+                // first, in the saved bonded-adapter order, then the rest.
+                let mut rows: Vec<AdapterRow> = views
                     .into_iter()
                     .map(|view| AdapterRow {
                         selected: view.selected,
@@ -1007,10 +1026,19 @@ impl App {
                         view,
                     })
                     .collect();
+                sort_adapter_rows(&mut rows, &self.settings.bonded_adapters);
+                self.adapters = rows;
             }
             Message::AdapterToggled(idx) => {
-                if let Some(row) = self.adapters.get_mut(idx) {
+                // S11 invariant: the list always reads selected-block-first
+                // (the block IS the priority order). Ticking ON appends at
+                // the END of the block (new adapter = lowest priority);
+                // ticking OFF drops it to the head of the unselected rest.
+                if idx < self.adapters.len() {
+                    let mut row = self.adapters.remove(idx);
                     row.selected = !row.selected;
+                    let insert_at = self.adapters.iter().filter(|r| r.selected).count();
+                    self.adapters.insert(insert_at, row);
                 }
                 self.adapters_status = None;
             }
@@ -1019,6 +1047,29 @@ impl App {
                     row.expanded = !row.expanded;
                 }
             }
+            Message::AdapterMoved(idx, delta) => {
+                // Swap with the previous/next SELECTED row — moving is a
+                // priority-order operation, so unselected rows in between
+                // are skipped, not displaced.
+                let Some(row) = self.adapters.get(idx) else {
+                    return iced::Task::none();
+                };
+                if !row.selected {
+                    return iced::Task::none();
+                }
+                if delta < 0 {
+                    if let Some(prev) = self.adapters[..idx].iter().rposition(|r| r.selected) {
+                        self.adapters.swap(idx, prev);
+                    }
+                } else if let Some(next) = self.adapters[idx + 1..]
+                    .iter()
+                    .position(|r| r.selected)
+                    .map(|i| i + idx + 1)
+                {
+                    self.adapters.swap(idx, next);
+                }
+                self.adapters_status = None;
+            }
             Message::AdaptersApply => {
                 let svc = Arc::clone(&self.service);
                 let ids: Vec<String> = self
@@ -1042,21 +1093,27 @@ impl App {
                                 .to_string()
                         } else {
                             format!(
-                                "applied — bonding across {n} adapter(s); a live session was restarted"
+                                "applied — bonding across {n} adapter(s) in this priority \
+                                 order; a live session was restarted"
                             )
                         });
                     }
                     Err(e) => self.adapters_status = Some(format!("apply failed: {e}")),
                 }
-                // Re-sync rows (the service re-derived `selected`);
-                // also refresh status so the link list reflects the new
-                // link set after the reconnect.
-                return self.load_adapters().chain(self.refresh_status());
+                // Re-sync: SETTINGS first (the saved bonded order is the
+                // sort key), then the rows (the service re-derived
+                // `selected`), then status so the link list reflects the
+                // new link set after the reconnect.
+                return self
+                    .load_settings()
+                    .chain(self.load_adapters())
+                    .chain(self.refresh_status());
             }
             Message::AdaptersReverted => {
                 for row in &mut self.adapters {
                     row.selected = row.view.selected;
                 }
+                sort_adapter_rows(&mut self.adapters, &self.settings.bonded_adapters);
                 self.adapters_status = None;
             }
             Message::AdaptersRefresh => {
@@ -1204,6 +1261,38 @@ impl App {
             Some(ServiceState::Connected)
         )
     }
+
+    /// S11: is the Adapters draft different from what is persisted?
+    /// Dirty = the selected SET differs OR the selected ORDER differs
+    /// from `settings.bonded_adapters` (restricted to adapters present in
+    /// the current enumeration — a vanished id cannot be re-ordered).
+    pub fn adapters_dirty(&self) -> bool {
+        let draft: Vec<String> = self
+            .adapters
+            .iter()
+            .filter(|r| r.selected)
+            .map(|r| r.view.id.clone())
+            .collect();
+        let saved: Vec<String> = self
+            .settings
+            .bonded_adapters
+            .iter()
+            .filter(|id| self.adapters.iter().any(|r| &r.view.id == *id))
+            .cloned()
+            .collect();
+        draft != saved
+    }
+
+    /// S11: the priority position (1-based) of adapter `id` in the SAVED
+    /// bonded order — `None` when the adapter is not bonded. Drives the
+    /// position chips on the Adapters and Status screens.
+    pub fn priority_position_of(&self, id: &str) -> Option<usize> {
+        self.settings
+            .bonded_adapters
+            .iter()
+            .position(|saved| saved == id)
+            .map(|p| p + 1)
+    }
 }
 
 /// Stable subscription ids (iced deduplicates by id per render).
@@ -1248,6 +1337,28 @@ pub fn link_matches_adapter(link_name: &str, adapter_name: &str) -> bool {
     bracketed == adapter_name || bracketed.starts_with(&format!("{adapter_name} #"))
 }
 
+/// S11: sort adapter rows into DISPLAY order — SELECTED rows first, in
+/// the saved bonded-adapter (priority) order, then the unselected rows in
+/// enumeration order. Pure + stable (unselected rows keep their relative
+/// order; a selected id missing from `saved_order` — a fresh draft tick —
+/// sorts last within the selected block, matching the toggle's
+/// append-at-end invariant).
+pub fn sort_adapter_rows(rows: &mut [AdapterRow], saved_order: &[String]) {
+    rows.sort_by_key(|row| {
+        if row.selected {
+            (
+                0,
+                saved_order
+                    .iter()
+                    .position(|id| id == &row.view.id)
+                    .unwrap_or(usize::MAX),
+            )
+        } else {
+            (1, 0)
+        }
+    });
+}
+
 /// Launches the GUI with the given service backend (the root crate calls
 /// this with its `RealService`; the `mt-app` binary passes a `MockService`).
 pub fn run_app(service: Arc<dyn MtService>, flags: Flags) -> Result<(), crate::Error> {
@@ -1515,52 +1626,126 @@ mod tests {
         assert!(app.status.is_some());
     }
 
-    /// Adapters screen flow (S7): toggle → apply → rows re-sync to the
-    /// persisted selection; revert drops a dirty draft; refresh reloads.
+    /// Adapters screen flow (S7 + S11): toggle → priority REORDER → apply
+    /// → rows re-sync to the persisted selection AND order; revert drops a
+    /// dirty draft (selection and order).
     #[cfg(feature = "mock")]
     #[tokio::test]
-    async fn adapters_screen_toggle_apply_revert() {
+    async fn adapters_screen_toggle_move_apply_revert() {
         let service = Arc::new(crate::mock::MockService::new());
         let mut app = App::init_async(service, Flags::default()).await;
 
-        // Seed (D22): mock-wifi AND mock-ethernet bonded (the demo setup),
-        // cellular not.
+        // Seed (D23): all THREE adapters bonded, saved order
+        // wifi → ethernet → cellular (the mock demo setup).
         assert_eq!(app.adapters.len(), 3);
-        assert!(app.adapters[0].selected);
-        assert!(app.adapters[1].selected);
+        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
+        assert_eq!(ids, vec!["mock-wifi", "mock-ethernet", "mock-cellular"]);
+        assert!(app.adapters.iter().all(|r| r.selected));
+        assert!(!app.adapters_dirty(), "fresh load matches the saved order");
+        assert_eq!(app.priority_position_of("mock-ethernet"), Some(2));
+        assert_eq!(app.priority_position_of("mock-cellular"), Some(3));
+        assert_eq!(app.priority_position_of("nope"), None);
+
+        // Untick cellular (draft dirty, not persisted): it drops out of the
+        // selected block but keeps its row position here (it was last).
+        drop(app.update(Message::AdapterToggled(2)));
         assert!(!app.adapters[2].selected);
+        assert!(app.adapters[2].view.selected, "draft only — not applied");
+        assert!(app.adapters_dirty());
 
-        // Toggle ethernet off (draft dirty, not persisted).
-        drop(app.update(Message::AdapterToggled(1)));
-        assert!(!app.adapters[1].selected);
-        assert!(app.adapters[1].view.selected, "draft only — not applied");
+        // Re-tick cellular: it must APPEND at the END of the selected
+        // block (a re-admitted adapter gets the lowest priority).
+        drop(app.update(Message::AdapterToggled(2)));
+        assert!(app.adapters[2].selected);
+        assert!(!app.adapters_dirty(), "back to the saved set and order");
 
-        // Apply persists the narrower bond (service call directly — the
+        // Untick ethernet: its row leaves the block (index moves!).
+        drop(app.update(Message::AdapterToggled(1)));
+        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
+        assert_eq!(
+            ids,
+            vec!["mock-wifi", "mock-cellular", "mock-ethernet"],
+            "unselected row sinks below the selected block"
+        );
+        // Re-tick ethernet (now index 2): appends at the END of the block.
+        drop(app.update(Message::AdapterToggled(2)));
+        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
+        assert_eq!(
+            ids,
+            vec!["mock-wifi", "mock-cellular", "mock-ethernet"],
+            "re-ticked adapter lands at the lowest priority"
+        );
+        assert!(app.adapters_dirty(), "order differs from the saved one");
+
+        // REORDER: move Wi-Fi (index 0) DOWN — swaps with cellular.
+        drop(app.update(Message::AdapterMoved(0, 1)));
+        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
+        assert_eq!(ids, vec!["mock-cellular", "mock-wifi", "mock-ethernet"]);
+        // Edge moves are no-ops (Wi-Fi is at the top of the block).
+        drop(app.update(Message::AdapterMoved(0, -1)));
+        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
+        assert_eq!(ids, vec!["mock-cellular", "mock-wifi", "mock-ethernet"]);
+
+        // Apply persists the new order (service call directly — the
         // established test idiom; the update task only wraps it in
-        // Task::perform).
-        let ids = ["mock-wifi"].iter().map(|s| s.to_string()).collect();
-        app.service.set_bonded_adapters(ids).await.expect("apply");
+        // Task::perform). The saved-handler chains settings + adapters +
+        // status reloads; drive those service calls and feed the results
+        // back in the same order.
+        let draft: Vec<String> = app.adapters.iter().map(|r| r.view.id.clone()).collect();
+        app.service.set_bonded_adapters(draft).await.expect("apply");
         drop(app.update(Message::AdaptersSaved(Ok(()))));
-        // The saved-handler chains a reload; drive the service call the
-        // handler would perform and feed the result back.
+        let settings = app.service.settings().await;
+        drop(app.update(Message::SettingsLoaded(settings)));
         let views = app.service.adapters().await;
         drop(app.update(Message::AdaptersLoaded(views)));
-        assert!(app.adapters[0].selected);
-        assert!(!app.adapters[1].selected);
-        assert!(!app.adapters[1].view.selected, "applied selection visible");
-        assert!(
-            app.adapters_status
-                .as_deref()
-                .unwrap_or("")
-                .contains("bonding across 1"),
-            "status line reports the applied selection"
+        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
+        assert_eq!(
+            ids,
+            vec!["mock-cellular", "mock-wifi", "mock-ethernet"],
+            "applied order survives the reload"
         );
+        assert!(!app.adapters_dirty());
+        assert_eq!(app.priority_position_of("mock-cellular"), Some(1));
+        assert!(app
+            .adapters_status
+            .as_deref()
+            .unwrap_or("")
+            .contains("priority"));
 
-        // Revert drops an unsaved draft.
-        drop(app.update(Message::AdapterToggled(0)));
-        drop(app.update(Message::AdapterToggled(2)));
+        // Revert drops an unsaved draft (selection AND order).
+        drop(app.update(Message::AdapterToggled(2))); // untick ethernet
+        drop(app.update(Message::AdapterMoved(0, 1))); // cellular below wifi
         drop(app.update(Message::AdaptersReverted));
-        assert!(app.adapters[0].selected && !app.adapters[2].selected);
+        let ids: Vec<&str> = app.adapters.iter().map(|r| r.view.id.as_str()).collect();
+        assert_eq!(
+            ids,
+            vec!["mock-cellular", "mock-wifi", "mock-ethernet"],
+            "revert restores the saved selection and order"
+        );
+        assert!(app.adapters.iter().all(|r| r.selected));
+    }
+
+    /// S11: `sort_adapter_rows` (the load/revert ordering primitive).
+    #[cfg(feature = "mock")]
+    #[test]
+    fn sort_adapter_rows_selected_first_in_saved_order() {
+        let views = crate::mock::seed_adapters_public();
+        let saved = vec!["mock-cellular".to_string(), "mock-wifi".to_string()];
+        let mut rows: Vec<AdapterRow> = views
+            .into_iter()
+            .map(|view| AdapterRow {
+                selected: saved.contains(&view.id),
+                expanded: false,
+                view,
+            })
+            .collect();
+        sort_adapter_rows(&mut rows, &saved);
+        let ids: Vec<&str> = rows.iter().map(|r| r.view.id.as_str()).collect();
+        assert_eq!(
+            ids,
+            vec!["mock-cellular", "mock-wifi", "mock-ethernet"],
+            "selected block first in SAVED order, rest in enumeration order"
+        );
     }
 
     /// Live re-theme: dispatching settings messages re-derives the theme.
diff --git a/crates/mt-app/src/mock.rs b/crates/mt-app/src/mock.rs
index ad323c8..b90b91f 100755
--- a/crates/mt-app/src/mock.rs
+++ b/crates/mt-app/src/mock.rs
@@ -79,14 +79,22 @@ impl MockService {
             theme_seed: 0x6750A4,
             dark_mode: false,
             system_proxy: false,
-            // D20: mirror the service default (round-robin — failover left
-            // the second ticked adapter idle; see real_service D20).
-            merge_policy: "round-robin".to_string(),
+            // D23: mirror the service default (balanced — live byte-load
+            // spreading; round-robin's connection-count evenness let one
+            // fat flow dominate a single adapter).
+            merge_policy: "balanced".to_string(),
             autostart: false,
-            // D22: both wired adapters bonded — mirrors the demo setup
-            // (round-robin default needs ≥2 ticked adapters to show its
-            // point) and gives both Status-tab adapter graphs traffic.
-            bonded_adapters: vec!["mock-wifi".to_string(), "mock-ethernet".to_string()],
+            // D22/D23: ALL THREE wired adapters bonded — mirrors the demo
+            // setup (balanced needs ≥2 ticked adapters to show its point)
+            // and gives every Status-tab adapter graph traffic. The mock's
+            // byte split is deliberately UNEVEN (Wi-Fi ≫ Ethernet ≫
+            // Cellular) — the "one adapter hogs everything" shape the
+            // balanced policy exists to fix.
+            bonded_adapters: vec![
+                "mock-wifi".to_string(),
+                "mock-ethernet".to_string(),
+                "mock-cellular".to_string(),
+            ],
             bond_mode: "per-connection".to_string(),
             merge_port: 7443,
             socks_port: 10808,
@@ -220,10 +228,11 @@ fn seed_rules() -> Vec<RuleView> {
 
 fn seed_links(rng0: u64) -> Vec<LinkStatus> {
     let jitter = |k: u64, base: u32| base + ((k % 25) as u32);
-    // D22: link names mirror the REAL engine's per-adapter links —
+    // D22/D23: link names mirror the REAL engine's per-adapter links —
     // "<profile> [<adapter>]" — so the GUI's per-adapter rate sampler
     // (link_matches_adapter) attributes mock traffic the same way it
     // does in production, and the Status tab's adapter graphs move.
+    // Three carriers: the demo of a 3-NIC bond.
     vec![
         LinkStatus {
             name: "synthetic-reality [Wi-Fi]".to_string(),
@@ -243,11 +252,21 @@ fn seed_links(rng0: u64) -> Vec<LinkStatus> {
             bytes_up: 0,
             bytes_down: 0,
         },
+        LinkStatus {
+            name: "synthetic-reality [Cellular]".to_string(),
+            ok: true,
+            latency_ms: Some(jitter(rng0 % 83, 121)),
+            ewma_ms: Some(128.0),
+            quarantined: false,
+            bytes_up: 0,
+            bytes_down: 0,
+        },
     ]
 }
 
-/// Synthetic adapters (S7): Wi-Fi + Ethernet up with IPv4s, a cellular
-/// dongle down without one — covers every Adapters-screen chip state.
+/// Synthetic adapters (S7): Wi-Fi + Ethernet + Cellular up with IPv4s —
+/// the 3-NIC demo bond (D23; the mock's traffic split is deliberately
+/// uneven so the balanced policy's effect is visible).
 fn seed_adapters() -> Vec<mt_service::AdapterView> {
     vec![
         mt_service::AdapterView {
@@ -269,14 +288,20 @@ fn seed_adapters() -> Vec<mt_service::AdapterView> {
         mt_service::AdapterView {
             id: "mock-cellular".to_string(),
             name: "Cellular".to_string(),
-            description: "Mock LTE dongle (unplugged)".to_string(),
-            ipv4: Vec::new(),
-            up: false,
+            description: "Mock 5G dongle".to_string(),
+            ipv4: vec!["100.72.14.5".to_string()],
+            up: true,
             selected: false,
         },
     ]
 }
 
+/// Test seam (S11): the synthetic adapter enumeration, for tests of the
+/// GUI's ordering primitives outside a running mock service.
+pub fn seed_adapters_public() -> Vec<mt_service::AdapterView> {
+    seed_adapters()
+}
+
 fn view_of(p: &mt_links::Profile, active_id: &Option<String>) -> ProfileView {
     ProfileView {
         id: p.id.clone(),
@@ -377,11 +402,18 @@ impl MtService for MockService {
                 let jitters: Vec<u32> = (0..g.links.len())
                     .map(|_| 30 + MockService::rng_below(&mut g, 90) as u32)
                     .collect();
+                // D23: deliberately UNEVEN split across the three bonded
+                // NICs (≈70% / 25% / 5%) — the "one adapter hogs
+                // everything, the third rarely sees traffic" field report
+                // the balanced policy exists to fix, made visible in the
+                // per-adapter graphs.
+                let shares = [70usize, 25, 5];
                 for (k, l) in g.links.iter_mut().enumerate() {
                     let ms = jitters[k];
+                    let share = shares.get(k).copied().unwrap_or(0);
                     l.ok = true;
-                    l.bytes_up += up / 4;
-                    l.bytes_down += down / 4;
+                    l.bytes_up += up * share as u64 / 100;
+                    l.bytes_down += down * share as u64 / 100;
                     l.latency_ms = Some(ms);
                     l.ewma_ms = Some(l.ewma_ms.unwrap_or(ms as f32) * 0.7 + ms as f32 * 0.3);
                     let _ = tx.send(ServiceEvent::LinkUpdated(l.clone()));
@@ -623,7 +655,7 @@ mod tests {
         let snap = svc.status().await;
         assert_eq!(snap.state, ServiceState::Connected);
         assert!(snap.up_since.is_some());
-        assert_eq!(snap.links.len(), 2);
+        assert_eq!(snap.links.len(), 3, "D23: three bonded mock adapters");
         assert!(snap.links.iter().all(|l| l.ok));
         assert!(snap.active_profile.is_some());
 
diff --git a/crates/mt-app/src/screens.rs b/crates/mt-app/src/screens.rs
index 0a1e998..5265b1f 100755
--- a/crates/mt-app/src/screens.rs
+++ b/crates/mt-app/src/screens.rs
@@ -537,6 +537,15 @@ fn status_adapter_row<'a>(app: &'a App, i: usize, r: &'a crate::app::AdapterRow)
     } else {
         chip("down", app, s.error_container, s.on_error_container)
     });
+    // S11: the adapter's saved priority position (bonded adapters only).
+    if let Some(p) = app.priority_position_of(&v.id) {
+        line = line.push(chip(
+            format!("#{p}"),
+            app,
+            s.primary_container,
+            s.on_primary_container,
+        ));
+    }
     if !r.selected {
         line = line.push(chip(
             "not bonded",
@@ -1005,8 +1014,9 @@ pub fn adapters(app: &App) -> Element<'_> {
     let s = app.theme.scheme;
     let ts = app.theme.ts;
 
-    let dirty = app.adapters.iter().any(|r| r.selected != r.view.selected);
+    let dirty = app.adapters_dirty();
     let selected = app.adapters.iter().filter(|r| r.selected).count();
+    let priority_mode = app.settings.merge_policy == "priority";
 
     let toolbar = row![
         m3_button(
@@ -1036,7 +1046,12 @@ pub fn adapters(app: &App) -> Element<'_> {
                 .size(ts.body_s.size)
                 .color(s.on_surface_variant),
             None => text(format!(
-                "{selected} selected — one engine per adapter, spread by merge policy"
+                "{selected} bonded — order below = adapter priority{}",
+                if priority_mode {
+                    " (priority policy: fills #1 until saturated, then #2…)"
+                } else {
+                    " (used by the 'priority' policy)"
+                }
             ))
             .size(ts.body_s.size)
             .color(s.on_surface_variant),
@@ -1050,16 +1065,19 @@ pub fn adapters(app: &App) -> Element<'_> {
             .size(ts.title_m.size)
             .color(s.on_surface),
         text(
-            "Check the network adapters the tunnel should bond across. Each checked \
-             adapter gets its own engine bound to that adapter (sendThrough). New \
-             connections spread round-robin across them (Settings → merge policy); \
-             bonding mode \"stream\" (Settings → Advanced) splits every connection's \
-             BYTES across them and needs mt-merge-server on the tunnel server \
-             (plus an Xray finalRules allow for the merge port — re-run \
-             server/setup-xray-reality.sh on the VM). \
-             Expand a row (▸, or press the adapter's name on the Status tab) \
-             for that adapter's live traffic graph. No selection \
-             = a single link and Windows picks the route.",
+            "Check the network adapters the tunnel should bond across — each \
+             checked adapter gets its own engine bound to that NIC \
+             (sendThrough). The ORDER of the checked rows is the ADAPTER \
+             PRIORITY: with the \"priority\" merge policy (Settings) traffic \
+             fills adapter #1 until its bandwidth is saturated, then the \
+             overflow goes to #2, then #3 — reorder with ↑/↓. Other policies \
+             ignore the order. Bonding mode \"stream\" (Settings → Advanced) \
+             additionally needs mt-merge-server on the tunnel server plus an \
+             Xray finalRules allow for the merge port (re-run \
+             server/setup-xray-reality.sh on the VM). Expand a row (▸, or \
+             press the adapter's name on the Status tab) for that adapter's \
+             live traffic graph. No selection = a single link and Windows \
+             picks the route.",
         )
         .size(ts.body_s.size)
         .color(s.on_surface_variant),
@@ -1074,9 +1092,18 @@ pub fn adapters(app: &App) -> Element<'_> {
                 .color(s.on_surface_variant),
         );
     }
+    // S11: position within the selected block (1-based) — the draft's
+    // priority order, i.e. exactly what Apply will persist.
+    let mut running_pos = 0usize;
     for (i, r) in app.adapters.iter().enumerate() {
         let v = &r.view;
         let arrow = if r.expanded { "▾" } else { "▸" };
+        let pos = if r.selected {
+            running_pos += 1;
+            Some(running_pos)
+        } else {
+            None
+        };
         let mut line = row![
             // D20: expand/collapse the per-interface graph (a plain text
             // button — no icon font in the zero-dep build).
@@ -1093,6 +1120,16 @@ pub fn adapters(app: &App) -> Element<'_> {
         .spacing(8)
         .align_y(iced::Alignment::Center);
 
+        // S11: priority position chip on every bonded row.
+        if let Some(p) = pos {
+            line = line.push(chip(
+                format!("#{p}"),
+                app,
+                s.primary_container,
+                s.on_primary_container,
+            ));
+        }
+
         if !v.description.is_empty() {
             line = line.push(
                 text(util::short(&v.description, 40))
@@ -1123,6 +1160,22 @@ pub fn adapters(app: &App) -> Element<'_> {
         } else {
             chip("down", app, s.error_container, s.on_error_container)
         });
+        // S11: reorder buttons — swap with the previous/next SELECTED row
+        // (disabled at the block edges; unselected rows carry no order).
+        let can_up = pos.is_some_and(|p| p > 1);
+        let can_down = pos.is_some() && app.adapters[i + 1..].iter().any(|r| r.selected);
+        line = line.push(
+            button(text("↑").size(ts.body_m.size))
+                .padding(Padding::new(2.0).left(6.0).right(6.0))
+                .style(t3::text_button(s))
+                .on_press_maybe(can_up.then_some(Message::AdapterMoved(i, -1))),
+        );
+        line = line.push(
+            button(text("↓").size(ts.body_m.size))
+                .padding(Padding::new(2.0).left(6.0).right(6.0))
+                .style(t3::text_button(s))
+                .on_press_maybe(can_down.then_some(Message::AdapterMoved(i, 1))),
+        );
 
         // D20: the adapter row + its expandable per-interface graph live in
         // ONE card so the graph reads as part of the row.
@@ -1149,14 +1202,22 @@ pub fn adapters(app: &App) -> Element<'_> {
             }
             .mini(110.0);
             let hint = if !app.is_connected() {
-                "connect to see this adapter's tunnel traffic"
+                "connect to see this adapter's tunnel traffic".to_string()
             } else if !r.selected {
-                "not bonded — tick the adapter to route tunnel traffic through it"
-            } else if !carries_traffic {
-                "bonded but idle — no tunnel bytes in the last 2 minutes"
+                "not bonded — tick the adapter to route tunnel traffic through it".to_string()
             } else {
-                "tunnel traffic through this adapter (↓ down / ↑ up, same colors \
-                 as the Status graph)"
+                let prio = match app.priority_position_of(&v.id) {
+                    Some(p) => format!("priority #{p}"),
+                    None => "bonded".to_string(),
+                };
+                if !carries_traffic {
+                    format!("{prio} — no tunnel bytes in the last 2 minutes")
+                } else {
+                    format!(
+                        "{prio} — tunnel traffic through this adapter \
+                         (↓ down / ↑ up, same colors as the Status graph)"
+                    )
+                }
             };
             card_col = card_col.push(
                 container(mini)
@@ -1268,14 +1329,18 @@ pub fn settings(app: &App) -> Element<'_> {
         .spacing(10)
         .align_y(iced::Alignment::Center),
         text(
-            "round-robin (default): new connections alternate across every ticked \
-             adapter — both NICs carry traffic. failover: everything rides the \
-             first healthy adapter (the second stays idle until it dies). \
-             lowest-latency: each connection picks the adapter with the best \
-             smoothed (EWMA) RTT — the faster link naturally takes more \
-             connections than a blind alternation. To use BOTH adapters for a \
-             SINGLE connection at the same time, set bonding mode \"stream\" \
-             in Advanced.",
+            "balanced (default): every new connection goes to the adapter \
+             carrying the least traffic RIGHT NOW — a fat download stops \
+             hogging one NIC and the spread evens out by BYTES. round-robin: \
+             strict turn-taking per connection (even by connection count, \
+             not by bytes). priority: adapters fill in the Adapters-tab \
+             order — #1 takes everything until its bandwidth saturates, \
+             then the overflow spills to #2, and so on. failover: everything \
+             rides the first healthy adapter (spares stay idle). \
+             lowest-latency: each connection picks the adapter with the \
+             best smoothed (EWMA) RTT. In stream bonding mode (Advanced) \
+             the policy also spreads every connection's BYTES across the \
+             bonded adapters.",
         )
         .size(ts.body_s.size)
         .color(s.on_surface_variant),
@@ -1348,10 +1413,13 @@ pub fn settings(app: &App) -> Element<'_> {
              connections are spread by the merge policy. stream: the BYTES of \
              every connection are split across ALL ticked adapters at once and \
              re-merged by mt-merge-server on the tunnel server — one download \
-             uses both NICs simultaneously; needs the merge daemon (setup \
-             script in the release zip, port below) and an Xray finalRules \
-             allow for that port on the server (re-run server/setup-xray-reality.sh) \
-             and falls back to per-connection with a log line when it is unreachable.",
+             uses every NIC simultaneously. In stream mode the merge policy \
+             drives the frame scheduler too: balanced spreads frames by live \
+             adapter backpressure (fast NICs take more), priority fills the \
+             Adapters-tab order. Needs the merge daemon (setup script in the \
+             release zip, port below) plus an Xray finalRules allow for that \
+             port on the server (re-run server/setup-xray-reality.sh); falls \
+             back to per-connection with a log line when it is unreachable.",
         )
         .size(ts.body_s.size)
         .color(s.on_surface_variant),
diff --git a/crates/mt-bond/src/endpoint.rs b/crates/mt-bond/src/endpoint.rs
index bc389b0..4027f69 100755
--- a/crates/mt-bond/src/endpoint.rs
+++ b/crates/mt-bond/src/endpoint.rs
@@ -139,6 +139,11 @@ struct Carrier {
     bytes_down: AtomicU64,
     /// Millis of the last frame received on this carrier (liveness).
     last_rx_ms: AtomicU64,
+    /// S11: bytes handed to the writer channel but not yet flushed to the
+    /// socket — the carrier's live BACKPRESSURE. `send_bytes` increments,
+    /// the writer task decrements after each socket write. A carrier whose
+    /// socket keeps up hovers near zero; a bottlenecked one grows.
+    queued: AtomicU64,
 }
 
 impl Carrier {
@@ -153,6 +158,7 @@ impl Carrier {
             bytes_up: AtomicU64::new(0),
             bytes_down: AtomicU64::new(0),
             last_rx_ms: AtomicU64::new(now_millis()),
+            queued: AtomicU64::new(0),
         })
     }
 
@@ -168,6 +174,89 @@ impl Carrier {
     }
 }
 
+// ---------------------------------------------------------------------------
+// S11 carrier scheduling (pure)
+// ---------------------------------------------------------------------------
+
+/// Scheduler snapshot of one LIVE carrier (the pure picker's input).
+#[derive(Debug, Clone, Copy)]
+struct CarrierSlot {
+    /// Carrier slot index == link index == the user's adapter priority
+    /// order (links are built one per bonded adapter, in settings order).
+    idx: usize,
+    /// Bytes queued for the socket writer (the live backpressure).
+    queued: u64,
+    /// Last PING round trip in ms (0 = no sample yet).
+    rtt_ms: u32,
+}
+
+/// A carrier whose writer queue exceeds this many BYTES counts as
+/// SATURATED for the `Priority` policy — its socket provably cannot keep
+/// up with the offered rate. 512 KiB ≈ 8 max-size frames: a healthy
+/// carrier hovers near zero, a bottlenecked one grows without bound, so
+/// the spill decision lands within a fraction of a second at any real
+/// rate difference.
+pub const PRIORITY_QUEUE_HIGH_WATER: u64 = 512 * 1024;
+
+/// Pure carrier pick (the D17 round-robin, made policy-aware in S11).
+/// Returns the SLOT INDEX to send the next frame on:
+/// - `Failover`       — first live carrier (idx order).
+/// - `RoundRobin`     — rotate over live carriers (cursor).
+/// - `LowestLatency`  — best sampled RTT (known beats unknown, ties keep
+///   the earlier carrier).
+/// - `Balanced`       — least queued bytes; ties rotate via the cursor.
+///   A fast adapter drains its queue quicker and naturally takes more
+///   frames — capacity-proportional even spreading.
+/// - `Priority`       — first carrier in USER ORDER whose queue is under
+///   [`PRIORITY_QUEUE_HIGH_WATER`]; when every carrier backs up, the
+///   least-queued one takes the excess.
+fn pick_carrier_index(
+    policy: mt_net::MergePolicy,
+    slots: &[CarrierSlot],
+    cursor: &mut u64,
+) -> Option<usize> {
+    if slots.is_empty() {
+        return None;
+    }
+    match policy {
+        mt_net::MergePolicy::Failover => Some(slots[0].idx),
+        mt_net::MergePolicy::RoundRobin => {
+            let s = slots[(*cursor % slots.len() as u64) as usize];
+            *cursor = cursor.wrapping_add(1);
+            Some(s.idx)
+        }
+        mt_net::MergePolicy::LowestLatency => {
+            let mut best = 0;
+            for i in 1..slots.len() {
+                let (b, c) = (slots[best], slots[i]);
+                let b_known = b.rtt_ms > 0;
+                let c_known = c.rtt_ms > 0;
+                if (c_known && !b_known) || (c_known == b_known && c.rtt_ms < b.rtt_ms) {
+                    best = i;
+                }
+            }
+            Some(slots[best].idx)
+        }
+        mt_net::MergePolicy::Balanced => {
+            let min = slots.iter().map(|s| s.queued).min().expect("non-empty");
+            let tied: Vec<usize> = (0..slots.len())
+                .filter(|&i| slots[i].queued == min)
+                .collect();
+            let pick = tied[(*cursor % tied.len() as u64) as usize];
+            *cursor = cursor.wrapping_add(1);
+            Some(slots[pick].idx)
+        }
+        mt_net::MergePolicy::Priority => {
+            for s in slots {
+                if s.queued < PRIORITY_QUEUE_HIGH_WATER {
+                    return Some(s.idx);
+                }
+            }
+            slots.iter().min_by_key(|s| s.queued).map(|s| s.idx) // every carrier backs up → least queued
+        }
+    }
+}
+
 // ---------------------------------------------------------------------------
 // Session (one end's half of a multiplexed stream)
 // ---------------------------------------------------------------------------
@@ -240,6 +329,10 @@ pub(crate) struct Endpoint {
     pub is_server: bool,
     carriers: RwLock<Vec<Arc<Carrier>>>,
     rr: AtomicU64,
+    /// S11: how `pick_carrier` spreads frames across live carriers (the
+    /// user's merge policy, mapped onto carrier scheduling; the daemon
+    /// side keeps plain round-robin).
+    carrier_policy: mt_net::MergePolicy,
     sessions: Mutex<HashMap<u32, Arc<Session>>>,
     /// Monotonic LOCAL session id (client side; starts at 1).
     next_session_id: AtomicU32,
@@ -261,11 +354,15 @@ pub(crate) struct Endpoint {
 }
 
 impl Endpoint {
-    /// Client-side endpoint (carriers dialed through `links`).
-    pub fn client(
+    /// Client-side endpoint with the carrier-scheduling policy (S11: the
+    /// GUI's merge policy rides the bond — `Balanced` spreads frames by
+    /// live writer backpressure, `Priority` fills carriers in user order
+    /// until one backs up; `RoundRobin` is the stock behavior).
+    pub fn client_with_policy(
         bond_id: u32,
         links: Vec<Arc<dyn Outbound>>,
         merge_target: mt_net::TargetAddr,
+        carrier_policy: mt_net::MergePolicy,
     ) -> Arc<Self> {
         let (shutdown, _) = watch::channel(false);
         let (death_tx, death_rx) = mpsc::unbounded_channel();
@@ -274,6 +371,7 @@ impl Endpoint {
             is_server: false,
             carriers: RwLock::new(Vec::new()),
             rr: AtomicU64::new(0),
+            carrier_policy,
             sessions: Mutex::new(HashMap::new()),
             next_session_id: AtomicU32::new(1),
             new_session_tx: Mutex::new(None),
@@ -287,7 +385,9 @@ impl Endpoint {
         })
     }
 
-    /// Server-side endpoint (carriers are accepted connections).
+    /// Server-side endpoint (carriers are accepted connections; the
+    /// daemon keeps plain round-robin — the user's priority order is a
+    /// CLIENT-side concern).
     pub fn server(bond_id: u32) -> Arc<Self> {
         let (shutdown, _) = watch::channel(false);
         let (death_tx, death_rx) = mpsc::unbounded_channel();
@@ -296,6 +396,7 @@ impl Endpoint {
             is_server: true,
             carriers: RwLock::new(Vec::new()),
             rr: AtomicU64::new(0),
+            carrier_policy: mt_net::MergePolicy::RoundRobin,
             sessions: Mutex::new(HashMap::new()),
             next_session_id: AtomicU32::new(1),
             new_session_tx: Mutex::new(None),
@@ -481,14 +582,21 @@ impl Endpoint {
                 }
                 item = rx.recv() => {
                     let Some(bytes) = item else { break };
-                    if let Err(e) = w.write_all(&bytes).await {
+                    let flushed = w.write_all(&bytes).await;
+                    // S11: the bytes left the writer queue either way
+                    // (a failed write kills the carrier — a fresh Carrier
+                    // with a zeroed counter replaces it on respawn).
+                    if let Some(c) = self.carrier_of(idx) {
+                        c.queued.fetch_sub(bytes.len() as u64, Ordering::Relaxed);
+                        if flushed.is_ok() {
+                            c.bytes_up.fetch_add(bytes.len() as u64, Ordering::Relaxed);
+                        }
+                    }
+                    if let Err(e) = flushed {
                         tracing::debug!(carrier = idx, error = %e, "bond carrier write failed");
                         let _ = self.death_tx.send((idx, "write failed"));
                         break;
                     }
-                    if let Some(c) = self.carrier_of(idx) {
-                        c.bytes_up.fetch_add(bytes.len() as u64, Ordering::Relaxed);
-                    }
                 }
             }
         }
@@ -825,18 +933,37 @@ impl Endpoint {
         let _ = self.schedule_bytes(close).await;
     }
 
-    /// Pick a live VERIFIED carrier (round-robin over the live set, D17 §4).
-    /// Unverified (phantom-live) carriers are never schedulable: bytes
-    /// queued into them are lost when the tunnel never actually came up.
+    /// Pick a live VERIFIED carrier under the endpoint's scheduling
+    /// policy (the D17 round-robin became policy-aware in S12 — see
+    /// [`pick_carrier_index`]). Unverified (phantom-live) carriers are
+    /// never schedulable: bytes queued into them are lost when the tunnel
+    /// never actually came up.
     fn pick_carrier(&self) -> Option<(usize, Arc<Carrier>)> {
         let cs = self.carriers.read().expect("carrier lock");
-        let live: Vec<&Arc<Carrier>> = cs.iter().filter(|c| c.usable()).collect();
-        if live.is_empty() {
-            return None;
-        }
-        let n = self.rr.fetch_add(1, Ordering::Relaxed);
-        let c = live[(n % live.len() as u64) as usize];
-        Some((c.idx, Arc::clone(c)))
+        let slots: Vec<CarrierSlot> = cs
+            .iter()
+            .filter(|c| c.usable())
+            .map(|c| CarrierSlot {
+                idx: c.idx,
+                queued: c.queued.load(Ordering::Relaxed),
+                rtt_ms: c.rtt_ms.load(Ordering::Relaxed),
+            })
+            .collect();
+        drop(cs);
+        // Best-effort fairness cursor: concurrent senders may lose a
+        // tie-rotation increment (both pick the same carrier — harmless,
+        // one extra frame there); correctness never depends on it.
+        let mut cursor = self.rr.load(Ordering::Relaxed);
+        let idx = pick_carrier_index(self.carrier_policy, &slots, &mut cursor)?;
+        self.rr.store(cursor, Ordering::Relaxed);
+        let carrier = self
+            .carriers
+            .read()
+            .expect("carrier lock")
+            .get(idx)
+            .cloned()
+            .filter(|c| c.usable())?;
+        Some((idx, carrier))
     }
 
     /// Queue encoded bytes on a specific carrier.
@@ -849,11 +976,24 @@ impl Endpoint {
             .cloned()
             .filter(|c| c.usable())
             .ok_or_else(|| anyhow::anyhow!("carrier {carrier_idx} not usable"))?;
+        // S12: count the bytes as queued BEFORE the send (backpressure
+        // accounting for the Balanced / Priority picks).
         carrier
+            .queued
+            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
+        if carrier
             .tx
             .send(bytes)
             .await
-            .map_err(|_| anyhow::anyhow!("carrier {carrier_idx} writer gone"))?;
+            .map_err(|_| anyhow::anyhow!("carrier {carrier_idx} writer gone"))
+            .is_err()
+        {
+            // The writer is gone: the queue will never drain. Reset so a
+            // respawned carrier (or this dead one, briefly) does not read
+            // as eternally backlogged.
+            carrier.queued.store(0, Ordering::Relaxed);
+            return Err(anyhow::anyhow!("carrier {carrier_idx} writer gone"));
+        }
         Ok(())
     }
 
@@ -1134,3 +1274,112 @@ impl Endpoint {
         self.session_of(id).and_then(|s| lock(&s.inbound_rx).take())
     }
 }
+
+#[cfg(test)]
+mod scheduling_tests {
+    use super::*;
+
+    fn slot(idx: usize, queued: u64, rtt_ms: u32) -> CarrierSlot {
+        CarrierSlot {
+            idx,
+            queued,
+            rtt_ms,
+        }
+    }
+
+    fn pick1(policy: mt_net::MergePolicy, slots: &[CarrierSlot]) -> Option<usize> {
+        let mut cursor = 0u64;
+        pick_carrier_index(policy, slots, &mut cursor)
+    }
+
+    #[test]
+    fn failover_takes_the_first_live_carrier() {
+        let slots = [slot(0, 900_000, 0), slot(1, 0, 0), slot(2, 0, 0)];
+        assert_eq!(pick1(mt_net::MergePolicy::Failover, &slots), Some(0));
+        assert_eq!(pick1(mt_net::MergePolicy::Failover, &[]), None);
+    }
+
+    #[test]
+    fn round_robin_cycles_the_cursor() {
+        let slots = [slot(0, 0, 0), slot(1, 0, 0), slot(2, 0, 0)];
+        let mut cursor = 0u64;
+        let seq: Vec<usize> = (0..4)
+            .map(|_| {
+                pick_carrier_index(mt_net::MergePolicy::RoundRobin, &slots, &mut cursor).unwrap()
+            })
+            .collect();
+        assert_eq!(seq, vec![0, 1, 2, 0]);
+        assert_eq!(cursor, 4);
+    }
+
+    #[test]
+    fn lowest_latency_prefers_sampled_rtt_then_known_over_unknown() {
+        // Fastest sampled carrier wins.
+        let slots = [slot(0, 0, 120), slot(1, 0, 0), slot(2, 0, 45)];
+        assert_eq!(
+            pick1(mt_net::MergePolicy::LowestLatency, &slots),
+            Some(2),
+            "known 45 ms beats unknown and known 120 ms"
+        );
+        // A sample beats no sample (0 = not yet measured).
+        let unknowns = [slot(0, 0, 0), slot(1, 0, 300)];
+        assert_eq!(
+            pick1(mt_net::MergePolicy::LowestLatency, &unknowns),
+            Some(1)
+        );
+        // All unknown → first.
+        let all_unknown = [slot(0, 0, 0), slot(1, 0, 0)];
+        assert_eq!(
+            pick1(mt_net::MergePolicy::LowestLatency, &all_unknown),
+            Some(0)
+        );
+    }
+
+    #[test]
+    fn balanced_picks_the_least_backlogged_carrier() {
+        // The user's exact complaint shape: one adapter carries the fat
+        // flow (deep queue), the others are nearly drained.
+        let slots = [
+            slot(0, 800_000, 30),
+            slot(1, 120_000, 40),
+            slot(2, 5_000, 55),
+        ];
+        assert_eq!(pick1(mt_net::MergePolicy::Balanced, &slots), Some(2));
+    }
+
+    #[test]
+    fn balanced_ties_rotate() {
+        let slots = [slot(0, 0, 0), slot(1, 0, 0), slot(2, 0, 0)];
+        let mut cursor = 0u64;
+        let seq: Vec<usize> = (0..3)
+            .map(|_| {
+                pick_carrier_index(mt_net::MergePolicy::Balanced, &slots, &mut cursor).unwrap()
+            })
+            .collect();
+        assert_eq!(seq, vec![0, 1, 2], "equal queues rotate like round-robin");
+    }
+
+    #[test]
+    fn priority_fills_in_order_until_the_queue_backs_up() {
+        // Everything below the high-water mark rides carrier 0.
+        let healthy = [slot(0, 100_000, 30), slot(1, 0, 40), slot(2, 0, 55)];
+        assert_eq!(pick1(mt_net::MergePolicy::Priority, &healthy), Some(0));
+        // Carrier 0 backed up (≥ high water) → spill to carrier 1.
+        let c0_saturated = [
+            slot(0, PRIORITY_QUEUE_HIGH_WATER, 30),
+            slot(1, 200_000, 40),
+            slot(2, 0, 55),
+        ];
+        assert_eq!(pick1(mt_net::MergePolicy::Priority, &c0_saturated), Some(1));
+        // Everything backed up → the least-queued carrier takes the excess.
+        let all_saturated = [
+            slot(0, 2_000_000, 30),
+            slot(1, 700_000, 40),
+            slot(2, 1_500_000, 55),
+        ];
+        assert_eq!(
+            pick1(mt_net::MergePolicy::Priority, &all_saturated),
+            Some(1)
+        );
+    }
+}
diff --git a/crates/mt-bond/src/lib.rs b/crates/mt-bond/src/lib.rs
index aac74a8..91b46a7 100755
--- a/crates/mt-bond/src/lib.rs
+++ b/crates/mt-bond/src/lib.rs
@@ -14,8 +14,8 @@
 //! Module map:
 //! - `frame`   — the 24-byte wire framing + OPEN target codec (pure).
 //! - `reorder` — per-session inbound reordering (pure).
-//! - `endpoint`— carriers, sessions, round-robin scheduling, cumulative
-//!   ACKs, replay-on-carrier-death, keepalives, sweeps.
+//! - `endpoint`— carriers, sessions, policy-aware scheduling (S11),
+//!   cumulative ACKs, replay-on-carrier-death, keepalives, sweeps.
 //! - `outbound`— [`BondOutbound`] (the client `mt_net::Outbound`) and the
 //!   app-facing [`outbound::BondStream`].
 //! - `daemon`  — the server half (`mt-merge-server` binary).
@@ -33,7 +33,8 @@ pub use daemon::{serve, BOND_IDLE_GC};
 pub use endpoint::{
     CarrierSnapshot, APP_BACKLOG_CAP, CARRIER_RESPAWN_ATTEMPTS, CARRIER_RESPAWN_BACKOFF,
     DIAL_TIMEOUT, MAX_SESSIONS_PER_BOND, PENDING_OPEN_TIMEOUT, PING_INTERVAL, PONG_TIMEOUT,
-    REORDER_CAP, STALL_TIMEOUT, SWEEP_INTERVAL, VERIFY_PONG_TIMEOUT, WRITER_QUEUE,
+    PRIORITY_QUEUE_HIGH_WATER, REORDER_CAP, STALL_TIMEOUT, SWEEP_INTERVAL,
+    VERIFY_PONG_TIMEOUT, WRITER_QUEUE,
 };
 pub use frame::{
     decode_header, decode_target, encode_target, read_frame, write_frame, CloseReason, Frame, Kind,
diff --git a/crates/mt-bond/src/outbound.rs b/crates/mt-bond/src/outbound.rs
index ed242b0..a14e25b 100755
--- a/crates/mt-bond/src/outbound.rs
+++ b/crates/mt-bond/src/outbound.rs
@@ -51,10 +51,25 @@ pub struct BondOutbound {
 }
 
 impl BondOutbound {
-    /// New bond over `links` (one per bonded adapter). `merge_target` is
-    /// the daemon address AS SEEN BY THE SERVER (e.g. 127.0.0.1:7443) —
-    /// carriers dial it THROUGH each link's tunnel.
+    /// New bond over `links` (one per bonded adapter) with the stock
+    /// round-robin carrier scheduling. `merge_target` is the daemon
+    /// address AS SEEN BY THE SERVER (e.g. 127.0.0.1:7443) — carriers
+    /// dial it THROUGH each link's tunnel.
     pub fn new(links: Vec<Arc<dyn Outbound>>, merge_target: TargetAddr) -> Arc<Self> {
+        Self::with_policy(links, merge_target, mt_net::MergePolicy::RoundRobin)
+    }
+
+    /// S11: new bond with an explicit carrier-scheduling policy — the
+    /// user's merge policy mapped onto frame spreading: `Balanced` sends
+    /// each frame down the carrier with the least writer backpressure
+    /// (capacity-proportional even spreading), `Priority` fills carriers
+    /// in the user's adapter order until one backs up (see
+    /// [`crate::endpoint::PRIORITY_QUEUE_HIGH_WATER`]).
+    pub fn with_policy(
+        links: Vec<Arc<dyn Outbound>>,
+        merge_target: TargetAddr,
+        policy: mt_net::MergePolicy,
+    ) -> Arc<Self> {
         // Bond id: time+pid hash — unique across reconnects (the daemon
         // groups carriers by it; a reconnect must form a NEW bond).
         let bond_id = {
@@ -68,7 +83,8 @@ impl BondOutbound {
                 .wrapping_add(pid.rotate_left(32));
             (h >> 32) as u32 ^ (h as u32)
         };
-        let endpoint = Endpoint::client(bond_id, links.clone(), merge_target.clone());
+        let endpoint =
+            Endpoint::client_with_policy(bond_id, links.clone(), merge_target.clone(), policy);
         Arc::new(Self {
             endpoint,
             links,
diff --git a/crates/mt-net/src/lib.rs b/crates/mt-net/src/lib.rs
index 9f27fcb..c41ad63 100755
--- a/crates/mt-net/src/lib.rs
+++ b/crates/mt-net/src/lib.rs
@@ -54,6 +54,11 @@ pub trait Outbound: Send + Sync {
 }
 
 /// Aggregation policy for spreading new connections across links.
+///
+/// S11: `Balanced` (byte-load-aware spreading) and `Priority` (user
+/// priority order with saturation spill) join the original three. The
+/// per-connection picker consumes the live LOAD fields of
+/// [`LinkHealth`] (`rate_bps`, `active_conns`, `capacity_bps`).
 #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
 #[serde(rename_all = "lowercase")]
 pub enum MergePolicy {
@@ -64,6 +69,15 @@ pub enum MergePolicy {
     RoundRobin,
     /// Prefer the link with the best (EWMA) latency.
     LowestLatency,
+    /// S11: spread by live BYTE LOAD — each new connection goes to the
+    /// link carrying the least traffic right now (rate + a per-connection
+    /// reservation), so fat flows stop skewing the spread.
+    Balanced,
+    /// S11: user priority order (link order = the Adapters-screen order).
+    /// The first link takes everything until its measured rate reaches its
+    /// learned capacity (saturation); the excess spills to the next link,
+    /// and so on. When every link is saturated the least-loaded one wins.
+    Priority,
 }
 
 /// Live health of one link, updated by the background prober.
@@ -76,6 +90,23 @@ pub struct LinkHealth {
     pub ewma_ms: f32,
     pub consecutive_failures: u32,
     pub quarantined_until: Option<Instant>,
+    /// S11 load tracking (per-connection scheduling): live connections
+    /// placed on this link (incremented at establish, decremented when the
+    /// connection ends).
+    pub active_conns: u32,
+    /// S11: EWMA of this link's total (up+down) throughput in bytes/s,
+    /// refreshed once per server tick from the data-plane counters.
+    pub rate_bps: u64,
+    /// S11: learned capacity estimate (peak observed rate, slowly decaying
+    /// toward the current rate while idle). Below [`merge::CAPACITY_FLOOR_BPS`]
+    /// it counts as "unknown" — the link is assumed to have headroom.
+    pub capacity_bps: u64,
+    /// S11: consecutive ticks without a meaningful capacity rise (the
+    /// RAMP→PLATEAU detector behind saturation; see [`merge::update_load`]).
+    pub plateau_ticks: u32,
+    /// S11: saturation verdict — rate pinned at a KNOWN capacity that has
+    /// stopped rising. Recomputed by [`merge::update_load`] every tick.
+    pub saturated: bool,
 }
 
 /// Routing rule matcher.
@@ -160,7 +191,10 @@ mod socks5;
 mod test_support;
 
 pub use error::NetError;
-pub use merge::{pick, Aggregator, QuarantinePolicy};
+pub use merge::{
+    pick, pressure_of, relative_load_of, update_load, Aggregator, QuarantinePolicy,
+    CAPACITY_FLOOR_BPS, CONN_RESERVE_BPS, SATURATION_FRACTION,
+};
 pub use outbound::DirectOutbound;
 pub use router::Router;
 pub use server::ProxyServer;
diff --git a/crates/mt-net/src/merge.rs b/crates/mt-net/src/merge.rs
index 9124290..5d759ba 100755
--- a/crates/mt-net/src/merge.rs
+++ b/crates/mt-net/src/merge.rs
@@ -1,15 +1,19 @@
 //! Link aggregation (net-merge style): health table, merge policies,
-//! quarantine and the per-link background prober.
+//! quarantine, per-link LOAD tracking and the per-link background prober.
 //!
-//! CONTRACT (harness/CONTRACTS-S6.md "Expert B"):
-//! - `MergePolicy::{Failover, RoundRobin, LowestLatency}` (frozen in lib).
+//! CONTRACT (harness/CONTRACTS-S6.md "Expert B", extended by S11/D23):
+//! - `MergePolicy::{Failover, RoundRobin, LowestLatency, Balanced, Priority}`.
 //! - `LinkHealth { ok, latency, ewma_ms, consecutive_failures,
-//!   quarantined_until }` (frozen in lib).
+//!   quarantined_until, active_conns, rate_bps, capacity_bps, saturated }`.
 //! - Quarantine after 3 consecutive failures for 30 s (constants
 //!   [`QUARANTINE_THRESHOLD`] / [`QUARANTINE_DURATION`], overridable per
 //!   aggregator via [`QuarantinePolicy`]).
 //! - `pick()` is a pure, unit-testable function.
 //! - A per-link background prober task with a configurable interval.
+//! - S11: the data plane feeds per-link byte counters + connection
+//!   open/close events into the health table once per tick
+//!   ([`Aggregator::tick_loads`]); `Balanced` picks by live byte load and
+//!   `Priority` picks by user order with a saturation gate.
 //!
 //! IMPLEMENTATION NOTES (S6-B decisions, "pick one and document it"):
 //! - ARCHITECTURE — shared state, not an actor. Healths live in one
@@ -53,7 +57,25 @@
 //!   (ties → lowest index); unknown-sample links rank last (but ahead of
 //!   nothing) — among unknowns the lowest index wins, so an all-unknown
 //!   table degrades gracefully to failover order.
-
+//! - S11 `Balanced`: pick the candidate with the smallest PRESSURE =
+//!   `rate_bps + active_conns × CONN_RESERVE_BPS`. The reservation makes
+//!   a burst of connections inside one 1 s tick spread instead of
+//!   stacking on the one link whose byte counters have not ticked yet;
+//!   ties rotate via the shared cursor (so an idle table behaves exactly
+//!   like round-robin).
+//! - S11 `Priority`: candidates are walked in LINK ORDER (which IS the
+//!   user's adapter priority order — links are built one per bonded
+//!   adapter in the settings-file order). The first candidate that is NOT
+//!   saturated wins; when every candidate is saturated, the one with the
+//!   smallest RELATIVE load (`rate_bps / capacity_bps`, unknown capacity
+//!   → 0 = most headroom) takes the excess. Saturation needs a KNOWN
+//!   capacity (≥ [`CAPACITY_FLOOR_BPS`]) so a fresh link is assumed to
+//!   have headroom — the first fat flow ramps on the top-priority link,
+//!   the capacity estimate converges on its plateau, and only then does
+//!   the next link start taking traffic (the "fill first, spill when
+//!   saturated" semantics the users asked for).
+
+use std::sync::atomic::{AtomicU64, Ordering};
 use std::sync::{Arc, Mutex, MutexGuard};
 use std::time::{Duration, Instant};
 
@@ -71,6 +93,97 @@ const EWMA_NEW_WEIGHT: f32 = 0.3;
 /// Budget for one background probe.
 pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
 
+// ---------------------------------------------------------------------------
+// S11 load tracking (per-connection Balanced / Priority scheduling)
+// ---------------------------------------------------------------------------
+
+/// Weight of a NEW rate sample in the throughput EWMA blend.
+pub const LOAD_EWMA_NEW_WEIGHT: f64 = 0.35;
+/// Per-tick decay of the learned capacity when the current rate sits
+/// below it (multiplicative; 0.995/s ≈ halves in ~2.3 min). Keeps a stale
+/// spike from pinning "saturated" forever while staying flat enough that
+/// a link sitting AT its capacity remains saturated.
+pub const CAPACITY_DECAY_PER_TICK: f64 = 0.995;
+/// A capacity estimate below this floor counts as UNKNOWN (the link is
+/// assumed to have headroom). 256 KiB/s ≈ 2 Mbps — a link carrying only
+/// background chatter should never count as saturated.
+pub const CAPACITY_FLOOR_BPS: u64 = 256 * 1024;
+/// A link counts as saturated when its rate reaches this fraction of its
+/// learned capacity (0.85 leaves headroom for ramp jitter).
+pub const SATURATION_FRACTION: f64 = 0.85;
+/// A capacity rise above this fraction (2%/tick) counts as STILL RAMPING
+/// and resets the plateau streak (a slower ramp is within the saturation
+/// band's headroom anyway).
+const CAPACITY_RISE_RESET: f64 = 1.02;
+/// Consecutive non-rising ticks before the capacity counts as a PLATEAU
+/// (the rate is pinned at the adapter's real bandwidth, not still
+/// climbing toward it).
+pub const PLATEAU_TICKS: u32 = 3;
+/// Assumed pressure of one OPEN connection with no measured bytes yet —
+/// lets bursts inside a single 1 s tick spread across links instead of
+/// stacking on the one whose counters have not ticked (128 KiB/s ≈ 1 Mbps
+/// of nominal expectation per connection).
+pub const CONN_RESERVE_BPS: u64 = 128 * 1024;
+
+/// Advance one link's load estimate by one tick (pure; unit-tested).
+///
+/// * `delta_bytes` — data-plane bytes observed on this link during the tick.
+/// * `dt` — the tick's wall length (rate = delta/dt).
+///
+/// Updates `rate_bps` (EWMA), `capacity_bps` (rising instantly to the
+/// observed rate, decaying by [`CAPACITY_DECAY_PER_TICK`] otherwise),
+/// `plateau_ticks` (consecutive ticks without a ≥2% capacity rise) and
+/// the `saturated` flag. A zero `dt` is treated as 1 s (defensive; the
+/// caller is the 1 s server ticker).
+///
+/// Saturation is a PLATEAU, not a level: because the capacity estimate
+/// rises to meet any new rate, "rate ≈ capacity" is true during a ramp
+/// too — so saturation additionally requires the capacity to have
+/// STOPPED rising for [`PLATEAU_TICKS`] ticks. A link still climbing is
+/// never saturated; a link pinned at its ceiling is.
+pub fn update_load(h: &mut LinkHealth, delta_bytes: u64, dt: Duration) {
+    let secs = dt.as_secs_f64();
+    let secs = if secs > 0.0 { secs } else { 1.0 };
+    let sample = delta_bytes as f64 / secs;
+    // EWMA of the rate (first sample seeds it).
+    let rate = if h.rate_bps == 0 {
+        sample
+    } else {
+        h.rate_bps as f64 * (1.0 - LOAD_EWMA_NEW_WEIGHT) + sample * LOAD_EWMA_NEW_WEIGHT
+    };
+    h.rate_bps = rate.max(0.0) as u64;
+    // Capacity: rises instantly, decays slowly toward the current rate.
+    let cap_old = h.capacity_bps as f64;
+    let cap_new = (cap_old * CAPACITY_DECAY_PER_TICK).max(h.rate_bps as f64);
+    h.capacity_bps = cap_new as u64;
+    // Plateau streak: a ≥2% rise means the link is still ramping.
+    if cap_new > cap_old * CAPACITY_RISE_RESET {
+        h.plateau_ticks = 0;
+    } else {
+        h.plateau_ticks = h.plateau_ticks.saturating_add(1);
+    }
+    // Saturation needs a KNOWN capacity, a rate at the ceiling and a
+    // capacity that has stopped rising.
+    h.saturated = h.capacity_bps >= CAPACITY_FLOOR_BPS
+        && h.plateau_ticks >= PLATEAU_TICKS
+        && h.rate_bps as f64 >= h.capacity_bps as f64 * SATURATION_FRACTION;
+}
+
+/// Balanced-policy pressure of one link (pure).
+pub fn pressure_of(h: &LinkHealth) -> u64 {
+    h.rate_bps
+        .saturating_add(h.active_conns as u64 * CONN_RESERVE_BPS)
+}
+
+/// Priority-policy relative load of one link (pure; 0.0 = most headroom).
+pub fn relative_load_of(h: &LinkHealth) -> f64 {
+    if h.capacity_bps < CAPACITY_FLOOR_BPS {
+        0.0
+    } else {
+        h.rate_bps as f64 / h.capacity_bps as f64
+    }
+}
+
 /// Quarantine knobs; `Default` = contract values (3 failures / 30 s).
 #[derive(Debug, Clone, Copy)]
 pub struct QuarantinePolicy {
@@ -165,12 +278,53 @@ pub(crate) fn pick_excluding(
             }
             Some(best)
         }
+        // S11: least live byte pressure; ties rotate (idle table ==
+        // round-robin; a burst spreads via the per-connection reserve).
+        MergePolicy::Balanced => {
+            let min = candidates
+                .iter()
+                .map(|&i| pressure_of(&healths[i]))
+                .min()
+                .expect("candidates is non-empty");
+            let tied: Vec<usize> = candidates
+                .iter()
+                .copied()
+                .filter(|&i| pressure_of(&healths[i]) == min)
+                .collect();
+            let idx = tied[(*counter % tied.len() as u64) as usize];
+            *counter = counter.wrapping_add(1);
+            Some(idx)
+        }
+        // S11: user priority order (link order); first unsaturated wins,
+        // all saturated → smallest relative load takes the excess.
+        MergePolicy::Priority => {
+            for &i in candidates {
+                if !healths[i].saturated {
+                    return Some(i);
+                }
+            }
+            let mut best = first;
+            let mut best_rel = relative_load_of(&healths[best]);
+            for &i in candidates.iter().skip(1) {
+                let rel = relative_load_of(&healths[i]);
+                if rel < best_rel {
+                    best = i;
+                    best_rel = rel;
+                }
+            }
+            Some(best)
+        }
     }
 }
 
+/// Poison-tolerant std lock (repo rule; generic over the guarded type).
+fn lock_std<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
+    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
+}
+
 /// Poison-tolerant lock for the tiny round-robin counter (root-crate style).
 fn lock_counter(m: &Mutex<u64>) -> MutexGuard<'_, u64> {
-    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
+    lock_std(m)
 }
 
 /// Optimistic initial health (see module docs).
@@ -181,28 +335,43 @@ fn initial_health() -> LinkHealth {
         ewma_ms: 0.0,
         consecutive_failures: 0,
         quarantined_until: None,
+        active_conns: 0,
+        rate_bps: 0,
+        capacity_bps: 0,
+        plateau_ticks: 0,
+        saturated: false,
     }
 }
 
 /// Shared-state aggregator over the link table: health recording, policy
-/// picks and snapshots. All mutable state is behind the async health mutex
-/// (poison-free) and the tiny counter mutex (poison-tolerant).
+/// picks, snapshots and (S11) live LOAD tracking. All mutable state is
+/// behind the async health mutex (poison-free) and the tiny counter mutex
+/// (poison-tolerant); the per-link byte counters are lock-free atomics the
+/// data-plane pumps bump directly.
 pub struct Aggregator {
     links: Vec<Arc<dyn Outbound>>,
     healths: Arc<AsyncMutex<Vec<LinkHealth>>>,
     counter: Mutex<u64>,
     quarantine: QuarantinePolicy,
+    /// S11: cumulative data-plane bytes per link (up+down), bumped by the
+    /// tunnel pumps; `tick_loads` derives the per-tick deltas.
+    link_bytes: Vec<Arc<AtomicU64>>,
+    /// S11: last cumulative reading per link (the delta base).
+    link_bytes_last: Mutex<Vec<u64>>,
 }
 
 impl Aggregator {
     /// New aggregator with optimistic initial health for every link.
     pub fn new(links: Vec<Arc<dyn Outbound>>) -> Self {
         let healths = links.iter().map(|_| initial_health()).collect();
+        let n = links.len();
         Self {
             links,
             healths: Arc::new(AsyncMutex::new(healths)),
             counter: Mutex::new(0),
             quarantine: QuarantinePolicy::default(),
+            link_bytes: (0..n).map(|_| Arc::new(AtomicU64::new(0))).collect(),
+            link_bytes_last: Mutex::new(vec![0; n]),
         }
     }
 
@@ -227,6 +396,56 @@ impl Aggregator {
         self.links.get(idx).cloned()
     }
 
+    /// S11: the cumulative byte counter of link `idx` (up+down). The
+    /// tunnel pumps `fetch_add` on it as bytes flow; [`Self::tick_loads`]
+    /// derives the deltas. `None` when `idx` is out of range.
+    pub fn link_bytes(&self, idx: usize) -> Option<Arc<AtomicU64>> {
+        self.link_bytes.get(idx).cloned()
+    }
+
+    /// S11: one live connection was placed on link `idx` (establish
+    /// success). Feeds the Balanced pressure and the GUI status.
+    pub async fn record_conn_open(&self, idx: usize) {
+        let mut hs = self.healths.lock().await;
+        if let Some(h) = hs.get_mut(idx) {
+            h.active_conns = h.active_conns.saturating_add(1);
+        }
+    }
+
+    /// S11: one connection on link `idx` ended (tunnel teardown).
+    pub async fn record_conn_close(&self, idx: usize) {
+        let mut hs = self.healths.lock().await;
+        if let Some(h) = hs.get_mut(idx) {
+            h.active_conns = h.active_conns.saturating_sub(1);
+        }
+    }
+
+    /// S11: advance every link's load estimate by one tick — reads the
+    /// cumulative per-link counters, derives deltas against the last
+    /// reading and runs [`update_load`] for each link. Called by the
+    /// server's 1 s ticker.
+    pub async fn tick_loads(&self, dt: Duration) {
+        // Collect-then-lock (repo rule: no guard across an await — the
+        // collect is sync, the update takes the async lock once).
+        let deltas: Vec<u64> = {
+            let mut last = lock_std(&self.link_bytes_last);
+            self.link_bytes
+                .iter()
+                .zip(last.iter_mut())
+                .map(|(c, l)| {
+                    let cur = c.load(Ordering::Relaxed);
+                    let d = cur.saturating_sub(*l);
+                    *l = cur;
+                    d
+                })
+                .collect()
+        };
+        let mut hs = self.healths.lock().await;
+        for (h, d) in hs.iter_mut().zip(deltas) {
+            update_load(h, d, dt);
+        }
+    }
+
     /// `(name, health)` snapshot of the whole table.
     pub async fn snapshot(&self) -> Vec<(String, LinkHealth)> {
         let hs = self.healths.lock().await;
@@ -346,6 +565,33 @@ mod tests {
             ewma_ms,
             consecutive_failures: 0,
             quarantined_until: quarantine.map(|d| Instant::now() + d),
+            active_conns: 0,
+            rate_bps: 0,
+            capacity_bps: 0,
+            plateau_ticks: 0,
+            saturated: false,
+        }
+    }
+
+    /// A link with explicit S11 load state.
+    fn loaded(
+        ok: bool,
+        rate_bps: u64,
+        capacity_bps: u64,
+        active_conns: u32,
+        saturated: bool,
+    ) -> LinkHealth {
+        LinkHealth {
+            ok,
+            latency: Some(Duration::from_millis(20)),
+            ewma_ms: 20.0,
+            consecutive_failures: 0,
+            quarantined_until: None,
+            active_conns,
+            rate_bps,
+            capacity_bps,
+            plateau_ticks: if saturated { PLATEAU_TICKS } else { 0 },
+            saturated,
         }
     }
 
@@ -447,11 +693,207 @@ mod tests {
             ewma_ms: 0.0,
             consecutive_failures: 3,
             quarantined_until: Some(past),
+            active_conns: 0,
+            rate_bps: 0,
+            capacity_bps: 0,
+            plateau_ticks: 0,
+            saturated: false,
         }];
         assert_eq!(pick1(MergePolicy::Failover, &mut hs, None), Some(0));
         assert_eq!(hs[0].quarantined_until, None, "expired quarantine cleared");
     }
 
+    // -----------------------------------------------------------------
+    // S11: Balanced / Priority / update_load
+    // -----------------------------------------------------------------
+
+    #[test]
+    fn balanced_picks_the_least_loaded_link() {
+        // Link 0 carries a fat flow, link 1 is idle, link 2 has some
+        // traffic — the exact "one adapter hogs everything" shape.
+        let mut hs = vec![
+            loaded(true, 8_000_000, 8_000_000, 1, false),
+            loaded(true, 0, 0, 0, false),
+            loaded(true, 2_000_000, 4_000_000, 1, false),
+        ];
+        assert_eq!(pick1(MergePolicy::Balanced, &mut hs, None), Some(1));
+    }
+
+    #[test]
+    fn balanced_counts_the_connection_reserve_for_bursts() {
+        // All rates zero (counters have not ticked yet): two links each
+        // hold one open connection, one holds none — the empty one wins
+        // WITHOUT waiting for a byte tick.
+        let mut hs = vec![
+            loaded(true, 0, 0, 2, false),
+            loaded(true, 0, 0, 0, false),
+            loaded(true, 0, 0, 1, false),
+        ];
+        assert_eq!(pick1(MergePolicy::Balanced, &mut hs, None), Some(1));
+    }
+
+    #[test]
+    fn balanced_ties_rotate_like_round_robin() {
+        // Three equally idle links: the cursor cycles (an idle table
+        // behaves exactly like round-robin).
+        let mut hs = vec![
+            loaded(true, 0, 0, 0, false),
+            loaded(true, 0, 0, 0, false),
+            loaded(true, 0, 0, 0, false),
+        ];
+        let mut counter = 0u64;
+        let now = Instant::now();
+        let seq: Vec<usize> = (0..3)
+            .map(|_| {
+                pick_excluding(&mut hs, MergePolicy::Balanced, &mut counter, now, None).unwrap()
+            })
+            .collect();
+        assert_eq!(seq, vec![0, 1, 2]);
+        assert_eq!(counter, 3);
+    }
+
+    #[test]
+    fn balanced_skips_quarantined_and_down_links() {
+        let mut hs = vec![
+            loaded(false, 0, 0, 0, false), // down → tier 2
+            loaded(true, 9_000_000, 9_000_000, 0, false),
+            loaded(true, 0, 0, 0, false),
+        ];
+        // Tier-1 candidates are 1 (loaded) and 2 (idle): 2 wins.
+        assert_eq!(pick1(MergePolicy::Balanced, &mut hs, None), Some(2));
+        // Quarantine is a hard gate for every policy.
+        hs[2].quarantined_until = Some(Instant::now() + Duration::from_secs(10));
+        assert_eq!(pick1(MergePolicy::Balanced, &mut hs, None), Some(1));
+    }
+
+    #[test]
+    fn priority_fills_the_first_unsaturated_link_in_order() {
+        // Link 0 saturated at its learned capacity → spill to link 1;
+        // link 1 unsaturated → wins despite link 2 being idle.
+        let mut hs = vec![
+            loaded(true, 8_000_000, 8_000_000, 3, true),
+            loaded(true, 1_000_000, 6_000_000, 1, false),
+            loaded(true, 0, 0, 0, false),
+        ];
+        assert_eq!(pick1(MergePolicy::Priority, &mut hs, None), Some(1));
+        // Link 0 unsaturated → everything goes to the top priority.
+        hs[0] = loaded(true, 1_000_000, 8_000_000, 1, false);
+        assert_eq!(pick1(MergePolicy::Priority, &mut hs, None), Some(0));
+    }
+
+    #[test]
+    fn priority_all_saturated_spills_to_least_relative_load() {
+        // 0 at 95% of capacity, 1 at 60%, 2 unknown (0%) — the unknown
+        // link has the most headroom and wins.
+        let mut hs = vec![
+            loaded(true, 9_500_000, 10_000_000, 2, true),
+            loaded(true, 6_000_000, 10_000_000, 1, true),
+            loaded(true, 500_000, 100_000, 1, true), // capacity < floor
+        ];
+        assert_eq!(pick1(MergePolicy::Priority, &mut hs, None), Some(2));
+        // Known capacities only: the smaller ratio wins.
+        hs[2] = loaded(true, 4_000_000, 10_000_000, 0, true);
+        assert_eq!(pick1(MergePolicy::Priority, &mut hs, None), Some(2));
+    }
+
+    #[test]
+    fn priority_honors_quarantine_and_exclusion() {
+        let mut hs = vec![loaded(true, 0, 0, 0, false), loaded(true, 0, 0, 0, false)];
+        hs[0].quarantined_until = Some(Instant::now() + Duration::from_secs(10));
+        assert_eq!(pick1(MergePolicy::Priority, &mut hs, None), Some(1));
+        // Transparent retry must land on a DIFFERENT link.
+        let mut hs2 = vec![loaded(true, 0, 0, 0, false), loaded(true, 0, 0, 0, false)];
+        assert_eq!(pick1(MergePolicy::Priority, &mut hs2, Some(0)), Some(1));
+    }
+
+    #[test]
+    fn update_load_ramps_then_saturates_then_recovers() {
+        let mut h = initial_health();
+        let s = Duration::from_secs(1);
+        // RAMP: 10% growth per tick — the capacity (tracking the rate
+        // EWMA) keeps rising by more than the 2% ramp threshold, so a
+        // climbing link is never marked saturated.
+        let mut rate = 4_000_000u64;
+        for _ in 0..10 {
+            update_load(&mut h, rate, s);
+            assert!(!h.saturated, "still ramping (rate {rate})");
+            rate = rate * 110 / 100;
+        }
+        // PLATEAU: the sample pins. The EWMA first converges up to it
+        // (still "ramping"), then the capacity stops rising and the link
+        // reads saturated — and STAYS saturated while pinned.
+        let mut saw_saturated = false;
+        for _ in 0..20 {
+            update_load(&mut h, rate, s);
+            if saw_saturated {
+                assert!(h.saturated, "pinned rate stays saturated");
+            } else {
+                saw_saturated = h.saturated;
+            }
+        }
+        assert!(saw_saturated, "a pinned rate must saturate within 20 ticks");
+        assert!(h.capacity_bps >= CAPACITY_FLOOR_BPS);
+        // RECOVERY: demand halves — the rate falls out of the saturation
+        // band long before the capacity decays.
+        update_load(&mut h, rate / 2, s);
+        update_load(&mut h, rate / 2, s);
+        assert!(!h.saturated, "load below the band → unsaturated");
+    }
+
+    #[test]
+    fn update_load_ignores_tiny_flows_and_zero_dt() {
+        let mut h = initial_health();
+        // Background chatter stays under the floor: never saturated.
+        for _ in 0..30 {
+            update_load(&mut h, 50_000, Duration::from_secs(1));
+        }
+        assert!(!h.saturated);
+        assert!(h.capacity_bps < CAPACITY_FLOOR_BPS);
+        // Zero dt is defensive, not a panic (fresh health → seeds).
+        let mut h2 = initial_health();
+        update_load(&mut h2, 12345, Duration::ZERO);
+        assert_eq!(h2.rate_bps, 12345);
+    }
+
+    #[tokio::test]
+    async fn aggregator_tick_loads_and_conn_accounting() {
+        let links: Vec<Arc<dyn Outbound>> = vec![
+            Arc::new(FakeOutbound::connect_fail("a")),
+            Arc::new(FakeOutbound::connect_fail("b")),
+        ];
+        let agg = Aggregator::new(links);
+        // Two open connections on link 0, one on link 1.
+        agg.record_conn_open(0).await;
+        agg.record_conn_open(0).await;
+        agg.record_conn_open(1).await;
+        // Bytes flow only on link 0.
+        agg.link_bytes(0)
+            .unwrap()
+            .fetch_add(5_000_000, Ordering::Relaxed);
+        agg.tick_loads(Duration::from_secs(1)).await;
+        let snap = agg.snapshot().await;
+        assert_eq!(snap[0].1.active_conns, 2);
+        assert_eq!(snap[1].1.active_conns, 1);
+        assert_eq!(snap[0].1.rate_bps, 5_000_000);
+        assert_eq!(snap[1].1.rate_bps, 0);
+        // Balanced now avoids link 0 (5 MB/s + 2 reserves) for link 1
+        // (1 reserve).
+        let (idx, _) = agg.pick_link(MergePolicy::Balanced, None).await.unwrap();
+        assert_eq!(idx, 1);
+        // A second tick with no NEW bytes: the delta base moved, rate
+        // decays but stays positive-ish; connections close → pressure
+        // drops.
+        agg.record_conn_close(0).await;
+        agg.record_conn_close(0).await;
+        agg.tick_loads(Duration::from_secs(1)).await;
+        let snap = agg.snapshot().await;
+        assert_eq!(snap[0].1.active_conns, 0);
+        assert!(
+            snap[0].1.rate_bps < 5_000_000,
+            "EWMA decays with no new bytes"
+        );
+    }
+
     #[tokio::test]
     async fn aggregator_quarantines_after_three_failures_then_recovers() {
         let links: Vec<Arc<dyn Outbound>> = vec![
diff --git a/crates/mt-net/src/server.rs b/crates/mt-net/src/server.rs
index 1cce8e7..fdfecf0 100755
--- a/crates/mt-net/src/server.rs
+++ b/crates/mt-net/src/server.rs
@@ -269,6 +269,7 @@ impl ProxyServer {
         }
         background.push(tokio::spawn(tick_loop(
             Arc::clone(&self.shared),
+            Arc::clone(&self.aggregator),
             self.events.clone(),
             shutdown.clone(),
         )));
@@ -323,16 +324,19 @@ impl ProxyServer {
 
     /// Pick a link (or go direct) and connect. `Err` carries the SOCKS5 REP
     /// code to report (the HTTP side maps any failure to 502). Reject is
-    /// handled by the callers before this point.
+    /// handled by the callers before this point. On the Proxy path the
+    /// third tuple element is the link's aggregator index (S11 load
+    /// accounting: the tunnel bumps that link's byte counter and closes
+    /// its connection slot at the end).
     async fn establish(
         &self,
         action: Action,
         target: &TargetAddr,
-    ) -> Result<(ProxyStream, String), u8> {
+    ) -> Result<(ProxyStream, String, Option<usize>), u8> {
         match action {
             Action::Reject => Err(rep::GENERAL_FAILURE), // defensive; callers reject earlier
             Action::Direct => match self.direct.connect(target).await {
-                Ok(s) => Ok((s, self.direct.name().to_string())),
+                Ok(s) => Ok((s, self.direct.name().to_string(), None)),
                 Err(e) => {
                     tracing::warn!(target = %target, "direct connect failed: {e:#}");
                     Err(socks5::rep_for_error(&e))
@@ -349,7 +353,8 @@ impl ProxyServer {
                 match link.connect(target).await {
                     Ok(s) => {
                         self.aggregator.record_result(idx, true, None).await;
-                        Ok((s, link.name().to_string()))
+                        self.aggregator.record_conn_open(idx).await;
+                        Ok((s, link.name().to_string(), Some(idx)))
                     }
                     Err(first) => {
                         // ONE transparent retry on a DIFFERENT link (see
@@ -370,7 +375,8 @@ impl ProxyServer {
                                         "retry after first-link failure succeeded"
                                     );
                                     self.aggregator.record_result(idx2, true, None).await;
-                                    Ok((s, link2.name().to_string()))
+                                    self.aggregator.record_conn_open(idx2).await;
+                                    Ok((s, link2.name().to_string(), Some(idx2)))
                                 }
                                 Err(second) => {
                                     self.aggregator.record_result(idx2, false, None).await;
@@ -410,15 +416,18 @@ impl ProxyServer {
             return;
         }
         match self.establish(action, &target).await {
-            Ok((upstream, link)) => {
+            Ok((upstream, link, link_idx)) => {
                 if socks5::send_success_reply(&mut stream).await.is_err() {
+                    if let Some(idx) = link_idx {
+                        self.aggregator.record_conn_close(idx).await;
+                    }
                     return;
                 }
                 self.emit(ProxyEvent::ConnOpened {
                     target: target.to_string(),
                     link: link.clone(),
                 });
-                self.tunnel(stream, upstream, &target, &link, Vec::new())
+                self.tunnel(stream, upstream, &target, &link, link_idx, Vec::new())
                     .await;
             }
             Err(rep_code) => {
@@ -447,20 +456,29 @@ impl ProxyServer {
             return;
         }
         match self.establish(action, &parsed.target).await {
-            Ok((mut upstream, link)) => {
+            Ok((mut upstream, link, link_idx)) => {
                 if parsed.is_connect {
                     if http::write_simple(&mut stream, http::RESP_OK_CONNECT)
                         .await
                         .is_err()
                     {
+                        if let Some(idx) = link_idx {
+                            self.aggregator.record_conn_close(idx).await;
+                        }
                         return;
                     }
                 } else {
                     let Some(forward) = parsed.forward else {
+                        if let Some(idx) = link_idx {
+                            self.aggregator.record_conn_close(idx).await;
+                        }
                         return;
                     };
                     if upstream.write_all(&forward).await.is_err() {
                         tracing::trace!(%peer, "upstream closed while forwarding request head");
+                        if let Some(idx) = link_idx {
+                            self.aggregator.record_conn_close(idx).await;
+                        }
                         return;
                     }
                 }
@@ -473,6 +491,7 @@ impl ProxyServer {
                     upstream,
                     &parsed.target,
                     &link,
+                    link_idx,
                     parsed.tunnel_prefix,
                 )
                 .await;
@@ -485,18 +504,23 @@ impl ProxyServer {
     }
 
     /// Bidirectional copy with byte counters + `ConnClosed` event (see
-    /// module docs for the pump semantics).
+    /// module docs for the pump semantics). `link_idx` (Proxy path only)
+    /// routes every pumped byte into that link's aggregator counter and
+    /// closes the link's connection slot when the tunnel ends (S11).
     async fn tunnel(
         &self,
         client: TcpStream,
         upstream: ProxyStream,
         target: &TargetAddr,
         link: &str,
+        link_idx: Option<usize>,
         prefix_up: Vec<u8>,
     ) {
         let started = Instant::now();
         let up_count = Arc::new(AtomicU64::new(0));
         let down_count = Arc::new(AtomicU64::new(0));
+        // S11: the per-link counter (up+down combined) the pumps bump.
+        let link_counter = link_idx.and_then(|i| self.aggregator.link_bytes(i));
         let (client_rd, client_wr) = tokio::io::split(client);
         let (up_rd, up_wr) = tokio::io::split(upstream);
         let pump_up = tokio::spawn(pump(
@@ -505,6 +529,7 @@ impl ProxyServer {
             prefix_up,
             Arc::clone(&up_count),
             Arc::clone(&self.shared.up_total),
+            link_counter.clone(),
         ));
         let pump_down = tokio::spawn(pump(
             up_rd,
@@ -512,6 +537,7 @@ impl ProxyServer {
             Vec::new(),
             Arc::clone(&down_count),
             Arc::clone(&self.shared.down_total),
+            link_counter,
         ));
         let (ra, rb) = join_pumps(pump_up, pump_down).await;
         for r in [&ra, &rb] {
@@ -523,6 +549,9 @@ impl ProxyServer {
         let down = down_count.load(Ordering::Relaxed);
         let dur_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
         tracing::debug!(target = %target, link, up, down, dur_ms, "connection closed");
+        if let Some(idx) = link_idx {
+            self.aggregator.record_conn_close(idx).await;
+        }
         self.emit(ProxyEvent::ConnClosed {
             link: link.to_string(),
             up,
@@ -542,6 +571,7 @@ async fn pump<R, W>(
     prefix: Vec<u8>,
     conn: Arc<AtomicU64>,
     global: Arc<AtomicU64>,
+    per_link: Option<Arc<AtomicU64>>,
 ) -> std::io::Result<()>
 where
     R: tokio::io::AsyncRead + Unpin + Send + 'static,
@@ -549,7 +579,7 @@ where
 {
     if !prefix.is_empty() {
         wr.write_all(&prefix).await?;
-        bump(&conn, &global, prefix.len() as u64);
+        bump(&conn, &global, per_link.as_deref(), prefix.len() as u64);
     }
     let mut buf = vec![0u8; PUMP_BUF];
     loop {
@@ -559,13 +589,16 @@ where
             return Ok(());
         }
         wr.write_all(&buf[..n]).await?;
-        bump(&conn, &global, n as u64);
+        bump(&conn, &global, per_link.as_deref(), n as u64);
     }
 }
 
-fn bump(conn: &AtomicU64, global: &AtomicU64, n: u64) {
+fn bump(conn: &AtomicU64, global: &AtomicU64, per_link: Option<&AtomicU64>, n: u64) {
     conn.fetch_add(n, Ordering::Relaxed);
     global.fetch_add(n, Ordering::Relaxed);
+    if let Some(link) = per_link {
+        link.fetch_add(n, Ordering::Relaxed);
+    }
 }
 
 /// Await both pumps: when the first one finishes with an ERROR, abort the
@@ -620,8 +653,11 @@ async fn accept_loop(server: Arc<ProxyServer>, listener: TcpListener, kind: Inbo
 }
 
 /// 1 s throughput ticker: bytes/s over the preceding window (see docs).
+/// Also advances the per-link LOAD estimates (S11) on the same cadence —
+/// the `Balanced` / `Priority` pickers read them one tick later.
 async fn tick_loop(
     shared: Arc<Shared>,
+    aggregator: Arc<Aggregator>,
     events: broadcast::Sender<ProxyEvent>,
     mut shutdown: watch::Receiver<bool>,
 ) {
@@ -629,11 +665,15 @@ async fn tick_loop(
     ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
     ticker.tick().await; // consume the immediate first tick
     let (mut last_up, mut last_down) = (0u64, 0u64);
+    let mut last_tick = Instant::now();
     loop {
         tokio::select! {
             _ = ticker.tick() => {}
             _ = wait_for_shutdown(&mut shutdown) => break,
         }
+        let now = Instant::now();
+        let dt = now - last_tick;
+        last_tick = now;
         let up = shared.up_total.load(Ordering::Relaxed);
         let down = shared.down_total.load(Ordering::Relaxed);
         let ev = ProxyEvent::Tick {
@@ -642,6 +682,7 @@ async fn tick_loop(
         };
         last_up = up;
         last_down = down;
+        aggregator.tick_loads(dt).await;
         let _ = events.send(ev);
     }
 }
@@ -813,6 +854,144 @@ mod tests {
             .expect("run ok");
     }
 
+    // ---------- S11: load-aware scheduling E2E ----------
+
+    /// Balanced avoids the link that is already carrying bytes, and
+    /// Priority keeps filling the first link while it is not saturated —
+    /// end-to-end through the real pumps + ticker (per-link counters →
+    /// tick_loads → picker).
+    #[tokio::test]
+    async fn balanced_and_priority_route_by_live_load_e2e() {
+        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
+        let echo = crate::test_support::spawn_echo().await;
+        let target = TargetAddr {
+            host: Host::Ip(echo.ip()),
+            port: echo.port(),
+        };
+
+        let start_policy = |policy| async move {
+            let links: Vec<Arc<dyn Outbound>> = vec![
+                Arc::new(FakeOutbound::echo("link-a", echo)),
+                Arc::new(FakeOutbound::echo("link-b", echo)),
+            ];
+            let server = Arc::new(ProxyServer::new(
+                "127.0.0.1:0".parse().unwrap(),
+                "127.0.0.1:0".parse().unwrap(),
+                links,
+                Router::new(Vec::new()),
+                policy,
+            ));
+            let events = server.subscribe();
+            let (shutdown_tx, shutdown_rx) = watch::channel(false);
+            let run = tokio::spawn(Arc::clone(&server).run(shutdown_rx));
+            wait_for_addr(|| server.socks_local_addr()).await;
+            TestServer {
+                server,
+                shutdown_tx,
+                events,
+                run,
+            }
+        };
+
+        // --- Balanced: conn 1 lands on link-a (idle tie → cursor); after a
+        // fat flow + one ticker tick, conn 2 must land on link-b. ---
+        let mut ts = start_policy(MergePolicy::Balanced).await;
+        let cli = socks5::Socks5Outbound::new(ts.server.socks_local_addr().unwrap());
+        let mut s1 = tokio::time::timeout(Duration::from_secs(3), cli.connect(&target))
+            .await
+            .expect("conn1 in time")
+            .expect("conn1 established");
+        // ~512 KiB through the tunnel (up + echo down) — comfortably above
+        // the capacity floor once the ticker samples it.
+        let chunk = vec![0x5au8; 64 * 1024];
+        for _ in 0..4 {
+            s1.write_all(&chunk).await.unwrap();
+            let mut back = vec![0u8; chunk.len()];
+            s1.read_exact(&mut back).await.unwrap();
+        }
+        // One real ticker tick (1 s cadence) feeds the counters into the
+        // health table.
+        tokio::time::sleep(Duration::from_millis(1300)).await;
+        let mut s2 = tokio::time::timeout(Duration::from_secs(3), cli.connect(&target))
+            .await
+            .expect("conn2 in time")
+            .expect("conn2 established");
+        s2.write_all(b"hi").await.unwrap();
+        let mut buf = [0u8; 2];
+        s2.read_exact(&mut buf).await.unwrap();
+        // Drain the two ConnOpened events and check their links.
+        let l1 = match wait_event(&mut ts.events, Duration::from_secs(2), |e| {
+            matches!(e, ProxyEvent::ConnOpened { .. })
+        })
+        .await
+        {
+            ProxyEvent::ConnOpened { link, .. } => link,
+            other => panic!("unexpected: {other:?}"),
+        };
+        let l2 = match wait_event(&mut ts.events, Duration::from_secs(2), |e| {
+            matches!(e, ProxyEvent::ConnOpened { .. })
+        })
+        .await
+        {
+            ProxyEvent::ConnOpened { link, .. } => link,
+            other => panic!("unexpected: {other:?}"),
+        };
+        assert_eq!(l1, "link-a", "idle tie rotates from the cursor start");
+        assert_eq!(
+            l2, "link-b",
+            "after a fat flow on link-a, Balanced must avoid it"
+        );
+        drop(s1);
+        drop(s2);
+        ts.shutdown_tx.send(true).unwrap();
+        let _ = tokio::time::timeout(Duration::from_secs(2), ts.run).await;
+
+        // --- Priority: while link-a is loaded but NOT saturated (its
+        // capacity is still rising — no plateau), new connections keep
+        // filling link-a. ---
+        let mut ts = start_policy(MergePolicy::Priority).await;
+        let cli = socks5::Socks5Outbound::new(ts.server.socks_local_addr().unwrap());
+        let mut s1 = tokio::time::timeout(Duration::from_secs(3), cli.connect(&target))
+            .await
+            .expect("conn1 in time")
+            .expect("conn1 established");
+        s1.write_all(&chunk).await.unwrap();
+        let mut back = vec![0u8; chunk.len()];
+        s1.read_exact(&mut back).await.unwrap();
+        tokio::time::sleep(Duration::from_millis(1300)).await;
+        let mut s2 = tokio::time::timeout(Duration::from_secs(3), cli.connect(&target))
+            .await
+            .expect("conn2 in time")
+            .expect("conn2 established");
+        s2.write_all(b"hi").await.unwrap();
+        s2.read_exact(&mut buf).await.unwrap();
+        let l1 = match wait_event(&mut ts.events, Duration::from_secs(2), |e| {
+            matches!(e, ProxyEvent::ConnOpened { .. })
+        })
+        .await
+        {
+            ProxyEvent::ConnOpened { link, .. } => link,
+            other => panic!("unexpected: {other:?}"),
+        };
+        let l2 = match wait_event(&mut ts.events, Duration::from_secs(2), |e| {
+            matches!(e, ProxyEvent::ConnOpened { .. })
+        })
+        .await
+        {
+            ProxyEvent::ConnOpened { link, .. } => link,
+            other => panic!("unexpected: {other:?}"),
+        };
+        assert_eq!(l1, "link-a");
+        assert_eq!(
+            l2, "link-a",
+            "a loaded-but-ramping link is not saturated — Priority keeps filling it"
+        );
+        drop(s1);
+        drop(s2);
+        ts.shutdown_tx.send(true).unwrap();
+        let _ = tokio::time::timeout(Duration::from_secs(2), ts.run).await;
+    }
+
     // ---------- HTTP inbound E2E (contract) ----------
 
     #[tokio::test]
diff --git a/harness/memory/DECISIONS.md b/harness/memory/DECISIONS.md
index 0e7dad6..ac6d9c0 100755
--- a/harness/memory/DECISIONS.md
+++ b/harness/memory/DECISIONS.md
@@ -297,3 +297,52 @@ panics on a non-total comparator.
    mt-net; the work was making it reachable and explaining the trade.
    "Both connections at once" = stream bonding (needs mt-merge-server on
    the VM, D17/D18); the Advanced card now says exactly that.
+
+## D23 — S11: even byte spreading (Balanced) + adapter priority order (Priority), v0.7.0
+
+User report: with 3+ bonded adapters, one carries most traffic, the
+second less, the third rarely any. Root cause: round-robin evens
+CONNECTION COUNT, not BYTES — a single fat flow dominates its adapter.
+(D22 was the S10.1 HiDPI fix — recorded in the ledger, no separate
+section here.)
+
+1. BALANCED (new stock default) is load-aware spreading: each new
+   connection goes to the link with the least live pressure
+   (`rate_bps + active_conns × 128 KiB/s`). The per-connection reserve
+   exists specifically so a burst of connections inside one 1 s counter
+   tick spreads instead of stacking on the one link whose byte counters
+   have not ticked yet. Ties rotate via the shared cursor, so an idle
+   table degrades to exactly round-robin. Settings migration v2 moves
+   the stock round-robin to balanced ONCE (D20 precedent: a version<2
+   file carrying the stock value is indistinguishable from an explicit
+   choice; an explicit re-choice persists).
+2. SATURATION IS A PLATEAU, NOT A LEVEL. The learned capacity rises to
+   meet any new rate, so "rate ≈ capacity" is true during a ramp too —
+   a naive threshold would mark a link saturated on its first fat flow.
+   Priority therefore requires the capacity to have STOPPED rising
+   (≥2%/tick resets the streak; the rate EWMA damps jitter below that)
+   for 3+ ticks AND rate ≥ 0.85×capacity AND capacity ≥ 256 KiB/s.
+   Consequences: a still-climbing link is never saturated (priority
+   keeps feeding #1 while it ramps); a pinned link saturates within
+   seconds of plateauing; when demand drops below the band the link
+   un-saturates immediately (priority returns traffic to #1 long before
+   the capacity decays).
+3. PRIORITY ORDER = the bonded_adapters LIST ORDER (ids persisted in
+   order since S8; links and stream carriers are built in that order,
+   so link index IS priority). No new settings field — the Adapters tab
+   became the ordering UI: selected rows render first with #n chips and
+   ↑/↓ buttons; ticking an adapter appends at the END (lowest priority).
+   All-saturated fallback = least relative load (unknown capacity ranks
+   as most-headroom).
+4. STREAM MODE uses WRITER-QUEUE BACKPRESSURE as the saturation signal
+   (no capacity estimation needed — TCP already measured it): carriers
+   track queued bytes (send_bytes +, writer_task −); Balanced picks the
+   least-queued carrier (capacity-proportional: fast NICs drain quicker
+   and take more frames), Priority fills carriers in order until one
+   exceeds 512 KiB queued (≈8 max frames — healthy carriers hover near
+   zero, bottlenecked ones grow without bound). The daemon keeps
+   round-robin; priority is a client-side concern.
+5. HONESTY about what per-connection Balanced can do: it places NEW
+   connections; an already-running fat flow stays on its adapter. Byte
+   evenness at the FLOW level is stream mode's job (per-connection
+   balanced + stream balanced now compose).
diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
index 646cef7..ac34291 100755
--- a/harness/memory/LEDGER.md
+++ b/harness/memory/LEDGER.md
@@ -170,3 +170,11 @@
 2026-11-19T09:47Z | s11-verify | E2E GREEN with shipped code: stream_bonding_survives_real_xray (control per-connection echo OK → 2 carriers live over real REALITY → 256 KiB byte-identical round trip → 8 short sessions → probe 38µs) and stream_bonding_falls_back_when_daemon_absent_over_real_xray (ensure_carriers fails HONESTLY in 5.1s — was phantom-attach "success" in 109ms + black-holed traffic). Gates: fmt clean, clippy --workspace --all-targets -D warnings clean, 26/26 workspace suites green (381 tests incl. the new E2E rig skipped without the env gate) + vendored iced 6. Incident: linker Bus error at 98% disk — removed target/debug/incremental (6.0G→4.6G); watch item again next release build.
 2026-11-19T11:35Z | s11-release | v0.6.2 RELEASED (tag on 231f453). First tag push (02c8759) failed the fmt gate — the clippy fixes to the E2E rig came after the fmt pass; fixed, tag force-moved to 231f453 (GH_TOKEN push, same auth as backup.sh). Release run attempt 1: build-windows wedged >60 min inside `cargo test --workspace` while CI's windows-latest leg PASSED the identical suite — wedged runner, not code; cancelled + rerun-failed-jobs (attempt 2, warm cache) → SUCCESS in ~12 min. CI attempt 1 ubuntu test leg wedged the same way → cancelled (rerun-failed-jobs 403 while cancelling; full /rerun after cancel completed) → attempt 2 ALL GREEN (lint/windows/ubuntu/cross-check). OPS LESSON: rust-cache saves only on job success, so a failed first tag build runs cold twice — expect it; a job stuck >~45 min in cargo test while a sibling runner passed the same suite = wedged runner → cancel + rerun-failed-jobs.
 2026-11-19T11:35Z | s11-release-assets | Assets verified by download: multitunnel-v0.6.2-windows-x64.zip 5,976,082 B (11-entry manifest, multitunnel.exe + multitunnel-tray.exe PE32+ x86-64; the zip's server/setup-xray-reality.sh carries the finalRules patch — grep finalRules=17, MERGE_PORT=21) + mt-merge-server x86_64/aarch64 ELFs. Staged in sandbox download/v062-verify/. v0.6.2 CLOSED — the user's remaining action is on their VM: `sudo bash server/setup-xray-reality.sh` (patches finalRules, credentials kept) and `sudo bash server/setup-merge-daemon.sh` if not yet installed; the Windows app then proves carriers with PING/PONG before trusting them.
+
+2026-11-19T13:50Z | s12 | S12 OPENED per user report (after v0.6.2): with 3+ bonded adapters "most traffic goes through one adapter, the second a little less, the third rarely any" — want (a) more even distribution, (b) a mode where adapters fill in a user-chosen ORDER (first adapter takes traffic until its bandwidth is saturated, then the next), and (c) UI to order adapters first/second/third. Root-cause read: the stock round-robin spreads CONNECTION COUNT evenly, not BYTES — one fat flow (video/download) dominates whichever adapter it landed on, which is exactly the reported shape. Sandbox had recycled again (fresh rootfs, no rustup); toolchain rebuilt per the STATE recipe (rustup 1.100.0 stable + win-gnu target, ziglang 0.14.1 wheel, cargo-zigbuild 0.24.1, python-xlib + xkb debs for the rig).
+2026-11-19T13:50Z | s12 | IMPLEMENTED (D23) — SCHEDULING: two new MergePolicy variants in mt-net. `Balanced` (new stock default, settings_version 2 migration of the stock round-robin, D20-precedent): pick() ranks candidates by live PRESSURE = rate_bps + active_conns×CONN_RESERVE_BPS (128 KiB/s reserve stops same-second bursts stacking on one link before the 1 s counter ticks); ties rotate via the shared cursor (an idle table behaves exactly like round-robin). `Priority`: walk links in USER ORDER (link order == bonded_adapters order); first candidate NOT saturated wins; all saturated → least RELATIVE load (rate/capacity, unknown capacity = most headroom). Saturation is a PLATEAU, not a level: the capacity estimate rises to meet any new rate, so "rate ≈ capacity" is true mid-ramp too — saturation additionally requires the (EWMA-tracked) capacity to have stopped rising for ≥3 ticks (CAPACITY_RISE_RESET 2%/tick; the EWMA damps jitter below that) plus rate ≥ 0.85×capacity with capacity ≥ 256 KiB/s floor (background chatter never saturates). Load plumbing: LinkHealth gained active_conns/rate_bps/capacity_bps/plateau_ticks/saturated; the server's tunnel() bumps a per-link AtomicU64 (send path via establish→link_idx→pumps), establish/close maintain active_conns, and the 1 s tick_loop calls aggregator.tick_loads(dt) (pure update_load per link, unit-tested ramp→plateau→recovery). set_settings now validates the policy name (fail loud) and treats a policy change as dataplane-changing (live reconnect).
+2026-11-19T13:50Z | s12 | IMPLEMENTED (D23) — STREAM MODE: the mt-bond carrier scheduler (D17 round-robin) is now policy-aware. Carrier gained a queued-bytes AtomicU64 (send_bytes fetch_add; writer_task fetch_sub after the socket write; writer-gone resets to 0 — a respawned carrier starts clean). pick_carrier snapshots live carriers (idx, queued, rtt_ms) into a PURE pick_carrier_index(): Failover=first, RoundRobin=cursor, LowestLatency=best sampled RTT (known beats unknown), Balanced=least queued with rotating ties (a fast NIC drains its queue quicker and naturally takes more frames — capacity-proportional even spreading), Priority=first carrier under PRIORITY_QUEUE_HIGH_WATER (512 KiB ≈ 8 max frames; a healthy carrier hovers near zero, a bottlenecked one grows) else least queued. BondOutbound::with_policy wires the GUI policy through Endpoint::client_with_policy; the daemon side keeps round-robin (priority order is a client concern). connect_flow log line now names the policy; settings.json default + mock default = "balanced".
+2026-11-19T13:50Z | s12 | IMPLEMENTED (D23) — UI: the Adapters tab is now the priority-ordering surface. Rows render selected-block-first in the SAVED bonded order (pure sort_adapter_rows; unselected rows keep enumeration order below); selected rows carry #1/#2/#3 position chips (primary container) and ↑/↓ buttons (on_press_maybe; disabled at block edges, no-ops for unselected rows); AdapterToggled maintains the block invariant (ticking ON appends at the END = lowest priority; OFF sinks below the block); AdapterMoved swaps with the previous/next SELECTED row; Apply persists the ordered id list (set_bonded_adapters already preserved order); dirty now includes ORDER changes (adapters_dirty compares draft ids vs saved ids restricted to present rows); AdaptersSaved chains load_settings → load_adapters → refresh_status so the sort key is fresh; SettingsLoaded re-sorts only when no draft is in flight. Status-tab adapter rows gained the same #n chips (priority_position_of). Settings explainer rewritten for all five policies; the Advanced bond-mode explainer notes the policy also drives the stream frame scheduler. Mock: all THREE adapters bonded (cellular now up with an IPv4) with a deliberately UNEVEN 70/25/5% byte split — the reported complaint shape, made visible in the per-adapter graphs.
+2026-11-19T13:50Z | s12 | VERIFIED: cargo test --workspace 396/0 (+18: mt-net balanced/priority/pressure/plateau/E2E-through-the-real-pumps, mt-bond pure picker ×6, root from_str/migration chains, mt-app ordering flow + sort primitive); vendored iced 6/0; fmt clean; clippy host + windows-gnu (zig shims) -D warnings clean; gui --smoke exit 0; zigbuild PE32+ ×4 (multitunnel 16.9MB, tray 9.7MB, mt-app 13MB, mt-merge-server 2.35MB) + linux mt-merge-server ELF 1.79MB. RIG (Xvfb+XTEST, pixel-verified — no VLM): the Adapters tab renders 3 bonded rows with #1/#2/#3 chips at x-offsets tracking name length; clicking a disabled ↑ (row 1) is a correct no-op; clicking ↓ swaps rows 1↔2 (chip x-centers 283↔305 — the swap is measurable); the Settings merge-policy pick_list opens a 5-row menu (change bands ×5 below the picker) and selecting row 3 changes the picker text and closes the menu; the Status tab (window resized to 1152 — the bottom card needs height) shows the same 3 priority chips on its adapter rows. NOTE: the iced default window is 1280x720 — at that height the Status adapters section sits below the fold; scroll inside the bottom card or resize taller.
+2026-11-19T13:50Z | s12 | v0.7.0: README (Scheduling & bonding modes rewritten for the five policies + priority ordering + stream-mode scheduler note), QUICKSTART (Steps 4/7), version 0.7.0. Local artifacts staged in sandbox download/: multitunnel-v0.7.0-windows-x64.zip (11,482,022 B, 8 entries, integrity OK) + mt-merge-server-v0.7.0-x86_64-unknown-linux-gnu (1,793,880 B ELF).
+2026-11-19T13:50Z | s12-merge | REBASE NOTE: this session's sandbox had recycled to a pre-S11 snapshot, so the scheduling work landed as ONE commit rebased onto the remote's v0.6.2 stream-bonding fixes (the first push was rejected; fetch showed origin/main had moved — finalRules script patch + verified carriers + tests/e2e_real_xray.rs). Conflicts resolved by MERGING BOTH: pick_carrier is policy-aware AND only considers VERIFIED carriers (usable()); send_bytes filters usable() AND tracks queued bytes; the GUI explainers carry both the priority-order wording and the finalRules/merge-daemon setup note; lib.rs exports VERIFY_PONG_TIMEOUT + PRIORITY_QUEUE_HIGH_WATER. Stage numbering: the remote session took S11 (stream bonding, v0.6.2); this work is S12. Full gates re-run after the rebase.
diff --git a/harness/memory/STATE.md b/harness/memory/STATE.md
index fafb360..0e61a4c 100755
--- a/harness/memory/STATE.md
+++ b/harness/memory/STATE.md
@@ -1,49 +1,58 @@
-# STATE — rewritten at the end of every stage. Last update: S11 (real-Xray E2E rig + stream-bonding field fix, v0.6.2)
+# STATE — rewritten at the end of every stage. Last update: S12 (balanced+priority scheduling + adapter priority order, v0.7.0, rebased onto v0.6.2)
 
 ## Current position
-- Stage: S11 CLOSED — v0.6.2 fixes the "per-connection works, stream bonding
-  does not" field report. ROOT CAUSE (server-side): Xray 26.x freedom
-  outbound's default safety policy BLACKHOLES loopback targets dialed from
-  VLESS ("target blocked by a finalrule") — the merge daemon lives on
-  127.0.0.1:<merge port>, so every carrier was black-holed. Client-side
-  aggravation: Xray's SOCKS inbound answers "connected" before the upstream
-  exists → phantom-live carriers. FIXES: (A) setup-xray-reality.sh writes a
-  narrow freedom finalRules allow (127.0.0.1:<MERGE_PORT>, tcp) and, without
-  FORCE, IDEMPOTENTLY PATCHES existing installs (backup + run -test +
-  restart, credentials kept) — the user's one-command VM fix; (B) mt-bond
-  honest carriers: a carrier is schedulable only after a PING/PONG proof
-  through the daemon (VERIFY_PONG_TIMEOUT=5s; ensure_carriers + respawn
-  demand it; fallback WARN names the fix). Daemon binary UNCHANGED.
-- NEW RIG: tests/e2e_real_xray.rs (gated MT_E2E_XRAY=1 + XRAY_BIN=...):
-  real VLESS+REALITY engines → real Xray server → in-process daemon → echo.
-  It caught the field bug that all mock-link tests could not. Dest must be a
-  REAL external site (openssl s_server breaks every REALITY handshake — and
-  s_server exits on stdin EOF; both documented in the rig).
-- Gates at close: fmt clean; clippy --workspace --all-targets -D warnings
-  clean; 26/26 workspace suites green (381 tests) + vendored iced 6;
-  E2E 2/2 green (survives + honest fallback).
-- USER ACTION REQUIRED for stream bonding: on the VM run
-  `sudo bash server/setup-xray-reality.sh` (patches finalRules, keeps
-  credentials) and confirm `sudo bash server/setup-merge-daemon.sh` is
-  installed/running; then reconnect with bonding mode "stream".
+- Stage: S12 CLOSED — v0.7.0 on the user's report "with 3+ adapters most
+  traffic rides one adapter, the second less, the third rarely any; want
+  even spreading AND a fill-in-order mode with a user-settable adapter
+  order + UI refinement". Two new merge policies (DECISIONS D23):
+  `balanced` (new stock default — live byte-load spreading with a
+  per-connection reserve; kills the one-fat-flow-hogs-one-NIC shape) and
+  `priority` (user order, fill #1 until its measured bandwidth saturates
+  — a ramp/plateau detector, not a naive threshold — then spill to #2…;
+  recovers when load drops). Stream-bond frame scheduling is
+  policy-aware too (least writer backpressure / queue high-water), and
+  the Adapters tab is the ordering UI (selected rows first with #n chips
+  + ↑/↓; the saved bonded_adapters ORDER is the priority).
+- S11 (remote session, v0.6.2) fixed STREAM BONDING: Xray 26.x freedom
+  default-policy blackholes loopback merge targets → the setup script
+  now writes a narrow finalRules allow; client carriers are VERIFIED
+  with a PING/PONG round trip before becoming schedulable (usable());
+  tests/e2e_real_xray.rs is the real-Xray E2E rig (MT_E2E_XRAY=1).
+  S12 was rebased onto it — pick_carrier is policy-aware AND
+  verified-only. THE USER'S VM may still need `sudo bash
+  server/setup-xray-reality.sh` (idempotent finalRules patch) for
+  stream mode to work.
+- Gates (re-run post-rebase): fmt clean; clippy host + windows-gnu -D
+  warnings clean; 396+ workspace tests / 0 failed (381 on the v0.6.2
+  base + the S12 scheduling/ordering tests); vendored iced 6/0; gui
+  --smoke exit 0; Xvfb rig pixel-verified the chips, the ↓ swap (chip
+  x-centers 283↔305), the 5-row policy menu and the Status chips.
+- Protocol/daemon UNCHANGED (D17 framing intact; no VM daemon upgrade).
 
 ## Next actions
-1. USER VERIFICATION of v0.6.2 stream bonding: patch the VM
-   (setup-xray-reality.sh without FORCE), confirm the daemon, connect in
-   stream mode — expect "carrier <name>: live" lines and BOTH NICs active
-   on one download (Status per-adapter graphs).
-2. Watch the v0.6.0 release run (tag push) — verify assets like v0.5.0/v0.5.1.
-3. Backlog unchanged: weighted carrier scheduling (D17 §4), subscription
-   auto-update, vmess validation, profiles.json locking, engine log
-   rotation, listener max_conns, upstream iced migration (re-base or
-   drop the vendor patches when moving past 0.13).
-4. S7 (native REALITY core) still parked.
+1. USER VERIFICATION of v0.7.0: Settings shows merge policy "balanced"
+   (upgraded files migrate from round-robin automatically); watch the
+   Status per-adapter graphs spread more evenly under load; pick
+   "priority" and reorder adapters on the Adapters tab (↑/↓) to fill
+   #1 first; in stream mode the policy drives frame spreading too. If
+   stream bonding still fails: re-run server/setup-xray-reality.sh on
+   the VM (finalRules) and server/setup-merge-daemon.sh.
+2. Watch the v0.7.0 release run (tag push) — verify assets like
+   v0.6.x. Local zip + linux daemon staged in ../download/ (NOTE: they
+   were built BEFORE the rebase — rebuild + restage if any size differs
+   from the released asset).
+3. Backlog: subscription auto-update, vmess validation, profiles.json
+   locking, engine log rotation, listener max_conns, upstream iced
+   migration, S7 native REALITY core (parked).
 
 ## Blockers / open items
 - None blocking. Watch item: disk headroom (release zigbuild needs ~5G;
-  /tmp/mt-target/debug was cleaned before the release build this stage).
-  After a sandbox recycle the whole toolchain needs reinstalling (recipe
-  below) — budget ~10 minutes.
+  /tmp/mt-target/debug was cleaned before the release build; the rig
+  rebuilds it). After a sandbox recycle the whole toolchain needs
+  reinstalling (recipe below) — budget ~10 minutes — AND the clone may
+  be a STALE snapshot: fetch origin/main FIRST and rebase before
+  building on last session's tree (this stage's push was rejected for
+  exactly that reason).
 
 ## Environment map (sandbox, rebuilt this stage)
 - Repo: /home/z/my-project/MultiTunnel — branch main, remote
@@ -56,32 +65,34 @@
      --component rustfmt,clippy`; `rustup target add
      x86_64-pc-windows-gnu`. (Rust 1.100.0.)
   2. zig: `pip3 install ziglang==0.14.1` (NETWORK install — the
-     ../tools/pypkgs EXTRACTION is INCOMPLETE: its ziglang/lib/ holds
-     only tsan/ubsan/zig.h, so the cached `zig` binary aborts with
-     "unable to find zig installation directory" on any real compile;
-     `python3 -m ziglang version` alone still works and LIES);
-     `cargo install cargo-zigbuild --locked`.
+     ../tools/pypkgs EXTRACTION is INCOMPLETE); `cargo install
+     cargo-zigbuild --locked`.
   3. Vendored-iced builds just work ([patch.crates-io] in the root
      Cargo.toml; vendor/ is committed).
 - Cross build: CARGO_TARGET_DIR=/tmp/mt-target cargo zigbuild --release
   --target x86_64-pc-windows-gnu --workspace (clean /tmp/mt-target/debug
-  first if disk-tight).
+  first if disk-tight). Zip manifest: multitunnel.exe,
+  multitunnel-tray.exe, config.example.json, LICENSE,
+  THIRD_PARTY_NOTICES.md, README.md, QUICKSTART.md.
 - GUI visual rig: python3 ../scripts/gui_drive.py --app
-  /tmp/mt-target/debug/mt-app --script "..." (verbs: wait|shot|click|
-  type|key|scroll|resize; scroll:X,Y[,up][,N] = XTEST wheel).
-  HiDPI repro: --env WINIT_X11_SCALE_FACTOR=1.5 + --size 2080x1750
-  (an sf=1.5 window defaults to 1536x1152 PHYSICAL — a smaller screen
-  clamps it and the Status graph card collapses to zero height).
-  sf=1.5 control coordinates (mock, resized 2000x1700): connect toggle
-  ≈(285,350), Check-exit-IP ≈(324,552), adapter name rows ≈(293,1333)+
-  — RE-DERIVE per run (scroll drift); pixel-scan the purple name text
-  (primary #6750A4-family bands) or the tonal button (232,222,248)
-  instead of trusting VLM coordinates. Needs: Xvfb (apt, preinstalled),
-  python-xlib (`pip3 install python-xlib`), ffmpeg (preinstalled), and
-  libxkbcommon-x11 + libxcb-xkb extracted into /tmp/xkblibs (Debian
-  debs, `apt-get download libxkbcommon-x11-0 libxcb-xkb1` +
-  `dpkg-deb -x`) with LD_LIBRARY_PATH — the driver wires all of this
-  itself. No WM runs under Xvfb: the driver calls SetInputFocus itself;
-  typed paths should be cwd-relative (XTEST Shift-layer chars are
-  unreliable). Verify claims with PIXEL scans — VLM coordinates are
-  approximate at best.
+  /tmp/mt-target/debug/mt-app --script "verb|verb|..." (verbs:
+  wait|shot|click|type|key|scroll|resize; script verbs are PIPE-separated,
+  scroll args comma-separated). Needs Xvfb (preinstalled), python-xlib
+  (`pip3 install python-xlib`), ffmpeg (preinstalled), libxkbcommon-x11 +
+  libxcb-xkb extracted into /tmp/xkblibs (Debian debs, `apt-get download
+  libxkbcommon-x11-0 libxcb-xkb1` + `dpkg-deb -x`) — the driver wires
+  LD_LIBRARY_PATH itself. No WM under Xvfb: the driver calls
+  SetInputFocus itself; typed paths should be cwd-relative.
+- RIG COORDINATES (mock, default 1280x720 window): rail buttons ≈
+  (58, 18+i·47) — Status 18, Servers 65, Adapters 112, Routing 159,
+  Settings 206, Logs 253. Adapters tab: row 1 chip ≈ y 254, ↑ ≈ (988,
+  254), ↓ ≈ (1015, 254) (rows every ~61px; the rightmost card edge is
+  ≈ x 1055; the "up" chip + its text sit at x 940-970). Settings: the
+  merge-policy pick_list ≈ (347, 359), menu rows ≈ every 33px below
+  y 376. The iced default window is 1280x720 — the Status adapters
+  section is below the fold at that height: `resize:1280x1150` (needs
+  --size ≥ that) makes the bottom card show; chip color for pixel
+  scans: primary container #EADDFF (234,221,255) tol 5, >40 px per
+  row band. RE-DERIVE per run (scroll drift); verify with PIXEL scans,
+  not VLM coordinates. A quick pixel-scan helper lives at
+  ../scripts/scan_priority_chips.py.
diff --git a/src/real_service.rs b/src/real_service.rs
index cf4ca4c..804c967 100755
--- a/src/real_service.rs
+++ b/src/real_service.rs
@@ -440,18 +440,19 @@ fn transport_name(t: &Transport) -> &'static str {
 }
 
 /// Lenient `merge_policy` string → policy. Unknown values fall back to
-/// `RoundRobin` with a warning — the D20 default (a failover default made
-/// a second ticked adapter silently idle, which reads as "bonding is
-/// broken"; round-robin gives every ticked adapter traffic). The GUI
-/// sends the canonical lowercase hyphenated names (its pick-list), which
-/// are accepted verbatim; the un-hyphenated mt-net serde names are
-/// accepted too. (S10: before this fix "lowest-latency" — the GUI's own
-/// string — fell into the fallback and silently behaved as round-robin.)
+/// `RoundRobin` with a warning — a fail-open default keeps the tunnel up
+/// (the D20 rationale). The GUI sends the canonical lowercase hyphenated
+/// names (its pick-list), which are accepted verbatim; the un-hyphenated
+/// mt-net serde names are accepted too. S11 adds `balanced` (live
+/// byte-load spreading — the v0.7.0 default) and `priority` (user
+/// adapter order with saturation spill).
 pub fn merge_policy_from_str(s: &str) -> MergePolicy {
     match s {
         "roundrobin" | "round-robin" => MergePolicy::RoundRobin,
         "lowestlatency" | "lowest-latency" => MergePolicy::LowestLatency,
         "failover" => MergePolicy::Failover,
+        "balanced" => MergePolicy::Balanced,
+        "priority" | "priority-order" | "waterfall" => MergePolicy::Priority,
         other => {
             tracing::warn!("unknown merge policy {other:?}; using round-robin");
             MergePolicy::RoundRobin
@@ -459,6 +460,24 @@ pub fn merge_policy_from_str(s: &str) -> MergePolicy {
     }
 }
 
+/// Does `s` name a KNOWN merge policy (any accepted spelling)? Used by
+/// `set_settings` so a typo'd policy fails loud instead of silently
+/// degrading to the fallback.
+pub fn is_known_merge_policy(s: &str) -> bool {
+    matches!(
+        s,
+        "roundrobin"
+            | "round-robin"
+            | "lowestlatency"
+            | "lowest-latency"
+            | "failover"
+            | "balanced"
+            | "priority"
+            | "priority-order"
+            | "waterfall"
+    )
+}
+
 /// `RuleView` (GUI) → `mt_net::Rule` (data plane). Errors are UI strings.
 fn rule_from_view(v: &RuleView) -> Result<Rule, String> {
     let matcher = match v.kind.as_str() {
@@ -527,10 +546,13 @@ impl Default for SettingsFile {
                 theme_seed: 0x6750A4,
                 dark_mode: false,
                 system_proxy: false,
-                // D20: round-robin spreads new connections across every
-                // ticked adapter; the v0.5.0 "failover" default left the
-                // second adapter idle (misread as a bonding bug).
-                merge_policy: "round-robin".to_string(),
+                // D23: "balanced" spreads new connections across the
+                // ticked adapters by LIVE byte load — the v0.7.0 default.
+                // Round-robin's connection-count evenness let one fat
+                // flow dominate a single adapter (the "one adapter hogs
+                // everything" report); failover (v0.5.0's default) left
+                // the second adapter idle entirely.
+                merge_policy: "balanced".to_string(),
                 autostart: false,
                 bonded_adapters: Vec::new(),
                 bond_mode: "per-connection".to_string(),
@@ -555,8 +577,9 @@ impl Default for SettingsFile {
     }
 }
 
-/// Current settings schema version (D20 = 1: stock merge-policy migration).
-const SETTINGS_VERSION: u32 = 1;
+/// Current settings schema version (D20 = 1: stock failover →
+/// round-robin; D23 = 2: stock round-robin → balanced).
+const SETTINGS_VERSION: u32 = 2;
 
 impl SettingsFile {
     fn load(path: &Path) -> Self {
@@ -580,14 +603,24 @@ impl SettingsFile {
         // "failover" merge policy, which sends EVERY connection down the
         // first healthy adapter — a second ticked adapter stayed idle and
         // users read that as broken bonding. Migrate the STOCK value to
-        // the new default (round-robin); an explicit re-choice of
-        // failover persists version >= 1 and is respected verbatim.
+        // the new default; an explicit re-choice persists version >= 1.
+        //
+        // D23 migration (v0.6.x → v0.7.0): the same one-time move for
+        // "round-robin" → "balanced". v0.5.1..v0.6.1 shipped round-robin
+        // as the STOCK default, so a version<2 file carrying it is
+        // indistinguishable from an explicit choice — following the D20
+        // precedent, the stock value migrates once; an explicit
+        // re-choice of round-robin persists (version 2) verbatim.
         let migrated = file.settings_version < SETTINGS_VERSION;
         if migrated {
             if file.settings_version < 1 && file.settings.merge_policy == "failover" {
                 file.settings.merge_policy = "round-robin".to_string();
                 tracing::info!("settings migration: stock merge policy failover → round-robin");
             }
+            if file.settings_version < 2 && file.settings.merge_policy == "round-robin" {
+                file.settings.merge_policy = "balanced".to_string();
+                tracing::info!("settings migration: stock merge policy round-robin → balanced");
+            }
             file.settings_version = SETTINGS_VERSION;
             // Best-effort persist so the Settings screen shows the
             // migrated value even before the next explicit save.
@@ -913,7 +946,7 @@ impl RealService {
                 };
                 // The bond keeps its OWN Arc clones of the engines; the
                 // originals stay usable for the per-connection fallback.
-                let candidate = BondOutbound::new(links.clone(), merge_target);
+                let candidate = BondOutbound::with_policy(links.clone(), merge_target, policy);
                 match candidate.ensure_carriers(CONNECT_PROBE_BUDGET).await {
                     Ok(report) => {
                         // D20: per-carrier verdicts in the GUI log — a dead
@@ -932,9 +965,9 @@ impl RealService {
                             )));
                         } else {
                             self.core.emit(ServiceEvent::LogLine(format!(
-                                "INFO  stream bonding active — {} carriers, bytes striped \
-                                 round-robin across them",
-                                report.live
+                                "INFO  stream bonding active — {} carriers, frames spread by the \
+                                 '{}' merge policy across them",
+                                report.live, settings.settings.merge_policy
                             )));
                         }
                         bond = Some(Arc::clone(&candidate));
@@ -1564,6 +1597,12 @@ impl MtService for RealService {
         if !matches!(settings.bond_mode.as_str(), "per-connection" | "stream") {
             return Err(format!("unknown bond mode {:?}", settings.bond_mode));
         }
+        if !is_known_merge_policy(&settings.merge_policy) {
+            return Err(format!(
+                "unknown merge policy {:?} — pick one from the Settings screen",
+                settings.merge_policy
+            ));
+        }
         if parse_http_url(&settings.exit_probe_url).is_none() {
             return Err(format!(
                 "exit probe URL must be plain http:// ({:?})",
@@ -1576,12 +1615,15 @@ impl MtService for RealService {
                 settings.probe_target
             ));
         }
-        // These change the LIVE data plane — remember for the restart below.
+        // These change the LIVE data plane — remember for the restart below
+        // (S11: the merge policy too — it drives per-connection spreading
+        // AND the stream-bond carrier scheduler now).
         let dataplane_changed = file.settings.socks_port != settings.socks_port
             || file.settings.http_port != settings.http_port
             || file.settings.allow_lan != settings.allow_lan
             || file.settings.bond_mode != settings.bond_mode
-            || file.settings.merge_port != settings.merge_port;
+            || file.settings.merge_port != settings.merge_port
+            || file.settings.merge_policy != settings.merge_policy;
         // Module note 15: `bonded_adapters` belongs to the Adapters screen
         // (`set_bonded_adapters`); a possibly-stale GUI settings copy must
         // never clobber a fresh selection — persist the server-side value.
@@ -2431,6 +2473,8 @@ mod tests {
 
         // (a) v0.5.0-shaped file: no settings_version, stock failover.
         // (All v0.5.0-required fields present, as a real v0.5.0 write.)
+        // D23: the migration CHAINS — failover (v0.5.0 stock) →
+        // round-robin (v0.5.1 stock) → balanced (v0.7.0 stock).
         let v050_json = concat!(
             r#"{"settings":{"theme_seed":4273518,"dark_mode":false,"system_proxy":false,"#,
             r#""merge_policy":"failover","autostart":false,"bonded_adapters":[],"#,
@@ -2439,19 +2483,42 @@ mod tests {
         std::fs::write(&path, v050_json).expect("write v0.5.0 file");
         let loaded = SettingsFile::load(&path);
         assert_eq!(
-            loaded.settings.merge_policy, "round-robin",
-            "stock failover migrates to round-robin"
+            loaded.settings.merge_policy, "balanced",
+            "stock failover migrates through the chain to balanced"
         );
         assert_eq!(loaded.settings_version, SETTINGS_VERSION);
         // The migration persists (a second load is a no-op with the same
         // value — explicit failover from NOW ON is respected).
         let persisted = std::fs::read_to_string(&path).expect("migration persisted");
-        assert!(persisted.contains("\"round-robin\""));
+        assert!(persisted.contains("\"balanced\""));
         assert!(persisted.contains(&format!("\"settings_version\": {SETTINGS_VERSION}")));
         let reloaded = SettingsFile::load(&path);
-        assert_eq!(reloaded.settings.merge_policy, "round-robin");
+        assert_eq!(reloaded.settings.merge_policy, "balanced");
+
+        // (b) v0.6.1-shaped file (version 1) with the then-stock
+        // round-robin: migrates ONCE to balanced.
+        let mut file = SettingsFile::default();
+        file.settings.merge_policy = "round-robin".to_string();
+        file.settings_version = 1;
+        file.save(&path).expect("save v0.6.1 stock");
+        let loaded = SettingsFile::load(&path);
+        assert_eq!(
+            loaded.settings.merge_policy, "balanced",
+            "v0.6.x stock round-robin migrates to balanced"
+        );
 
-        // (b) CURRENT file with an explicit failover: never touched.
+        // (c) CURRENT file with an explicit round-robin: never touched.
+        let mut file = SettingsFile::default();
+        file.settings.merge_policy = "round-robin".to_string();
+        file.settings_version = SETTINGS_VERSION;
+        file.save(&path).expect("save explicit round-robin");
+        let loaded = SettingsFile::load(&path);
+        assert_eq!(
+            loaded.settings.merge_policy, "round-robin",
+            "explicit round-robin on a current file is respected"
+        );
+
+        // (d) CURRENT file with an explicit failover: never touched.
         let mut file = SettingsFile::default();
         file.settings.merge_policy = "failover".to_string();
         file.settings_version = SETTINGS_VERSION;
@@ -2462,10 +2529,10 @@ mod tests {
             "explicit failover on a current file is respected"
         );
 
-        // (c) Fresh default (no file at all) is round-robin.
+        // (e) Fresh default (no file at all) is balanced.
         std::fs::remove_file(&path).expect("remove settings");
         let fresh = SettingsFile::load(&path);
-        assert_eq!(fresh.settings.merge_policy, "round-robin");
+        assert_eq!(fresh.settings.merge_policy, "balanced");
         assert_eq!(fresh.settings_version, SETTINGS_VERSION);
     }
 
@@ -2493,7 +2560,7 @@ mod tests {
     /// S10 regression: the GUI's pick-list sends the HYPHENATED names —
     /// before this fix "lowest-latency" (and "round-robin") fell into the
     /// unknown-value fallback, so the lowest-latency policy silently
-    /// behaved as round-robin.
+    /// behaved as round-robin. S11: the two new policies parse too.
     #[test]
     fn hyphenated_merge_policy_names_parse() {
         assert!(matches!(
@@ -2504,6 +2571,17 @@ mod tests {
             merge_policy_from_str("lowest-latency"),
             MergePolicy::LowestLatency
         ));
+        assert!(matches!(
+            merge_policy_from_str("balanced"),
+            MergePolicy::Balanced
+        ));
+        assert!(matches!(
+            merge_policy_from_str("priority"),
+            MergePolicy::Priority
+        ));
+        assert!(is_known_merge_policy("balanced"));
+        assert!(is_known_merge_policy("priority"));
+        assert!(!is_known_merge_policy("fastest"));
     }
 
     #[test]
```

