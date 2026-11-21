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

- **Stage**: IDLE — S18 CLOSED, v0.12.0 released and verified (release
  run 44879546925 first-try SUCCESS; the ci windows leg was a 69-min
  cold-cache cancel with same-commit green proof from the release's own
  build-windows — see LEDGER 08:30Z). The six asks shipped (D28): 3-line tray tooltip
  (name+version / totals / speeds, CRLF), transparent-background
  contrast-boosted LARGER runtime icons (tray ICOs + logo_64 + logo_24),
  Settings accent COLOR WHEEL (HSV disc asset + inverse marker math,
  live re-theme, persist on release), System/Light/Dark mode (OS
  detection chain: MT_THEME_SYSTEM_DARK env seam → Win32
  AppsUseLightTheme → gsettings/gtk; 5 s repoll; legacy dark_mode kept
  as the forward-compat shadow), mt-bond BLEST tie-guard + ECF
  completion scheduling (papers survey: docs/research/), UI changelog
  survey (docs/research/). 447/0 tests; rig verified sf=1.0+1.5.
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
### Commit: 7203362 - backup(s18-close-v0.12.0): 2026-11-21T08:27:44Z (Z User, 2026-11-21)
diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
index 5487b37..ea28030 100755
--- a/harness/memory/LEDGER.md
+++ b/harness/memory/LEDGER.md
@@ -212,3 +212,5 @@
 2026-11-21T07:15Z | s18 | S18 OPENED per user asks (D28): 3-line tray tooltip (name+version / totals / speeds); clear-background LARGER tray+taskbar icons; Settings color wheel for custom accent colors; system/light/dark mode selection; bonding performance research from prominent papers; UI research from comparable apps' changelogs. Sandbox had recycled again (registry src stale, rustup+zig wiped, xkblibs gone): D2 bootstrap rerun (rust 1.100.0, windows-gnu + aarch64 targets, cargo-zigbuild 0.24.1), zig 0.17.1 re-downloaded (the old tools/zig tarball was TRUNCATED — lib/std/std.zig missing, "unable to find zig installation directory"), registry src wiped + refetched, /tmp/xkblibs re-extracted from the root debs; env.sh rewritten (RUSTUP_HOME/CARGO_HOME/tools-zig on PATH). Disk filled to 100% mid-gates twice: stale /tmp/my-project recycle snapshots (1.6G) + mt-target/debug (5.8G) cleaned per the watch item.
 2026-11-21T07:25Z | s18 | IMPLEMENTED (D28): (1) tray tooltip 3 lines via CRLF (SpeedFan/qBittorrent multiline precedent) — "MultiTunnel v0.12.0" / "Total ↓x ↑y" / "↓x/s ↑y/s", ≤70 NIF_TIP units worst case, tests pin shape+budget; (2) assets: ALL runtime icons (icon.ico, icon_mono.ico, logo_64.rgba, logo_24.rgba) transparent-bg + contrast_boost (hue-preserving luminance remap [56,240]) + margin 0.08→0.04 — old connected icon was 92% >200-lum near-white opaque tile, new is 33% opaque with dark-stroke bands; (3) color wheel: scripts/make_color_wheel.py 256×256 HSV disc (hue=clockwise angle from 12h, sat=radius, V=1) → assets/color_wheel.rgba; theme.rs wheel_seed_at/wheel_marker_position inverse pair (cardinals+roundtrip tested); Settings Appearance = wheel(176px, mouse_area press/move/release, canvas marker w/ halo+ring) + current-seed chip + hex + 8 presets + seed field; drag re-themes live, release persists (global left-release ends outside-drags); (4) theme modes: SettingsView.theme_mode string (serde default "" → legacy dark_mode decides; dark_mode KEPT as forward-compat shadow written at effective state), ThemeMode enum + effective_dark in mt-service (3 migration tests), system detection chain env MT_THEME_SYSTEM_DARK → Win32 AppsUseLightTheme (RegGetValueW, windows-sys Win32_System_Registry feature added) → gsettings → gtk ini → GTK_THEME, 5-tick repoll in System mode with live re-theme + log; Settings UI = System/Light/Dark segmented buttons + resolution caption; save_settings syncs the shadow; (5) mt-bond scheduler per the paper survey: BLEST tie-guard (250ms drain cap among min-score-tied carriers; leader always eligible — first attempt filtered the leader too and FAILED the S14 saturation test, reconciled to guard-only-narrows-the-tied-set) + ECF completion (LowestLatency w/ reports = rtt + queue/rate; 5 new tests); (6) docs/research/2026-11-21-bonding-performance-survey.md (BLEST/ECF/coupled-CC/MP-QUIC/DaMPS/ToN-2023 mapped to shipped+roadmap) + 2026-11-21-ui-changelog-survey.md (CVR/v2rayN/FlClash/Hiddify/Speedify → shipped vs proposed).
 2026-11-21T07:30Z | s18 | VERIFIED: 447/0 workspace tests (+14 over S17: 3 mt-service mode migration, 2 theme wheel/system-seam, 2 app wheel-drag/mode-messages, 5 mt-bond BLEST/ECF, 2 tooltip); gates fmt/clippy host+windows-gnu (after clippy::explicit_counter_loop fix in the registry read) clean; vendored iced 6/0; zigbuild PE32+ ×4 + ELF ×2; mt-app --smoke 0. RIG sf=1.0: Settings → wheel click at 3 o'clock (hue 90) → seed chip purple(106,85,160)→GREEN(145,239,50), System-button fill purple→green, marker disc+halo+ring at the pick point; Dark click → page lum 236→37; Light click → 235 with green accent preserved. RIG env B (MT_THEME_SYSTEM_DARK=1): boots DARK (lum 27), System filled light-purple in dark scheme. RIG sf=1.5: same flow green chip + dark(36)/light(236) — HiDPI transform holds. Titlebar emblem lum spread 114 (light) / 212 (dark) — the boosted transparent mark reads on both.
+2026-11-21T07:17Z | s18 | v0.12.0 RELEASED: push attempt-1 REJECTED by GitHub — PROJECT_MEMORY.md had crossed the 100 MB per-file hard limit (the update_memory.py self-embedding recursion: each ritual run committed its own previous section into the file, so the next diff contained it — 81 MB at S17 close → 134 MB after this session's two runs; detected via the LFS-error push message, NOTHING reached origin). FIX: update_memory.py now excludes PROJECT_MEMORY.md's own diff (pathspec :(exclude) + an elision note), local history rewritten pre-push (reset --soft to origin/main, regenerated the section → 77 KB, re-committed s18 + re-tagged v0.12.0 — the oversized blobs never left the sandbox and are now unreachable). Attempt-2 push OK → release run 44879546925 SUCCESS on the first try (no cold-cache stall this time) + ci 44879546922 running. Assets verified by download+unzip: multitunnel-v0.12.0-windows-x64.zip 6,430,469 B integrity OK (multitunnel.exe PE32+ console + multitunnel-tray.exe PE32+ GUI + README v0.9–v0.12 section + QUICKSTART v0.12 note + server scripts + LICENSE + THIRD_PARTY_NOTICES + config.example) and mt-merge-server {x86_64 1,826,272 B, aarch64 1,860,808 B} ELFs.
+2026-11-21T08:30Z | s18 | OPS: the ci run on the s18 commit (44879546922) — lint/cross-check/test(ubuntu) all green; the windows test leg sat 69 min inside cargo test with no live logs fetchable mid-run → CANCELLED per the S11/S17 ops protocol. Verdict: 100%-cold rust-cache, not a wedge — the s18 commit changed Cargo.lock (version bumps + windows-sys feature), no windows job with that lock had ever completed, so Swatinem/rust-cache had nothing to restore and the leg compiled ~450 crates from zero (the exact S17 attempt-1 signature, cancelled at 68 min). Same-commit green proof on a REAL windows-latest runner exists: release run 44879546925's build-windows leg ran fmt + clippy + cargo test + packaging and PASSED (its zip is the verified v0.12.0 artifact). No rerun: the next ci.yml run would face the same cold cache (the release workflow's cache is keyed to a different workflow file); the S13 precedent (slow-cold cancelled, green proof elsewhere, no rerun) applies. The NEXT stage's ci will find the cache the release job saved for ITS key only — expect one more cold windows leg on the next Cargo.lock change.
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index 0022655..9b16150 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "d910ee5 2026-11-20T17:04:40Z",
+  "as_of": "9d4111b 2026-11-21T07:17:18Z",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 260,
-  "edge_count": 269,
+  "node_count": 263,
+  "edge_count": 272,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -1095,6 +1095,26 @@
         }
       ]
     },
+    {
+      "id": "commit:784569f",
+      "type": "commit",
+      "title": "s18: 3-line tray tooltip, transparent boosted icons, accent color wheel, system/light/dark modes, BLEST+ECF scheduling — (1) tray tooltip: three CRLF-separated lines per the user's exact spec (line 1 'MultiTunnel v<CARGO_PKG_VERSION>', line 2 'Total ↓x ↑y', line 3 '↓x/s ↑y/s'; not-connected states are 2 lines name+version/state) — <=70 NIF_TIP units worst case, tests pin shape + budget; (2) icon assets (make_logo_assets.py): ALL runtime icons — icon.ico 16/32/48+256, icon_mono.ico, logo_64.rgba (window/taskbar), logo_24.rgba (titlebar) — now TRANSPARENT background + contrast_boost (hue-preserving percentile luminance remap onto [56,240]) + margin 0.08→0.04 (mark fills ~8% more); the old connected icon was a 92%-near-white opaque tile, the new one is 33% opaque with real dark-stroke bands (verified by pixel stats; rig confirms emblem lum spread 114 light / 212 dark); (3) color wheel: scripts/make_color_wheel.py emits assets/color_wheel.rgba (256px HSV disc, hue=clockwise angle from 12h, sat=radius, V=1); theme.rs adds wheel_seed_at/wheel_marker_position inverse pair + system_prefers_dark; Settings Appearance = wheel (mouse_area press/move/release — press picks under-cursor, drag re-themes LIVE, release persists; global left-release ends outside-drags) + current-seed chip + hex + presets + seed field; (4) theme modes: mt-service SettingsView.theme_mode string ('system'|'light'|'dark', serde default '' → legacy dark_mode decides; dark_mode KEPT serialized at effective state = forward-compat shadow for v0.11 binaries) + ThemeMode enum + effective_dark; detection chain ",
+      "tags": [
+        "2026-11-21"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "784569f"
+        }
+      ],
+      "passages": [
+        {
+          "text": "s18: 3-line tray tooltip, transparent boosted icons, accent color wheel, system/light/dark modes, BLEST+ECF scheduling — (1) tray tooltip: three CRLF-separated lines per the user's exact spec (line 1 'MultiTunnel v<CARGO_PKG_VERSION>', line 2 'Total ↓x ↑y', line 3 '↓x/s ↑y/s'; not-connected states are 2 lines name+version/state) — <=70 NIF_TIP units worst case, tests pin shape + budget; (2) icon assets (make_logo_assets.py): ALL runtime icons — icon.ico 16/32/48+256, icon_mono.ico, logo_64.rgba (window/taskbar), logo_24.rgba (titlebar) — now TRANSPARENT background + contrast_boost (hue-preserving percentile luminance remap onto [56,240]) + margin 0.08→0.04 (mark fills ~8% more); the old connected icon was a 92%-near-white opaque tile, the new one is 33% opaque with real dark-stroke bands (verified by pixel stats; rig confirms emblem lum spread 114 light / 212 dark); (3) color wheel: scripts/make_color_wheel.py emits assets/color_wheel.rgba (256px HSV disc, hue=clockwise angle from 12h, sat=radius, V=1); theme.rs adds wheel_seed_at/wheel_marker_position inverse pair + system_prefers_dark; Settings Appearance = wheel (mouse_area press/move/release — press picks under-cursor, drag re-themes LIVE, release persists; global left-release ends outside-drags) + current-seed chip + hex + presets + seed field; (4) theme modes: mt-service SettingsView.theme_mode string ('system'|'light'|'dark', serde default '' → legacy dark_mode decides; dark_mode KEPT serialized at effective state = forward-compat shadow for v0.11 binaries) + ThemeMode enum + effective_dark; detection chain ",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:799f6cd",
       "type": "commit",
@@ -1235,6 +1255,26 @@
         }
       ]
     },
+    {
+      "id": "commit:9d4111b",
+      "type": "commit",
+      "title": "backup(s18-tag-v0.12.0): 2026-11-21T07:17:18Z [no changes]",
+      "tags": [
+        "2026-11-21"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "9d4111b"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s18-tag-v0.12.0): 2026-11-21T07:17:18Z [no changes]",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:9fafc6c",
       "type": "commit",
@@ -4367,6 +4407,14 @@
         {
           "text": "2026-11-21T07:30Z | VERIFIED: 447/0 workspace tests (+14 over S17: 3 mt-service mode migration, 2 theme wheel/system-seam, 2 app wheel-drag/mode-messages, 5 mt-bond BLEST/ECF, 2 tooltip); gates fmt/clippy host+windows-gnu (after clippy::explicit_counter_loop fix in the registry read) clean; vendored iced 6/0; zigbuild PE32+ ×4 + ELF ×2; mt-app --smoke 0. RIG sf=1.0: Settings → wheel click at 3 o'clock (hue 90) → seed chip purple(106,85,160)→GREEN(145,239,50), System-button fill purple→green, marker disc+halo+ring at the pick point; Dark click → page lum 236→37; Light click → 235 with green accent preserved. RIG env B (MT_THEME_SYSTEM_DARK=1): boots DARK (lum 27), System filled light-purple in dark scheme. RIG sf=1.5: same flow green chip + dark(36)/light(236) — HiDPI transform holds. Titlebar emblem lum spread 114 (light) / 212 (dark) — the boosted transparent mark reads on both.",
           "kind": "body"
+        },
+        {
+          "text": "2026-11-21T07:17Z | v0.12.0 RELEASED: push attempt-1 REJECTED by GitHub — PROJECT_MEMORY.md had crossed the 100 MB per-file hard limit (the update_memory.py self-embedding recursion: each ritual run committed its own previous section into the file, so the next diff contained it — 81 MB at S17 close → 134 MB after this session's two runs; detected via the LFS-error push message, NOTHING reached origin). FIX: update_memory.py now excludes PROJECT_MEMORY.md's own diff (pathspec :(exclude) + an elision note), local history rewritten pre-push (reset --soft to origin/main, regenerated the section → 77 KB, re-committed s18 + re-tagged v0.12.0 — the oversized blobs never left the sandbox and are now unreachable). Attempt-2 push OK → release run 44879546925 SUCCESS on the first try (no cold-cache stall this time) + ci 44879546922 running. Assets verified by download+unzip: multitunnel-v0.12.0-windows-x64.zip 6,430,469 B integrity OK (multitunnel.exe PE32+ console + multitunnel-tray.exe PE32+ GUI + README v0.9–v0.12 section + QUICKSTART v0.12 note + server scripts + LICENSE + THIRD_PARTY_NOTICES + config.example) and mt-merge-server {x86_64 1,826,272 B, aarch64 1,860,808 B} ELFs.",
+          "kind": "body"
+        },
+        {
+          "text": "2026-11-21T08:30Z | OPS: the ci run on the s18 commit (44879546922) — lint/cross-check/test(ubuntu) all green; the windows test leg sat 69 min inside cargo test with no live logs fetchable mid-run → CANCELLED per the S11/S17 ops protocol. Verdict: 100%-cold rust-cache, not a wedge — the s18 commit changed Cargo.lock (version bumps + windows-sys feature), no windows job with that lock had ever completed, so Swatinem/rust-cache had nothing to restore and the leg compiled ~450 crates from zero (the exact S17 attempt-1 signature, cancelled at 68 min). Same-commit green proof on a REAL windows-latest runner exists: release run 44879546925's build-windows leg ran fmt + clippy + cargo test + packaging and PASSED (its zip is the verified v0.12.0 artifact). No rerun: the next ci.yml run would face the same cold cache (the release workflow's cache is keyed to a different workflow file); the S13 precedent (slow-cold cancelled, green proof elsewhere, no rerun) applies. The NEXT stage's ci will find the cache the release job saved for ITS key only — expect one more cold windows leg on the next Cargo.lock change.",
+          "kind": "body"
         }
       ]
     },
@@ -5323,6 +5371,26 @@
         }
       ]
     },
