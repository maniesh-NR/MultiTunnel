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

- **Stage**: S18 (v0.12.0) — implementation + local gates COMPLETE,
  release running. The six asks shipped (D28): 3-line tray tooltip
  (name+version / totals / speeds, CRLF), transparent-background
  contrast-boosted LARGER runtime icons (tray ICOs + logo_64 + logo_24),
  Settings accent COLOR WHEEL (HSV disc asset + inverse marker math,
  live re-theme, persist on release), System/Light/Dark mode (OS
  detection chain: MT_THEME_SYSTEM_DARK env seam → Win32
  AppsUseLightTheme → gsettings/gtk; 5 s repoll; legacy dark_mode kept
  as the forward-compat shadow), mt-bond BLEST tie-guard + ECF
  completion scheduling (papers survey: docs/research/), UI changelog
  survey (docs/research/). 447/0 tests; rig verified sf=1.0+1.5.
- **Prior stage**: S17 (v0.11.0, titlebar version + live speeds, logs
  floating jump buttons + autoscroll).
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
### Commit: d910ee5 - backup(s17-memory-node): 2026-11-20T17:04:40Z (Z User, 2026-11-20)
diff --git a/harness/memory/curated/edges.json b/harness/memory/curated/edges.json
index 2649327..ff10e85 100644
--- a/harness/memory/curated/edges.json
+++ b/harness/memory/curated/edges.json
@@ -150,6 +150,18 @@
       "rel": "implements",
       "evidence": "S14 protocol upgrade, user directive"
     },
+    {
+      "src": "task:s17-titlebar-logs",
+      "dst": "commit:ce6af07",
+      "rel": "released_in",
+      "evidence": "tag v0.11.0, release run 44766745650 attempt 2"
+    },
+    {
+      "src": "task:s17-titlebar-logs",
+      "dst": "component:mt-app",
+      "rel": "implements",
+      "evidence": "crates/mt-app/src/screens.rs logs()+titlebar(); app.rs messages+autoscroll_logs_now"
+    },
     {
       "src": "task:saturation-aware-split",
       "dst": "bug:reorder-overflow",
diff --git a/harness/memory/curated/nodes.json b/harness/memory/curated/nodes.json
index c06361f..63d3251 100644
--- a/harness/memory/curated/nodes.json
+++ b/harness/memory/curated/nodes.json
@@ -430,6 +430,24 @@
         }
       ]
     },
+    {
+      "id": "task:s17-titlebar-logs",
+      "type": "task",
+      "title": "S17 titlebar version+speeds, logs viewport navigation (D27, v0.11.0)",
+      "tags": [],
+      "sources": [
+        {
+          "file": "harness/memory/LEDGER.md#s17",
+          "anchor": ""
+        }
+      ],
+      "passages": [
+        {
+          "text": "S17 CLOSED (v0.11.0): titlebar shows the release version next to the app name (workspace version inheritance — [workspace.package] + version.workspace, one-line release bumps) and live global down/up rates on the right (self.throughput, the tray tooltip's same 1s sample). Logs screen: scaffold_fixed + dedicated viewport scrollable (id logs-lines), floating top/bottom jump buttons (26px right stand-off — RIG BUG: flush-corner placement sat in the scrollbar grab zone and the rail swallowed clicks), autoscroll toggle default ON (LogLine re-anchors via scrollable::snap_to RelativeOffset y=1.0 gated by autoscroll_logs_now()); mock gained a connect burst + deterministic 2s keepalive. Release ops: cold-cache cancel+rerun (68min leg -> 6min warm retry; rust-cache saves in post-steps even on cancellation) and CI runner-flake round 2 (carrier_death then socks_probe deadline, green on attempt 3).",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "task:saturation-aware-split",
       "type": "task",
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index 142da4f..d0e8659 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "25f93ff 2026-11-20T15:47:04Z",
+  "as_of": "66147c6 2026-11-20T17:04:04Z",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 254,
-  "edge_count": 262,
+  "node_count": 256,
+  "edge_count": 265,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -815,6 +815,26 @@
         }
       ]
     },
+    {
+      "id": "commit:66147c6",
+      "type": "commit",
+      "title": "backup(s17-close-v0.11.0): 2026-11-20T17:04:00Z",
+      "tags": [
+        "2026-11-20"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "66147c6"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s17-close-v0.11.0): 2026-11-20T17:04:00Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:6a4bd7c",
       "type": "commit",
@@ -7263,6 +7283,24 @@
         }
       ]
     },
+    {
+      "id": "task:s17-titlebar-logs",
+      "type": "task",
+      "title": "S17 titlebar version+speeds, logs viewport navigation (D27, v0.11.0)",
+      "tags": [],
+      "sources": [
+        {
+          "file": "harness/memory/LEDGER.md#s17",
+          "anchor": ""
+        }
+      ],
+      "passages": [
+        {
+          "text": "S17 CLOSED (v0.11.0): titlebar shows the release version next to the app name (workspace version inheritance — [workspace.package] + version.workspace, one-line release bumps) and live global down/up rates on the right (self.throughput, the tray tooltip's same 1s sample). Logs screen: scaffold_fixed + dedicated viewport scrollable (id logs-lines), floating top/bottom jump buttons (26px right stand-off — RIG BUG: flush-corner placement sat in the scrollbar grab zone and the rail swallowed clicks), autoscroll toggle default ON (LogLine re-anchors via scrollable::snap_to RelativeOffset y=1.0 gated by autoscroll_logs_now()); mock gained a connect burst + deterministic 2s keepalive. Release ops: cold-cache cancel+rerun (68min leg -> 6min warm retry; rust-cache saves in post-steps even on cancellation) and CI runner-flake round 2 (carrier_death then socks_probe deadline, green on attempt 3).",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "task:saturation-aware-split",
       "type": "task",
@@ -7528,6 +7566,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s15)"
     },
+    {
+      "src": "commit:66147c6",
+      "dst": "ledger:s17",
+      "rel": "part_of",
+      "evidence": "subject stage label (s17)"
+    },
     {
       "src": "commit:6a4bd7c",
       "dst": "ledger:s9.1",
@@ -8830,6 +8874,18 @@
       "rel": "implements",
       "evidence": "S14 protocol upgrade, user directive"
     },
+    {
+      "src": "task:s17-titlebar-logs",
+      "dst": "commit:ce6af07",
+      "rel": "released_in",
+      "evidence": "tag v0.11.0, release run 44766745650 attempt 2"
+    },
+    {
+      "src": "task:s17-titlebar-logs",
+      "dst": "component:mt-app",
+      "rel": "implements",
+      "evidence": "crates/mt-app/src/screens.rs logs()+titlebar(); app.rs messages+autoscroll_logs_now"
+    },
     {
       "src": "task:saturation-aware-split",
       "dst": "bug:reorder-overflow",

### Commit: 66147c6 - backup(s17-close-v0.11.0): 2026-11-20T17:04:00Z (Z User, 2026-11-20)
diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
index 5f99ca9..6fa2751 100755
--- a/harness/memory/LEDGER.md
+++ b/harness/memory/LEDGER.md
@@ -204,3 +204,8 @@
 2026-11-20T11:55Z | s16 | IMPLEMENTED (D26): (1) mt-service SettingsView.always_on_top (#[serde(default)], D8 migration, 2 tests) + real_service/mock/test sites; (2) tray.rs: TrayCommand::Icon(TrayIcon{Color,Mono}) + TrayIcon::for_state (Connecting/Failed→Mono per the user's wording) + tooltip_live(up_bps,down_bps,tot_up,tot_down) "MultiTunnel — ↓X ↑Y • Σ↓Z ↑W" (≤70 UTF-16 units worst case, < 128 NIF_TIP cap) + ICON_MONO_BYTES embedded; (3) tray_win.rs: both HICONs loaded at spawn (colored + mono, CreateIconFromResourceEx 32px), UiState.icon_style + icon_for() (mono-missing → colored fallback), Icon cmd swaps via NIM_MODIFY(NIF_ICON), TaskbarCreated re-adds the CURRENT style, teardown destroys both, spawn seeds Mono (app boots not-connected); (4) app.rs: session_bytes (up,down) accumulate link-counter deltas inside sample_adapter_rates (counter RESET counts full new value — totals never dip; wiped on leaving Connected), tooltip = tooltip_live(self.throughput, session_bytes) while connected / word form otherwise, icon dedup via tray_icon field; (5) titlebar window menu: mouse_area .on_right_press(TitleContext) on the drag strip + iced::event::listen_with(CursorMoved) cursor tracking + window_size from Resized; anchor = cursor clamped to window minus menu_bounds (232x196) dropping below the 40px strip, fallback top-right; overlay = iced::widget::stack layer (base + full-window dismiss mouse_area on_RELEASE — child buttons capture first so a row pick fires its action exactly once and outside-clicks dismiss); items Always on top (✓ checkable)/Minimize/Maximize|Restore/Close (reuse existing Title* paths); always-on-top applied via iced::window::change_level(AlwaysOnTop/Normal) — CROSS-PLATFORM (winit: HWND_TOPMOST / _NET_WM_STATE_ABOVE), one mutation point set_always_on_top (save+apply), honored at startup from Opened AND SettingsLoaded (either order); Settings → Window & tray second toggler; (6) theme.rs menu_card (surface_container_high + the repo's first elevation shadow) / menu_item (transparent→secondary_container hover) / menu_rule (outline-variant hairline); (7) rig: rclick verb (XTEST button-3).
 2026-11-20T12:10Z | s16 | RIG VERIFIED (sf=1.0 AND 1.5): transparent emblem renders (117-px cluster at strip center, pastel palette intact — was invisible before the area-resize fix); rclick on the strip opens the window menu AT THE CURSOR (22,494 card-region px differ at sf=1.0, 49,215 at sf=1.5 = exact 1.5x geometry, 0 stray px elsewhere); clicking "Always on top" closes the menu AND the ✓ check glyph renders on re-open (84 px in the check area differ — setting persisted through the mock service); click-outside dismisses (0 px remain); D24 scroll invariant HOLDS under the new stack wrapper (connection frame byte-identical y100-290, page below moves 134k px at sf=1.5); ✕ still exits. GATES: fmt clean; clippy host + windows-gnu (zig shims, ziglang 0.14.1 reinstalled after the recycle) --all-targets -D warnings clean; workspace tests 429/0 (+8: app session_totals/title_menu_anchor_clamps/always_on_top roundtrip, tray tooltip_live/tray_icon_for_state/mono-ICO parse, mt-service 2 migration); vendored iced 6/0; mt-app --smoke exit 0; zigbuild release PE32+ ×4 (multitunnel 18.7MB, multitunnel-tray GUI 10.0MB, mt-app 14.5MB, mt-merge-server 2.4MB) + linux ELF ×2; version 0.10.0.
 2026-11-20T12:45Z | s16 | v0.10.0 RELEASED: first attempt (run 44719389267 attempt 1) hit TWO GitHub-runner flakes in the mt-bond integration leg (windows: carrier_death_mid_stream second-half write "no live bond carrier"; linux: hang inside cargo test -p mt-bond for 38 min with the sweeper's STALL_TIMEOUT=10s making a real product hang implausible) while the SAME commit passed CI on BOTH OSes (44717917257) minutes earlier and 25x solo + 12x full-suite stress runs under 8-way CPU load locally = 408 executions 0 failures — verdict runner flake (precedent v0.5.1), cancelled + re-run. Attempt 2: BOTH jobs green (linux-daemon success — the earlier hang did not reproduce, confirming the verdict). Assets verified by download+unzip (download/v0100-verify/): multitunnel-v0.10.0-windows-x64.zip 6,311,338 B (10 entries, integrity OK; multitunnel.exe PE32+ console + multitunnel-tray.exe PE32+ GUI; README carries Attribution + banner + logo_transparent references, QUICKSTART the v0.10 note) + mt-merge-server {x86_64 1,824,296 B, aarch64 1,860,432 B} ELFs. KNOWN MINOR (not release-blocking): the ZIP's README references assets/banner.png relatively — renders on GitHub (repo path), not inside the extracted zip (no assets/ in the archive, matching S15's manifest). S16 CLOSED — all seven asks shipped: About+tags, attribution, banner+logo README, transparent titlebar emblem, monochrome not-connected tray icon, hover speeds+totals, titlebar right-click menu with always-on-top. User actions: none.
+2026-11-20T15:40Z | s17 | S17 OPENED per user asks (D27): version number next to the app name in the titlebar; current global download/upload speed on the right side of the titlebar; Logs tab floating scroll-to-top/bottom buttons + autoscroll-to-bottom toggle. Side question answered from server/setup-*.sh: GCP VM merge-daemon setup = clone/curl the two scripts, then sudo DOMAIN=<gateway> bash setup-xray-reality.sh + sudo bash setup-merge-daemon.sh (idempotent; daemon auto-downloads the arch-correct binary from GitHub releases).
+2026-11-20T15:45Z | s17 | IMPLEMENTED (D27): (1) [workspace.package] version 0.11.0, root + mt-app inherit (version.workspace) — the titlebar shows v0.11.0 via app_version()=env!(CARGO_PKG_VERSION) and release bumps stay one line (mt-app.exe PE metadata now carries the release version); (2) titlebar right: live global rates from self.throughput (the tray tooltip's same 1s sample; ↓ tertiary / ↑ primary, zeros pre-sample) as plain text inside the drag strip — no click capture; (3) logs screen: scaffold_fixed + dedicated viewport scrollable (id "logs-lines" via screens::logs_scroll_id()), floating ↑/↓ jump buttons in a surface_container_high card bottom-right, toolbar autoscroll toggle (Tonal on / Text off, default ON); update: LogLine→snap_to(RelativeOffset y=1.0) gated by pure autoscroll_logs_now() (toggle ON && Screen::Logs), entering Logs with autoscroll ON re-anchors, re-enabling the toggle re-anchors; LogsScrollTop/Bottom = snap_to y=0.0/1.0; (4) mock connect burst (engine/config/adapters/routing/listeners/bond/reality/merge/tunnel-up + per-link lines) + deterministic every-2s keepalive (tick.is_multiple_of(2)); (5) zig shims regenerated to `exec zig` from PATH (python -m ziglang fallback died with the pypkgs wheel in the recycle; tools/zig now a 0.17.1 tarball install, CI-parity). SANDBOX RECOVERY this session: 11th recycle — tree was s13-stale (fetch+rebase+autostash dance, stash conflict resolved to origin's side); toolchain partially survived (rustup 1.100.0 + cargo-zigbuild intact; zig re-downloaded; registry/src wiped per D2); /tmp/xkblibs re-extracted (libxkbcommon{,-x11} + libxcb-xkb debs); DISK: rootfs 9.9G hit 100% mid-gates — the stale /tmp/my-project/tools (4.3G) is LOWER-LAYER (overlay whiteouts free nothing; du shrinks, df doesn't) — the real lever is /tmp/mt-target/debug (upper layer, 5.4G; delete+rebuild ~2min) + x86_64-pc-windows-gnu between phases (the s14 dance, now with the why: overlayfs lower-layer deletes are whiteouts, not frees).
+2026-11-20T15:50Z | s17 | RIG VERIFIED + REAL BUG FOUND&FIXED: the floating card flush to the viewport corner sat inside the scrollable's SCROLLBAR GRAB ZONE — the rail swallowed the button clicks (repro: clicks dead-center on the ↑ pill at sf=1.0 did nothing while rail buttons/togglers worked; VLM ruler-zoom saw the scrollbar strip THROUGH the capsule). FIX: overlay padding right 26px — capsule clears the rail+grab, the rail renders separately again. RIG LESSONS: (a) coordinate guessing burned an hour — the connect toggler's pill is at y252-288 (track-arc scan: runs of 6<d<90 between text rows finds the pill; VLM y-estimates drift ~25px, ruler-zoom crops make it usable; XTEST cursor renders BLACK in ffmpeg x11grab shots — a 31-px diff cluster at the click point is the CURSOR, not a widget change); (b) full-window diffs vs boot: connect=18755px (state+card+graph+speeds), autoscroll toggle=4409, jumps=3686/6307, wheel=2845 — pixel thresholds now known for this window size; (c) VLM reads content reliably (log lines verbatim, speeds "↓ 10.9 MB/s ↑ 2.9 MB/s" live-ticking, "connecting: spinning up links" as the true first line after ↑) but must be prompted "plain text only, no HTML" or it hallucinates mockups. sf=1.0 AND sf=1.5 both verified end-to-end (version cluster x131-169 @1.0, speeds live, toggler flips, autoscroll on→off, top/bottom jumps, wheel through overlay, keepalive tail following).
+2026-11-20T16:15Z | s17 | RUNNER FLAKE ROUND 2 (verdict: fleet, not code — same class as the s16 v0.10.0 event): ci run 44766745503 attempt 1 failed mt-bond carrier_death_mid_stream (the s16-documented flake, byte-identical signature "bond send failed: no live bond carrier"); rerun attempt 2 failed a DIFFERENT test — multitunnel core_proc socks_probe_total_deadline_bounds_byte_dripping_proxy ("probe ran 211.66ms — failed before the budget", a timing-budget assertion); attempt 3 ALL GREEN. Corroborating green on the SAME commit: windows-latest ci, lint, cross-check, the sibling ci run 44766699760 (ubuntu, full suite), the release run's own build-linux-daemon leg, and local cargo test --workspace 433/0 x2. Neither failing test's crate was touched by S17 (mt-app GUI + Cargo.toml version only). PATTERN CONFIRMED: ubuntu runner fleet intermittently stalls IO/timing-sensitive tests (carrier liveness + deadline budgets); if a THIRD distinct test flakes, consider serializing the timing-sensitive set (MT_SERIAL_TESTS=1 style) instead of rerun-chasing.
+2026-11-20T17:05Z | s17 | v0.11.0 RELEASED: attempt 1's build-windows leg sat 68 min inside cargo test on a 100%-cold rust-cache (the version bump changed Cargo.lock; every other leg incl. build-linux-daemon green) — cancelled per the >45-min protocol (rust-cache@v2 saves in post-steps even on cancellation), attempt 2 completed SUCCESS in ~6 min WARM exactly as predicted (the s9 pattern, now proven twice). Assets verified by download+unzip (download/v0110-verify/): multitunnel-v0.11.0-windows-x64.zip 6,314,832 B (10 entries, integrity OK; multitunnel.exe PE32+ console 13.26 MB + multitunnel-tray.exe PE32+ GUI 2.95 MB; README carries the v0.9-v0.11 section, QUICKSTART the v0.11 note, server/ scripts included) + mt-merge-server {x86_64 1,824,296 B, aarch64 1,860,432 B} ELFs. CI on the same commit: green on attempt 3 after the two-test runner-flake round (see 16:15Z entry). S17 CLOSED — the three asks shipped: version next to the app name in the titlebar, live global down/up speeds on the titlebar's right, Logs tab floating top/bottom jump buttons + autoscroll toggle (default on). User actions: none.
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index 0347f1d..142da4f 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "6c7eedd 2026-11-20T11:21:55Z",
+  "as_of": "25f93ff 2026-11-20T15:47:04Z",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 249,
-  "edge_count": 257,
+  "node_count": 254,
+  "edge_count": 262,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -375,6 +375,26 @@
         }
       ]
     },
+    {
+      "id": "commit:25f93ff",
+      "type": "commit",
+      "title": "backup(s17-tag-v0.11.0): 2026-11-20T15:47:04Z [no changes]",
+      "tags": [
+        "2026-11-20"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "25f93ff"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s17-tag-v0.11.0): 2026-11-20T15:47:04Z [no changes]",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:34b56ba",
       "type": "commit",
@@ -1535,6 +1555,26 @@
         }
       ]
     },
+    {
+      "id": "commit:ce6af07",
+      "type": "commit",
+      "title": "backup(s17-pre-release): 2026-11-20T15:46:40Z [no changes]",
+      "tags": [
+        "2026-11-20"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "ce6af07"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s17-pre-release): 2026-11-20T15:46:40Z [no changes]",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:d241e42",
       "type": "commit",
@@ -1835,6 +1875,26 @@
         }
       ]
     },