+    {
+      "id": "release:v0.12.0",
+      "type": "release",
+      "title": "Release v0.12.0 (tagged 784569f)",
+      "tags": [
+        "v0.12.0"
+      ],
+      "sources": [
+        {
+          "file": "git:tags/v0.12.0",
+          "anchor": "784569f"
+        }
+      ],
+      "passages": [
+        {
+          "text": "Release v0.12.0: tag points at commit 784569f.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "release:v0.2.0",
       "type": "release",
@@ -7765,6 +7833,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s6-close)"
     },
+    {
+      "src": "commit:9d4111b",
+      "dst": "ledger:s18",
+      "rel": "part_of",
+      "evidence": "subject stage label (s18)"
+    },
     {
       "src": "commit:9fafc6c",
       "dst": "ledger:s2-fix",
@@ -8503,6 +8577,12 @@
       "rel": "part_of",
       "evidence": "project ledger"
     },
+    {
+      "src": "ledger:s18",
+      "dst": "release:v0.12.0",
+      "rel": "released_in",
+      "evidence": "2026-11-21T07:17Z | v0.12.0 RELEASED: push attempt-1 REJECTED by GitHub — PROJECT_MEMORY.md had crossed the 100 MB per-file hard limit (the update_memory.py self-embedding recursion: each ritual run committed its own previous section into the file, so the next diff contained it — 81 MB at S17 close → 134 MB after this session's two runs; detected via the LFS-error push message, NOTHING reached origin). FIX: update_memory.py now excludes PROJECT_MEMORY.md's own diff (pathspec :(exclude) + an elision note), local history rewritten pre-push (reset --soft to origin/main, regenerated the section → 77 KB, re-committed s18 + re-tagged v0.12.0 — the oversized blobs never left the sandbox and are now unreachable). Attempt-2 push OK → release run 44879546925 SUCCESS on the first try (no cold-cache stall this time) + ci 44879546922 running. Assets verified by download+unzip: multitunnel-v0.12.0-windows-x64.zip 6,430,469 B integrity OK (multitunnel.exe PE32+ console + multitunnel-tray.exe PE32+ GUI + README v0.9–v0.12 section + QUICKSTART v0.12 note + server scripts + LICENSE + THIRD_PARTY_NOTICES + config.example) and mt-merge-server {x86_64 1,826,272 B, aarch64 1,860,808 B} ELFs."
+    },
     {
       "src": "ledger:s2",
       "dst": "project:multitunnel",
@@ -8719,6 +8799,12 @@
       "rel": "built_from",
       "evidence": "git tag v0.11.0"
     },
+    {
+      "src": "release:v0.12.0",
+      "dst": "commit:784569f",
+      "rel": "built_from",
+      "evidence": "git tag v0.12.0"
+    },
     {
       "src": "release:v0.2.0",
       "dst": "commit:06881a6",

### Commit: 784569f - s18: 3-line tray tooltip, transparent boosted icons, accent color wheel, system/light/dark modes, BLEST+ECF scheduling — (1) tray tooltip: three CRLF-separated lines per the user's exact spec (line 1 'MultiTunnel v<CARGO_PKG_VERSION>', line 2 'Total ↓x ↑y', line 3 '↓x/s ↑y/s'; not-connected states are 2 lines name+version/state) — <=70 NIF_TIP units worst case, tests pin shape + budget; (2) icon assets (make_logo_assets.py): ALL runtime icons — icon.ico 16/32/48+256, icon_mono.ico, logo_64.rgba (window/taskbar), logo_24.rgba (titlebar) — now TRANSPARENT background + contrast_boost (hue-preserving percentile luminance remap onto [56,240]) + margin 0.08→0.04 (mark fills ~8% more); the old connected icon was a 92%-near-white opaque tile, the new one is 33% opaque with real dark-stroke bands (verified by pixel stats; rig confirms emblem lum spread 114 light / 212 dark); (3) color wheel: scripts/make_color_wheel.py emits assets/color_wheel.rgba (256px HSV disc, hue=clockwise angle from 12h, sat=radius, V=1); theme.rs adds wheel_seed_at/wheel_marker_position inverse pair + system_prefers_dark; Settings Appearance = wheel (mouse_area press/move/release — press picks under-cursor, drag re-themes LIVE, release persists; global left-release ends outside-drags) + current-seed chip + hex + presets + seed field; (4) theme modes: mt-service SettingsView.theme_mode string ('system'|'light'|'dark', serde default '' → legacy dark_mode decides; dark_mode KEPT serialized at effective state = forward-compat shadow for v0.11 binaries) + ThemeMode enum + effective_dark; detection chain MT_THEME_SYSTEM_DARK env (rig seam) → Windows AppsUseLightTheme (RegGetValueW, windows-sys Win32_System_Registry) → gsettings color-scheme → gtk3 settings.ini → GTK_THEME; 5-tick repoll in System mode with live re-theme; Settings UI = System/Light/Dark segmented buttons + resolution caption (replaces the dark toggler); (5) mt-bond scheduler per the papers survey: BLEST tie-guard (Ferlin et al. 2016) — among carriers tied at the min relative-load score, a >250ms queue-drain carrier is skipped (HoL avoidance at the merge reorder buffer), the score leader always eligible (S14 semantics preserved — the naive all-carriers filter failed the S14 saturation test); ECF (Lim et al. 2017) — lowest-latency with delivery reports = earliest completion (rtt + queued/deliver), empty 60ms beats backed-up 20ms; (6) docs/research/: 2026-11-21-bonding-performance-survey.md (BLEST/ECF/coupled-CC/MP-QUIC/DaMPS/ToN-2023 mapped to shipped + roadmap: redundant mode, FEC parity, reinjection, delay-aware score) + 2026-11-21-ui-changelog-survey.md (Clash Verge Rev/v2rayN/FlClash/Hiddify/Speedify → shipped vs proposed). ALSO: update_memory.py no longer embeds PROJECT_MEMORY.md's own diff (the recursive self-embedding doubled the file every ritual run — 81MB → 134MB, over GitHub's 100MB per-file limit; push rejected and history rewritten locally before anything reached origin; the file is 77KB again). TESTS: 447/0 (+14: 3 mt-service mode-migration, 2 theme wheel/env-seam, 2 app wheel-drag + mode coverage, 5 mt-bond BLEST/ECF, 2 tray tooltip); gates fmt/clippy host+windows-gnu/vendored-iced 6/0/zigbuild PE32+ x4 + ELF x2 green; rig sf=1.0+1.5: wheel click purple→green end-to-end (chip + System-button fill + marker), Dark 236→37 lum, Light back 235, MT_THEME_SYSTEM_DARK=1 boots dark (27) with System filled in the dark scheme; version 0.12.0; README v0.12 sections + QUICKSTART note (Z User, 2026-11-21)
diff --git a/Cargo.lock b/Cargo.lock
index 210bd62..cc65bd2 100755
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1962,7 +1962,7 @@ dependencies = [
 
 [[package]]
 name = "mt-app"
-version = "0.11.0"
+version = "0.12.0"
 dependencies = [
  "async-trait",
  "iced",
@@ -2073,7 +2073,7 @@ dependencies = [
 
 [[package]]
 name = "multitunnel"
-version = "0.11.0"
+version = "0.12.0"
 dependencies = [
  "anyhow",
  "async-trait",
diff --git a/Cargo.toml b/Cargo.toml
index 3a35da5..058ccd7 100755
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -18,7 +18,7 @@ members = ["crates/*"]
 resolver = "2"
 
 [workspace.package]
-version = "0.11.0"
+version = "0.12.0"
 
 [lib]
 name = "multitunnel"
diff --git a/QUICKSTART.md b/QUICKSTART.md
index 0946384..19e1b1c 100755
--- a/QUICKSTART.md
+++ b/QUICKSTART.md
@@ -84,6 +84,11 @@ needed for the usual per-user system-proxy writes (expect the SmartScreen
 > Logs screen gained floating jump-to-top/bottom buttons plus an
 > Auto-scroll toggle (on by default — it keeps the newest line in view).
 
+> v0.12: the tray tooltip is now three lines (app name + version, session
+> totals, live speeds), the tray/taskbar icons are transparent with a
+> bigger mark, and Settings gained a full accent COLOR WHEEL plus a
+> System / Light / Dark appearance mode (System follows the OS).
+
 **SMOKE TEST — this is the first time the GUI binary runs on real Windows.**
 The development sandbox is Linux; the window has been cross-compiled and
 unit-tested, never displayed. Do not walk away after launching it — verify
diff --git a/README.md b/README.md
index ce69866..3c78aaa 100755
--- a/README.md
+++ b/README.md
@@ -306,7 +306,7 @@ bonding mode to `stream` (Advanced): the bytes of every connection are
 split across all ticked adapters and re-merged by `mt-merge-server` on
 the tunnel server — see "Stream bonding (v0.5)" above.
 
-## App identity & tray (v0.9–v0.11) — logo, blended title bar, close-to-tray
+## App identity & tray (v0.9–v0.12) — logo, blended title bar, close-to-tray
 
 The window itself became a first-class part of the app (all of it in the
 GUI, no new dependencies):
@@ -321,6 +321,10 @@ GUI, no new dependencies):
   background (flood-fill key-out with area-averaged resampling), so the
   mark blends into the window surface instead of sitting on a colored
   chip — same for the README banner above.
+  Since v0.12 ALL the runtime icons (tray + taskbar/window icon) are
+  transparent-background with a hue-preserving contrast boost and a
+  tighter margin — the mark is larger and legible on light AND dark
+  taskbars (no more opaque tile).
 - **Chromium-style title bar**: the native window chrome is replaced by
   an in-app strip on the SAME surface as the page — logo + app name on
   the left, minimize/maximize/close on the right, and every other pixel
@@ -342,10 +346,12 @@ GUI, no new dependencies):
   colored logo; in every not-connected state it switches to a
   monochrome, transparent-background version of the same mark — the
   connection state is readable from across the room.
-- **Live speeds on hover (v0.10)**: hovering the tray icon shows the
-  current download/upload speed AND the session's total uploaded /
-  downloaded volume (e.g. `MultiTunnel — ↓9.4 MB/s ↑1.2 MB/s •
-  Σ↓3.0 GB ↑42.9 MB`).
+- **Live speeds on hover (v0.10, reworked v0.12)**: hovering the tray
+  icon shows a three-line tooltip — the app name with the release
+  version on line 1, the session's total transferred volume on line 2
+  and the current download/upload speed on line 3 (e.g.
+  `MultiTunnel v0.12.0` / `Total ↓3.0 GB ↑42.9 MB` /
+  `↓9.4 MB/s ↑1.2 MB/s`).
 - **Version + live speeds in the title bar (v0.11)**: the release
   version rides right next to the app name (it moves in lock-step with
   the release tag — one workspace version line), and the strip's right
@@ -358,6 +364,16 @@ GUI, no new dependencies):
   the toolbar — ON by default, it keeps the newest line in view as
   events land; turn it off to read back through history without the
   view jumping to the tail.
+- **Accent color wheel + appearance mode (v0.12)**: Settings → Appearance
+  now has a full COLOR WHEEL — click or drag to pick ANY accent color
+  (hue = angle, saturation = distance from the center; the theme
+  re-derives live while you drag and persists on release). The 8 preset
+  swatches and the numeric seed field stay for exact values. Next to it,
+  the appearance mode picks **System / Light / Dark** — System follows
+  the OS (the Windows Personalize setting / the freedesktop
+  color-scheme), re-checked every few seconds while the app runs. A
+  survey of the multipath-bonding literature (BLEST, ECF, MP-QUIC…) and
+  how it maps onto the scheduler shipped alongside in `docs/research/`.
 - **Quick Settings on right-click**: the tray menu is built fresh from
   live state — Connect/Disconnect (state-reflecting), the server list
   (radio-checked on the active profile), Open MultiTunnel and Exit.
diff --git a/assets/color_wheel.rgba b/assets/color_wheel.rgba
new file mode 100644
index 0000000..a96a9ca
Binary files /dev/null and b/assets/color_wheel.rgba differ
diff --git a/assets/icon.ico b/assets/icon.ico
index d770ee3..404c848 100755
Binary files a/assets/icon.ico and b/assets/icon.ico differ
diff --git a/assets/icon_mono.ico b/assets/icon_mono.ico
index 2fce76f..e4e2de4 100644
Binary files a/assets/icon_mono.ico and b/assets/icon_mono.ico differ
diff --git a/assets/logo_24.rgba b/assets/logo_24.rgba
index 885e51a..67642e6 100644
Binary files a/assets/logo_24.rgba and b/assets/logo_24.rgba differ
diff --git a/assets/logo_64.rgba b/assets/logo_64.rgba
index 5985019..630a1b9 100644
Binary files a/assets/logo_64.rgba and b/assets/logo_64.rgba differ
diff --git a/crates/mt-app/Cargo.toml b/crates/mt-app/Cargo.toml
index 5ba51be..aa8f7f2 100755
--- a/crates/mt-app/Cargo.toml
+++ b/crates/mt-app/Cargo.toml
@@ -33,5 +33,7 @@ mock = []
 
 # S15 (D25): the in-app tray is raw Win32 (same windows-sys pick as the
 # root crate's legacy tray, D4) — target-gated, no new cross-platform deps.
+# S18: Win32_System_Registry reads AppsUseLightTheme for the System theme
+# mode (same crate, one more feature gate, no new dependency).
 [target.'cfg(windows)'.dependencies]
-windows-sys = { version = "0.61.2", features = ["Win32_Foundation", "Win32_UI_Shell", "Win32_UI_WindowsAndMessaging"] }
+windows-sys = { version = "0.61.2", features = ["Win32_Foundation", "Win32_UI_Shell", "Win32_UI_WindowsAndMessaging", "Win32_System_Registry"] }
diff --git a/crates/mt-app/src/app.rs b/crates/mt-app/src/app.rs
index 0551aed..11f42bd 100755
--- a/crates/mt-app/src/app.rs
+++ b/crates/mt-app/src/app.rs
@@ -276,8 +276,17 @@ pub enum Message {
     SeedFieldChanged(String),
     /// Commit the numeric seed field.
     SeedCommitted,
-    /// Dark mode toggled.
-    DarkToggled(bool),
+    /// S18: appearance mode picked — "system" | "light" | "dark"
+    /// (replaces the S9 dark-mode toggler).
+    ThemeModePicked(String),
+    /// S18: the accent color wheel was pressed (drag starts; the color
+    /// under the cursor is picked immediately).
+    WheelPressed,
+    /// S18: the cursor moved over the wheel (local coords; live seed
+    /// update while dragging).
+    WheelMoved(iced::Point),
+    /// S18: the wheel drag ended (release anywhere) — persist the seed.
+    WheelReleased,
     /// System proxy toggled.
     SystemProxyToggled(bool),
     /// Merge policy picked.
@@ -487,6 +496,17 @@ pub struct App {
     /// (default ON — a log viewer should track the tail until the user
     /// opts out; the toggle lives in the Logs toolbar).
     pub logs_autoscroll: bool,
+    /// S18: last-detected OS dark preference (System mode resolves through
+    /// it; `MT_THEME_SYSTEM_DARK` env override for the rig).
+    pub sys_dark: bool,
+    /// S18: Tick counter for the System-mode OS preference re-poll.
+    pub sys_poll: u32,
+    /// S18: the accent wheel drag is in progress (seed updates live, the
+    /// pick persists on release).
+    pub wheel_dragging: bool,
+    /// S18: last cursor position inside the wheel (local coords) — the
+    /// press picks the color under the cursor without needing a move.
+    pub wheel_cursor: Option<iced::Point>,
 }
 
 /// S17: the release version shown next to the app name in the titlebar.
@@ -503,6 +523,8 @@ impl App {
         let settings = SettingsView {
             theme_seed: SEED_SWATCHES[0],
             dark_mode: false,
+            // S18: follow the OS until the user picks otherwise.
+            theme_mode: "system".to_string(),
             system_proxy: false,
             merge_policy: MERGE_POLICIES[0].to_string(),
             autostart: false,
@@ -518,8 +540,11 @@ impl App {
             close_to_tray: false,
             always_on_top: false,
         };
+        // S18: detect the OS appearance once so the System mode (the
+        // stock default) starts honest.
+        let sys_dark = crate::theme::system_prefers_dark();
         Self {
-            theme: MtTheme::from_settings(&settings),
+            theme: MtTheme::derive(settings.theme_seed, settings.effective_dark(sys_dark)),
             service,
             events_rx,
             flags,
@@ -562,6 +587,11 @@ impl App {
             tray_icon: crate::tray::TrayIcon::Mono,
             // S17
             logs_autoscroll: true,
+            // S18
+            sys_dark,
+            sys_poll: 0,
+            wheel_dragging: false,
+            wheel_cursor: None,
         }
     }
 
@@ -615,6 +645,12 @@ impl App {
         app.rules_saved = app.rules.clone();
         app.settings = app.service.settings().await;
         app.seed_text = app.settings.theme_seed.to_string();
+        // S18: resolve light/dark through the picked mode + OS preference.
+        app.sys_dark = crate::theme::system_prefers_dark();
+        app.theme.rederive(
+            app.settings.theme_seed,
+            app.settings.effective_dark(app.sys_dark),
+        );
         app.adapters = app
             .service
             .adapters()
@@ -626,8 +662,6 @@ impl App {
                 view,
             })
             .collect();
-        app.theme
-            .rederive(app.settings.theme_seed, app.settings.dark_mode);
         app.seed_initial_logs();
         app
     }
@@ -769,6 +803,11 @@ impl App {
     }
 
     fn save_settings(&self, s: SettingsView) -> iced::Task<Message> {
+        // S18: keep the legacy `dark_mode` bool at the EFFECTIVE state so a
+        // v0.11-or-older binary reading this file lands on the appearance
+        // the user currently sees (forward-compat shadow; D8).
+        let mut s = s;
+        s.dark_mode = s.effective_dark(self.sys_dark);
         let svc = Arc::clone(&self.service);
         iced::Task::perform(
             async move { svc.set_settings(s).await },
@@ -812,6 +851,27 @@ impl App {
         match message {
             Message::Tick => {
                 self.pump_events();
+                // S18: System mode follows the OS — re-check the preference
+                // every few ticks (cheap on Windows: one registry read; the
+                // env seam makes the rig deterministic).
+                if self.settings.theme_mode() == mt_service::ThemeMode::System {
+                    self.sys_poll = self.sys_poll.saturating_add(1);
+                    if self.sys_poll >= crate::theme::SYSTEM_THEME_POLL_TICKS {
+                        self.sys_poll = 0;
+                        let now_dark = crate::theme::system_prefers_dark();
+                        if now_dark != self.sys_dark {
+                            self.sys_dark = now_dark;
+                            util::push_log(
+                                &mut self.logs,
+                                format!(
+                                    "INFO  gui: system appearance changed → {}",
+                                    if now_dark { "dark" } else { "light" }
+                                ),
+                            );
+                            self.apply_theme();
+                        }
+                    }
+                }
                 let t = self.refresh_status();
                 self.sync_tray();
                 return t;
@@ -1109,7 +1169,10 @@ impl App {
             Message::SettingsLoaded(s) => {
                 self.settings = s.clone();
                 self.seed_text = s.theme_seed.to_string();
-                self.theme.rederive(s.theme_seed, s.dark_mode);
+                // S18: light/dark resolves through the picked mode + the
+                // CURRENT OS preference (not the legacy bool).
+                self.sys_dark = crate::theme::system_prefers_dark();
+                self.apply_theme();
                 // S16: honor a persisted always-on-top now that settings
                 // exist (the Opened path covers the reverse order).
                 let top = s.always_on_top;
@@ -1126,14 +1189,14 @@ impl App {
             Message::SeedSwatch(seed) => {
                 self.settings.theme_seed = seed;
                 self.seed_text = seed.to_string();
-                self.theme.rederive(seed, self.settings.dark_mode);
+                self.apply_theme();
                 return self.save_settings(self.settings.clone());
             }
             Message::SeedFieldChanged(t) => self.seed_text = t,
             Message::SeedCommitted => match parse_seed(&self.seed_text) {
                 Some(seed) => {
                     self.settings.theme_seed = seed;
-                    self.theme.rederive(seed, self.settings.dark_mode);
+                    self.apply_theme();
                     return self.save_settings(self.settings.clone());
                 }
                 None => {
@@ -1141,11 +1204,45 @@ impl App {
                         Some("invalid seed: use decimal or 0xRRGGBB".to_string());
                 }
             },
-            Message::DarkToggled(dark) => {
-                self.settings.dark_mode = dark;
-                self.theme.rederive(self.settings.theme_seed, dark);
+            Message::ThemeModePicked(mode) => {
+                if !["system", "light", "dark"].contains(&mode.as_str()) {
+                    self.settings_status = Some(format!("unknown theme mode: {mode:?}"));
+                    return iced::Task::none();
+                }
+                self.settings.theme_mode = mode;
+                self.apply_theme();
                 return self.save_settings(self.settings.clone());
             }
+            // S18: the accent wheel. Press picks the color under the
+            // cursor (the move that GOT the cursor there already recorded
+            // the position); drag updates live; release persists.
+            Message::WheelPressed => {
+                self.wheel_dragging = true;
+                if let Some(p) = self.wheel_cursor {
+                    self.set_wheel_seed(crate::theme::wheel_seed_at(
+                        p,
+                        crate::theme::COLOR_WHEEL_DRAW,
+                    ));
+                }
+            }
+            Message::WheelMoved(p) => {
+                self.wheel_cursor = Some(p);
+                if self.wheel_dragging {
+                    self.set_wheel_seed(crate::theme::wheel_seed_at(
+                        p,
+                        crate::theme::COLOR_WHEEL_DRAW,
+                    ));
+                }
+            }
+            Message::WheelReleased => {
+                let was_dragging = self.wheel_dragging;
+                self.wheel_dragging = false;
+                if was_dragging {
+                    // The mouse_area's own on_release also lands here — the
+                    // second arrival no-ops (dragging already false).
+                    return self.save_settings(self.settings.clone());
+                }
+            }
             Message::SystemProxyToggled(v) => {
                 self.settings.system_proxy = v;
                 return self.save_settings(self.settings.clone());
@@ -1718,11 +1815,16 @@ impl App {
             // S15: window events (Open/CloseRequested/Resized) + the tray
             // event stream.
             iced::window::events().map(|(id, ev)| Message::WindowEvent(id, ev)),
-            // S16: cursor tracking (anchors the titlebar window menu).
+            // S16: cursor tracking (anchors the titlebar window menu) +
+            // S18: a left release ANYWHERE ends an accent-wheel drag (the
+            // mouse_area only sees releases inside its own bounds).
             iced::event::listen_with(|event, _status, _window| match event {
                 iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                     Some(Message::CursorMoved(position))
                 }
+                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
+                    iced::mouse::Button::Left,
+                )) => Some(Message::WheelReleased),
                 _ => None,
             }),
             tray_events,
@@ -1747,6 +1849,29 @@ impl App {
         self.logs_autoscroll && self.screen == Screen::Logs
     }
 
+    /// S18: the dark flag the picked mode + OS preference resolve to right
+    /// now (the single source of truth for `MtTheme` re-derivation).
+    pub fn theme_dark(&self) -> bool {
+        self.settings.effective_dark(self.sys_dark)
+    }
+
+    /// S18: re-derive the theme snapshot from the CURRENT seed + resolved
+    /// light/dark (every seed/mode change funnels through here).
+    pub fn apply_theme(&mut self) {
+        self.theme
+            .rederive(self.settings.theme_seed, self.theme_dark());
+    }
+
+    /// S18: live-update the accent seed during a wheel drag/pick (no disk
+    /// write — the release persists; see [`Message::WheelReleased`]).
+    fn set_wheel_seed(&mut self, seed: u32) {
+        if seed != self.settings.theme_seed {
+            self.settings.theme_seed = seed;
+            self.seed_text = seed.to_string();
+            self.apply_theme();
+        }
+    }
+
     /// S11: is the Adapters draft different from what is persisted?
     /// Dirty = the selected SET differs OR the selected ORDER differs
     /// from `settings.bonded_adapters` (restricted to adapters present in
@@ -2424,9 +2549,19 @@ mod tests {
         drop(app.update(Message::SeedCommitted));
         assert_eq!(app.theme.palette.seed(), 0x42F45B);
 
-        drop(app.update(Message::DarkToggled(true)));
+        // S18: explicit Dark mode flips the theme now (and the legacy
+        // dark_mode shadow follows on save).
+        drop(app.update(Message::ThemeModePicked("dark".to_string())));
         assert!(app.theme.dark);
-        assert!(app.settings.dark_mode);
+        drop(app.update(Message::ThemeModePicked("light".to_string())));
+        assert!(!app.theme.dark);
+        // System mode resolves through the detected OS preference.
+        drop(app.update(Message::ThemeModePicked("system".to_string())));
+        assert_eq!(app.theme.dark, app.theme_dark());
+        assert_eq!(app.settings.theme_mode, "system");
+        // Garbage is rejected, leaving the mode untouched.
+        drop(app.update(Message::ThemeModePicked("neon".to_string())));
+        assert_eq!(app.settings.theme_mode, "system");
 
         // Invalid seed commit leaves the theme untouched.
         drop(app.update(Message::SeedFieldChanged("nope".to_string())));
@@ -2439,6 +2574,43 @@ mod tests {
             .contains("invalid seed"));
     }
 
+    /// S18: the accent wheel — press picks the color under the cursor,
+    /// drag follows, release persists via the same save path as swatches.
+    #[cfg(feature = "mock")]
+    #[tokio::test]
+    async fn wheel_drag_updates_seed_live_and_persists_on_release() {
+        let service = Arc::new(crate::mock::MockService::new());
+        let mut app = App::init_async(service, Flags::default()).await;
+        let seed_before = app.theme.palette.seed();
+        let s = crate::theme::COLOR_WHEEL_DRAW;
+
+        // Cursor moves over the wheel WITHOUT dragging: remembered, no pick.
+        drop(app.update(Message::WheelMoved(iced::Point::new(s / 2.0, s / 2.0))));
+        assert_eq!(app.theme.palette.seed(), seed_before);
+
+        // Press at the 12 o'clock rim: red seed, immediately applied.
+        drop(app.update(Message::WheelPressed));
+        assert!(app.wheel_dragging);
+        let red = crate::theme::wheel_seed_at(iced::Point::new(s / 2.0, 4.0), s);
+        drop(app.update(Message::WheelMoved(iced::Point::new(s / 2.0, 4.0))));
+        assert_eq!(app.theme.palette.seed(), red);
+        assert_eq!(app.seed_text, red.to_string());
+
+        // Release: the drag ends and the pick rides the SAME save path as
+        // the seed swatches (`save_settings` → Task::perform; the iced
+        // runtime owns task execution, so a unit test drops the task —
+        // the mock round-trip test covers the service save itself).
+        drop(app.update(Message::WheelReleased));
+        assert!(!app.wheel_dragging);
+        assert_eq!(app.settings.theme_seed, red);
+        assert_eq!(app.theme.palette.seed(), red);
+        assert_eq!(app.settings.theme_mode, "system");
+
+        // A stray release (no drag in progress) does nothing.
+        drop(app.update(Message::WheelReleased));
+        assert_eq!(app.theme.palette.seed(), red);
+    }
+
     /// GUI update loop over the mock service: screen switches, the rules
     /// editor round-trips through the service, log events land, and the
     /// connect lifecycle is observable through the event pump.
diff --git a/crates/mt-app/src/lib.rs b/crates/mt-app/src/lib.rs
index 64040ad..f998ed8 100755
--- a/crates/mt-app/src/lib.rs
+++ b/crates/mt-app/src/lib.rs
@@ -15,7 +15,9 @@
 //!   switches, text fields, cards on tonal surfaces; all styling derived from
 //!   [`mt_m3`] tokens (colors, type scale, shape radii, state layers).
 //! * Live re-theme: the whole palette is re-derived from
-//!   `SettingsView::theme_seed` (+ `dark_mode`) on every change.
+//!   `SettingsView::theme_seed` (+ the S18 appearance mode —
+//!   system/light/dark, `System` resolving through the OS preference) on
+//!   every change.
 //! * Screens: Status (state card, connect switch, links with latency chips,
 //!   throughput), Servers (add-by-paste with parse preview via
 //!   [`mt_links::parse_share_link`], set active, latency test, delete,
diff --git a/crates/mt-app/src/mock.rs b/crates/mt-app/src/mock.rs
index 9257ad0..538b295 100755
--- a/crates/mt-app/src/mock.rs
+++ b/crates/mt-app/src/mock.rs
@@ -78,6 +78,8 @@ impl MockService {
         let settings = SettingsView {
             theme_seed: 0x6750A4,
             dark_mode: false,
+            // S18: follow the OS (mirrors the service default).
+            theme_mode: "system".to_string(),
             system_proxy: false,
             // D23: mirror the service default (balanced — live byte-load
             // spreading; round-robin's connection-count evenness let one
@@ -653,8 +655,8 @@ impl MtService for MockService {
         self.log(
             "INFO",
             &format!(
-                "settings saved (seed {}, dark {})",
-                settings.theme_seed, settings.dark_mode
+                "settings saved (seed {}, mode {})",
+                settings.theme_seed, settings.theme_mode
             ),
         );
         Ok(())
@@ -811,10 +813,10 @@ mod tests {
         let svc = MockService::new();
         let mut s = svc.settings().await;
         s.theme_seed = 0x4285F4;
-        s.dark_mode = true;
+        s.theme_mode = "dark".to_string();
         svc.set_settings(s).await.expect("save ok");
         let s2 = svc.settings().await;
-        assert!(s2.dark_mode);
+        assert_eq!(s2.theme_mode, "dark");
         assert_eq!(s2.theme_seed, 0x4285F4);
 
         let rules = svc.routing_rules().await;
diff --git a/crates/mt-app/src/screens.rs b/crates/mt-app/src/screens.rs
index b834a65..5e41daf 100755
--- a/crates/mt-app/src/screens.rs
+++ b/crates/mt-app/src/screens.rs
@@ -7,11 +7,11 @@
 //! Adapters screen).
 
 use iced::widget::{
-    button, checkbox, column, container, horizontal_space, image, mouse_area, pick_list, row,
-    scrollable, stack, text, text_input, toggler, vertical_space,
+    button, canvas, checkbox, column, container, horizontal_space, image, mouse_area, pick_list,
+    row, scrollable, stack, text, text_input, toggler, vertical_space, Canvas,
 };
 use iced::{Length, Padding};
-use mt_service::{ProfileView, ServiceState};
+use mt_service::{ProfileView, ServiceState, ThemeMode};
 
 use crate::app::{
     App, LatencyState, LogFilter, Message, RuleRow, Screen, TitleMenuItem, BOND_MODES, LOG_LEVELS,
@@ -1450,13 +1450,94 @@ pub fn adapters(app: &App) -> Element<'_> {
 // Settings
 // ---------------------------------------------------------------------------
 
-/// Settings screen: theme seed picker + dark, system proxy, merge policy,
-/// autostart — all mapped onto SettingsView with immediate save.
+/// S18: the accent wheel's selection marker — a canvas overlay stacked on
+/// the wheel image, drawing a filled disc of the CURRENT seed color with
+/// an on-surface ring (visible over any wheel hue). The canvas is sized
+/// to the same square as the wheel, so [`theme::wheel_marker_position`]'s
+/// local coordinates line up with the image underneath.
+#[derive(Debug, Clone, Copy)]
+pub struct WheelMarker {
+    /// The seed whose wheel position the marker shows.
+    pub seed: u32,
+    /// Ring color (on_surface — theme-dependent).
+    pub ring: iced::Color,
+}
+
+impl canvas::Program<Message> for WheelMarker {
+    type State = ();
+
+    fn draw(
+        &self,
+        _state: &Self::State,
+        renderer: &iced::Renderer,
+        _theme: &iced::Theme,
+        bounds: iced::Rectangle,
+        _cursor: iced::mouse::Cursor,
+    ) -> Vec<canvas::Geometry> {
+        let mut frame = canvas::Frame::new(renderer, bounds.size());
+        let side = bounds.width.min(bounds.height).max(1.0);
+        let p = crate::theme::wheel_marker_position(self.seed, side);
+        let c = mt_m3::M3Color::from_argb(self.seed);
+        // Halo + disc + ring: readable over identical hues and at the rim.
+        frame.fill(
+            &canvas::Path::circle(p, 9.0),
+            iced::Color::from(c).scale_alpha(0.35),
+        );
+        frame.fill(&canvas::Path::circle(p, 6.0), iced::Color::from(c));
+        frame.stroke(
+            &canvas::Path::circle(p, 6.0),
+            canvas::Stroke::default()
+                .with_color(self.ring)
+                .with_width(2.0),
+        );
+        vec![frame.into_geometry()]
+    }
+}
+
+/// Settings screen: accent color wheel + presets + seed, theme mode
+/// (system/light/dark), system proxy, merge policy, autostart — all
+/// mapped onto SettingsView with immediate save.
 pub fn settings(app: &App) -> Element<'_> {
     let s = app.theme.scheme;
     let ts = app.theme.ts;
 
     // Appearance card ------------------------------------------------------
+    //
+    // S18: the accent picker is a color WHEEL (click or drag; the seed
+    // updates live and persists on release) — the 8 presets and the
+    // numeric seed field stay for exact values. Next to it, the
+    // appearance MODE picks system / light / dark.
+    let wheel_img = image(image::Handle::from_rgba(
+        t3::COLOR_WHEEL_PX,
+        t3::COLOR_WHEEL_PX,
+        t3::COLOR_WHEEL_BYTES.to_vec(),
+    ))
+    .width(Length::Fixed(t3::COLOR_WHEEL_DRAW))
+    .height(Length::Fixed(t3::COLOR_WHEEL_DRAW));
+    let marker = Canvas::new(WheelMarker {
+        seed: app.settings.theme_seed,
+        ring: t3::col(s.on_surface),
+    })
+    .width(Length::Fixed(t3::COLOR_WHEEL_DRAW))
+    .height(Length::Fixed(t3::COLOR_WHEEL_DRAW));
+    let wheel = mouse_area(stack![wheel_img, marker])
+        .on_press(Message::WheelPressed)
+        .on_release(Message::WheelReleased)
+        .on_move(Message::WheelMoved)
+        .interaction(iced::mouse::Interaction::Crosshair);
+
+    // The current accent: a filled chip + hex readout (also the rig's
+    // pixel-assertion target — the chip shows the picked seed verbatim).
+    let seed = app.settings.theme_seed;
+    let seed_chip = container(vertical_space().height(Length::Fixed(28.0)))
+        .width(Length::Fixed(48.0))
+        .style(move |_t| iced::widget::container::Style {
+            background: t3::bg(mt_m3::M3Color::from_argb(seed)),
+            border: t3::border(8.0, s.on_surface_variant, 1.0),
+            ..Default::default()
+        });
+    let hex = format!("#{seed:06X}", seed = seed & 0xFF_FFFF);
+
     let mut swatches = row![].spacing(8);
     for seed in t3::SEED_SWATCHES {
         let selected = seed == app.settings.theme_seed;
@@ -1486,25 +1567,87 @@ pub fn settings(app: &App) -> Element<'_> {
         .width(Length::Fixed(180.0))
         .style(t3::text_field(s));
 
+    // S18 appearance mode: segmented System / Light / Dark (selected =
+    // filled). System shows what it currently resolves to.
+    let mode = app.settings.theme_mode();
+    let mode_button = |label: &str, m: ThemeMode, msg: Message| {
+        m3_button(
+            label,
+            true,
+            if mode == m {
+                ButtonKind::Filled
+            } else {
+                ButtonKind::Outlined
+            },
+            msg,
+            app,
+        )
+    };
+    let mode_caption = match mode {
+        ThemeMode::System => format!(
+            "system — currently {}",
+            if app.theme_dark() { "dark" } else { "light" }
+        ),
+        ThemeMode::Light => "light — always".to_string(),
+        ThemeMode::Dark => "dark — always".to_string(),
+    };
+    let mode_row = row![
+        mode_button(
+            "System",
+            ThemeMode::System,
+            Message::ThemeModePicked("system".into())
+        ),
+        mode_button(
+            "Light",
+            ThemeMode::Light,
+            Message::ThemeModePicked("light".into())
+        ),
+        mode_button(
+            "Dark",
+            ThemeMode::Dark,
+            Message::ThemeModePicked("dark".into())
+        ),
+    ]
+    .spacing(8);
+
     let appearance = column![
         text("Appearance").size(ts.title_m.size).color(s.on_surface),
-        swatches,
         row![
-            seed_field,
-            m3_button(
-                "Set seed",
-                true,
-                ButtonKind::Tonal,
-                Message::SeedCommitted,
-                app
-            ),
+            wheel,
+            column![
+                text("accent color")
+                    .size(ts.label_l.size)
+                    .color(s.on_surface_variant),
+                row![
+                    seed_chip,
+                    text(hex).size(ts.body_m.size).color(s.on_surface)
+                ]
+                .spacing(10)
+                .align_y(iced::Alignment::Center),
+                swatches,
+                row![
+                    seed_field,
+                    m3_button(
+                        "Set seed",
+                        true,
+                        ButtonKind::Tonal,
+                        Message::SeedCommitted,
+                        app
+                    ),
+                ]
+                .spacing(8),
+            ]
+            .spacing(10),
         ]
-        .spacing(8),
-        toggler(app.settings.dark_mode)
-            .label("dark mode")
-            .size(28.0)
-            .on_toggle(Message::DarkToggled)
-            .style(t3::switch(s)),
+        .spacing(20)
+        .align_y(iced::Alignment::Start),
+        text("mode")
+            .size(ts.label_l.size)
+            .color(s.on_surface_variant),
+        mode_row,
+        text(mode_caption)
+            .size(ts.label_m.size)
+            .color(s.on_surface_variant),
     ]
     .spacing(10);
 
diff --git a/crates/mt-app/src/theme.rs b/crates/mt-app/src/theme.rs
index 52affcb..34468bf 100755
--- a/crates/mt-app/src/theme.rs
+++ b/crates/mt-app/src/theme.rs
@@ -455,6 +455,200 @@ where
     iced::widget::text(s).color(c)
 }
 
+// ---------------------------------------------------------------------------
+// S18: OS appearance detection (System theme mode)
+// ---------------------------------------------------------------------------
+
+/// How often the System mode re-checks the OS preference (Tick is 1 s).
+pub const SYSTEM_THEME_POLL_TICKS: u32 = 5;
+
+/// Does the OS prefer a DARK app appearance right now? (S18 System mode.)
+///
+/// Order of authority:
+/// 1. `MT_THEME_SYSTEM_DARK` env override — the rig/test seam (Xvfb has
+///    no appearance setting; a deterministic override keeps GUI-rig
+///    assertions honest).
+/// 2. Windows: `AppsUseLightTheme` in the Personalize registry key (the
+///    exact setting the Windows 10/11 "Choose your mode" toggle writes).
+/// 3. Linux desktops: the freedesktop `color-scheme` (gsettings/portal),
+///    then the GTK3 settings file, then `GTK_THEME` conventions.
+/// 4. Unknown → light (the historic default of this app).
+pub fn system_prefers_dark() -> bool {
+    if let Ok(v) = std::env::var("MT_THEME_SYSTEM_DARK") {
+        return matches!(v.trim(), "1" | "true" | "yes" | "dark" | "on");
+    }
+    sys_prefers_dark_platform()
+}
+
+#[cfg(windows)]
+fn sys_prefers_dark_platform() -> bool {
+    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
+
+    let sub_key: [u16; 64] = {
+        // "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"
+        const S: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\u{0}";
+        let mut a = [0u16; 64];
+        for (slot, u) in a.iter_mut().zip(S.encode_utf16()) {
+            *slot = u;
+        }
+        a
+    };
+    let value: [u16; 20] = {
+        const V: &str = "AppsUseLightTheme\u{0}";
+        let mut a = [0u16; 20];
+        for (slot, u) in a.iter_mut().zip(V.encode_utf16()) {
+            *slot = u;
+        }
+        a
+    };
+    let mut data: u32 = 1;
+    let mut len = std::mem::size_of::<u32>() as u32;
+    // SAFETY: RegGetValueW with RRF_RT_REG_DWORD writes at most 4 bytes
+    // into `data` (len in/out) and reads the two NUL-terminated UTF-16
+    // arrays above; all pointers stay valid for the call. Failure (any
+    // return code != 0) means "unknown" → light.
+    let rc = unsafe {
+        RegGetValueW(
+            HKEY_CURRENT_USER,
+            sub_key.as_ptr(),
+            value.as_ptr(),
+            RRF_RT_REG_DWORD,
+            std::ptr::null_mut(),
+            &mut data as *mut u32 as *mut core::ffi::c_void,
+            &mut len,
+        )
+    };
+    rc == 0 && data == 0 // AppsUseLightTheme=0 → dark
+}
+
+#[cfg(not(windows))]
+fn sys_prefers_dark_platform() -> bool {
+    // gsettings (GNOME/most modern desktops) — prefer-dark is the
+    // freedesktop "color-scheme" answer.
+    if let Ok(out) = std::process::Command::new("gsettings")
+        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
+        .output()
+    {
+        if out.status.success() {
+            let s = String::from_utf8_lossy(&out.stdout);
+            if s.contains("prefer-dark") {
+                return true;
+            }
+            if s.contains("prefer-light") {
+                return false;
+            }
+        }
+    }
+    // GTK3 settings.ini fallback.
+    if let Some(home) = std::env::var_os("HOME") {
+        let ini = std::path::Path::new(&home).join(".config/gtk-3.0/settings.ini");
+        if let Ok(raw) = std::fs::read_to_string(ini) {
+            for line in raw.lines() {
+                let l = line.trim().to_ascii_lowercase();
+                if l.starts_with("gtk-application-prefer-dark-theme") {
+                    return l.ends_with("=true") || l.ends_with("=1");
+                }
+            }
+        }
+    }
+    // GTK_THEME=Adwaita:dark style env override.
+    matches!(std::env::var("GTK_THEME"), Ok(t) if t.ends_with(":dark") || t.ends_with("-dark"))
+}
+
+// ---------------------------------------------------------------------------
+// S18: the accent color wheel (HSV disc → seed ARGB)
+// ---------------------------------------------------------------------------
+
+/// The embedded 256×256 HSV wheel (RGBA8; `scripts/make_color_wheel.py`).
+/// Geometry contract — BOTH sides must stay in sync:
+/// - hue    = angle around the center, 0° RED at 12 o'clock, clockwise;
+/// - chroma = distance from the center (center ≈ white), full at the rim;
+/// - value  = 1.0 (TonalSpot keys on hue+chroma; the tone is normalized
+///   away by the palette generator).
+pub const COLOR_WHEEL_PX: u32 = 256;
+pub const COLOR_WHEEL_BYTES: &[u8] = include_bytes!("../../../assets/color_wheel.rgba");
+
+/// Draw size of the wheel in the Settings card (logical px).
+pub const COLOR_WHEEL_DRAW: f32 = 176.0;
+
+/// HSV → RGB (h in degrees 0..360, s/v in 0..1). Returns (r, g, b) 0..255.
+fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
+    let h = ((h % 360.0) + 360.0) % 360.0;
+    let c = v * s;
+    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
+    let m = v - c;
+    let (r, g, b) = match (h / 60.0) as u32 {
+        0 => (c, x, 0.0),
+        1 => (x, c, 0.0),
+        2 => (0.0, c, x),
+        3 => (0.0, x, c),
+        4 => (x, 0.0, c),
+        _ => (c, 0.0, x),
+    };
+    (
+        ((r + m) * 255.0).round().clamp(0.0, 255.0) as u8,
+        ((g + m) * 255.0).round().clamp(0.0, 255.0) as u8,
+        ((b + m) * 255.0).round().clamp(0.0, 255.0) as u8,
+    )
+}
+
+/// RGB → HSV (h in degrees 0..360, s/v in 0..1).
+fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
+    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
+    let max = r.max(g).max(b);
+    let min = r.min(g).min(b);
+    let d = max - min;
+    let h = if d == 0.0 {
+        0.0
+    } else if (max - r).abs() < f32::EPSILON {
+        60.0 * ((g - b) / d).rem_euclid(6.0)
+    } else if (max - g).abs() < f32::EPSILON {
+        60.0 * ((b - r) / d + 2.0)
+    } else {
+        60.0 * ((r - g) / d + 4.0)
+    };
+    let s = if max == 0.0 { 0.0 } else { d / max };
+    (h, s, max)
+}
+
+/// A point in the wheel's LOCAL square (top-left origin, side = the draw
+/// size) → the seed ARGB under it. Outside the rim clamps to the rim at
+/// that angle (the center of the edge/corner direction); the exact
+/// center (radius 0) is the white seed.
+pub fn wheel_seed_at(p: iced::Point, side: f32) -> u32 {
+    let cx = p.x - side / 2.0;
+    let cy = p.y - side / 2.0;
+    // Screen y grows DOWN; the wheel's hue grows CLOCKWISE from 12
+    // o'clock, so (dx, dy) already gives clockwise-positive angles.
+    let (dx, dy) = (cx, cy);
+    let r = (dx * dx + dy * dy).sqrt().clamp(0.0, side / 2.0);
+    let sat = if side <= 0.0 {
+        0.0
+    } else {
+        (r / (side / 2.0)).clamp(0.0, 1.0)
+    };
+    let hue = if r < f32::EPSILON {
+        0.0
+    } else {
+        dy.atan2(dx).to_degrees() + 90.0 // 12 o'clock = 0°
+    };
+    let (rr, gg, bb) = hsv_to_rgb(hue, sat, 1.0);
+    0xFF00_0000 | (rr as u32) << 16 | (gg as u32) << 8 | bb as u32
+}
+
+/// The inverse map (for the selection marker): a seed's position in the
+/// wheel's local square. Preset swatch seeds land on their wheel spot.
+pub fn wheel_marker_position(seed: u32, side: f32) -> iced::Point {
+    let (h, s, _) = rgb_to_hsv(
+        ((seed >> 16) & 0xFF) as u8,
+        ((seed >> 8) & 0xFF) as u8,
+        (seed & 0xFF) as u8,
+    );
+    let r = s * side / 2.0;
+    let rad = (h - 90.0).to_radians();
+    iced::Point::new(side / 2.0 + rad.cos() * r, side / 2.0 + rad.sin() * r)
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
@@ -481,6 +675,7 @@ mod tests {
         let mut t = MtTheme::from_settings(&SettingsView {
             theme_seed: 0x4285F4,
             dark_mode: false,
+            theme_mode: "light".into(),
             system_proxy: true,
             merge_policy: "failover".into(),
             autostart: false,
@@ -547,4 +742,103 @@ mod tests {
             text("hello").color(t.scheme.primary);
         let _el2 = tinted_text("hi", t.scheme.on_background);
     }
+
+    /// S18: the System env seam decides first (rig determinism).
+    #[test]
+    fn system_prefers_dark_env_override_wins() {
+        // SAFETY of env mutation: tests in this module are single-threaded
+        // per process feature set; the var is restored before return.
+        unsafe { std::env::set_var("MT_THEME_SYSTEM_DARK", "1") };
+        assert!(system_prefers_dark());
+        unsafe { std::env::set_var("MT_THEME_SYSTEM_DARK", "true") };
+        assert!(system_prefers_dark());
+        unsafe { std::env::set_var("MT_THEME_SYSTEM_DARK", "0") };
+        assert!(!system_prefers_dark());
+        unsafe { std::env::set_var("MT_THEME_SYSTEM_DARK", "") };
+        assert!(!system_prefers_dark());
+        unsafe { std::env::remove_var("MT_THEME_SYSTEM_DARK") };
+    }
+
+    /// S18 wheel geometry: the cardinal spots are the hues the asset
+    /// generator draws (0° red at 12 o'clock, clockwise).
+    #[test]
+    fn wheel_seed_matches_cardinal_hues() {
+        let s = COLOR_WHEEL_DRAW;
+        let c = s / 2.0;
+        // 12 o'clock = red at full saturation.
+        assert_eq!(wheel_seed_at(iced::Point::new(c, 0.0), s), 0xFFFF_0000);
+        // 3 o'clock = 90° = chartreuse (#80FF00 at v=1).
+        assert_eq!(wheel_seed_at(iced::Point::new(s, c), s), 0xFF80_FF00);
+        // 6 o'clock = 180° = cyan.
+        assert_eq!(wheel_seed_at(iced::Point::new(c, s), s), 0xFF00_FFFF);
+        // 9 o'clock = 270° = violet (#8000FF at v=1).
+        assert_eq!(wheel_seed_at(iced::Point::new(0.0, c), s), 0xFF80_00FF);
+        // center = white.
+        assert_eq!(wheel_seed_at(iced::Point::new(c, c), s), 0xFFFF_FFFF);
+        // half-way out, 12 o'clock: s=0.5 red → #FF8080.
+        assert_eq!(
+            wheel_seed_at(iced::Point::new(c, c - s / 4.0), s),
+            0xFFFF_8080
+        );
+    }
+
+    /// Outside the disc clamps to the rim; the seed stays a legal ARGB.
+    #[test]
+    fn wheel_seed_clamps_outside_points_to_the_rim() {
+        let s = COLOR_WHEEL_DRAW;
+        let far = wheel_seed_at(iced::Point::new(-50.0, s / 2.0), s);
+        assert_eq!(far, 0xFF80_00FF, "9 o'clock rim (full sat at hue 270)");
+    }
+
+    /// The inverse map: a picked point's seed lands its marker back at
+    /// the same spot, and preset swatches sit inside the disc.
+    #[test]
+    fn wheel_marker_roundtrips_picked_points() {
+        let s = COLOR_WHEEL_DRAW;
+        for p in [
+            iced::Point::new(s / 2.0, 4.0),
+            iced::Point::new(s - 12.0, s / 2.0),
+            iced::Point::new(s * 0.3, s * 0.7),
+            iced::Point::new(s / 2.0, s / 2.0),
+        ] {
+            let seed = wheel_seed_at(p, s);
+            let back = wheel_marker_position(seed, s);
+            assert!(
+                (back.x - p.x).abs() < 1.0 && (back.y - p.y).abs() < 1.0,
+                "p {p:?} seed {seed:#x} → {back:?}"
+            );
+        }
+        // The Material baseline purple preset (a mid-saturation violet)
+        // lands inside the disc, off-center.
+        let m = wheel_marker_position(0x6750A4, s);
+        let d = ((m.x - s / 2.0).powi(2) + (m.y - s / 2.0).powi(2)).sqrt();
+        assert!(d > 4.0 && d < s / 2.0, "marker {m:?} dist {d}");
+    }
+
+    /// HSV helpers: pure channel edges and the grayscale center.
+    #[test]
+    fn hsv_conversions_roundtrip_primaries() {
+        for &(r, g, b) in &[
+            (255u8, 0u8, 0u8),
+            (0, 255, 0),
+            (0, 0, 255),
+            (255, 255, 0),
+            (255, 255, 255),
+            (18, 52, 86),
+        ] {
+            let (h, s, v) = rgb_to_hsv(r, g, b);
+            let (r2, g2, b2) = hsv_to_rgb(h, s, v);
+            assert_eq!((r, g, b), (r2, g2, b2), "rgb {r:?} via ({h}, {s}, {v})");
+        }
+        assert_eq!(rgb_to_hsv(80, 80, 80).1, 0.0, "gray has no saturation");
+    }
+
+    /// The embedded wheel asset is the size the contract promises.
+    #[test]
+    fn wheel_asset_dimensions() {
+        assert_eq!(
+            COLOR_WHEEL_BYTES.len(),
+            (COLOR_WHEEL_PX * COLOR_WHEEL_PX * 4) as usize
+        );
+    }
 }
diff --git a/crates/mt-app/src/tray.rs b/crates/mt-app/src/tray.rs
index 15a1188..f8aa50b 100644
--- a/crates/mt-app/src/tray.rs
+++ b/crates/mt-app/src/tray.rs
@@ -166,8 +166,21 @@ pub fn action_for_id(ids: &[(u16, QuickAction)], id: u16) -> Option<QuickAction>
     ids.iter().find(|(i, _)| *i == id).map(|(_, a)| a.clone())
 }
 
+/// The release version carried in tray tooltips (workspace-inherited —
+/// the same number the titlebar shows and the release zip carries).
+fn app_version() -> &'static str {
+    env!("CARGO_PKG_VERSION")
+}
+
+/// Line separator inside NIF_TIP tooltips: Windows' tooltip control
+/// breaks lines on CR/LF (the classic SpeedFan/qBittorrent multi-line
+/// tray tooltips). Counted against the 128-unit budget by the tests.
+const TIP_EOL: &str = "\r\n";
+
 /// Tray tooltip (NIF_TIP caps at 128 UTF-16 units incl. NUL; the caller
 /// may pass any string — the FFI side truncates safely).
+///
+/// S18 format: line 1 = app name + version, line 2 = the state word.
 pub fn tooltip(state: ServiceState, active_tag: &str) -> String {
     let word = match state {
         ServiceState::Connected => "connected",
@@ -176,23 +189,27 @@ pub fn tooltip(state: ServiceState, active_tag: &str) -> String {
         ServiceState::Disconnected => "disconnected",
     };
     if active_tag.is_empty() {
-        format!("MultiTunnel — {word}")
+        format!("MultiTunnel v{}{TIP_EOL}{word}", app_version())
     } else {
-        format!("MultiTunnel — {word} [{active_tag}]")
+        format!(
+            "MultiTunnel v{}{TIP_EOL}{word} [{active_tag}]",
+            app_version()
+        )
     }
 }
 
-/// S16: the hover tooltip while CONNECTED — live speeds + session totals
-/// (the user's ask: "current upload/download speed and the total
-/// upload/download volume so far"). Single line, well under the 128-unit
-/// NIF_TIP cap even with TB-scale numbers (≈70 units worst case).
+/// S18: the hover tooltip while CONNECTED — the user's exact 3-line ask:
+/// line 1 the app name with the version, line 2 the total transferred
+/// volume, line 3 the current download/upload speed. Well under the
+/// 128-unit NIF_TIP cap even with TB-scale numbers (≈70 units worst case).
 pub fn tooltip_live(up_bps: u64, down_bps: u64, total_up: u64, total_down: u64) -> String {
     format!(
-        "MultiTunnel — ↓{} ↑{} • Σ↓{} ↑{}",
-        crate::util::fmt_bps(down_bps),
-        crate::util::fmt_bps(up_bps),
+        "MultiTunnel v{}{TIP_EOL}Total ↓{} ↑{}{TIP_EOL}↓{} ↑{}",
+        app_version(),
         crate::util::fmt_bytes(total_down),
         crate::util::fmt_bytes(total_up),
+        crate::util::fmt_bps(down_bps),
+        crate::util::fmt_bps(up_bps),
     )
 }
 
@@ -511,24 +528,43 @@ mod tests {
 
     #[test]
     fn tooltip_states() {
+        let v = env!("CARGO_PKG_VERSION");
         assert_eq!(
             tooltip(ServiceState::Connected, "VM-1"),
-            "MultiTunnel — connected [VM-1]"
+            format!("MultiTunnel v{v}\r\nconnected [VM-1]")
         );
         assert_eq!(
             tooltip(ServiceState::Disconnected, ""),
-            "MultiTunnel — disconnected"
+            format!("MultiTunnel v{v}\r\ndisconnected")
         );
         assert_eq!(
             tooltip(ServiceState::Connecting, ""),
-            "MultiTunnel — connecting…"
+            format!("MultiTunnel v{v}\r\nconnecting…")
         );
+        // Every state stays inside the 128-unit NIF_TIP budget.
+        for (st, tag) in [
+            (ServiceState::Connected, "a-very-long-profile-tag-name"),
+            (ServiceState::Failed, ""),
+        ] {
+            let tip = tooltip(st, tag);
+            assert!(tip.encode_utf16().count() < 128, "tip: {tip:?}");
+        }
     }
 
     #[test]
     fn tooltip_live_formats_speeds_and_totals() {
+        let v = env!("CARGO_PKG_VERSION");
         let tip = tooltip_live(1_234_567, 9_876_543, 45_000_000, 3_200_000_000);
-        assert_eq!(tip, "MultiTunnel — ↓9.4 MB/s ↑1.2 MB/s • Σ↓3.0 GB ↑42.9 MB");
+        assert_eq!(
+            tip,
+            format!("MultiTunnel v{v}\r\nTotal ↓3.0 GB ↑42.9 MB\r\n↓9.4 MB/s ↑1.2 MB/s")
+        );
+        // The user's exact 3-line shape: name+version / totals / speeds.
+        let lines: Vec<&str> = tip.split("\r\n").collect();
+        assert_eq!(lines.len(), 3);
+        assert!(lines[0].starts_with("MultiTunnel v"));
+        assert!(lines[1].starts_with("Total ↓"));
+        assert!(lines[2].starts_with("↓") && lines[2].ends_with("/s"));
         // Always inside the 128-unit NIF_TIP budget (127 + NUL).
         let worst = tooltip_live(u64::MAX / 8, u64::MAX / 8, u64::MAX, u64::MAX);
         assert!(worst.encode_utf16().count() < 128, "tip: {worst}");
diff --git a/crates/mt-bond/src/endpoint.rs b/crates/mt-bond/src/endpoint.rs
index d158fe5..4fbb5a8 100755
--- a/crates/mt-bond/src/endpoint.rs
+++ b/crates/mt-bond/src/endpoint.rs
@@ -228,6 +228,25 @@ fn carrier_score(slot: &CarrierSlot) -> f64 {
     slot.assign_bps / slot.deliver_bps.max(PROBE_BPS)
 }
 
+/// S18 (ECF, Lim et al. 2017): the EARLIEST-COMPLETION estimate for one
+/// frame on this carrier, in milliseconds — the sampled RTT plus the time
+/// the already-queued backlog needs to drain at the peer-reported delivery
+/// rate. Scheduling by this (instead of raw RTT) stops the pile-up the
+/// classic minRTT scheduler suffers when every frame lands on the
+/// low-RTT path regardless of its backlog: a low-RTT carrier with a deep
+/// queue LOSES to a slightly-slower one that is empty.
+fn ecf_completion_ms(slot: &CarrierSlot) -> f64 {
+    let drain_ms = 1000.0 * slot.queued as f64 / slot.deliver_bps.max(PROBE_BPS);
+    f64::from(slot.rtt_ms) + drain_ms
+}
+
+/// S18 (BLEST, Ferlin et al. 2016): a carrier whose writer queue needs
+/// longer than this to drain is BLOCKED for the `Balanced` picker — the
+/// frame that would ride it stalls the peer's reorder buffer
+/// (head-of-line blocking) while the same frame on a faster carrier
+/// completes sooner. BLEST's send-window guard, expressed in drain time.
+pub const BALANCED_DRAIN_CAP_MS: f64 = 250.0;
+
 /// A carrier whose writer queue exceeds this many BYTES counts as
 /// SATURATED for the `Priority` policy — its socket provably cannot keep
 /// up with the offered rate. 512 KiB ≈ 8 max-size frames: a healthy
@@ -290,6 +309,20 @@ fn pick_carrier_index(
             Some(s.idx)
         }
         mt_net::MergePolicy::LowestLatency => {
+            // S18 (ECF): with delivery-rate reports, "lowest latency" is
+            // EARLIEST COMPLETION — RTT + queue drain time — so a deep
+            // backlog on the fastest path correctly loses to an empty
+            // slower one. Without reports this stays the S11 sampled-RTT
+            // behavior (known beats unknown, ties keep the earlier slot).
+            if slots.iter().any(|s| s.deliver_bps > 0.0) {
+                let mut best = 0;
+                for i in 1..slots.len() {
+                    if ecf_completion_ms(&slots[i]) < ecf_completion_ms(&slots[best]) {
+                        best = i;
+                    }
+                }
+                return Some(slots[best].idx);
+            }
             let mut best = 0;
             for i in 1..slots.len() {
                 let (b, c) = (slots[best], slots[i]);
@@ -313,14 +346,35 @@ fn pick_carrier_index(
                 *cursor = cursor.wrapping_add(1);
                 return Some(slots[pick].idx);
             }
-            // Capacity-aware: least relative load; ties rotate.
+            // Capacity-aware: least relative load; ties rotate. S18 (BLEST,
+            // Ferlin et al. 2016): among the carriers TIED at the minimum
+            // score, one whose writer queue needs > BALANCED_DRAIN_CAP_MS
+            // to drain is skipped — scheduling onto it delays the frame
+            // behind the backlog and stalls the peer's reorder buffer
+            // (head-of-line blocking) while a same-score carrier is empty.
+            // The refinement only narrows the tied set: when every tied
+            // carrier is backed up (or a single leader holds the minimum),
+            // the S14 semantics stand untouched — a deep queue alone never
+            // disqualifies the score leader, because queue depth is not a
+            // saturation signal (the S14 lesson).
             let min = slots
                 .iter()
                 .map(carrier_score)
                 .fold(f64::INFINITY, f64::min);
-            let tied: Vec<usize> = (0..slots.len())
+            let mut tied: Vec<usize> = (0..slots.len())
                 .filter(|&i| (carrier_score(&slots[i]) - min).abs() < f64::EPSILON)
                 .collect();
+            let eligible: Vec<usize> = tied
+                .iter()
+                .copied()
+                .filter(|&i| {
+                    1000.0 * slots[i].queued as f64 / slots[i].deliver_bps.max(PROBE_BPS)
+                        <= BALANCED_DRAIN_CAP_MS
+                })
+                .collect();
+            if !eligible.is_empty() {
+                tied = eligible;
+            }
             let pick = tied[(*cursor % tied.len() as u64) as usize];
             *cursor = cursor.wrapping_add(1);
             Some(slots[pick].idx)
@@ -1739,4 +1793,99 @@ mod scheduling_tests {
         ];
         assert_eq!(pick1(mt_net::MergePolicy::Balanced, &slots), Some(0));
     }
+
+    /// S18 (BLEST): among EQUAL-score carriers, one whose writer queue
+    /// needs > BALANCED_DRAIN_CAP_MS to drain is skipped — the frame rides
+    /// the empty same-score carrier instead of waiting behind the backlog
+    /// (HoL avoidance at the peer's reorder buffer).
+    #[test]
+    fn balanced_blest_guard_skips_slow_draining_tied_carrier() {
+        // Equal relative load (both 1.0, well above the probe floor).
+        // Carrier 0: 1 MiB queued on a 2 MiB/s path = 500 ms drain
+        // (> cap) → BLOCKED. Carrier 1: empty queue → picked.
+        let slots = [
+            cap_slot(0, 1_048_576, 30, 2_048_000.0, 2_048_000.0),
+            cap_slot(1, 0, 40, 2_048_000.0, 2_048_000.0),
+        ];
+        assert_eq!(
+            pick1(mt_net::MergePolicy::Balanced, &slots),
+            Some(1),
+            "tied score: the empty-queue carrier must win"
+        );
+        // Both blocked → the score leader still takes the frame (the bond
+        // must not stall; the leader is always eligible by construction).
+        let both = [
+            cap_slot(0, 2_097_152, 30, 2_048_000.0, 2_048_000.0),
+            cap_slot(1, 1_048_576, 40, 1_024_000.0, 1_024_000.0),
+        ];
+        assert_eq!(
+            pick1(mt_net::MergePolicy::Balanced, &both),
+            Some(0),
+            "both blocked → the score leader (0: 1.0 vs 1: 1.0, first tie) still picks"
+        );
+    }
+
+    /// S18 (BLEST, the S14 reconciliation): a deep queue alone never
+    /// disqualifies the SCORE LEADER — queue depth is not a saturation
+    /// signal (the S14 lesson); only near-equal scores get the guard.
+    #[test]
+    fn balanced_blest_guard_never_bypasses_the_score_leader() {
+        // Leader (score 0.1) has the DEEP queue (500 ms drain); the
+        // follower (score 1.6) is empty. The leader keeps the frame.
+        let slots = [
+            cap_slot(
+                0,
+                1_048_576,
+                30,
+                0.2 * 1024.0 * 1024.0,
+                2.0 * 1024.0 * 1024.0,
+            ),
+            cap_slot(1, 0, 40, 1.6 * 1024.0 * 1024.0, 1.0 * 1024.0 * 1024.0),
+        ];
+        assert_eq!(
+            pick1(mt_net::MergePolicy::Balanced, &slots),
+            Some(0),
+            "score leader with a deep queue still wins (S14 semantics)"
+        );
+    }
+
+    /// S18 (ECF): with delivery reports, LowestLatency = earliest
+    /// completion — RTT plus queue-drain time — so a low-RTT carrier with
+    /// a deep backlog correctly loses to an empty slower one.
+    #[test]
+    fn lowest_latency_ecf_prefers_earliest_completion() {
+        // Carrier 0: 20 ms RTT but 1 MiB queued on 2 MiB/s = 500 ms drain
+        // → completion ≈ 520 ms. Carrier 1: 60 ms RTT, empty → 60 ms.
+        let slots = [
+            cap_slot(0, 1_048_576, 20, 2_048_000.0, 2_048_000.0),
+            cap_slot(1, 0, 60, 2_048_000.0, 2_048_000.0),
+        ];
+        assert_eq!(
+            pick1(mt_net::MergePolicy::LowestLatency, &slots),
+            Some(1),
+            "empty 60 ms path beats backed-up 20 ms path (520 ms completion)"
+        );
+        // Same shape with shallow queues → raw RTT decides (the classic
+        // behavior): 20 ms beats 60 ms.
+        let shallow = [
+            cap_slot(0, 10_000, 20, 2_048_000.0, 2_048_000.0),
+            cap_slot(1, 0, 60, 2_048_000.0, 2_048_000.0),
+        ];
+        assert_eq!(pick1(mt_net::MergePolicy::LowestLatency, &shallow), Some(0));
+        // Mixed reports (one silent): the ECF arm needs rates; the
+        // sampled-RTT arm handles unknowns (existing behavior).
+        let mixed = [cap_slot(0, 0, 25, 1_000_000.0, 1_000_000.0), slot(1, 0, 90)];
+        assert_eq!(pick1(mt_net::MergePolicy::LowestLatency, &mixed), Some(0));
+    }
+
+    /// S18: the ECF completion estimate itself (RTT + queue/rate).
+    #[test]
+    fn ecf_completion_is_rtt_plus_drain_time() {
+        let s = cap_slot(0, 300_000, 25, 1_200_000.0, 1_200_000.0);
+        // 300000/1200000 s = 250 ms; + 25 ms RTT = 275 ms.
+        assert!((ecf_completion_ms(&s) - 275.0).abs() < 1e-6);
+        // Unreported rate → the probe floor drains the queue: 32 KiB/s.
+        let silent = cap_slot(1, 32_768, 10, 0.0, 0.0);
+        assert!((ecf_completion_ms(&silent) - 1010.0).abs() < 1e-6);
+    }
 }
diff --git a/crates/mt-service/src/lib.rs b/crates/mt-service/src/lib.rs
index e08536a..24c343d 100755
--- a/crates/mt-service/src/lib.rs
+++ b/crates/mt-service/src/lib.rs
@@ -97,6 +97,14 @@ pub struct RuleView {
 pub struct SettingsView {
     pub theme_seed: u32,
     pub dark_mode: bool,
+    /// S18: how the app resolves light vs dark — "system" | "light" |
+    /// "dark". Absent in files written by v0.11 and older → the legacy
+    /// [`SettingsView::dark_mode`] bool decides (true → Dark, false →
+    /// Light); see [`SettingsView::theme_mode`]. `dark_mode` itself stays
+    /// serialized (the GUI keeps it at the EFFECTIVE state on every save)
+    /// so a v0.11-or-older binary still reads future files.
+    #[serde(default)]
+    pub theme_mode: String,
     pub system_proxy: bool,
     pub merge_policy: String,
     pub autostart: bool,
@@ -147,6 +155,69 @@ pub struct SettingsView {
     pub always_on_top: bool,
 }
 
+/// S18: how the app resolves light vs dark appearance. Serialized as
+/// the lowercase string in [`SettingsView::theme_mode`] (the existing
+/// merge_policy String convention; the GUI owns the parse).
+#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
+pub enum ThemeMode {
+    /// Follow the OS preference (re-polled while the app runs).
+    System,
+    /// Always light.
+    Light,
+    /// Always dark.
+    Dark,
+}
+
+impl ThemeMode {
+    /// The serialized form (also the pick_list value).
+    pub fn as_str(self) -> &'static str {
+        match self {
+            Self::System => "system",
+            Self::Light => "light",
+            Self::Dark => "dark",
+        }
+    }
+
+    /// Parse a stored `theme_mode` string. Unknown/absent values fall
+    /// back to the LEGACY `dark_mode` bool (files written by v0.11 and
+    /// older carry no `theme_mode` key).
+    pub fn parse(s: &str, legacy_dark_mode: bool) -> Self {
+        match s {
+            "system" => Self::System,
+            "light" => Self::Light,
+            "dark" => Self::Dark,
+            // Absent (serde default "") or garbage: keep what the legacy
+            // bool said so an upgrade never flips the user's theme.
+            _ if legacy_dark_mode => Self::Dark,
+            _ => Self::Light,
+        }
+    }
+
+    /// The effective dark flag given the OS preference (`System` mode).
+    #[must_use]
+    pub fn resolves_dark(self, system_prefers_dark: bool) -> bool {
+        match self {
+            Self::System => system_prefers_dark,
+            Self::Light => false,
+            Self::Dark => true,
+        }
+    }
+}
+
+impl SettingsView {
+    /// S18: the parsed appearance mode (legacy `dark_mode` decides when
+    /// the stored string is absent/invalid).
+    pub fn theme_mode(&self) -> ThemeMode {
+        ThemeMode::parse(&self.theme_mode, self.dark_mode)
+    }
+
+    /// S18: the dark flag the GUI should apply right now.
+    #[must_use]
+    pub fn effective_dark(&self, system_prefers_dark: bool) -> bool {
+        self.theme_mode().resolves_dark(system_prefers_dark)
+    }
+}
+
 fn default_bond_mode() -> String {
     "per-connection".to_string()
 }
@@ -692,6 +763,7 @@ mod close_to_tray_migration_tests {
             Self {
                 theme_seed: 0,
                 dark_mode: false,
+                theme_mode: String::new(),
                 system_proxy: false,
                 merge_policy: "balanced".into(),
                 autostart: false,
@@ -746,3 +818,60 @@ mod always_on_top_migration_tests {
         assert!(back.always_on_top);
     }
 }
+
+#[cfg(test)]
+mod theme_mode_migration_tests {
+    use super::*;
+
+    /// S18: settings.json written by v0.11 and older (no `theme_mode`
+    /// key) loads with the mode derived from the legacy `dark_mode` bool —
+    /// an upgrade never flips the user's appearance (D8 version-free
+    /// migration via serde default + the legacy fallback).
+    #[test]
+    fn old_settings_derive_mode_from_legacy_dark_mode() {
+        let base = r#"{
+            "theme_seed": 42,
+            "dark_mode": {},
+            "system_proxy": false,
+            "merge_policy": "balanced",
+            "autostart": false
+        }"#;
+        let dark: SettingsView =
+            serde_json::from_str(&base.replace("{}", "true")).expect("legacy dark loads");
+        assert_eq!(dark.theme_mode(), ThemeMode::Dark);
+        assert!(dark.effective_dark(false));
+        let light: SettingsView =
+            serde_json::from_str(&base.replace("{}", "false")).expect("legacy light loads");
+        assert_eq!(light.theme_mode(), ThemeMode::Light);
+        assert!(!light.effective_dark(true));
+    }
+
+    /// The three S18 modes round-trip through JSON and resolve correctly
+    /// against both OS preferences.
+    #[test]
+    fn theme_mode_strings_roundtrip_and_resolve() {
+        for (mode, sys_dark, want) in [
+            (ThemeMode::System, true, true),
+            (ThemeMode::System, false, false),
+            (ThemeMode::Light, true, false),
+            (ThemeMode::Dark, false, true),
+        ] {
+            let mut v = SettingsView::default_settings_for_tests();
+            v.theme_mode = mode.as_str().to_string();
+            let json = serde_json::to_string(&v).expect("serialize");
+            assert!(json.contains(&format!("\"theme_mode\":\"{}\"", mode.as_str())));
+            let back: SettingsView = serde_json::from_str(&json).expect("reload");
+            assert_eq!(back.theme_mode(), mode);
+            assert_eq!(back.effective_dark(sys_dark), want);
+        }
+    }
+
+    /// Garbage in the stored string keeps the legacy bool's answer
+    /// (never a panic, never a silent theme flip).
+    #[test]
+    fn invalid_theme_mode_falls_back_to_legacy_bool() {
+        assert_eq!(ThemeMode::parse("neon", true), ThemeMode::Dark);
+        assert_eq!(ThemeMode::parse("neon", false), ThemeMode::Light);
+        assert_eq!(ThemeMode::parse("", true), ThemeMode::Dark);
+    }
+}
diff --git a/docs/research/2026-11-21-bonding-performance-survey.md b/docs/research/2026-11-21-bonding-performance-survey.md
new file mode 100644
index 0000000..6e48772
--- /dev/null
+++ b/docs/research/2026-11-21-bonding-performance-survey.md
@@ -0,0 +1,150 @@
+# Bonding & tunneling performance — literature survey (S18, 2026-11-21)
+
+Scope: what prominent multipath-transport research says about improving
+throughput, latency and stability on heterogeneous paths (Wi-Fi +
+Ethernet + cellular — exactly MultiTunnel's bonded-carrier setting), and
+how each finding maps onto this codebase. Two findings shipped in
+v0.12.0 (BLEST tie-guard, ECF completion scheduling); the rest is a
+prioritized roadmap with the concrete mechanism we would build.
+
+MultiTunnel's stream bonding (mt-bond) is a user-space analog of MPTCP:
+frames of one virtual stream are spread across N carrier TCP tunnels and
+re-sequenced by mt-merge-server. The MPTCP scheduler literature transfers
+almost 1:1 because the failure modes are the same — head-of-line (HoL)
+blocking at the receiver, one-backup-all-backup coupling, and scheduling
+onto a path whose send buffer is full.
+
+## 1. Shipped in v0.12.0
+
+### 1.1 BLEST — avoid scheduling onto a backed-up path (Ferlin et al., 2016)
+
+Paper: *BLEST: Blocking Estimation-based MPTCP Scheduler for Heterogeneous
+Networks*, IFIP Networking 2016 (Ö. Alay, T. Narten, et al. group;
+S. Ferlin first author).
+https://ieeexplore.ieee.org/document/7497206
+https://www.computer.org/csdl/proceedings-article/ifip-networking/2016/07497206/12OmNxisQT9
+
+Finding: the default MPTCP scheduler keeps filling the send window of the
+slow path whenever the fast path's window is momentarily full; the
+receiver then waits on the slow path's straggler segment — HoL blocking —
+and aggregate throughput collapses below the single-path baseline. BLEST
+estimates whether sending on a subflow will block and, if so, buffers
+briefly on the faster subflow instead.
+
+MultiTunnel mapping (IMPLEMENTED, `mt-bond/src/endpoint.rs`): among
+carriers TIED at the minimum relative-load score, a carrier whose writer
+queue needs > `BALANCED_DRAIN_CAP_MS` (250 ms) to drain at its
+peer-reported delivery rate is skipped — the frame rides an empty
+same-score carrier instead of queuing behind a backlog that stalls the
+merge daemon's reorder buffer. The refinement only narrows the tied set:
+a deep queue alone never disqualifies the score leader (the S14 lesson —
+queue depth is not a saturation signal; relative load is).
+
+### 1.2 ECF — schedule by earliest completion, not raw RTT (Lim et al., 2017)
+
+Paper: *ECF: An MPTCP Path Scheduler to Manage Heterogeneous Paths*
+(Y. Lim, E. M. Nahum, et al., 2017).
+https://dl.acm.org/doi/10.1145/3143314.3078552
+(Cited by ~358.)
+
+Finding: the classic minRTT scheduler piles every segment onto the
+lowest-RTT path regardless of its congestion window / backlog; ECF
+computes the expected completion time of a segment per path from RTT and
+available window and picks the minimum — markedly better fast-path
+utilization and completion times on Wi-Fi+LTE style asymmetry.
+
+MultiTunnel mapping (IMPLEMENTED): `lowest-latency` now means earliest
+completion — `rtt_ms + 1000 × queued / max(deliver_bps, probe floor)`
+— whenever delivery-rate reports exist (S14 peer reporting). A 20 ms
+carrier with a 500 ms backlog correctly loses to an empty 60 ms one;
+without reports the policy stays the S11 sampled-RTT behavior.
+
+## 2. Roadmap (prioritized, with the concrete mechanism)
+
+### 2.1 Redundant mode for interactive streams (Speedify 17 evidence; DAPS/DaMPS lineage)
+
+Speedify's Redundant Mode sends each packet on every connection
+simultaneously (up to 3) — first arrival wins; v17 added stream detection
+so live streams get priority inside that mode, plus faster failover
+detection. https://speedify.com/blog/release-notes/introducing-speedify-17
+Academic lineage: deadline-aware schedulers replicate time-critical data
+— DaMPS (CUHK) reports 27–149% deadline-adherence gains at comparable
+throughput: http://www.cs.cuhk.hk/~cslui/PUBLICATION/DaMPS.pdf
+
+Mechanism for mt-bond: a third bond mode ("redundant") — `send_data`
+broadcasts the frame on ALL live carriers; the merge daemon's reorder
+window already deduplicates by sequence number, so the change is almost
+entirely on the client side. Bandwidth cost = N×; restrict to a
+per-session opt-in (the SOCKS inbound marks streams, e.g. by target
+port or a "live" profile toggle). Highest user-visible win for calls.
+
+### 2.2 FEC parity frames across carriers (Speedify-style, Raptor-ish)
+
+Mechanism: every K data frames, emit one XOR (or systematic Reed–Solomon
+block) frame scheduled round-robin; the reorder buffer recovers a lost
+frame from parity instead of stalling until retransmission. Works with
+the existing frame sequencing; needs a frame-type flag and a small
+parity encoder (XOR over equal-length padded frames is enough for
+K≤8). Removes tail-latency spikes from single-carrier loss bursts
+without duplicating all traffic.
+
+### 2.3 Reinjection of stalled frames (MPTCP's reinject-on-timeout)
+
+mt-bond already tracks per-session unacked frames (seq → carrier,
+bytes). Mechanism: when a carrier is judged dead/very-slow (writer
+drain > several seconds), re-enqueue its unacked frames on the best
+other carrier (dedupe by seq at the daemon's reorder window). Today a
+dying carrier's in-flight frames stall the session until the 10 s stall
+timeout closes it; reinjection converts that hard failure into a
+sub-second blip.
+
+### 2.4 Delay-aware coupled rate management (wVegas / BALIA line)
+
+The coupled-CC family (LIA → OLIA → BALIA, plus delay-based wVegas)
+exists so one multipath flow does not compete with itself across paths —
+the aggregate converges instead of flapping capacity between subflows.
+Survey of the four Linux algorithms: arXiv 1812.03210
+(https://arxiv.org/abs/1812.03210); a 2024 IEEE comparison finds BALIA
+and wVegas the most responsive under congestion
+(https://ieeexplore.ieee.org/document/10783253).
+
+MultiTunnel analog: the S14 assign/deliver score already behaves like a
+crude coupled controller (equilibrium = capacity-proportional split).
+The literature suggests two refinements: (a) include the EWMA RTT trend
+(wVegas-style delay signal) in `carrier_score` so bufferbloat on one
+carrier is visible BEFORE its delivery rate drops; (b) dampen score
+oscillation with hysteresis (BALIA's coupled window moves in one
+direction at a time) to avoid flapping assignments on jittery reports.
+
+### 2.5 Per-path ACK clocking / RTT trend (MP-QUIC design lessons)
+
+Paper: *Multipath QUIC: Design and Evaluation*, CoNEXT 2017
+(Q. De Coninck, O. Bonaventure) — https://dl.acm.org/doi/10.1145/3143361.3143370
+(cited by ~525). Key structural ideas: per-path packet number spaces and
+ack frames (no cross-path HoL on the ACK clock), and path migration
+without connection reset.
+
+MultiTunnel already has per-carrier framing (no shared TCP), which is
+why the MP-QUIC lesson mostly translates as: keep the carrier PING RTT
+EWMA (S11) as the pacing signal and expose its TREND (rising fast =
+bufferbloat forming) to the scheduler score — feeding 2.4(a).
+
+### 2.6 Learning-assisted scheduling (ToN 2023) — watch item
+
+*An Online Learning Assisted Packet Scheduler for MPTCP* (Xing et al.,
+IEEE/ACM ToN 2023, USTC) learns scheduler behavior across network
+scenarios for consistently quality-guaranteed mobile service
+(http://staff.ustc.edu.cn/~kpxue/paper/ToN-MPTCP-YitaoXing-2023.10.pdf).
+Interesting direction, but a learned policy is hard to reason about in a
+safety-critical tunnel; revisit if the heuristic stack (S14 + S18)
+proves insufficient on real Wi-Fi+LTE traces.
+
+## 3. What we deliberately did NOT adopt
+
+- **Aggressive send-window prediction (BLEST's full window math)**: our
+  writer queue + peer delivery reports approximate the same signal with
+  less coupling to TCP internals we do not own (Xray owns the sockets).
+- **Pure redundant scheduling as the default**: N× bandwidth cost;
+  Speedify themselves position Redundant Mode for calls/streams only.
+- **New congestion controllers**: the carriers ride Xray TCP; CC belongs
+  to the OS. We steer demand (which carrier gets which frame), not supply.
diff --git a/docs/research/2026-11-21-ui-changelog-survey.md b/docs/research/2026-11-21-ui-changelog-survey.md
new file mode 100644
index 0000000..da21cd6
--- /dev/null
+++ b/docs/research/2026-11-21-ui-changelog-survey.md
@@ -0,0 +1,90 @@
+# UI survey — changelogs of similar applications (S18, 2026-11-21)
+
+Scope: what recent releases of comparable proxy/bonding clients added or
+fixed on the UI/UX surface, and how each item maps to MultiTunnel
+(already shipped / shipped in v0.12.0 / proposed). Sources are the
+projects' own release notes; every claim below is anchored to the linked
+page as it read on 2026-11-21.
+
+## Sources reviewed
+
+- **Clash Verge Rev** (Tauri-based Clash client) — releases page,
+  v2.5.9 / v2.5.10 (Nov 2026):
+  https://github.com/clash-verge-rev/clash-verge-rev/releases
+- **v2rayN** (Xray/sing-box GUI, the import-compatibility reference for
+  this project) — https://v2rayn.app + community changelog mirror.
+- **FlClash** (multi-platform Clash client) — releases page:
+  https://github.com/chen08209/FlClash/releases
+- **Hiddify** (sing-box GUI, mobile-first) — Google Play release notes:
+  https://play.google.com/store/apps/details?id=app.hiddify.com
+- **Speedify** (commercial channel-bonding VPN — the closest analog to
+  MultiTunnel's bonding) — Speedify 17 release notes:
+  https://speedify.com/blog/release-notes/introducing-speedify-17
+
+## Findings → MultiTunnel mapping
+
+### Already shipped in MultiTunnel (parity confirmed)
+
+| Feature (source) | MultiTunnel state |
+|---|---|
+| Latency tests per node; auto-select fastest (CVR, FlClash) | Servers screen per-profile latency test (S9) |
+| Live traffic speeds surfaced, incl. tray (CVR tray speed discussions, v2rayN traffic stats) | Titlebar live global rates + tray hover tooltip (S17/S18) |
+| Theme customization (FlClash themes, CVR custom CSS) | M3 seed theming; v0.12.0 adds the color WHEEL + system/light/dark mode |
+| Dark/light mode (all clients) | v0.12.0 adds System mode following the OS |
+| Log viewing with tail tracking (v2rayN, CVR) | Logs viewport + autoscroll + floating jump buttons (S17) |
+| Adapter/link visibility (Speedify connection list) | Status links card + per-adapter graphs (S10/S13) |
+
+### Shipped in v0.12.0 (this survey's direct asks)
+
+- **Tray hover = name+version / totals / live speeds** (v2rayN and
+  Speedify both surface session traffic in one glance; our previous
+  tooltip was a single dense line).
+- **Clear-background, larger tray/taskbar icons** (Speedify's icon
+  legibility at small sizes; the old opaque tile read as a blank square).
+- **Color wheel accent picker** (FlClash's theme work moved toward free
+  color choice rather than fixed palettes).
+- **System / Light / Dark mode** (universal in the 2025-2026 releases of
+  all five clients).
+
+### Proposed next (prioritized)
+
+1. **Per-link live status + Signal-style ping quality badges**
+   (Hiddify's headline home-screen rework: "live status for every proxy,
+   Signal-style ping indicators"). MultiTunnel has per-profile latency
+   on demand; a continuous 3-level badge (good/ok/bad) on Status link
+   rows + Servers rows is a small poll loop away, and directly answers
+   "which carrier is degraded" mid-session.
+2. **Exit-IP auto-refresh toggle** (CVR v2.5.9 added an "IP Info
+   auto-refresh" toggle). Our exit probe is manual; a Settings toggle +
+   10-min refresh while Connected is cheap and mirrors theirs.
+3. **Empty states** (FlClash: "new app glyphs, empty states"). Servers
+   and Logs render bare cards when empty; a centered hint + primary
+   action ("paste your first vless:// link") would orient new users.
+4. **Streaming/AI unlock detection** (CVR's test suite incl. "Muse AI
+   unlock detection") — out of scope for a private tunnel, but a
+   "does the tunnel actually reach X" one-click URL test list belongs in
+   a Diagnostics card; the exit probe already proves the model.
+5. **Update policy controls** (v2rayN: pick release channel/version;
+   CVR installer polish). mt-platform's update skeleton exists
+   (`latest_release_for(CARGO_PKG_VERSION)`); a Settings "check for
+   updates" button + channel choice is the UI half of that.
+6. **Traffic statistics durability** (v2rayN repeatedly fixing traffic
+   stats display). Our session_bytes reset each connect; persisting
+   cumulative per-profile totals in settings.json would give a
+   "lifetime" row on Servers without schema risk (new optional field).
+7. **Bufferbloat guard visibility** (Speedify 17 fixed over-aggressive
+   bufferbloat protection). When 2.4(a) of the bonding survey lands
+   (RTT trend in the score), surface a "latency rising" chip on the
+   affected adapter row — invisible protection is undervalued by users.
+
+### Deliberately not adopted
+
+- **TUN mode / virtual adapters** (CVR, FlClash): a system-wide packet
+  layer is a different risk class (driver-level, admin, AV friction);
+  the SOCKS/HTTP listeners + system proxy cover the use cases this
+  project targets.
+- **Unlock-test catalogs** (CVR's streaming/AI matrix): content-service
+  fingerprints rot quickly and imply a maintenance stream this
+  one-developer project should not take on.
+- **Custom CSS** (CVR): the M3 theme engine already exposes the levers
+  users actually change (accent, mode).
diff --git a/harness/memory/DECISIONS.md b/harness/memory/DECISIONS.md
index d388abc..a3f7c46 100755
--- a/harness/memory/DECISIONS.md
+++ b/harness/memory/DECISIONS.md
@@ -435,3 +435,80 @@ on tray right-click.
    ride TrayEvent → existing Message paths (ConnectToggle/SetActive) —
    no new service calls. Tooltip: "MultiTunnel — <state> [<profile>]",
    refreshed on tick only when changed.
+
+## D28 — S18: tray tooltip/icon rework, color wheel + theme modes, literature-driven scheduler refinements (2026-11-21)
+
+User asks: (1) tray hover = name+version / total volume / current speeds,
+three lines; (2) tray+taskbar icons with CLEAR backgrounds and a larger
+mark; (3) a color wheel in Settings for custom accent colors; (4) system /
+light / dark mode selection; (5) explore bonding performance from
+prominent scientific papers; (6) explore UI improvements from comparable
+apps' changelogs.
+
+1. TOOLTIP = 3 lines with CRLF separators (the Win32 tooltip control
+   breaks lines on \r\n — the SpeedFan/qBittorrent multi-line tray
+   precedent): line 1 `MultiTunnel v<CARGO_PKG_VERSION>`, line 2
+   `Total ↓x ↑y`, line 3 `↓x/s ↑y/s` (the S16 single-line form is gone;
+   not-connected states are 2 lines: name+version / state word). Worst
+   case ≈70 UTF-16 units — safely inside the 128-unit NIF_TIP budget,
+   pinned by tests.
+2. ICON ASSETS (make_logo_assets.py): all RUNTIME icons — icon.ico
+   (16/32/48 emblem + 256 lockup), icon_mono.ico, logo_64.rgba (the
+   window/taskbar icon) and logo_24.rgba (titlebar emblem) — now share
+   ONE treatment: flood-fill keyed-out TRANSPARENT background,
+   `contrast_boost` (hue-preserving luminance remap onto [56,240],
+   percentile-normalized like desaturate but per-channel scaled so hue
+   and chroma survive) and a tighter margin (0.08 → 0.04, the glyph
+   fills ~8% more of the tile). Rationale: the old connected icon was a
+   nearly-white opaque tile (92% of pixels >200 luminance —
+   indistinguishable from a blank square); the emblem itself carries
+   dark strokes (16% below 110), which the boost deepens for light
+   taskbars while the pastel body still pops on dark ones. Verified by
+   pixel stats (33% opaque, corners alpha 0, dark-stroke bands) and the
+   rig (titlebar emblem lum spread 114 on light / 212 on dark).
+3. COLOR WHEEL = precomputed 256×256 HSV disc asset
+   (make_color_wheel.py; hue = clockwise angle from 12 o'clock,
+   saturation = radius, V=1) shown at 176 logical px via
+   image::Handle; hit-testing + marker placement are the INVERSE pair
+   `wheel_seed_at`/`wheel_marker_position` in theme.rs (unit-tested
+   roundtrip, cardinal hues pinned). Interaction: mouse_area
+   on_press/on_move(Point)/on_release; press picks the color under the
+   cursor (the move that got the cursor there recorded the position),
+   drag re-derives the theme LIVE (no disk I/O), release persists via
+   the shared save_settings path; a global left-release listener
+   (listen_with, same subscription as the S16 cursor tracking) ends a
+   drag released OUTSIDE the wheel. The marker is a canvas overlay
+   (seed disc + white halo + on-surface ring). Presets + numeric seed
+   field stay.
+4. THEME MODES = `SettingsView::theme_mode: String` ("system"|"light"|
+   "dark", serde default "") + `dark_mode: bool` KEPT as the
+   forward-compat shadow (written at the effective state on every save,
+   so a v0.11 binary reading a v0.12 file still lands on the appearance
+   the user sees; an absent theme_mode parses from the legacy bool —
+   D8 version-free migration, pinned by tests). System resolution:
+   `MT_THEME_SYSTEM_DARK` env override (the rig seam) → Windows
+   AppsUseLightTheme registry (windows-sys Win32_System_Registry — same
+   crate, one feature) → gsettings color-scheme → GTK3 settings.ini →
+   GTK_THEME suffix → light. Re-polled every 5 ticks while in System
+   mode (a flip logs + re-themes live). The Settings UI replaces the
+   dark toggler with System/Light/Dark segmented buttons (selected =
+   filled) + a caption showing what System currently resolves to.
+5. SCHEDULER (papers → code, docs/research/2026-11-21-bonding-
+   performance-survey.md): (a) BLEST (Ferlin et al., IFIP Networking
+   2016) — among carriers TIED at the min relative-load score, one
+   whose queue drains slower than 250 ms at its reported rate is
+   skipped (HoL avoidance at the merge daemon's reorder buffer). The
+   guard ONLY narrows the tied set: the score leader always stays
+   eligible, because queue depth alone is not a saturation signal (the
+   S14 lesson — the deep-queue-but-headroom carrier in the S14 test
+   must keep winning; a naive BLEST filter would have regressed it and
+   DID fail its first test run). (b) ECF (Lim et al. 2017) —
+   LowestLatency with delivery reports = earliest completion
+   (rtt + 1000·queued/deliver): a 20 ms carrier with a 500 ms backlog
+   loses to an empty 60 ms one; without reports the S11 sampled-RTT
+   behavior stands. Roadmap (redundant mode, FEC parity, reinjection,
+   wVegas-style delay signal) documented with mechanisms in the survey.
+6. UI SURVEY (docs/research/2026-11-21-ui-changelog-survey.md): Clash
+   Verge Rev / v2rayN / FlClash / Hiddify / Speedify changelogs mapped
+   to shipped vs proposed (next: per-link live status badges, exit-IP
+   auto-refresh, empty states).
diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
index 6fa2751..5487b37 100755
--- a/harness/memory/LEDGER.md
+++ b/harness/memory/LEDGER.md
@@ -209,3 +209,6 @@
 2026-11-20T15:50Z | s17 | RIG VERIFIED + REAL BUG FOUND&FIXED: the floating card flush to the viewport corner sat inside the scrollable's SCROLLBAR GRAB ZONE — the rail swallowed the button clicks (repro: clicks dead-center on the ↑ pill at sf=1.0 did nothing while rail buttons/togglers worked; VLM ruler-zoom saw the scrollbar strip THROUGH the capsule). FIX: overlay padding right 26px — capsule clears the rail+grab, the rail renders separately again. RIG LESSONS: (a) coordinate guessing burned an hour — the connect toggler's pill is at y252-288 (track-arc scan: runs of 6<d<90 between text rows finds the pill; VLM y-estimates drift ~25px, ruler-zoom crops make it usable; XTEST cursor renders BLACK in ffmpeg x11grab shots — a 31-px diff cluster at the click point is the CURSOR, not a widget change); (b) full-window diffs vs boot: connect=18755px (state+card+graph+speeds), autoscroll toggle=4409, jumps=3686/6307, wheel=2845 — pixel thresholds now known for this window size; (c) VLM reads content reliably (log lines verbatim, speeds "↓ 10.9 MB/s ↑ 2.9 MB/s" live-ticking, "connecting: spinning up links" as the true first line after ↑) but must be prompted "plain text only, no HTML" or it hallucinates mockups. sf=1.0 AND sf=1.5 both verified end-to-end (version cluster x131-169 @1.0, speeds live, toggler flips, autoscroll on→off, top/bottom jumps, wheel through overlay, keepalive tail following).
 2026-11-20T16:15Z | s17 | RUNNER FLAKE ROUND 2 (verdict: fleet, not code — same class as the s16 v0.10.0 event): ci run 44766745503 attempt 1 failed mt-bond carrier_death_mid_stream (the s16-documented flake, byte-identical signature "bond send failed: no live bond carrier"); rerun attempt 2 failed a DIFFERENT test — multitunnel core_proc socks_probe_total_deadline_bounds_byte_dripping_proxy ("probe ran 211.66ms — failed before the budget", a timing-budget assertion); attempt 3 ALL GREEN. Corroborating green on the SAME commit: windows-latest ci, lint, cross-check, the sibling ci run 44766699760 (ubuntu, full suite), the release run's own build-linux-daemon leg, and local cargo test --workspace 433/0 x2. Neither failing test's crate was touched by S17 (mt-app GUI + Cargo.toml version only). PATTERN CONFIRMED: ubuntu runner fleet intermittently stalls IO/timing-sensitive tests (carrier liveness + deadline budgets); if a THIRD distinct test flakes, consider serializing the timing-sensitive set (MT_SERIAL_TESTS=1 style) instead of rerun-chasing.
 2026-11-20T17:05Z | s17 | v0.11.0 RELEASED: attempt 1's build-windows leg sat 68 min inside cargo test on a 100%-cold rust-cache (the version bump changed Cargo.lock; every other leg incl. build-linux-daemon green) — cancelled per the >45-min protocol (rust-cache@v2 saves in post-steps even on cancellation), attempt 2 completed SUCCESS in ~6 min WARM exactly as predicted (the s9 pattern, now proven twice). Assets verified by download+unzip (download/v0110-verify/): multitunnel-v0.11.0-windows-x64.zip 6,314,832 B (10 entries, integrity OK; multitunnel.exe PE32+ console 13.26 MB + multitunnel-tray.exe PE32+ GUI 2.95 MB; README carries the v0.9-v0.11 section, QUICKSTART the v0.11 note, server/ scripts included) + mt-merge-server {x86_64 1,824,296 B, aarch64 1,860,432 B} ELFs. CI on the same commit: green on attempt 3 after the two-test runner-flake round (see 16:15Z entry). S17 CLOSED — the three asks shipped: version next to the app name in the titlebar, live global down/up speeds on the titlebar's right, Logs tab floating top/bottom jump buttons + autoscroll toggle (default on). User actions: none.
+2026-11-21T07:15Z | s18 | S18 OPENED per user asks (D28): 3-line tray tooltip (name+version / totals / speeds); clear-background LARGER tray+taskbar icons; Settings color wheel for custom accent colors; system/light/dark mode selection; bonding performance research from prominent papers; UI research from comparable apps' changelogs. Sandbox had recycled again (registry src stale, rustup+zig wiped, xkblibs gone): D2 bootstrap rerun (rust 1.100.0, windows-gnu + aarch64 targets, cargo-zigbuild 0.24.1), zig 0.17.1 re-downloaded (the old tools/zig tarball was TRUNCATED — lib/std/std.zig missing, "unable to find zig installation directory"), registry src wiped + refetched, /tmp/xkblibs re-extracted from the root debs; env.sh rewritten (RUSTUP_HOME/CARGO_HOME/tools-zig on PATH). Disk filled to 100% mid-gates twice: stale /tmp/my-project recycle snapshots (1.6G) + mt-target/debug (5.8G) cleaned per the watch item.
+2026-11-21T07:25Z | s18 | IMPLEMENTED (D28): (1) tray tooltip 3 lines via CRLF (SpeedFan/qBittorrent multiline precedent) — "MultiTunnel v0.12.0" / "Total ↓x ↑y" / "↓x/s ↑y/s", ≤70 NIF_TIP units worst case, tests pin shape+budget; (2) assets: ALL runtime icons (icon.ico, icon_mono.ico, logo_64.rgba, logo_24.rgba) transparent-bg + contrast_boost (hue-preserving luminance remap [56,240]) + margin 0.08→0.04 — old connected icon was 92% >200-lum near-white opaque tile, new is 33% opaque with dark-stroke bands; (3) color wheel: scripts/make_color_wheel.py 256×256 HSV disc (hue=clockwise angle from 12h, sat=radius, V=1) → assets/color_wheel.rgba; theme.rs wheel_seed_at/wheel_marker_position inverse pair (cardinals+roundtrip tested); Settings Appearance = wheel(176px, mouse_area press/move/release, canvas marker w/ halo+ring) + current-seed chip + hex + 8 presets + seed field; drag re-themes live, release persists (global left-release ends outside-drags); (4) theme modes: SettingsView.theme_mode string (serde default "" → legacy dark_mode decides; dark_mode KEPT as forward-compat shadow written at effective state), ThemeMode enum + effective_dark in mt-service (3 migration tests), system detection chain env MT_THEME_SYSTEM_DARK → Win32 AppsUseLightTheme (RegGetValueW, windows-sys Win32_System_Registry feature added) → gsettings → gtk ini → GTK_THEME, 5-tick repoll in System mode with live re-theme + log; Settings UI = System/Light/Dark segmented buttons + resolution caption; save_settings syncs the shadow; (5) mt-bond scheduler per the paper survey: BLEST tie-guard (250ms drain cap among min-score-tied carriers; leader always eligible — first attempt filtered the leader too and FAILED the S14 saturation test, reconciled to guard-only-narrows-the-tied-set) + ECF completion (LowestLatency w/ reports = rtt + queue/rate; 5 new tests); (6) docs/research/2026-11-21-bonding-performance-survey.md (BLEST/ECF/coupled-CC/MP-QUIC/DaMPS/ToN-2023 mapped to shipped+roadmap) + 2026-11-21-ui-changelog-survey.md (CVR/v2rayN/FlClash/Hiddify/Speedify → shipped vs proposed).
+2026-11-21T07:30Z | s18 | VERIFIED: 447/0 workspace tests (+14 over S17: 3 mt-service mode migration, 2 theme wheel/system-seam, 2 app wheel-drag/mode-messages, 5 mt-bond BLEST/ECF, 2 tooltip); gates fmt/clippy host+windows-gnu (after clippy::explicit_counter_loop fix in the registry read) clean; vendored iced 6/0; zigbuild PE32+ ×4 + ELF ×2; mt-app --smoke 0. RIG sf=1.0: Settings → wheel click at 3 o'clock (hue 90) → seed chip purple(106,85,160)→GREEN(145,239,50), System-button fill purple→green, marker disc+halo+ring at the pick point; Dark click → page lum 236→37; Light click → 235 with green accent preserved. RIG env B (MT_THEME_SYSTEM_DARK=1): boots DARK (lum 27), System filled light-purple in dark scheme. RIG sf=1.5: same flow green chip + dark(36)/light(236) — HiDPI transform holds. Titlebar emblem lum spread 114 (light) / 212 (dark) — the boosted transparent mark reads on both.
diff --git a/harness/memory/STATE.md b/harness/memory/STATE.md
index b231842..e2b89a7 100755
--- a/harness/memory/STATE.md
+++ b/harness/memory/STATE.md
@@ -1,52 +1,44 @@
-# STATE — rewritten at the end of every stage. Last update: S16 (repo polish + window menu + state-aware tray, v0.10.0)
+# STATE — rewritten at the end of every stage. Last update: S18 (tray tooltip/icon rework + color wheel + theme modes + research-driven scheduler refinements, v0.12.0)
 
 ## Current position
-- Stage: S16 IN FLIGHT — implementation + gates COMPLETE, release v0.10.0
-  re-running (first attempt: two independent GitHub-runner flakes in the
-  mt-bond integration leg — `carrier_death_mid_stream_fails_over_without_
-  data_loss` failed on windows (second-half write: "no live bond carrier")
-  and the linux leg hung inside `cargo test -p mt-bond`; the SAME commit
-  passed CI on BOTH OSes (run 44717917257) minutes earlier, 25x solo +
-  12x full-suite stress runs under 8-way CPU load locally = 408 executions
-  0 failures, and the identical suite shipped S14/S15 releases — verdict
-  runner flake, precedent v0.5.1 cancel+rerun; run 44719389267 cancelled
-  + re-run).
-- S16 user asks ALL implemented (D26): (1) GitHub About section rewritten
-  (was a stale description of an unrelated project) + homepage → releases;
-  20 relevant topic tags set via API; (2) Attribution section in README
-  (Xray-core, iced + vendored patches, Material You, v2rayN, net-merge,
-  BondTunnel predecessor, own logo/banner artwork, cargo-zigbuild/zig);
-  (3) README banner (assets/banner.png, 1280x360 lockup on the logo's own
-  canvas color) + transparent logo in the identity section; (4) titlebar
-  emblem on a TRANSPARENT background (flood-fill key-out + area-averaged
-  resample with premultiplied alpha — bilinear punched holes at 20:1);
-  (5) tray icon state-aware: colored logo while connected, MONOCHROME
-  transparent variant in every not-connected state (NIM_MODIFY swap,
-  TaskbarCreated re-adds the current style, boots Mono); (6) tray hover =
-  live ↓/↑ speeds + Σ totals (session_bytes from link-counter deltas,
-  counter-reset-safe, wiped on leaving Connected; tooltip < 128 NIF_TIP
-  units); (7) titlebar RIGHT-CLICK window menu (stack overlay layer,
-  cursor-anchored + clamped, dismiss on outside release): Always on top
-  (checkable, serde-default settings field, applied cross-platform via
-  iced window::change_level, honored at startup) / Minimize / Maximize /
-  Close. ALSO FIXED: latent S15 bug — the titlebar identity cluster sat
-  flush at the strip top (container align_y(Center) added; was hidden by
-  the old opaque emblem tile).
-- Memory graph + RAG (S14) remain live: query_memory.py / write_memory.py;
-  graph rebuilt after every change (18/18 tests).
+- Stage: S18 IMPLEMENTED + all local gates green, releasing v0.12.0.
+- The user's six asks shipped (D28): (1) 3-line tray tooltip —
+  `MultiTunnel v0.12.0` / `Total ↓x ↑y` / `↓x/s ↑y/s` (CRLF, ≤70 NIF_TIP
+  units worst case); (2) ALL runtime icons (tray ICOs, window/taskbar
+  logo_64, titlebar logo_24) transparent-background + hue-preserving
+  contrast boost + tighter margin (0.08→0.04); (3) Settings accent COLOR
+  WHEEL (256px HSV disc asset + inverse hit-test/marker math in theme.rs,
+  live re-theme during drag, persist on release); (4) System/Light/Dark
+  mode with OS detection (env seam → Windows AppsUseLightTheme →
+  gsettings/gtk) re-polled every 5 s in System mode, legacy dark_mode
+  kept as the forward-compat shadow; (5) docs/research/ bonding survey —
+  BLEST tie-guard + ECF completion scheduling IMPLEMENTED in mt-bond
+  (reconciled with S14: the guard only narrows the min-score tied set),
+  redundant-mode/FEC/reinjection/delay-signal roadmap documented; (6)
+  docs/research/ UI changelog survey (CVR/v2rayN/FlClash/Hiddify/
+  Speedify) with a prioritized adopt-next list.
+- 447/0 workspace tests; fmt/clippy host+windows-gnu; vendored iced 6/0;
+  zigbuild PE32+ ×4 + ELF ×2; rig verified sf=1.0+1.5 (wheel pick
+  purple→green end-to-end, mode buttons light↔dark, system env boot,
+  emblem legibility both modes).
+- Sandbox recycled AGAIN before the stage (registry/rustup/zig/xkblibs
+  gone, tools/zig tarball truncated): full D2 bootstrap re-run, env.sh
+  rewritten; disk hit 100% twice — stale /tmp/my-project snapshots +
+  mt-target/debug cleaned.
 
 ## Next actions
-1. Watch release run 44719389267 (re-run); on success verify assets by
-   download+unzip (11-entry manifest, PE32+ x2, README/QUICKSTART carried);
-   then LEDGER close entry + backup push.
-2. If the mt-bond flake recurs on the runner: do NOT blind-rerun again —
-   capture the hung job's logs FIRST (jobs API), then decide (test-harness
-   hardening: bound read_to_end with tokio timeout is the leading option,
-   but only with a reproduced defect in hand).
-3. USER VERIFICATION of v0.9/v0.10 asks on the real Windows box: tray mono
-   ↔ colored transition, hover speeds, titlebar right-click → always-on-top
-   actually floating above other windows, transparent emblem on both light
-   and dark titlebars.
-4. Backlog: subscription auto-update, vmess validation, profiles.json
-   locking, engine log rotation, listener max_conns, upstream iced
-   migration, S7 native REALITY core (parked).
+1. Tag v0.12.0 → release.yml; watch the run; verify assets by
+   download+unzip (PE32+ ×2 in zip, ELFs, README/QUICKSTART carried).
+2. User-side Windows verification: 3-line hover tooltip, icon size/
+   transparency on light AND dark taskbars, wheel + System mode against
+   the real OS theme flip (Settings → Personalization → Color).
+3. Backlog candidates from the research docs when asked: redundant bond
+   mode (calls), FEC parity frames, reinjection, per-link live status
+   badges, exit-IP auto-refresh, empty states.
+
+## Watch items (unchanged unless noted)
+- Disk: /tmp/mt-target release trees ~4.5G; clean debug trees after gate
+  runs; the 100%-full rootfs manifests as linker "signal 7" or ENOSPC
+  temp-dir panics (hit BOTH this stage).
+- Background processes DO NOT survive between sandbox tool calls — long
+  builds must run in the foreground (setsid/nohup both reaped).
diff --git a/harness/memory/curated/edges.json b/harness/memory/curated/edges.json
index ff10e85..f04fdfa 100644
--- a/harness/memory/curated/edges.json
+++ b/harness/memory/curated/edges.json
@@ -162,6 +162,12 @@
       "rel": "implements",
       "evidence": "crates/mt-app/src/screens.rs logs()+titlebar(); app.rs messages+autoscroll_logs_now"
     },
+    {
+      "src": "task:s18-wheel-theme-research",
+      "dst": "decision:D28",
+      "rel": "implements",
+      "evidence": "DECISIONS.md D28, S18"
+    },
     {
       "src": "task:saturation-aware-split",
       "dst": "bug:reorder-overflow",
diff --git a/harness/memory/curated/nodes.json b/harness/memory/curated/nodes.json
index 63d3251..577db45 100644
--- a/harness/memory/curated/nodes.json
+++ b/harness/memory/curated/nodes.json
@@ -448,6 +448,26 @@
         }
       ]
     },
+    {
+      "id": "task:s18-wheel-theme-research",
+      "type": "task",
+      "title": "S18: tray tooltip/icon rework, accent color wheel, system/light/dark modes, paper-driven scheduler refinements (v0.12.0)",
+      "tags": [
+        "s18"
+      ],
+      "sources": [
+        {
+          "file": "user-directive+LEDGER-s18",
+          "anchor": ""
+        }
+      ],
+      "passages": [
+        {
+          "text": "User asks (D28): 3-line tray tooltip (name+version, session totals, live speeds); clear-background larger tray/taskbar icons; Settings color wheel for custom accent colors; system/light/dark mode selection; bonding-performance research from prominent papers; UI research from comparable apps' changelogs. Shipped: CRLF 3-line tooltip (<=70 NIF_TIP units); all runtime icons transparent + hue-preserving contrast boost + tighter margin; HSV disc wheel (make_color_wheel.py asset + wheel_seed_at/wheel_marker_position inverse pair in theme.rs, live re-theme during drag, persist on release, canvas marker overlay); ThemeMode System/Light/Dark with OS detection (env seam MT_THEME_SYSTEM_DARK -> Win32 AppsUseLightTheme RegGetValueW -> gsettings -> gtk ini) re-polled every 5 ticks, dark_mode kept as forward-compat shadow written at effective state; mt-bond BLEST tie-guard (250ms drain cap among min-score-tied carriers, leader always eligible - reconciled with S14 saturation semantics after the naive filter failed the S14 test) + ECF earliest-completion for lowest-latency with reports; docs/research/2026-11-21-bonding-performance-survey.md (BLEST/ECF/coupled-CC/MP-QUIC/DaMPS/ToN-2023 -> shipped+roadmap) and 2026-11-21-ui-changelog-survey.md (Clash Verge Rev/v2rayN/FlClash/Hiddify/Speedify). 447/0 tests; rig sf=1.0+1.5 verified wheel pick + mode flips + system env boot.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "task:saturation-aware-split",
       "type": "task",
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index d0e8659..0022655 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "66147c6 2026-11-20T17:04:04Z",
+  "as_of": "d910ee5 2026-11-20T17:04:40Z",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 256,
-  "edge_count": 265,
+  "node_count": 260,
+  "edge_count": 269,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -1715,6 +1715,26 @@
         }
       ]
     },
+    {
+      "id": "commit:d910ee5",
+      "type": "commit",
+      "title": "backup(s17-memory-node): 2026-11-20T17:04:40Z",
+      "tags": [
+        "2026-11-20"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "d910ee5"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s17-memory-node): 2026-11-20T17:04:40Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:ddd2420",
       "type": "commit",
@@ -2947,6 +2967,30 @@
         }
       ]
     },
+    {
+      "id": "decision:D28",
+      "type": "decision",
+      "title": "D28 — S18: tray tooltip/icon rework, color wheel + theme modes, literature-driven scheduler refinements (2026-11-21)",
+      "tags": [
+        "immutable"
+      ],
+      "sources": [
+        {
+          "file": "harness/memory/DECISIONS.md",
+          "anchor": "D28"
+        }
+      ],
+      "passages": [
+        {
+          "text": "User asks: (1) tray hover = name+version / total volume / current speeds, three lines; (2) tray+taskbar icons with CLEAR backgrounds and a larger mark; (3) a color wheel in Settings for custom accent colors; (4) system / light / dark mode selection; (5) explore bonding performance from prominent scientific papers; (6) explore UI improvements from comparable apps' changelogs.",
+          "kind": "body"
+        },
+        {
+          "text": "1. TOOLTIP = 3 lines with CRLF separators (the Win32 tooltip control breaks lines on \\r\\n — the SpeedFan/qBittorrent multi-line tray precedent): line 1 `MultiTunnel v<CARGO_PKG_VERSION>`, line 2 `Total ↓x ↑y`, line 3 `↓x/s ↑y/s` (the S16 single-line form is gone; not-connected states are 2 lines: name+version / state word). Worst case ≈70 UTF-16 units — safely inside the 128-unit NIF_TIP budget, pinned by tests. 2. ICON ASSETS (make_logo_assets.py): all RUNTIME icons — icon.ico (16/32/48 emblem + 256 lockup), icon_mono.ico, logo_64.rgba (the window/taskbar icon) and logo_24.rgba (titlebar emblem) — now share ONE treatment: flood-fill keyed-out TRANSPARENT background, `contrast_boost` (hue-preserving luminance remap onto [56,240], percentile-normalized like desaturate but per-channel scaled so hue and chroma survive) and a tighter margin (0.08 → 0.04, the glyph fills ~8% more of the tile). Rationale: the old connected icon was a nearly-white opaque tile (92% of pixels >200 luminance — indistinguishable from a blank square); the emblem itself carries dark strokes (16% below 110), which the boost deepens for light taskbars while the pastel body still pops on dark ones. Verified by pixel stats (33% opaque, corners alpha 0, dark-stroke bands) and the rig (titlebar emblem lum spread 114 on light / 212 on dark). 3.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "decision:D3",
       "type": "decision",
@@ -4297,6 +4341,35 @@
         }
       ]
     },
+    {
+      "id": "ledger:s18",
+      "type": "ledger",
+      "title": "Stage s18 ledger",
+      "tags": [
+        "s18",
+        "append-only"
+      ],
+      "sources": [
+        {
+          "file": "harness/memory/LEDGER.md",
+          "anchor": "s18"
+        }
+      ],
+      "passages": [
+        {
+          "text": "2026-11-21T07:15Z | S18 OPENED per user asks (D28): 3-line tray tooltip (name+version / totals / speeds); clear-background LARGER tray+taskbar icons; Settings color wheel for custom accent colors; system/light/dark mode selection; bonding performance research from prominent papers; UI research from comparable apps' changelogs. Sandbox had recycled again (registry src stale, rustup+zig wiped, xkblibs gone): D2 bootstrap rerun (rust 1.100.0, windows-gnu + aarch64 targets, cargo-zigbuild 0.24.1), zig 0.17.1 re-downloaded (the old tools/zig tarball was TRUNCATED — lib/std/std.zig missing, \"unable to find zig installation directory\"), registry src wiped + refetched, /tmp/xkblibs re-extracted from the root debs; env.sh rewritten (RUSTUP_HOME/CARGO_HOME/tools-zig on PATH). Disk filled to 100% mid-gates twice: stale /tmp/my-project recycle snapshots (1.6G) + mt-target/debug (5.8G) cleaned per the watch item.",
+          "kind": "body"
+        },
+        {
+          "text": "2026-11-21T07:25Z | IMPLEMENTED (D28): (1) tray tooltip 3 lines via CRLF (SpeedFan/qBittorrent multiline precedent) — \"MultiTunnel v0.12.0\" / \"Total ↓x ↑y\" / \"↓x/s ↑y/s\", ≤70 NIF_TIP units worst case, tests pin shape+budget; (2) assets: ALL runtime icons (icon.ico, icon_mono.ico, logo_64.rgba, logo_24.rgba) transparent-bg + contrast_boost (hue-preserving luminance remap [56,240]) + margin 0.08→0.04 — old connected icon was 92% >200-lum near-white opaque tile, new is 33% opaque with dark-stroke bands; (3) color wheel: scripts/make_color_wheel.py 256×256 HSV disc (hue=clockwise angle from 12h, sat=radius, V=1) → assets/color_wheel.rgba; theme.rs wheel_seed_at/wheel_marker_position inverse pair (cardinals+roundtrip tested); Settings Appearance = wheel(176px, mouse_area press/move/release, canvas marker w/ halo+ring) + current-seed chip + hex + 8 presets + seed field; drag re-themes live, release persists (global left-release ends outside-drags); (4) theme modes: SettingsView.theme_mode string (serde default \"\" → legacy dark_mode decides; dark_mode KEPT as forward-compat shadow written at effective state), ThemeMode enum + effective_dark in mt-service (3 migration tests), system detection chain env MT_THEME_SYSTEM_DARK → Win32 AppsUseLightTheme (RegGetValueW, windows-sys Win32_System_Registry feature added) → gsettings → gtk ini → GTK_THEME, 5-tick repoll in System mode with live re-theme + log; Settings UI = System/Light/Dark segmented buttons + resolution caption; save_settings syncs the shadow; (5) mt-bond scheduler per the paper survey: BLEST tie-guard (250ms drain cap ",
+          "kind": "body"
+        },
+        {
+          "text": "2026-11-21T07:30Z | VERIFIED: 447/0 workspace tests (+14 over S17: 3 mt-service mode migration, 2 theme wheel/system-seam, 2 app wheel-drag/mode-messages, 5 mt-bond BLEST/ECF, 2 tooltip); gates fmt/clippy host+windows-gnu (after clippy::explicit_counter_loop fix in the registry read) clean; vendored iced 6/0; zigbuild PE32+ ×4 + ELF ×2; mt-app --smoke 0. RIG sf=1.0: Settings → wheel click at 3 o'clock (hue 90) → seed chip purple(106,85,160)→GREEN(145,239,50), System-button fill purple→green, marker disc+halo+ring at the pick point; Dark click → page lum 236→37; Light click → 235 with green accent preserved. RIG env B (MT_THEME_SYSTEM_DARK=1): boots DARK (lum 27), System filled light-purple in dark scheme. RIG sf=1.5: same flow green chip + dark(36)/light(236) — HiDPI transform holds. Titlebar emblem lum spread 114 (light) / 212 (dark) — the boosted transparent mark reads on both.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "ledger:s2",
       "type": "ledger",
@@ -5053,7 +5126,7 @@
       ],
       "passages": [
         {
-          "text": "- **What**: MultiTunnel — Rust rewrite of BondTunnel; zero-dependency all-in-one (v2rayN import + net-merge), Material You GUI. Repo: github.com/maniesh-NR/MultiTunnel. - **Data path**: Windows client → local SOCKS/HTTP → ProxyServer → BondOutbound → N carriers (one per bonded NIC, VLESS+REALITY via Xray `sendThrough`, engine self-provisioned D14) → VM Xray (public 8443) → freedom → mt-merge-server (127.0.0.1:7443) → real target. Reverse symmetric. Auth at the Xray edge. - **Two bond modes**: per-connection (default) and stream bonding (D17 framing, needs the VM merge daemon; carriers verified by PING/PONG before schedulable). - **Scheduling (D23, v0.7.0)**: five merge policies in mt-net — round-robin, failover, lowest-latency (EWMA), **balanced** (stock default; pressure = rate_bps + active_conns × 128 KiB/s reserve), **priority** (user order; #1 fills until bandwidth plateaus (ramp detector, ≥3 flat ticks + rate ≥ 0.85 × capacity ≥ 256 KiB/s), then spills). Stream-mode frame scheduler is policy-aware too (least writer backpressure / 512 KiB queue high-water). Adapter priority == saved `bonded_adapters` ORDER; Adapters tab = ordering UI (selected rows first, #n chips, ↑/↓). - **Crates**: mt-net (proxy servers, merge, policy), mt-bond (stream framing, carrier scheduling, daemon), mt-engine (ManagedEngine lifecycle), mt-links (vless:// + subscriptions), mt-platform (Win32 adapter FFI), mt-service (service trait + real_service), mt-m3 + mt-app (Material You GUI, iced). Root crate = CLI binaries (multitunnel, tray, mt-app).",
+          "text": "- **What**: MultiTunnel — Rust rewrite of BondTunnel; zero-dependency all-in-one (v2rayN import + net-merge), Material You GUI. Repo: github.com/maniesh-NR/MultiTunnel. - **Data path**: Windows client → local SOCKS/HTTP → ProxyServer → BondOutbound → N carriers (one per bonded NIC, VLESS+REALITY via Xray `sendThrough`, engine self-provisioned D14) → VM Xray (public 8443) → freedom → mt-merge-server (127.0.0.1:7443) → real target. Reverse symmetric. Auth at the Xray edge. - **Two bond modes**: per-connection (default) and stream bonding (D17 framing, needs the VM merge daemon; carriers verified by PING/PONG before schedulable). - **Scheduling (D23, v0.7.0)**: five merge policies in mt-net — round-robin, failover, lowest-latency (EWMA), **balanced** (stock default; pressure = rate_bps + active_conns × 128 KiB/s reserve), **priority** (user order; #1 fills until bandwidth plateaus (ramp detector, ≥3 flat ticks + rate ≥ 0.85 × capacity ≥ 256 KiB/s), then spills). Stream-mode frame scheduler is policy-aware too (least writer backpressure / 512 KiB queue high-water). S18 adds: BLEST tie-guard (250 ms drain cap among min-score-tied carriers; the score leader is always eligible — queue depth alone is never a saturation signal) and ECF completion for lowest-latency (rtt + queued/deliver when reports exist; sampled-RTT otherwise). Adapter priority == saved `bonded_adapters` ORDER; Adapters tab = ordering UI (selected rows first, #n chips, ↑/↓).",
           "kind": "body"
         }
       ]
@@ -5097,7 +5170,7 @@
       ],
       "passages": [
         {
-          "text": "- **Stage**: IDLE — S17 CLOSED, v0.11.0 released and verified. The three asks shipped: release version next to the app name in the titlebar (workspace version inheritance — one-line release bumps; mt-app.exe PE metadata now carries the release version), live global ↓/↑ rates on the strip's right (the tray tooltip's same 1 s sample), Logs tab = fixed toolbar + dedicated viewport with floating top/bottom jump buttons (26px right stand-off — the rig caught the scrollbar grab-zone swallowing flush-corner clicks) + autoscroll toggle (default ON, follows the tail; re-enabling re-anchors). All 8 gates green; rig verified sf=1.0+1.5 (VLM content reads + pixel diffs); 433/0 tests. Release: attempt-1 windows leg cancelled at 68 min on a cold rust-cache (Cargo.lock changed), attempt-2 SUCCESS in 6 min warm (s9 pattern proven); CI green on attempt 3 after a two-test ubuntu runner-flake round (carrier_death + socks_probe deadline, different tests per attempt, all other legs/local green — LEDGER 16:15Z). - **Prior stages**: S16 (v0.10.0, About+topics+attribution+banner, transparent emblem, mono tray icon, hover speeds+totals, titlebar menu + always-on-top), S15 (v0.9.0, logo + blended titlebar + close-to-tray + persistent tray + Quick Settings), S13 (v0.7.1, status tab: one connection frame + one page scrollable, graph fills frame), S12 (v0.7.0, balanced+priority scheduling + adapter ordering), S11 (v0.6.2, stream bonding finalRules + verified carriers), S10/10.1 (v0.6.0/01, rendering fixes + per-adapter graphs).",
+          "text": "- **Stage**: S18 (v0.12.0) — implementation + local gates COMPLETE, release running. The six asks shipped (D28): 3-line tray tooltip (name+version / totals / speeds, CRLF), transparent-background contrast-boosted LARGER runtime icons (tray ICOs + logo_64 + logo_24), Settings accent COLOR WHEEL (HSV disc asset + inverse marker math, live re-theme, persist on release), System/Light/Dark mode (OS detection chain: MT_THEME_SYSTEM_DARK env seam → Win32 AppsUseLightTheme → gsettings/gtk; 5 s repoll; legacy dark_mode kept as the forward-compat shadow), mt-bond BLEST tie-guard + ECF completion scheduling (papers survey: docs/research/), UI changelog survey (docs/research/). 447/0 tests; rig verified sf=1.0+1.5. - **Prior stage**: S17 (v0.11.0, titlebar version + live speeds, logs floating jump buttons + autoscroll). The three asks shipped: release version next to the app name in the titlebar (workspace version inheritance — one-line release bumps; mt-app.exe PE metadata now carries the release version), live global ↓/↑ rates on the strip's right (the tray tooltip's same 1 s sample), Logs tab = fixed toolbar + dedicated viewport with floating top/bottom jump buttons (26px right stand-off — the rig caught the scrollbar grab-zone swallowing flush-corner clicks) + autoscroll toggle (default ON, follows the tail; re-enabling re-anchors). All 8 gates green; rig verified sf=1.0+1.5 (VLM content reads + pixel diffs); 433/0 tests.",
           "kind": "body"
         }
       ]
@@ -7137,7 +7210,7 @@
     {
       "id": "state:current",
       "type": "state",
-      "title": "STATE — current position (last update: S16 (repo polish + window menu + state-aware tray, v0.10.0)",
+      "title": "STATE — current position (last update: S18 (tray tooltip/icon rework + color wheel + theme modes + research-driven scheduler refinements, v0.12.0)",
       "tags": [
         "durable"
       ],
@@ -7149,15 +7222,19 @@
       ],
       "passages": [
         {
-          "text": "# STATE — rewritten at the end of every stage. Last update: S16 (repo polish + window menu + state-aware tray, v0.10.0)",
+          "text": "# STATE — rewritten at the end of every stage. Last update: S18 (tray tooltip/icon rework + color wheel + theme modes + research-driven scheduler refinements, v0.12.0)",
           "kind": "title"
         },
         {
-          "text": "## Current position - Stage: S16 IN FLIGHT — implementation + gates COMPLETE, release v0.10.0 re-running (first attempt: two independent GitHub-runner flakes in the mt-bond integration leg — `carrier_death_mid_stream_fails_over_without_ data_loss` failed on windows (second-half write: \"no live bond carrier\") and the linux leg hung inside `cargo test -p mt-bond`; the SAME commit passed CI on BOTH OSes (run 44717917257) minutes earlier, 25x solo + 12x full-suite stress runs under 8-way CPU load locally = 408 executions 0 failures, and the identical suite shipped S14/S15 releases — verdict runner flake, precedent v0.5.1 cancel+rerun; run 44719389267 cancelled + re-run).",
+          "text": "## Current position - Stage: S18 IMPLEMENTED + all local gates green, releasing v0.12.0. - The user's six asks shipped (D28): (1) 3-line tray tooltip — `MultiTunnel v0.12.0` / `Total ↓x ↑y` / `↓x/s ↑y/s` (CRLF, ≤70 NIF_TIP units worst case); (2) ALL runtime icons (tray ICOs, window/taskbar logo_64, titlebar logo_24) transparent-background + hue-preserving contrast boost + tighter margin (0.08→0.04); (3) Settings accent COLOR WHEEL (256px HSV disc asset + inverse hit-test/marker math in theme.rs, live re-theme during drag, persist on release); (4) System/Light/Dark mode with OS detection (env seam → Windows AppsUseLightTheme → gsettings/gtk) re-polled every 5 s in System mode, legacy dark_mode kept as the forward-compat shadow; (5) docs/research/ bonding survey — BLEST tie-guard + ECF completion scheduling IMPLEMENTED in mt-bond (reconciled with S14: the guard only narrows the min-score tied set), redundant-mode/FEC/reinjection/delay-signal roadmap documented; (6) docs/research/ UI changelog survey (CVR/v2rayN/FlClash/Hiddify/ Speedify) with a prioritized adopt-next list. - 447/0 workspace tests; fmt/clippy host+windows-gnu; vendored iced 6/0; zigbuild PE32+ ×4 + ELF ×2; rig verified sf=1.0+1.5 (wheel pick purple→green end-to-end, mode buttons light↔dark, system env boot, emblem legibility both modes). - Sandbox recycled AGAIN before the stage (registry/rustup/zig/xkblibs gone, tools/zig tarball truncated): full D2 bootstrap re-run, env.sh rewritten; disk hit 100% twice — stale /tmp/my-project snapshots + mt-target/debug cleaned.",
           "kind": "title"
         },
         {
-          "text": "## Next actions 1. Watch release run 44719389267 (re-run); on success verify assets by download+unzip (11-entry manifest, PE32+ x2, README/QUICKSTART carried); then LEDGER close entry + backup push. 2. If the mt-bond flake recurs on the runner: do NOT blind-rerun again — capture the hung job's logs FIRST (jobs API), then decide (test-harness hardening: bound read_to_end with tokio timeout is the leading option, but only with a reproduced defect in hand). 3. USER VERIFICATION of v0.9/v0.10 asks on the real Windows box: tray mono ↔ colored transition, hover speeds, titlebar right-click → always-on-top actually floating above other windows, transparent emblem on both light and dark titlebars. 4. Backlog: subscription auto-update, vmess validation, profiles.json locking, engine log rotation, listener max_conns, upstream iced migration, S7 native REALITY core (parked).",
+          "text": "## Next actions 1. Tag v0.12.0 → release.yml; watch the run; verify assets by download+unzip (PE32+ ×2 in zip, ELFs, README/QUICKSTART carried). 2. User-side Windows verification: 3-line hover tooltip, icon size/ transparency on light AND dark taskbars, wheel + System mode against the real OS theme flip (Settings → Personalization → Color). 3. Backlog candidates from the research docs when asked: redundant bond mode (calls), FEC parity frames, reinjection, per-link live status badges, exit-IP auto-refresh, empty states.",
+          "kind": "title"
+        },
+        {
+          "text": "## Watch items (unchanged unless noted) - Disk: /tmp/mt-target release trees ~4.5G; clean debug trees after gate runs; the 100%-full rootfs manifests as linker \"signal 7\" or ENOSPC temp-dir panics (hit BOTH this stage). - Background processes DO NOT survive between sandbox tool calls — long builds must run in the foreground (setsid/nohup both reaped).",
           "kind": "title"
         }
       ]
@@ -7301,6 +7378,26 @@
         }
       ]
     },
+    {
+      "id": "task:s18-wheel-theme-research",
+      "type": "task",
+      "title": "S18: tray tooltip/icon rework, accent color wheel, system/light/dark modes, paper-driven scheduler refinements (v0.12.0)",
+      "tags": [
+        "s18"
+      ],
+      "sources": [
+        {
+          "file": "user-directive+LEDGER-s18",
+          "anchor": ""
+        }
+      ],
+      "passages": [
+        {
+          "text": "User asks (D28): 3-line tray tooltip (name+version, session totals, live speeds); clear-background larger tray/taskbar icons; Settings color wheel for custom accent colors; system/light/dark mode selection; bonding-performance research from prominent papers; UI research from comparable apps' changelogs. Shipped: CRLF 3-line tooltip (<=70 NIF_TIP units); all runtime icons transparent + hue-preserving contrast boost + tighter margin; HSV disc wheel (make_color_wheel.py asset + wheel_seed_at/wheel_marker_position inverse pair in theme.rs, live re-theme during drag, persist on release, canvas marker overlay); ThemeMode System/Light/Dark with OS detection (env seam MT_THEME_SYSTEM_DARK -> Win32 AppsUseLightTheme RegGetValueW -> gsettings -> gtk ini) re-polled every 5 ticks, dark_mode kept as forward-compat shadow written at effective state; mt-bond BLEST tie-guard (250ms drain cap among min-score-tied carriers, leader always eligible - reconciled with S14 saturation semantics after the naive filter failed the S14 test) + ECF earliest-completion for lowest-latency with reports; docs/research/2026-11-21-bonding-performance-survey.md (BLEST/ECF/coupled-CC/MP-QUIC/DaMPS/ToN-2023 -> shipped+roadmap) and 2026-11-21-ui-changelog-survey.md (Clash Verge Rev/v2rayN/FlClash/Hiddify/Speedify). 447/0 tests; rig sf=1.0+1.5 verified wheel pick + mode flips + system env boot.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "task:saturation-aware-split",
       "type": "task",
@@ -7776,6 +7873,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s2)"
     },
+    {
+      "src": "commit:d910ee5",
+      "dst": "ledger:s17",
+      "rel": "part_of",
+      "evidence": "subject stage label (s17)"
+    },
     {
       "src": "commit:ddd2420",
       "dst": "ledger:s10.1",
@@ -8034,6 +8137,12 @@
       "rel": "part_of",
       "evidence": "project decision log"
     },
+    {
+      "src": "decision:D28",
+      "dst": "project:multitunnel",
+      "rel": "part_of",
+      "evidence": "project decision log"
+    },
     {
       "src": "decision:D3",
       "dst": "project:multitunnel",
@@ -8388,6 +8497,12 @@
       "rel": "released_in",
       "evidence": "2026-11-20T17:05Z | v0.11.0 RELEASED: attempt 1's build-windows leg sat 68 min inside cargo test on a 100%-cold rust-cache (the version bump changed Cargo.lock; every other leg incl. build-linux-daemon green) — cancelled per the >45-min protocol (rust-cache@v2 saves in post-steps even on cancellation), attempt 2 completed SUCCESS in ~6 min WARM exactly as predicted (the s9 pattern, now proven twice). Assets verified by download+unzip (download/v0110-verify/): multitunnel-v0.11.0-windows-x64.zip 6,314,832 B (10 entries, integrity OK; multitunnel.exe PE32+ console 13.26 MB + multitunnel-tray.exe PE32+ GUI 2.95 MB; README carries the v0.9-v0.11 section, QUICKSTART the v0.11 note, server/ scripts included) + mt-merge-server {x86_64 1,824,296 B, aarch64 1,860,432 B} ELFs. CI on the same commit: green on attempt 3 after the two-test runner-flake round (see 16:15Z entry). S17 CLOSED — the three asks shipped: version next to the app name in the titlebar, live global down/up speeds on the titlebar's right, Logs tab floating top/bottom jump buttons + autoscroll toggle (default on). User actions: none."
     },
+    {
+      "src": "ledger:s18",
+      "dst": "project:multitunnel",
+      "rel": "part_of",
+      "evidence": "project ledger"
+    },
     {
       "src": "ledger:s2",
       "dst": "project:multitunnel",
@@ -8886,6 +9001,12 @@
       "rel": "implements",
       "evidence": "crates/mt-app/src/screens.rs logs()+titlebar(); app.rs messages+autoscroll_logs_now"
     },
+    {
+      "src": "task:s18-wheel-theme-research",
+      "dst": "decision:D28",
+      "rel": "implements",
+      "evidence": "DECISIONS.md D28, S18"
+    },
     {
       "src": "task:saturation-aware-split",
       "dst": "bug:reorder-overflow",
diff --git a/scripts/make_color_wheel.py b/scripts/make_color_wheel.py
new file mode 100644
index 0000000..84e0f41
--- /dev/null
+++ b/scripts/make_color_wheel.py
@@ -0,0 +1,93 @@
+#!/usr/bin/env python3
+"""Generate assets/color_wheel.rgba — the S18 Settings accent picker disc.
+
+Geometry contract (MUST stay in sync with crates/mt-app/src/theme.rs):
+  - 256x256 RGBA8, transparent outside the disc;
+  - hue    = angle around the center, 0deg RED at 12 o'clock, CLOCKWISE;
+  - chroma = distance from the center: 0 at the center (white), full at
+    the rim;
+  - value  = 1.0 (the M3 TonalSpot generator keys on hue+chroma).
+
+The Rust side computes the exact same HSV(h, s, 1) for hit-testing, so the
+asset and the picker agree by construction; the only freedom here is the
+anti-aliased rim (the Rust side clamps to the rim at that angle, which the
+AA edge rounds to the same color).
+
+Usage:  python3 scripts/make_color_wheel.py  [output path, default
+        assets/color_wheel.rgba]
+Deterministic output; stdlib only.
+"""
+
+import math
+import os
+import sys
+
+PX = 256
+
+
+def hsv_to_rgb(h, s, v):
+    """Mirror of the Rust helper (degrees / 0..1 floats / 0..255 ints)."""
+    h = h % 360.0
+    c = v * s
+    x = c * (1.0 - abs((h / 60.0) % 2.0 - 1.0))
+    m = v - c
+    if h < 60:
+        r, g, b = c, x, 0.0
+    elif h < 120:
+        r, g, b = x, c, 0.0
+    elif h < 180:
+        r, g, b = 0.0, c, x
+    elif h < 240:
+        r, g, b = 0.0, x, c
+    elif h < 300:
+        r, g, b = x, 0.0, c
+    else:
+        r, g, b = c, 0.0, x
+    return (
+        int(round((r + m) * 255)),
+        int(round((g + m) * 255)),
+        int(round((b + m) * 255)),
+    )
+
+
+def main() -> int:
+    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join("assets", "color_wheel.rgba")
+    cx = cy = (PX - 1) / 2.0
+    radius = PX / 2.0
+    buf = bytearray(PX * PX * 4)
+    # 2x2 supersample per pixel for a smooth rim (deterministic grid).
+    for y in range(PX):
+        for x in range(PX):
+            r_sum = g_sum = b_sum = a_sum = 0.0
+            for sy in (0.25, 0.75):
+                for sx in (0.25, 0.75):
+                    dx = (x + sx) - 0.5 - cx
+                    dy = (y + sy) - 0.5 - cy
+                    dist = math.hypot(dx, dy)
+                    if dist <= radius:
+                        sat = dist / radius
+                        hue = math.degrees(math.atan2(dy, dx)) + 90.0
+                        r, g, b = hsv_to_rgb(hue, sat, 1.0)
+                        r_sum += r
+                        g_sum += g
+                        b_sum += b
+                        a_sum += 255.0
+            o = (y * PX + x) * 4
+            n = 4.0
+            if a_sum > 0:
+                buf[o] = int(round(r_sum / n))
+                buf[o + 1] = int(round(g_sum / n))
+                buf[o + 2] = int(round(b_sum / n))
+                buf[o + 3] = int(round(a_sum / n))
+    with open(out, "wb") as f:
+        f.write(buf)
+    opaque = sum(1 for i in range(PX * PX) if buf[i * 4 + 3] > 0)
+    print(
+        f"wrote {out}: {len(buf)} bytes ({PX}x{PX} RGBA, "
+        f"{opaque} in-disc px, transparent rim corners)"
+    )
+    return 0
+
+
+if __name__ == "__main__":
+    raise SystemExit(main())
diff --git a/scripts/make_logo_assets.py b/scripts/make_logo_assets.py
index 5ba4dc5..6d8fa69 100644
--- a/scripts/make_logo_assets.py
+++ b/scripts/make_logo_assets.py
@@ -361,6 +361,39 @@ def desaturate(img, lo=64, hi=216):
     return (w, h, bytes(out))
 
 
+def contrast_boost(img, lo=56, hi=240):
+    """S18: HUE-PRESERVING luminance contrast boost for the transparent
+    tray/taskbar icons. The source emblem is pale pastel (median luminance
+    ≈200/255, nothing darker than ≈146) — fine on the logo's own canvas,
+    near-invisible on a light taskbar once the background is keyed out.
+
+    Each opaque pixel's luminance is remapped through a percentile-
+    normalized curve onto [lo, hi] (like `desaturate`, but the RGB channels
+    are scaled by the luminance ratio, so hue and relative chroma survive).
+    Alpha is untouched."""
+    w, h, buf = img
+    out = bytearray(buf)
+    lums = []
+    for i in range(w * h):
+        o = i * 4
+        if buf[o + 3] > 0:
+            lums.append(0.299 * buf[o] + 0.587 * buf[o + 1] + 0.114 * buf[o + 2])
+    if not lums:
+        return img
+    lums.sort()
+    p2, p98 = lums[len(lums) // 50], lums[49 * len(lums) // 50]
+    span = max(p98 - p2, 1e-6)
+    for i in range(w * h):
+        o = i * 4
+        if buf[o + 3] > 0:
+            lum = 0.299 * buf[o] + 0.587 * buf[o + 1] + 0.114 * buf[o + 2]
+            target = lo + (min(max(lum, p2), p98) - p2) * (hi - lo) / span
+            scale = max(target / max(lum, 1.0), 0.0)
+            for c in range(3):
+                out[o + c] = int(round(min(255.0, buf[o + c] * scale)))
+    return (w, h, bytes(out))
+
+
 def composite_on(img, size_wh, bg, margin=0.14):
     """Alpha-composite a transparent RGBA raster centered on a solid
     `bg` canvas of size_wh (w, h), with a symmetric margin."""
@@ -500,18 +533,23 @@ def main() -> int:
     opaque = sum(1 for i in range(len(emblem_t[2])) if i % 4 == 3 and emblem_t[2][i] > 0)
     print(f"transparent emblem: {opaque} opaque px of {emblem_t[0]*emblem_t[1]}")
 
-    # titlebar emblem: TRANSPARENT background (S16 ask), padded to a square
-    # so the mark stays centered at 20x20 draw size.
-    sq24 = square_on_bg(emblem_t, 24, (0, 0, 0, 0))
+    # S18: titlebar emblem: TRANSPARENT background with a contrast boost
+    # (the pale pastel mark would wash out on LIGHT titlebars — light mode
+    # shipped the same day), tight margin so the mark fills more of the
+    # 24px tile (0.08 → 0.04: the glyph grows ~8%).
+    emblem_boosted = contrast_boost(emblem_t)
+    sq24 = square_on_bg(emblem_boosted, 24, (0, 0, 0, 0), margin=0.04)
     with open(os.path.join(outdir, "logo_24.rgba"), "wb") as f:
         f.write(sq24[2])
-    print(f"logo_24.rgba: {len(sq24[2])} bytes (24x24, transparent bg)")
+    print(f"logo_24.rgba: {len(sq24[2])} bytes (24x24, transparent bg, contrast-boosted)")
 
-    # window icon: colored square on the logo's own canvas (unchanged look)
-    sq64 = square_on_bg(emblem_img, 64, bg)
+    # S18: window/taskbar icon: TRANSPARENT background + the same boost
+    # (was the emblem on the opaque canvas color — a hard tile in the
+    # taskbar). Tight margin, same as the tray entries.
+    sq64 = square_on_bg(emblem_boosted, 64, (0, 0, 0, 0), margin=0.04)
     with open(os.path.join(outdir, "logo_64.rgba"), "wb") as f:
         f.write(sq64[2])
-    print(f"logo_64.rgba: {len(sq64[2])} bytes (64x64)")
+    print(f"logo_64.rgba: {len(sq64[2])} bytes (64x64, transparent bg, contrast-boosted)")
 
     # transparent lockup (PNG) — README + compositing source
     png = encode_png(lockup_t)
@@ -526,23 +564,40 @@ def main() -> int:
         f.write(banner_png)
     print(f"banner.png: {len(banner_png)} bytes (1280x360)")
 
-    # tray icon (connected): 16/32/48 emblem squares + 256 full lockup
+    # S18 tray icon (connected): CLEAR (transparent) background + contrast
+    # boost + tighter margin (0.08 → 0.04) so the mark is larger and reads
+    # on light AND dark taskbars. 16/32/48 = boosted emblem; 256 = the full
+    # lockup, transparent (was the opaque canvas tile).
+    lockup_boosted = contrast_boost(lockup_t)
     entries = []
     for size in (16, 32, 48, 256):
-        sq = square_on_bg(emblem_img if size < 64 else lockup_img, size, bg)
+        src = emblem_boosted if size < 64 else lockup_boosted
+        sq = square_on_bg(src, size, (0, 0, 0, 0), margin=0.04 if size < 64 else 0.10)
         entries.append((size, ico_image(sq)))
     n = write_ico(os.path.join(outdir, "icon.ico"), entries)
-    print(f"icon.ico: {n} bytes (16/32/48 emblem + 256 lockup, BMP entries)")
+    print(
+        f"icon.ico: {n} bytes (16/32/48 boosted emblem + 256 lockup, "
+        "transparent, BMP entries)"
+    )
 
-    # tray icon (not connected): monochrome + transparent (S16)
+    # S18 tray icon (not connected): monochrome + transparent, the same
+    # tighter margin (0.08 → 0.04).
     mono_emblem = desaturate(emblem_t)
     mono_lockup = desaturate(lockup_t)
     entries = []
     for size in (16, 32, 48, 256):
-        sq = square_on_bg(mono_emblem if size < 64 else mono_lockup, size, (0, 0, 0, 0))
+        sq = square_on_bg(
+            mono_emblem if size < 64 else mono_lockup,
+            size,
+            (0, 0, 0, 0),
+            margin=0.04 if size < 64 else 0.10,
+        )
         entries.append((size, ico_image(sq)))
     n = write_ico(os.path.join(outdir, "icon_mono.ico"), entries)
-    print(f"icon_mono.ico: {n} bytes (16/32/48 emblem + 256 lockup, mono)")
+    print(
+        f"icon_mono.ico: {n} bytes (16/32/48 emblem + 256 lockup, mono, "
+        "transparent, tighter margin)"
+    )
     return 0
 
 
diff --git a/src/real_service.rs b/src/real_service.rs
index 4c052b6..be86059 100755
--- a/src/real_service.rs
+++ b/src/real_service.rs
@@ -545,6 +545,11 @@ impl Default for SettingsFile {
                 // Default seed: the Material baseline primary #6750A4.
                 theme_seed: 0x6750A4,
                 dark_mode: false,
+                // S18: fresh installs follow the OS appearance (the user's
+                // ask); the legacy dark_mode stays false so an old binary
+                // reading this file lands on light, matching what a
+                // light-mode OS user sees.
+                theme_mode: "system".to_string(),
                 system_proxy: false,
                 // D23: "balanced" spreads new connections across the
                 // ticked adapters by LIVE byte load — the v0.7.0 default.
@@ -1959,6 +1964,9 @@ mod tests {
             settings: SettingsView {
                 theme_seed: 0x6750A4,
                 dark_mode: false,
+                // v0.5-era file shape: NO theme_mode key (the S18 legacy
+                // migration tests build on files like this one).
+                theme_mode: String::new(),
                 system_proxy,
                 merge_policy: "failover".to_string(),
                 autostart: false,
@@ -2301,6 +2309,7 @@ mod tests {
         let view = SettingsView {
             theme_seed: 0xEA4335,
             dark_mode: true,
+            theme_mode: "dark".to_string(),
             system_proxy: false, // never true in tests: autostart/sysproxy touch the host
             merge_policy: "lowestlatency".to_string(),
             autostart: false,
diff --git a/update_memory.py b/update_memory.py
index 63109da..fc69353 100755
--- a/update_memory.py
+++ b/update_memory.py
@@ -6,7 +6,15 @@ MEMORY_FILE = Path("PROJECT_MEMORY.md")
 NUM_COMMITS = 3  # Number of recent commits to pull diffs for
 
 def get_recent_git_diffs():
-    """Fetches recent commit metadata and diff patches using git log."""
+    """Fetches recent commit metadata and diff patches using git log.
+
+    S18 FIX: PROJECT_MEMORY.md is EXCLUDED from the embedded diffs. The
+    old script embedded its own previous section (each run's output is
+    committed into the file, so the next diff contained it) — the file
+    doubled every cycle and crossed GitHub's 100 MB per-file limit
+    (81 MB → 134 MB on 2026-11-21, push rejected). Long-form history
+    lives in harness/memory/LEDGER.md; the elision is noted per diff.
+    """
     try:
         # Run git log with patch output for the last N commits
         result = subprocess.run(
@@ -15,7 +23,9 @@ def get_recent_git_diffs():
                 f"-n{NUM_COMMITS}", 
                 "--pretty=format:### Commit: %h - %s (%an, %ad)", 
                 "--date=short", 
-                "-p"
+                "-p",
+                "--", ".",
+                ":(exclude)PROJECT_MEMORY.md",
             ],
             capture_output=True,
             text=True,
@@ -25,7 +35,7 @@ def get_recent_git_diffs():
             # .rgba bytes in diffs; never let that break the refresh.
             errors="replace"
         )
-        return result.stdout.strip()
+        return result.stdout.strip() + "\n\n( PROJECT_MEMORY.md's own diff is elided here — the durable\n  sections above are hand-maintained; see harness/memory/LEDGER.md\n  for the full stage history. )"
     except subprocess.CalledProcessError as e:
         print(f"[!] Error running git command: {e}")
         return ""

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

( PROJECT_MEMORY.md's own diff is elided here — the durable
  sections above are hand-maintained; see harness/memory/LEDGER.md
  for the full stage history. )
```