+    {
+      "id": "commit:f79a3c9",
+      "type": "commit",
+      "title": "s17: titlebar version + live global speeds, logs viewport with floating jump buttons + autoscroll — (1) workspace version inheritance ([workspace.package] 0.11.0; root + mt-app inherit) so the titlebar 'v0.11.0' rides next to the app name and moves in lock-step with the release tag (one-line bumps; mt-app.exe ships with the release version in its PE metadata now); (2) titlebar right side: live global down/up rates (the same 1 s throughput sample the tray tooltip reports; down in tertiary, up in primary, zeros before the first sample) as plain text inside the drag strip; (3) logs screen restructure (D27): fixed toolbar + dedicated viewport scrollable (the page no longer scrolls as a whole), stable scrollable id shared with update; floating jump-to-top/bottom buttons over the viewport's bottom-right corner in a surface_container_high card, autoscroll toggle in the toolbar (Tonal while following the tail, Text while off, default ON); (4) update paths: LogLine re-anchors to bottom while autoscroll ON + Logs on screen (gated by pure autoscroll_logs_now()), entering Logs with autoscroll ON lands on the tail, re-enabling the toggle re-anchors immediately, LogsScrollTop/Bottom via scrollable::snap_to RelativeOffset 0.0/1.0; (5) RIG FINDING fixed: the floating card anchored flush to the corner sat inside the scrollable's scrollbar grab zone — the rail swallowed the button clicks (reproduced at sf=1.0: clicks dead-center on the pill did nothing); overlay now stands off 26px from the right edge, clear of the rail+grab, and the clicks land (VLM-verified jump to the true first line at ",
+      "tags": [
+        "2026-11-20"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "f79a3c9"
+        }
+      ],
+      "passages": [
+        {
+          "text": "s17: titlebar version + live global speeds, logs viewport with floating jump buttons + autoscroll — (1) workspace version inheritance ([workspace.package] 0.11.0; root + mt-app inherit) so the titlebar 'v0.11.0' rides next to the app name and moves in lock-step with the release tag (one-line bumps; mt-app.exe ships with the release version in its PE metadata now); (2) titlebar right side: live global down/up rates (the same 1 s throughput sample the tray tooltip reports; down in tertiary, up in primary, zeros before the first sample) as plain text inside the drag strip; (3) logs screen restructure (D27): fixed toolbar + dedicated viewport scrollable (the page no longer scrolls as a whole), stable scrollable id shared with update; floating jump-to-top/bottom buttons over the viewport's bottom-right corner in a surface_container_high card, autoscroll toggle in the toolbar (Tonal while following the tail, Text while off, default ON); (4) update paths: LogLine re-anchors to bottom while autoscroll ON + Logs on screen (gated by pure autoscroll_logs_now()), entering Logs with autoscroll ON lands on the tail, re-enabling the toggle re-anchors immediately, LogsScrollTop/Bottom via scrollable::snap_to RelativeOffset 0.0/1.0; (5) RIG FINDING fixed: the floating card anchored flush to the corner sat inside the scrollable's scrollbar grab zone — the rail swallowed the button clicks (reproduced at sf=1.0: clicks dead-center on the pill did nothing); overlay now stands off 26px from the right edge, clear of the rail+grab, and the clicks land (VLM-verified jump to the true first line at ",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:f7db603",
       "type": "commit",
@@ -4180,6 +4240,43 @@
         }
       ]
     },
+    {
+      "id": "ledger:s17",
+      "type": "ledger",
+      "title": "Stage s17 ledger",
+      "tags": [
+        "s17",
+        "append-only"
+      ],
+      "sources": [
+        {
+          "file": "harness/memory/LEDGER.md",
+          "anchor": "s17"
+        }
+      ],
+      "passages": [
+        {
+          "text": "2026-11-20T15:40Z | S17 OPENED per user asks (D27): version number next to the app name in the titlebar; current global download/upload speed on the right side of the titlebar; Logs tab floating scroll-to-top/bottom buttons + autoscroll-to-bottom toggle. Side question answered from server/setup-*.sh: GCP VM merge-daemon setup = clone/curl the two scripts, then sudo DOMAIN=<gateway> bash setup-xray-reality.sh + sudo bash setup-merge-daemon.sh (idempotent; daemon auto-downloads the arch-correct binary from GitHub releases).",
+          "kind": "body"
+        },
+        {
+          "text": "2026-11-20T15:45Z | IMPLEMENTED (D27): (1) [workspace.package] version 0.11.0, root + mt-app inherit (version.workspace) — the titlebar shows v0.11.0 via app_version()=env!(CARGO_PKG_VERSION) and release bumps stay one line (mt-app.exe PE metadata now carries the release version); (2) titlebar right: live global rates from self.throughput (the tray tooltip's same 1s sample; ↓ tertiary / ↑ primary, zeros pre-sample) as plain text inside the drag strip — no click capture; (3) logs screen: scaffold_fixed + dedicated viewport scrollable (id \"logs-lines\" via screens::logs_scroll_id()), floating ↑/↓ jump buttons in a surface_container_high card bottom-right, toolbar autoscroll toggle (Tonal on / Text off, default ON); update: LogLine→snap_to(RelativeOffset y=1.0) gated by pure autoscroll_logs_now() (toggle ON && Screen::Logs), entering Logs with autoscroll ON re-anchors, re-enabling the toggle re-anchors; LogsScrollTop/Bottom = snap_to y=0.0/1.0; (4) mock connect burst (engine/config/adapters/routing/listeners/bond/reality/merge/tunnel-up + per-link lines) + deterministic every-2s keepalive (tick.is_multiple_of(2)); (5) zig shims regenerated to `exec zig` from PATH (python -m ziglang fallback died with the pypkgs wheel in the recycle; tools/zig now a 0.17.1 tarball install, CI-parity).",
+          "kind": "body"
+        },
+        {
+          "text": "2026-11-20T15:50Z | RIG VERIFIED + REAL BUG FOUND&FIXED: the floating card flush to the viewport corner sat inside the scrollable's SCROLLBAR GRAB ZONE — the rail swallowed the button clicks (repro: clicks dead-center on the ↑ pill at sf=1.0 did nothing while rail buttons/togglers worked; VLM ruler-zoom saw the scrollbar strip THROUGH the capsule). FIX: overlay padding right 26px — capsule clears the rail+grab, the rail renders separately again. RIG LESSONS: (a) coordinate guessing burned an hour — the connect toggler's pill is at y252-288 (track-arc scan: runs of 6<d<90 between text rows finds the pill; VLM y-estimates drift ~25px, ruler-zoom crops make it usable; XTEST cursor renders BLACK in ffmpeg x11grab shots — a 31-px diff cluster at the click point is the CURSOR, not a widget change); (b) full-window diffs vs boot: connect=18755px (state+card+graph+speeds), autoscroll toggle=4409, jumps=3686/6307, wheel=2845 — pixel thresholds now known for this window size; (c) VLM reads content reliably (log lines verbatim, speeds \"↓ 10.9 MB/s ↑ 2.9 MB/s\" live-ticking, \"connecting: spinning up links\" as the true first line after ↑) but must be prompted \"plain text only, no HTML\" or it hallucinates mockups. sf=1.0 AND sf=1.5 both verified end-to-end (version cluster x131-169 @1.0, speeds live, toggler flips, autoscroll on→off, top/bottom jumps, wheel through overlay, keepalive tail following).",
+          "kind": "body"
+        },
+        {
+          "text": "2026-11-20T16:15Z | RUNNER FLAKE ROUND 2 (verdict: fleet, not code — same class as the s16 v0.10.0 event): ci run 44766745503 attempt 1 failed mt-bond carrier_death_mid_stream (the s16-documented flake, byte-identical signature \"bond send failed: no live bond carrier\"); rerun attempt 2 failed a DIFFERENT test — multitunnel core_proc socks_probe_total_deadline_bounds_byte_dripping_proxy (\"probe ran 211.66ms — failed before the budget\", a timing-budget assertion); attempt 3 ALL GREEN. Corroborating green on the SAME commit: windows-latest ci, lint, cross-check, the sibling ci run 44766699760 (ubuntu, full suite), the release run's own build-linux-daemon leg, and local cargo test --workspace 433/0 x2. Neither failing test's crate was touched by S17 (mt-app GUI + Cargo.toml version only). PATTERN CONFIRMED: ubuntu runner fleet intermittently stalls IO/timing-sensitive tests (carrier liveness + deadline budgets); if a THIRD distinct test flakes, consider serializing the timing-sensitive set (MT_SERIAL_TESTS=1 style) instead of rerun-chasing.",
+          "kind": "body"
+        },
+        {
+          "text": "2026-11-20T17:05Z | v0.11.0 RELEASED: attempt 1's build-windows leg sat 68 min inside cargo test on a 100%-cold rust-cache (the version bump changed Cargo.lock; every other leg incl. build-linux-daemon green) — cancelled per the >45-min protocol (rust-cache@v2 saves in post-steps even on cancellation), attempt 2 completed SUCCESS in ~6 min WARM exactly as predicted (the s9 pattern, now proven twice). Assets verified by download+unzip (download/v0110-verify/): multitunnel-v0.11.0-windows-x64.zip 6,314,832 B (10 entries, integrity OK; multitunnel.exe PE32+ console 13.26 MB + multitunnel-tray.exe PE32+ GUI 2.95 MB; README carries the v0.9-v0.11 section, QUICKSTART the v0.11 note, server/ scripts included) + mt-merge-server {x86_64 1,824,296 B, aarch64 1,860,432 B} ELFs. CI on the same commit: green on attempt 3 after the two-test runner-flake round (see 16:15Z entry). S17 CLOSED — the three asks shipped: version next to the app name in the titlebar, live global down/up speeds on the titlebar's right, Logs tab floating top/bottom jump buttons + autoscroll toggle (default on). User actions: none.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "ledger:s2",
       "type": "ledger",
@@ -4980,7 +5077,7 @@
       ],
       "passages": [
         {
-          "text": "- **Stage**: IDLE — all committed task chains closed and released through v0.10.0. S16 (repo polish + window menu + state-aware tray, D26) CLOSED: the seven asks shipped — GitHub About + 20 topic tags (the About description had been a stale leftover of an unrelated project), README attribution section + banner + transparent logo, transparent titlebar emblem (flood-fill key-out + AREA resample — bilinear punches holes at 20:1), monochrome not-connected tray icon (NIM_MODIFY swap, boots Mono), tray hover = live speeds + Σ totals, titlebar right-click window menu with always-on-top (cross-platform iced change_level, persisted, honored at startup); also fixed the latent S15 top-aligned identity cluster (align_y(Center)). v0.10.0 RELEASED and verified (re-run after two GitHub-runner flakes in the mt-bond leg — verdict + retry protocol in LEDGER s16; release re-confirmed live 2026-11-20 via API: 3 assets + About + 20 topics). - **Task E core (v0.8.0, S14, CLOSED)**: saturation-aware scheduling shipped — the \"one adapter maxes out, no spill\" complaint fixed by peer-reported per-carrier delivery rates (1 s PINGs), carrier_score relative load, Balanced capacity-proportional spreading, Priority spill at 1.15 + writer backlog. S14 also shipped the agentic memory graph + RAG (query_memory.py / write_memory.py, 18/18 tests).",
+          "text": "- **Stage**: IDLE — S17 CLOSED, v0.11.0 released and verified. The three asks shipped: release version next to the app name in the titlebar (workspace version inheritance — one-line release bumps; mt-app.exe PE metadata now carries the release version), live global ↓/↑ rates on the strip's right (the tray tooltip's same 1 s sample), Logs tab = fixed toolbar + dedicated viewport with floating top/bottom jump buttons (26px right stand-off — the rig caught the scrollbar grab-zone swallowing flush-corner clicks) + autoscroll toggle (default ON, follows the tail; re-enabling re-anchors). All 8 gates green; rig verified sf=1.0+1.5 (VLM content reads + pixel diffs); 433/0 tests. Release: attempt-1 windows leg cancelled at 68 min on a cold rust-cache (Cargo.lock changed), attempt-2 SUCCESS in 6 min warm (s9 pattern proven); CI green on attempt 3 after a two-test ubuntu runner-flake round (carrier_death + socks_probe deadline, different tests per attempt, all other legs/local green — LEDGER 16:15Z). - **Prior stages**: S16 (v0.10.0, About+topics+attribution+banner, transparent emblem, mono tray icon, hover speeds+totals, titlebar menu + always-on-top), S15 (v0.9.0, logo + blended titlebar + close-to-tray + persistent tray + Quick Settings), S13 (v0.7.1, status tab: one connection frame + one page scrollable, graph fills frame), S12 (v0.7.0, balanced+priority scheduling + adapter ordering), S11 (v0.6.2, stream bonding finalRules + verified carriers), S10/10.1 (v0.6.0/01, rendering fixes + per-adapter graphs).",
           "kind": "body"
         }
       ]
@@ -5113,6 +5210,26 @@
         }
       ]
     },
+    {
+      "id": "release:v0.11.0",
+      "type": "release",
+      "title": "Release v0.11.0 (tagged ce6af07)",
+      "tags": [
+        "v0.11.0"
+      ],
+      "sources": [
+        {
+          "file": "git:tags/v0.11.0",
+          "anchor": "ce6af07"
+        }
+      ],
+      "passages": [
+        {
+          "text": "Release v0.11.0: tag points at commit ce6af07.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "release:v0.2.0",
       "type": "release",
@@ -7297,6 +7414,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s8)"
     },
+    {
+      "src": "commit:25f93ff",
+      "dst": "ledger:s17",
+      "rel": "part_of",
+      "evidence": "subject stage label (s17)"
+    },
     {
       "src": "commit:3507ff3",
       "dst": "ledger:s11",
@@ -7579,6 +7702,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s13)"
     },
+    {
+      "src": "commit:ce6af07",
+      "dst": "ledger:s17",
+      "rel": "part_of",
+      "evidence": "subject stage label (s17)"
+    },
     {
       "src": "commit:d241e42",
       "dst": "ledger:s12",
@@ -8203,6 +8332,18 @@
       "rel": "released_in",
       "evidence": "2026-11-20T12:45Z | v0.10.0 RELEASED: first attempt (run 44719389267 attempt 1) hit TWO GitHub-runner flakes in the mt-bond integration leg (windows: carrier_death_mid_stream second-half write \"no live bond carrier\"; linux: hang inside cargo test -p mt-bond for 38 min with the sweeper's STALL_TIMEOUT=10s making a real product hang implausible) while the SAME commit passed CI on BOTH OSes (44717917257) minutes earlier and 25x solo + 12x full-suite stress runs under 8-way CPU load locally = 408 executions 0 failures — verdict runner flake (precedent v0.5.1), cancelled + re-run. Attempt 2: BOTH jobs green (linux-daemon success — the earlier hang did not reproduce, confirming the verdict). Assets verified by download+unzip (download/v0100-verify/): multitunnel-v0.10.0-windows-x64.zip 6,311,338 B (10 entries, integrity OK; multitunnel.exe PE32+ console + multitunnel-tray.exe PE32+ GUI; README carries Attribution + banner + logo_transparent references, QUICKSTART the v0.10 note) + mt-merge-server {x86_64 1,824,296 B, aarch64 1,860,432 B} ELFs. KNOWN MINOR (not release-blocking): the ZIP's README references assets/banner.png relatively — renders on GitHub (repo path), not inside the extracted zip (no assets/ in the archive, matching S15's manifest). S16 CLOSED — all seven asks shipped: About+tags, attribution, banner+logo README, transparent titlebar emblem, monochrome not-connected tray icon, hover speeds+totals, titlebar right-click menu with always-on-top. User actions: none."
     },
+    {
+      "src": "ledger:s17",
+      "dst": "project:multitunnel",
+      "rel": "part_of",
+      "evidence": "project ledger"
+    },
+    {
+      "src": "ledger:s17",
+      "dst": "release:v0.11.0",
+      "rel": "released_in",
+      "evidence": "2026-11-20T17:05Z | v0.11.0 RELEASED: attempt 1's build-windows leg sat 68 min inside cargo test on a 100%-cold rust-cache (the version bump changed Cargo.lock; every other leg incl. build-linux-daemon green) — cancelled per the >45-min protocol (rust-cache@v2 saves in post-steps even on cancellation), attempt 2 completed SUCCESS in ~6 min WARM exactly as predicted (the s9 pattern, now proven twice). Assets verified by download+unzip (download/v0110-verify/): multitunnel-v0.11.0-windows-x64.zip 6,314,832 B (10 entries, integrity OK; multitunnel.exe PE32+ console 13.26 MB + multitunnel-tray.exe PE32+ GUI 2.95 MB; README carries the v0.9-v0.11 section, QUICKSTART the v0.11 note, server/ scripts included) + mt-merge-server {x86_64 1,824,296 B, aarch64 1,860,432 B} ELFs. CI on the same commit: green on attempt 3 after the two-test runner-flake round (see 16:15Z entry). S17 CLOSED — the three asks shipped: version next to the app name in the titlebar, live global down/up speeds on the titlebar's right, Logs tab floating top/bottom jump buttons + autoscroll toggle (default on). User actions: none."
+    },
     {
       "src": "ledger:s2",
       "dst": "project:multitunnel",
@@ -8413,6 +8554,12 @@
       "rel": "built_from",
       "evidence": "git tag v0.10.0"
     },
+    {
+      "src": "release:v0.11.0",
+      "dst": "commit:ce6af07",
+      "rel": "built_from",
+      "evidence": "git tag v0.11.0"
+    },
     {
       "src": "release:v0.2.0",
       "dst": "commit:06881a6",

### Commit: f79a3c9 - s17: titlebar version + live global speeds, logs viewport with floating jump buttons + autoscroll — (1) workspace version inheritance ([workspace.package] 0.11.0; root + mt-app inherit) so the titlebar 'v0.11.0' rides next to the app name and moves in lock-step with the release tag (one-line bumps; mt-app.exe ships with the release version in its PE metadata now); (2) titlebar right side: live global down/up rates (the same 1 s throughput sample the tray tooltip reports; down in tertiary, up in primary, zeros before the first sample) as plain text inside the drag strip; (3) logs screen restructure (D27): fixed toolbar + dedicated viewport scrollable (the page no longer scrolls as a whole), stable scrollable id shared with update; floating jump-to-top/bottom buttons over the viewport's bottom-right corner in a surface_container_high card, autoscroll toggle in the toolbar (Tonal while following the tail, Text while off, default ON); (4) update paths: LogLine re-anchors to bottom while autoscroll ON + Logs on screen (gated by pure autoscroll_logs_now()), entering Logs with autoscroll ON lands on the tail, re-enabling the toggle re-anchors immediately, LogsScrollTop/Bottom via scrollable::snap_to RelativeOffset 0.0/1.0; (5) RIG FINDING fixed: the floating card anchored flush to the corner sat inside the scrollable's scrollbar grab zone — the rail swallowed the button clicks (reproduced at sf=1.0: clicks dead-center on the pill did nothing); overlay now stands off 26px from the right edge, clear of the rail+grab, and the clicks land (VLM-verified jump to the true first line at both scale factors); (6) mock: realistic connect burst (engine/config/adapters/routing/listeners/bond/reality/merge/tunnel-up + per-link 'link n/N up: name (latency X ms)') and a deterministic every-2s keepalive — live logs for the demo and a stable rig signal; zig shims regenerated (exec zig from PATH — the python -m ziglang fallback broke when the wheel vanished in a recycle). TESTS: 433/0 (+4: autoscroll default+toggle, autoscroll screen gating, version triplet, scroll id stability); gates fmt/clippy host+windows-gnu/zigbuild PE32+ x4 + ELF x2 green; rig sf=1.0+1.5: version cluster x131-169 at 1.0, live speeds verified ('10.9 MB/s'/'2.8 MB/s' readings), toggler connect (knob right, purple track), autoscroll on->off toggle, jump top (true first line 'connecting: spinning up links') and bottom, wheel scrolls through the overlay, auto-scroll follows keepalive tail; README v0.9-v0.11 section + QUICKSTART v0.11 note (Z User, 2026-11-20)
diff --git a/Cargo.lock b/Cargo.lock
index 96b0732..210bd62 100755
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1962,7 +1962,7 @@ dependencies = [
 
 [[package]]
 name = "mt-app"
-version = "0.1.0"
+version = "0.11.0"
 dependencies = [
  "async-trait",
  "iced",
@@ -2073,7 +2073,7 @@ dependencies = [
 
 [[package]]
 name = "multitunnel"
-version = "0.10.0"
+version = "0.11.0"
 dependencies = [
  "anyhow",
  "async-trait",
diff --git a/Cargo.toml b/Cargo.toml
index 98eb5cc..3a35da5 100755
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -1,6 +1,9 @@
 [package]
 name = "multitunnel"
-version = "0.10.0"
+# Single source of truth for the release version (S17): crates that ship
+# (this one + mt-app's titlebar display) inherit the workspace version, so a
+# release bumps exactly one line.
+version.workspace = true
 edition = "2021"
 license = "MIT"
 description = "Windows supervisor that combines an Xray VLESS+REALITY tunnel with net-merge multi-link bonding"
@@ -14,6 +17,9 @@ publish = false
 members = ["crates/*"]
 resolver = "2"
 
+[workspace.package]
+version = "0.11.0"
+
 [lib]
 name = "multitunnel"
 path = "src/lib.rs"
diff --git a/QUICKSTART.md b/QUICKSTART.md
index ee30b06..0946384 100755
--- a/QUICKSTART.md
+++ b/QUICKSTART.md
@@ -78,6 +78,11 @@ needed for the usual per-user system-proxy writes (expect the SmartScreen
 > download/upload speeds plus the session totals. Right-clicking the
 > title bar opens a window menu with "Always on top" (also under
 > Settings → Window & tray), Minimize, Maximize/Restore and Close.
+>
+> v0.11: the title bar carries the release version next to the app name
+> and the live global download/upload speeds on its right side, and the
+> Logs screen gained floating jump-to-top/bottom buttons plus an
+> Auto-scroll toggle (on by default — it keeps the newest line in view).
 
 **SMOKE TEST — this is the first time the GUI binary runs on real Windows.**
 The development sandbox is Linux; the window has been cross-compiled and
diff --git a/README.md b/README.md
index e5cc101..ce69866 100755
--- a/README.md
+++ b/README.md
@@ -306,7 +306,7 @@ bonding mode to `stream` (Advanced): the bytes of every connection are
 split across all ticked adapters and re-merged by `mt-merge-server` on
 the tunnel server — see "Stream bonding (v0.5)" above.
 
-## App identity & tray (v0.9/v0.10) — logo, blended title bar, close-to-tray
+## App identity & tray (v0.9–v0.11) — logo, blended title bar, close-to-tray
 
 The window itself became a first-class part of the app (all of it in the
 GUI, no new dependencies):
@@ -346,6 +346,18 @@ GUI, no new dependencies):
   current download/upload speed AND the session's total uploaded /
   downloaded volume (e.g. `MultiTunnel — ↓9.4 MB/s ↑1.2 MB/s •
   Σ↓3.0 GB ↑42.9 MB`).
+- **Version + live speeds in the title bar (v0.11)**: the release
+  version rides right next to the app name (it moves in lock-step with
+  the release tag — one workspace version line), and the strip's right
+  side shows the live global download/upload rates (the same 1 s
+  throughput sample the tray tooltip reports), so the current speeds
+  are readable without opening the app's Status tab.
+- **Logs screen navigation (v0.11)**: the Logs tab scrolls only the log
+  lines (toolbar stays put), with floating jump-to-top/jump-to-bottom
+  buttons over the viewport's corner and an **Auto-scroll** toggle in
+  the toolbar — ON by default, it keeps the newest line in view as
+  events land; turn it off to read back through history without the
+  view jumping to the tail.
 - **Quick Settings on right-click**: the tray menu is built fresh from
   live state — Connect/Disconnect (state-reflecting), the server list
   (radio-checked on the active profile), Open MultiTunnel and Exit.
diff --git a/cargo/zig-ar b/cargo/zig-ar
index bfd30b3..e0cdc23 100755
--- a/cargo/zig-ar
+++ b/cargo/zig-ar
@@ -1,2 +1,2 @@
 #!/bin/sh
-exec python3 -m ziglang ar "$@"
+exec zig ar "$@"
diff --git a/cargo/zig-cc b/cargo/zig-cc
index f3a2878..a429f7d 100755
--- a/cargo/zig-cc
+++ b/cargo/zig-cc
@@ -10,4 +10,4 @@ for a in "$@"; do
   esac
 done
 shift "$n"
-exec python3 -m ziglang cc "$@"
+exec zig cc "$@"
diff --git a/cargo/zig-cxx b/cargo/zig-cxx
index 8d88253..c8d0078 100755
--- a/cargo/zig-cxx
+++ b/cargo/zig-cxx
@@ -10,4 +10,4 @@ for a in "$@"; do
   esac
 done
 shift "$n"
-exec python3 -m ziglang cxx "$@"
+exec zig cxx "$@"
diff --git a/crates/mt-app/Cargo.toml b/crates/mt-app/Cargo.toml
index 8bc2562..5ba51be 100755
--- a/crates/mt-app/Cargo.toml
+++ b/crates/mt-app/Cargo.toml
@@ -1,6 +1,8 @@
 [package]
 name = "mt-app"
-version = "0.1.0"
+# S17: inherit the release version — the titlebar shows it next to the name
+# (see `app_version()`), so it must move in lock-step with the release tag.
+version.workspace = true
 edition = "2021"
 license = "MIT"
 publish = false
diff --git a/crates/mt-app/src/app.rs b/crates/mt-app/src/app.rs
index e42d416..0551aed 100755
--- a/crates/mt-app/src/app.rs
+++ b/crates/mt-app/src/app.rs
@@ -12,6 +12,8 @@ use mt_service::{ExitInfo, MtService, ServiceEvent, ServiceState, SettingsView,
 use tokio::sync::broadcast;
 use tokio::sync::Mutex as AsyncMutex;
 
+use iced::widget::scrollable;
+
 use crate::theme::{MtTheme, SEED_SWATCHES};
 use crate::{screens, util, Element};
 
@@ -288,6 +290,13 @@ pub enum Message {
     LogFilterPicked(String),
     /// Clear the log buffer.
     LogsCleared,
+    // --- S17: logs screen navigation + autoscroll (D27) ---
+    /// Scroll the logs viewport to the very top (floating button).
+    LogsScrollTop,
+    /// Scroll the logs viewport to the very bottom (floating button).
+    LogsScrollBottom,
+    /// Toggle autoscroll-to-bottom on new log lines.
+    LogsAutoscrollToggled,
     /// Loaded the adapter list (Adapters screen).
     AdaptersLoaded(Vec<mt_service::AdapterView>),
     /// An adapter checkbox flipped (draft state, index into `adapters`).
@@ -474,6 +483,16 @@ pub struct App {
     /// S16: tray icon style last pushed (dedup; starts Mono — the app
     /// boots not-connected).
     pub tray_icon: crate::tray::TrayIcon,
+    /// S17: autoscroll the logs viewport to the bottom as new lines land
+    /// (default ON — a log viewer should track the tail until the user
+    /// opts out; the toggle lives in the Logs toolbar).
+    pub logs_autoscroll: bool,
+}
+
+/// S17: the release version shown next to the app name in the titlebar.
+/// Inherited from the workspace package version (one-line release bumps).
+pub fn app_version() -> &'static str {
+    env!("CARGO_PKG_VERSION")
 }
 
 impl App {
@@ -541,6 +560,8 @@ impl App {
             title_menu: None,
             session_bytes: (0, 0),
             tray_icon: crate::tray::TrayIcon::Mono,
+            // S17
+            logs_autoscroll: true,
         }
     }
 
@@ -832,7 +853,18 @@ impl App {
                         LatencyState::Done(ms.map_or_else(|| Err(String::from("no reply")), Ok)),
                     );
                 }
-                ServiceEvent::LogLine(line) => util::push_log(&mut self.logs, line),
+                ServiceEvent::LogLine(line) => {
+                    util::push_log(&mut self.logs, line);
+                    // S17: keep the tail in view while autoscroll is on and
+                    // the Logs screen is showing (the task no-ops if the
+                    // scrollable with this id is not on screen).
+                    if self.autoscroll_logs_now() {
+                        return scrollable::snap_to(
+                            screens::logs_scroll_id(),
+                            scrollable::RelativeOffset { x: 0.5, y: 1.0 },
+                        );
+                    }
+                }
             },
             Message::Screen(s) => {
                 self.screen = s;
@@ -840,6 +872,12 @@ impl App {
                     Screen::Servers => self.load_profiles(),
                     Screen::Adapters => self.load_adapters(),
                     Screen::Routing => self.load_rules(),
+                    // S17: entering Logs with autoscroll on lands on the
+                    // tail — the natural place to start reading live logs.
+                    Screen::Logs if self.logs_autoscroll => scrollable::snap_to(
+                        screens::logs_scroll_id(),
+                        scrollable::RelativeOffset { x: 0.5, y: 1.0 },
+                    ),
                     _ => iced::Task::none(),
                 };
             }
@@ -1129,6 +1167,30 @@ impl App {
             },
             Message::LogFilterPicked(l) => self.log_filter = LogFilter::from_label(&l),
             Message::LogsCleared => self.logs.clear(),
+            // S17: floating navigation buttons + autoscroll toggle (D27).
+            Message::LogsScrollTop => {
+                return scrollable::snap_to(
+                    screens::logs_scroll_id(),
+                    scrollable::RelativeOffset { x: 0.5, y: 0.0 },
+                );
+            }
+            Message::LogsScrollBottom => {
+                return scrollable::snap_to(
+                    screens::logs_scroll_id(),
+                    scrollable::RelativeOffset { x: 0.5, y: 1.0 },
+                );
+            }
+            Message::LogsAutoscrollToggled => {
+                self.logs_autoscroll = !self.logs_autoscroll;
+                // Flipping it back ON should immediately re-anchor to the
+                // tail (the user asked to follow again).
+                if self.logs_autoscroll {
+                    return scrollable::snap_to(
+                        screens::logs_scroll_id(),
+                        scrollable::RelativeOffset { x: 0.5, y: 1.0 },
+                    );
+                }
+            }
             Message::AdaptersLoaded(views) => {
                 // Fresh enumeration: the draft resets to the persisted
                 // selection (an unsaved draft is discarded — same contract
@@ -1675,6 +1737,16 @@ impl App {
         )
     }
 
+    /// S17: should a freshly-landed log line re-anchor the logs viewport
+    /// to the bottom? Pure gating for the autoscroll task (unit-tested):
+    /// the user asked for it AND the Logs screen is the one on display
+    /// (scrolling a viewport that is not on screen is a no-op task, but
+    /// skipping it outright keeps the update path allocation-free when
+    /// the user is elsewhere).
+    pub fn autoscroll_logs_now(&self) -> bool {
+        self.logs_autoscroll && self.screen == Screen::Logs
+    }
+
     /// S11: is the Adapters draft different from what is persisted?
     /// Dirty = the selected SET differs OR the selected ORDER differs
     /// from `settings.bonded_adapters` (restricted to adapters present in
@@ -2109,6 +2181,57 @@ mod tests {
         assert!(LogFilter::Error.matches("ERROR core: handshake failed"));
     }
 
+    /// S17: autoscroll defaults ON (a log viewer tracks the tail until the
+    /// user opts out), the toggle flips it, and re-enabling re-anchors.
+    #[test]
+    fn logs_autoscroll_default_and_toggle() {
+        let mut app = App::new(Arc::new(crate::mock::MockService::new()), Flags::default());
+        assert!(app.logs_autoscroll, "autoscroll defaults ON");
+        let _ = app.update(Message::LogsAutoscrollToggled);
+        assert!(!app.logs_autoscroll);
+        let _ = app.update(Message::LogsAutoscrollToggled);
+        assert!(app.logs_autoscroll);
+    }
+
+    /// S17: the autoscroll task is gated on the Logs screen being on
+    /// display (scrolling an off-screen viewport is pointless work).
+    #[test]
+    fn logs_autoscroll_gated_on_screen() {
+        let mut app = App::new(Arc::new(crate::mock::MockService::new()), Flags::default());
+        app.screen = Screen::Status;
+        assert!(!app.autoscroll_logs_now(), "off-screen: no task");
+        app.screen = Screen::Logs;
+        assert!(app.autoscroll_logs_now());
+        app.logs_autoscroll = false;
+        assert!(!app.autoscroll_logs_now(), "user opted out: no task");
+    }
+
+    /// S17: the titlebar version comes from the workspace-inherited
+    /// CARGO_PKG_VERSION — a bare x.y.z triplet that moves with the
+    /// release tag (guards the `version.workspace = true` wiring).
+    #[test]
+    fn app_version_is_release_triplet() {
+        let v = app_version();
+        let parts: Vec<&str> = v.split('.').collect();
+        assert_eq!(parts.len(), 3, "version '{v}' must be x.y.z");
+        assert!(
+            parts
+                .iter()
+                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
+            "version '{v}' must be numeric"
+        );
+    }
+
+    /// S17: the logs scrollable id is stable across view rebuilds (the
+    /// update path's snap_to targets the id the viewport was given).
+    #[test]
+    fn logs_scroll_id_stable() {
+        assert_eq!(
+            crate::screens::logs_scroll_id(),
+            crate::screens::logs_scroll_id()
+        );
+    }
+
     /// Smoke run against the mock: init → tick loop → all screen views built.
     #[cfg(feature = "mock")]
     #[tokio::test]
diff --git a/crates/mt-app/src/mock.rs b/crates/mt-app/src/mock.rs
index 598ad88..9257ad0 100755
--- a/crates/mt-app/src/mock.rs
+++ b/crates/mt-app/src/mock.rs
@@ -434,11 +434,44 @@ impl MtService for MockService {
                 g.links = seed_links(MockService::rng_next(&mut g));
                 let _ = tx.send(ServiceEvent::StateChanged(ServiceState::Connected));
             }
+            // S17: a realistic bring-up burst so the Logs screen (and its
+            // scroll/autoscroll rig checks) has enough volume to overflow
+            // the viewport right after connecting — same vocabulary the
+            // real service uses, clearly mock-flavored.
+            this.log("INFO", "engine: bundled core spawned (mock)");
+            this.log("INFO", "config: 3 profiles loaded, active gcp-vm");
+            this.log("INFO", "adapters: 8 enumerated, 3 bonded");
+            this.log("INFO", "routing: fail-open, private ranges direct");
+            this.log(
+                "INFO",
+                "listeners: socks 127.0.0.1:10808, http 127.0.0.1:10809",
+            );
+            this.log("INFO", "bond: balanced policy, capacity-proportional");
+            this.log("INFO", "reality: handshake ok, dest borrow accepted");
+            this.log("INFO", "merge: daemon reachable at 127.0.0.1:7443");
             this.log("INFO", "tunnel up: all links healthy");
+            // Per-link bring-up detail: name + measured latency.
+            {
+                let g = inner.lock().await;
+                for (i, l) in g.links.iter().enumerate() {
+                    this.log(
+                        "INFO",
+                        &format!(
+                            "link {}/{} up: {} (latency {} ms)",
+                            i + 1,
+                            g.links.len(),
+                            l.name,
+                            l.latency_ms.unwrap_or(0)
+                        ),
+                    );
+                }
+            }
 
             // Per-second session loop; exits when state/gen changes.
+            let mut tick: u64 = 0;
             loop {
                 tokio::time::sleep(SESSION_TICK).await;
+                tick += 1;
                 let mut g = inner.lock().await;
                 if g.state != ServiceState::Connected || g.gen != cancel {
                     break;
@@ -469,10 +502,14 @@ impl MtService for MockService {
                     up_bps: up,
                     down_bps: down,
                 });
-                match MockService::rng_below(&mut g, 10) {
-                    0 => this.log("WARN", "backup link: latency spike, staying failover"),
-                    9 => this.log("INFO", "core: keepalive ok"),
-                    _ => {}
+                // S17: deterministic heartbeat (every 2nd tick) instead of
+                // the old 1-in-10 keepalive — live logs for the Logs
+                // screen demo AND a stable autoscroll signal for the rig.
+                if tick.is_multiple_of(2) {
+                    this.log("INFO", "core: keepalive ok");
+                }
+                if MockService::rng_below(&mut g, 10) == 0 {
+                    this.log("WARN", "backup link: latency spike, staying failover");
                 }
             }
         });
diff --git a/crates/mt-app/src/screens.rs b/crates/mt-app/src/screens.rs
index 85e350d..b834a65 100755
--- a/crates/mt-app/src/screens.rs
+++ b/crates/mt-app/src/screens.rs
@@ -8,7 +8,7 @@
 
 use iced::widget::{
     button, checkbox, column, container, horizontal_space, image, mouse_area, pick_list, row,
-    scrollable, text, text_input, toggler, vertical_space,
+    scrollable, stack, text, text_input, toggler, vertical_space,
 };
 use iced::{Length, Padding};
 use mt_service::{ProfileView, ServiceState};
@@ -43,21 +43,47 @@ pub fn titlebar(app: &App) -> Element<'_> {
     let title = text("MultiTunnel")
         .size(ts.title_m.size)
         .color(s.on_surface_variant);
+    // S17: the release version rides next to the name — the same number
+    // the release zip carries (workspace-inherited CARGO_PKG_VERSION).
+    let version = text(format!("v{}", crate::app::app_version()))
+        .size(ts.label_m.size)
+        .color(s.on_surface_variant);
     let identity = row![logo, title]
         .spacing(10)
         .align_y(iced::Alignment::Center);
+    let identity = row![identity, version]
+        .spacing(6)
+        .align_y(iced::Alignment::Center);
+    // S17: live global rates on the strip's right, just before the window
+    // controls — the same 1 s throughput sample the tray tooltip reports
+    // (down in tertiary, up in primary; zeros before the first sample).
+    let (up_bps, down_bps) = app.throughput.unwrap_or((0, 0));
+    let speeds = row![
+        text(format!("↓ {}", util::fmt_bps(down_bps)))
+            .size(ts.label_m.size)
+            .color(s.tertiary),
+        text(format!("↑ {}", util::fmt_bps(up_bps)))
+            .size(ts.label_m.size)
+            .color(s.primary),
+    ]
+    .spacing(10)
+    .align_y(iced::Alignment::Center);
     // The whole strip (minus the buttons — inner widgets capture first)
     // drags the window; a fast second press maximizes; a RIGHT press
-    // opens the window menu (S16).
+    // opens the window menu (S16). The speeds cluster is plain text —
+    // it drags like the rest of the strip.
+    let strip = row![identity, horizontal_space().width(Length::Fill), speeds,]
+        .align_y(iced::Alignment::Center)
+        .width(Length::Fill);
     let drag = mouse_area(
-        container(identity)
+        container(strip)
             .width(Length::Fill)
             .height(Length::Fill)
             // S16: the identity row is Shrink-height; without this it sat
             // flush at the strip's TOP (hidden with the old opaque emblem
             // tile, obvious with the transparent one).
             .align_y(iced::Alignment::Center)
-            .padding(Padding::new(0.0).left(14.0)),
+            .padding(Padding::new(0.0).left(14.0).right(12.0)),
     )
     .on_press(Message::TitleDrag)
     .on_right_press(Message::TitleContext);
@@ -1723,7 +1749,17 @@ pub fn settings(app: &App) -> Element<'_> {
 // Logs
 // ---------------------------------------------------------------------------
 
-/// Logs screen: streaming buffer fed by ServiceEvent::LogLine + filter.
+/// Logs screen (S17, D27): fixed toolbar + a dedicated viewport that owns
+/// the page's scrolling, with floating top/bottom jump buttons and an
+/// autoscroll-to-bottom toggle. The toolbar (level filter, autoscroll,
+/// Clear) stays put — only the log lines scroll.
+///
+/// The scrollable's id is shared with `App::update` (programmatic jumps +
+/// autoscroll re-anchoring on every fresh line while the toggle is on).
+pub fn logs_scroll_id() -> scrollable::Id {
+    scrollable::Id::new("logs-lines")
+}
+
 pub fn logs(app: &App) -> Element<'_> {
     let s = app.theme.scheme;
     let ts = app.theme.ts;
@@ -1736,12 +1772,27 @@ pub fn logs(app: &App) -> Element<'_> {
     .style(t3::pick_list_style(s))
     .menu_style(t3::menu_style(s));
 
+    // S17: the autoscroll toggle doubles as its own state indicator —
+    // Tonal (filled) while following the tail, quiet Text while off.
+    let (auto_label, auto_kind) = if app.logs_autoscroll {
+        ("Auto-scroll: on", ButtonKind::Tonal)
+    } else {
+        ("Auto-scroll: off", ButtonKind::Text)
+    };
+
     let toolbar = row![
         text("level:")
             .size(ts.body_m.size)
             .color(s.on_surface_variant),
         filter,
         horizontal_space().width(Length::Fill),
+        m3_button(
+            auto_label,
+            true,
+            auto_kind,
+            Message::LogsAutoscrollToggled,
+            app,
+        ),
         m3_button("Clear", true, ButtonKind::Text, Message::LogsCleared, app),
     ]
     .spacing(8)
@@ -1771,12 +1822,66 @@ pub fn logs(app: &App) -> Element<'_> {
         );
     }
 
+    // The viewport: the ONLY scrollable on this screen (the page itself is
+    // fixed via scaffold_fixed), so wheel/keys always move the log lines.
+    let viewport = scrollable(
+        container(lines)
+            .width(Length::Fill)
+            // Right padding keeps the newest line clear of the floating
+            // jump buttons anchored over the same corner.
+            .padding(Padding::new(2.0).right(8.0)),
+    )
+    .id(logs_scroll_id())
+    .width(Length::Fill)
+    .height(Length::Fill);
+
+    // S17: floating navigation — top/bottom jumps overlaid on the
+    // viewport's bottom-right corner. The anchor is a PLAIN container
+    // (non-interactive): wheel events and clicks between the buttons pass
+    // straight through to the scrollable beneath; only the two buttons
+    // capture. Same ↑/↓ glyphs as the adapter reorder controls.
+    let jump = |glyph: &'static str, msg: Message| {
+        button(text(glyph).size(ts.body_m.size))
+            .style(t3::tonal_button(s))
+            .padding(Padding::new(4.0).left(12.0).right(12.0))
+            .on_press(msg)
+    };
+    let floating = container(
+        column![
+            jump("↑", Message::LogsScrollTop),
+            jump("↓", Message::LogsScrollBottom),
+        ]
+        .spacing(6),
+    )
+    .padding(Padding::new(6.0))
+    .style(t3::card(s, s.surface_container_high));
+    // RIG FINDING (S17): anchored flush to the corner, the card sat inside
+    // the scrollable's scrollbar hit zone (iced grabs a margin around the
+    // rail) — the rail swallowed the button clicks. Stand off the right
+    // edge by more than the rail+grab width so the buttons own their
+    // pixels; keep the bottom inset modest (the corner reads best).
+    let overlay = container(floating)
+        .width(Length::Fill)
+        .height(Length::Fill)
+        .align_x(iced::Alignment::End)
+        .align_y(iced::Alignment::End)
+        .padding(Padding::new(0.0).right(26.0).bottom(4.0));
+
+    let viewport = stack![viewport].push(overlay);
+
     let card = container(
-        column![toolbar, vertical_space().height(Length::Fixed(6.0)), lines].width(Length::Fill),
+        column![
+            toolbar,
+            vertical_space().height(Length::Fixed(6.0)),
+            viewport
+        ]
+        .width(Length::Fill)
+        .height(Length::Fill),
     )
     .padding(16)
     .style(t3::card(s, s.surface_container_lowest))
-    .width(Length::Fill);
+    .width(Length::Fill)
+    .height(Length::Fill);
 
-    scaffold(app, "Logs", card.into())
+    scaffold_fixed(app, "Logs", card.into())
 }
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index 26c3759..0347f1d 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "04baaf6 2026-11-20T11:07:57Z",
+  "as_of": "6c7eedd 2026-11-20T11:21:55Z",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 248,
-  "edge_count": 256,
+  "node_count": 249,
+  "edge_count": 257,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -815,6 +815,26 @@
         }
       ]
     },
+    {
+      "id": "commit:6c7eedd",
+      "type": "commit",
+      "title": "backup(s16-verified-idle): 2026-11-20T11:21:54Z",
+      "tags": [
+        "2026-11-20"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "6c7eedd"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s16-verified-idle): 2026-11-20T11:21:54Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:6d0e58a",
       "type": "commit",
@@ -4960,7 +4980,7 @@
       ],
       "passages": [
         {
-          "text": "- **Stage**: S16 — repo polish + window menu + state-aware tray (D26), implementation and ALL gates green; v0.10.0 release re-running (first attempt hit two GitHub-runner flakes in the mt-bond integration leg — see LEDGER s16 and Watch items). The seven asks: GitHub About + 20 topic tags (done via API — the About description was a stale leftover of an unrelated project), README attribution section + banner + transparent logo, transparent titlebar emblem (flood-fill key-out + AREA resample — bilinear punches holes at 20:1), monochrome not-connected tray icon (NIM_MODIFY swap, boots Mono), tray hover = live speeds + Σ totals, titlebar right-click window menu with always-on-top (cross-platform iced change_level, persisted setting, honored at startup). Also fixed: latent S15 top-aligned identity cluster (container align_y(Center)). - **Task E core (v0.8.0, S14, CLOSED)**: saturation-aware scheduling shipped — the \"one adapter maxes out, no spill\" complaint fixed by peer-reported per-carrier delivery rates (1 s PINGs), carrier_score relative load, Balanced capacity-proportional spreading, Priority spill at 1.15 + writer backlog. S14 also shipped the agentic memory graph + RAG (query_memory.py / write_memory.py, 18/18 tests).",
+          "text": "- **Stage**: IDLE — all committed task chains closed and released through v0.10.0. S16 (repo polish + window menu + state-aware tray, D26) CLOSED: the seven asks shipped — GitHub About + 20 topic tags (the About description had been a stale leftover of an unrelated project), README attribution section + banner + transparent logo, transparent titlebar emblem (flood-fill key-out + AREA resample — bilinear punches holes at 20:1), monochrome not-connected tray icon (NIM_MODIFY swap, boots Mono), tray hover = live speeds + Σ totals, titlebar right-click window menu with always-on-top (cross-platform iced change_level, persisted, honored at startup); also fixed the latent S15 top-aligned identity cluster (align_y(Center)). v0.10.0 RELEASED and verified (re-run after two GitHub-runner flakes in the mt-bond leg — verdict + retry protocol in LEDGER s16; release re-confirmed live 2026-11-20 via API: 3 assets + About + 20 topics). - **Task E core (v0.8.0, S14, CLOSED)**: saturation-aware scheduling shipped — the \"one adapter maxes out, no spill\" complaint fixed by peer-reported per-carrier delivery rates (1 s PINGs), carrier_score relative load, Balanced capacity-proportional spreading, Priority spill at 1.15 + writer backlog. S14 also shipped the agentic memory graph + RAG (query_memory.py / write_memory.py, 18/18 tests).",
           "kind": "body"
         }
       ]
@@ -7391,6 +7411,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s9.1)"
     },
+    {
+      "src": "commit:6c7eedd",
+      "dst": "ledger:s16",
+      "rel": "part_of",
+      "evidence": "subject stage label (s16)"
+    },
     {
       "src": "commit:6e5502e",
       "dst": "ledger:s11",

( PROJECT_MEMORY.md's own diff is elided here — the durable
  sections above are hand-maintained; see harness/memory/LEDGER.md
  for the full stage history. )
```

