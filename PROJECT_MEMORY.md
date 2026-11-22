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
### Commit: 4923566 - s20: Copilot code-review round (Lite) — 12 findings fixed, v0.13.1 — via POST /repos/.../pulls/1/copilot-code-review the cloud Copilot reviewer finally RAN from this sandbox (max + balanced effort were rejected by the account's remaining quota; Lite succeeded — the documented effort-ladder fallback). The standalone @github/copilot CLI path stays blocked: the fine-grained PAT lacks the "Copilot Requests" permission (api.individual.githubcopilot.com 401 "Personal Access Token does not have Copilot Requests permission" — authentic server response, network path fine; permission can only be granted by regenerating/editing the PAT in the web UI). All 12 findings addressed: (1) [high] connect step-0 self-heal now CLEARS sysproxy_set on a successful restore — the stale flag made a later no-session disconnect restore with NO snapshot, which DISABLES the user's pre-existing proxy (restore_plan(Ok(None)) = Disable); (2) [high] give_up's check-and-set serialized with disconnect via new Core::proxy_mtx (std Mutex, never held across an await — the black-hole BIND is hoisted out): the old window let an explicit disconnect restore the proxy and THEN have give_up's awaited sysproxy::set re-install the black-hole with sysproxy_set left true after disconnect returned; (3) [high] give_up BINDS the black-hole port BEFORE sysproxy::set and only re-points the proxy when ownership is proven — the old set-then-bind order let a local process that owned port 9 receive the black-holed traffic; bind failure = keep the stale (refused) entry, never route to a foreign listener; spawn_blackhole split into bind_blackhole (async, unlocked) + engage_blackhole (sync, under the lock); (4) [high] every restore path (both disconnect branches, kill-switch toggle-off, step-0) now restores FIRST and only releases ownership + the listener on SUCCESS — a failed restore keeps the black-hole engaged (fail-closed), logs an ERROR line and retries on the next disconnect/toggle instead of stranding a black-holed proxy with no owner and a false "restored" log; (5) [high] diagnostics header renders user-configurable URLs via new redact_url() — scheme://host:port/path only; userinfo (http://user:pass@), query (?token=) and fragment stripped (those slots carry API keys in custom probe/speed endpoints); unparseable URLs never echo; (6) [medium] enabling diagnostics on a LIVE session reconnects it (engines read the log level only at spawn — the sink silently missed engine debug output otherwise); (7) [medium] a failed diagnostics::enable REVERTS the persisted toggle + ERROR log (the UI previously showed the mode on with no file being written and no retry path); (8) [medium] exit probe + speed test only ride the live session when it was BUILT from the target profile (Session.profile_id; switching the active profile without reconnecting now falls back to the temporary probe tunnel instead of reporting the old tunnel's exit against the new profile — speed_test's old is_active check had the same hole); (9) [medium] speed-test response head capped independently at 64 KiB (SPEED_TEST_MAX_HEAD) — until \r\n\r\n lands nothing counts toward the 64 MiB payload cap, so a header-less byte flood could grow the buffer for the whole 30 s budget; (10) [medium] edit rejects port 0 (u16::parse accepted it; imported links and the dialog range are 1-65535); (11) [medium] edited REALITY material gets the link parser's structural rules (sni non-empty, pbk 43-char unpadded base64url via crate::vless::is_base64url, sid empty-or-1-16-hex) inside the existing capability gate — never-convertible profiles still edit freely; fingerprint intentionally unchecked (the import parser accepts any fp= verbatim — parity); (12) [medium] temp_link_for takes the FIRST link (top-priority bonded adapter — engine_specs_for preserves the user's order; pop() silently selected the LAST) — test_latency's identical pop() fixed for consistency. TESTS: 474/0 (+8: selfheal ownership clear, port-0/REALITY edit rejection, diag URL redaction, header flood, profile-switch fallback both probes, first-link end-to-end, diag enable revert, diag live reconnect). Gates: fmt/clippy host+windows-gnu -D warnings clean; vendored iced 6/0; mt-app --smoke 0; rig launch+render-healthy (no GUI code changed); zigbuild PE32+ x4 (multitunnel 19.2MB, mt-app 14.9MB, tray 10.1MB, merge-server 2.4MB) + ELF x2 (x86_64 1.23MB, aarch64 1.21MB); version 0.13.1; README v0.13.1 section + QUICKSTART note. (Z User, 2026-11-22)
diff --git a/Cargo.lock b/Cargo.lock
index ca35e7c..91f9783 100755
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1962,7 +1962,7 @@ dependencies = [
 
 [[package]]
 name = "mt-app"
-version = "0.13.0"
+version = "0.13.1"
 dependencies = [
  "async-trait",
  "iced",
@@ -2073,7 +2073,7 @@ dependencies = [
 
 [[package]]
 name = "multitunnel"
-version = "0.13.0"
+version = "0.13.1"
 dependencies = [
  "anyhow",
  "async-trait",
diff --git a/Cargo.toml b/Cargo.toml
index c40fea9..e436401 100755
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -18,7 +18,7 @@ members = ["crates/*"]
 resolver = "2"
 
 [workspace.package]
-version = "0.13.0"
+version = "0.13.1"
 
 [lib]
 name = "multitunnel"
diff --git a/QUICKSTART.md b/QUICKSTART.md
index fbe19d9..2662060 100755
--- a/QUICKSTART.md
+++ b/QUICKSTART.md
@@ -97,6 +97,17 @@ needed for the usual per-user system-proxy writes (expect the SmartScreen
 > proxy blocks traffic instead of silently going direct) and a
 > DIAGNOSTICS mode that writes one paste-ready support log file
 > (connection lifecycle + engine output, credentials redacted).
+>
+> v0.13.1 (Copilot code-review round): kill-switch hardening — the
+> black-hole port is owned BEFORE the proxy points at it, the proxy
+> hand-off is serialized with disconnect, a failed restore keeps the
+> block and retries, and a stale ownership flag can no longer disable a
+> pre-existing proxy; diagnostics now redact URL credentials, reconnect
+> a live session when enabled (so engines log at debug) and revert the
+> toggle if the log file cannot be created; exit-IP/speed probes only
+> ride a live session that belongs to the selected server and offline
+> probes use the top-priority bonded adapter; the Edit dialog rejects
+> port 0 and invalid REALITY fields.
 
 **SMOKE TEST — this is the first time the GUI binary runs on real Windows.**
 The development sandbox is Linux; the window has been cross-compiled and
diff --git a/README.md b/README.md
index 69d7e74..beca1eb 100755
--- a/README.md
+++ b/README.md
@@ -398,6 +398,25 @@ GUI, no new dependencies):
   settings/profile summary — UUIDs, passwords and public keys never
   appear in it. Turn it on, reproduce a connection issue, then paste the
   file into a support chat.
+- **v0.13.1 — Copilot code-review hardening**: a GitHub Copilot review
+  pass over the v0.13 changes produced twelve findings, all addressed.
+  The kill switch now proves ownership of the black-hole port BEFORE the
+  system proxy is re-pointed at it (a foreign process can never receive
+  the blocked traffic), the proxy hand-off is serialized with
+  disconnect (no re-block after an explicit disconnect), a failed
+  restore keeps the block and retries on the next disconnect, and the
+  connect self-heal clears stale ownership (a leftover flag could
+  otherwise disable a user's pre-existing proxy). Diagnostics redact
+  URL credentials (userinfo/query never reach the support file), a
+  failed log-file start reverts the toggle, and enabling the mode on a
+  live session reconnects it so the engines actually log at `debug`.
+  The exit-IP check and speed test only ride a live session that was
+  built from the selected server (switching servers without
+  reconnecting now falls back to a temporary probe tunnel), offline
+  probes use the top-priority bonded adapter, the speed test caps the
+  response head independently of the payload limit, and the Edit dialog
+  rejects port 0 and structurally invalid REALITY fields (empty SNI,
+  malformed public key / short ID).
 - **Quick Settings on right-click**: the tray menu is built fresh from
   live state — Connect/Disconnect (state-reflecting), the server list
   (radio-checked on the active profile), Open MultiTunnel and Exit.
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index 1747d50..b881b6a 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "666bc16 2026-11-21T14:58:33Z",
+  "as_of": "52f1d3c 2026-11-21T15:07:33Z",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 274,
-  "edge_count": 281,
+  "node_count": 275,
+  "edge_count": 282,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -695,6 +695,26 @@
         }
       ]
     },
+    {
+      "id": "commit:52f1d3c",
+      "type": "commit",
+      "title": "backup(s19-close-v0.13.0): 2026-11-21T15:07:33Z",
+      "tags": [
+        "2026-11-21"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "52f1d3c"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s19-close-v0.13.0): 2026-11-21T15:07:33Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:56340a5",
       "type": "commit",
@@ -8016,6 +8036,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s15)"
     },
+    {
+      "src": "commit:52f1d3c",
+      "dst": "ledger:s19",
+      "rel": "part_of",
+      "evidence": "subject stage label (s19)"
+    },
     {
       "src": "commit:56340a5",
       "dst": "ledger:s4",
diff --git a/src/real_service.rs b/src/real_service.rs
index 65e2703..0b8d457 100755
--- a/src/real_service.rs
+++ b/src/real_service.rs
@@ -161,6 +161,12 @@ const SPEED_TEST_MAX_BYTES: u64 = 64 * 1024 * 1024;
 /// S19: per-read stall limit inside a speed test — a server that trickles
 /// slower than this yields the partial measurement instead of hanging.
 const SPEED_TEST_STALL: Duration = Duration::from_secs(5);
+/// S20 (Copilot round 2): the response-head buffer in a speed test is
+/// capped INDEPENDENTLY of the 64 MiB payload limit — until `\r\n\r\n`
+/// is found nothing counts toward `total`, so an endpoint streaming a
+/// header-less byte flood would otherwise grow the buffer for the whole
+/// 30 s budget. A legitimate HTTP head never approaches 64 KiB.
+const SPEED_TEST_MAX_HEAD: usize = 64 * 1024;
 /// S19: the kill switch's black-hole proxy target — a loopback port
 /// nothing legitimately listens on (discard). With the system proxy
 /// pointed here, WinINET consumers get instant connection-refused
@@ -746,6 +752,12 @@ fn default_lan_rules() -> Vec<Rule> {
 /// One live connect session (server + its tasks + lifecycle signals).
 struct Session {
     server: Arc<ProxyServer>,
+    /// S20 (Copilot round 2): the profile this session was BUILT from —
+    /// the active profile can be switched without reconnecting, and the
+    /// exit probe / speed test must only ride the live listeners when
+    /// they actually belong to the target profile (otherwise the old
+    /// tunnel's exit gets reported against the new profile's endpoint).
+    profile_id: String,
     /// S9: present when stream bonding engaged — the per-carrier status
     /// surface AND the owner of the per-adapter engines.
     bond: Option<Arc<BondOutbound>>,
@@ -769,6 +781,14 @@ struct Core {
     /// Per-link `(up, down)` byte totals accrued from `ConnClosed` events.
     bytes: Mutex<HashMap<String, (u64, u64)>>,
     sysproxy_set: AtomicBool,
+    /// S20 (Copilot round 2): serializes every system-proxy hand-off
+    /// (give_up's kill-switch engage, both disconnect restore paths, the
+    /// kill-switch toggle-off escape hatch, connect step-0 self-heal and
+    /// the connect-time arming) so a `give_up` racing an explicit
+    /// disconnect can never re-install the black-hole AFTER the disconnect
+    /// already restored the user's proxy. Std lock, never held across an
+    /// await (the black-hole BIND is hoisted out, see `give_up`).
+    proxy_mtx: Mutex<()>,
     /// S19 (review fix): the kill switch's black-hole listener — while
     /// engaged we OWN [`KILL_SWITCH_BLACKHOLE_PORT`] so a local process
     /// cannot pre-bind it and turn the black hole into its own proxy.
@@ -867,6 +887,7 @@ impl RealService {
                 session: Mutex::new(None),
                 bytes: Mutex::new(HashMap::new()),
                 sysproxy_set: AtomicBool::new(false),
+                proxy_mtx: Mutex::new(()),
                 blackhole: Mutex::new(None),
                 disconnect_requested: AtomicBool::new(false),
                 restarts: AtomicU32::new(0),
@@ -918,8 +939,25 @@ impl RealService {
         //    may have left the registry pointing at a dead local listener.
         //    Best-effort — a failure never blocks connecting. This also
         //    clears any KILL SWITCH black-hole from a previous session.
-        let _ = crate::sysproxy::restore(&self.core.paths.state_dir);
-        stop_blackhole(&self.core);
+        //    S20 (Copilot round 2): under the proxy lock — and the OWNERSHIP
+        //    flag clears with a successful restore. Leaving it set after
+        //    the self-heal consumed the snapshot meant a later disconnect
+        //    "restored" with no snapshot, which DISABLES the user's
+        //    pre-existing proxy instead of leaving it untouched.
+        {
+            let _guard = lock(&self.core.proxy_mtx);
+            let engaged = self.core.sysproxy_set.load(Ordering::SeqCst);
+            if crate::sysproxy::restore(&self.core.paths.state_dir).is_ok() {
+                self.core.sysproxy_set.store(false, Ordering::SeqCst);
+                stop_blackhole(&self.core);
+            } else if !engaged {
+                // Nothing owned — release any dangling listener.
+                stop_blackhole(&self.core);
+            }
+            // engaged && restore failed: keep the flag AND the listener —
+            // the proxy still points at the black hole (fail-closed) and
+            // the next disconnect retries the restore.
+        }
 
         let settings = SettingsFile::load(&self.core.paths.settings_file());
         // S9: refresh the engine log level BEFORE links are built.
@@ -1140,6 +1178,7 @@ impl RealService {
 
         let session = Session {
             server,
+            profile_id: profile.id.clone(),
             bond,
             shutdown: shutdown_tx,
             run_handle,
@@ -1162,6 +1201,9 @@ impl RealService {
         // System proxy AFTER Connected, non-fatal (step 5 of the v0.2
         // connect_sequence discipline). Skipped for ephemeral binds (port 0
         // in settings — the tests' mode; a registry write would be pointless).
+        // S20 (Copilot round 2): under the proxy lock — an explicit
+        // disconnect landing mid-connect must never observe a half-armed
+        // proxy hand-off.
         if settings.settings.system_proxy && settings.socks_port != 0 && settings.http_port != 0 {
             let http_addr = lock(&self.core.session)
                 .as_ref()
@@ -1170,6 +1212,7 @@ impl RealService {
                 // The v0.2 full LAN-bypass override (S6-REV-C P1-4): LAN and
                 // loopback hosts must never ride the tunnel.
                 let override_list = crate::config::default_proxy_override();
+                let _guard = lock(&self.core.proxy_mtx);
                 match crate::sysproxy::set(
                     &http.to_string(),
                     &override_list,
@@ -1288,18 +1331,31 @@ async fn pump(
 /// fail-closed); only the local pre-bind guarantee is lost. On Windows an
 /// unprivileged process may bind port 9; dev hosts without that right
 /// simply take the fallback.
-async fn spawn_blackhole(core: &Core) {
+///
+/// S20 (Copilot round 2): split into [`bind_blackhole`] (the async bind,
+/// hoisted OUT of the proxy lock) and [`engage_blackhole`] (the sync
+/// accept-loop install) so `give_up` can bind FIRST and only point the
+/// system proxy at the port once ownership is proven — the old order
+/// (set-then-bind) left a window where a local process could own port 9
+/// and receive the black-holed traffic.
+async fn bind_blackhole() -> Option<tokio::net::TcpListener> {
     let addr = SocketAddr::from(([127, 0, 0, 1], KILL_SWITCH_BLACKHOLE_PORT));
-    let listener = match tokio::net::TcpListener::bind(addr).await {
-        Ok(l) => l,
+    match tokio::net::TcpListener::bind(addr).await {
+        Ok(l) => Some(l),
         Err(e) => {
             tracing::warn!(
                 "kill switch: could not own the black-hole port {addr} ({e}) \
                  — dead-port fallback (still fail-closed)"
             );
-            return;
+            None
         }
-    };
+    }
+}
+
+/// S20 (Copilot round 2): install the accept loop for an ALREADY-BOUND
+/// listener and record it in [`Core::blackhole`]. Sync by design — called
+/// with the proxy lock held (the bind was done beforehand).
+fn engage_blackhole(core: &Core, listener: tokio::net::TcpListener) {
     let (tx, mut rx) = tokio::sync::watch::channel(false);
     tokio::spawn(async move {
         loop {
@@ -1348,48 +1404,81 @@ fn stop_blackhole(core: &Core) {
 ///   listeners die with the session and proxied connections were already
 ///   refused when all links were down.
 ///
+/// S20 (Copilot round 2) hardening:
+/// - the black-hole port is BOUND FIRST — `sysproxy::set` only runs once
+///   ownership is proven, so a local process that owns port 9 can never
+///   receive the black-holed traffic (the stale proxy entry keeps
+///   blocking instead, which is still fail-closed);
+/// - the whole hand-off runs under [`Core::proxy_mtx`], so an explicit
+///   `disconnect()` can no longer restore the user's proxy in the window
+///   between `give_up`'s flag read and its registry write (which used to
+///   re-install the black-hole AFTER the disconnect had returned).
+///
 /// Best-effort throughout — the app never dies invisibly (D15).
 async fn give_up(core: &Core, shutdown: &tokio::sync::watch::Sender<bool>) {
     tracing::warn!("supervisor: all tunnel links exhausted — giving up");
-    if core.sysproxy_set.load(Ordering::SeqCst) {
-        let kill_switch = SettingsFile::load(&core.paths.settings_file())
-            .settings
-            .kill_switch;
-        if kill_switch {
-            // Keep sysproxy_set TRUE: we still own the proxy state (the
-            // black hole IS ours); the next explicit disconnect or connect
-            // self-heal restores it.
-            let override_list = crate::config::default_proxy_override();
-            match crate::sysproxy::set(
-                KILL_SWITCH_BLACKHOLE_PROXY,
-                &override_list,
-                &core.paths.state_dir,
-            ) {
-                Ok(()) => {
-                    // Review fix: OWN the port so a local process cannot
-                    // pre-bind it and turn the black hole into a proxy of
-                    // its own (the accept loop swallows connections).
-                    spawn_blackhole(core).await;
+    // Bind BEFORE any registry write (and outside the proxy lock — the
+    // bind awaits). Ownership is proven up front; the listener is dropped
+    // unused whenever the decisions below do not engage the kill switch.
+    let listener = bind_blackhole().await;
+    {
+        let _guard = lock(&core.proxy_mtx);
+        if core.sysproxy_set.load(Ordering::SeqCst) {
+            let kill_switch = SettingsFile::load(&core.paths.settings_file())
+                .settings
+                .kill_switch;
+            if kill_switch {
+                // Keep sysproxy_set TRUE: we still own the proxy state (the
+                // black hole IS ours); the next explicit disconnect or
+                // connect self-heal restores it.
+                let override_list = crate::config::default_proxy_override();
+                if let Some(listener) = listener {
+                    match crate::sysproxy::set(
+                        KILL_SWITCH_BLACKHOLE_PROXY,
+                        &override_list,
+                        &core.paths.state_dir,
+                    ) {
+                        Ok(()) => {
+                            // Review fix: OWN the port so a local process
+                            // cannot pre-bind it and turn the black hole
+                            // into a proxy of its own (the accept loop
+                            // swallows connections).
+                            engage_blackhole(core, listener);
+                            core.emit(ServiceEvent::LogLine(
+                                "KILL SWITCH engaged: the tunnel died on its own — system traffic is \
+                                 now BLOCKED (fail-closed) instead of leaking direct. Connect again \
+                                 to resume, or toggle the kill switch off in Settings."
+                                    .into(),
+                            ));
+                        }
+                        Err(e) => {
+                            // The registry still points at OUR dead local
+                            // listener — still fail-closed, just less tidy.
+                            // The freshly-bound listener is useless now
+                            // (nothing points at port 9): release it.
+                            drop(listener);
+                            core.emit(ServiceEvent::LogLine(format!(
+                                "KILL SWITCH: could not engage the black-hole proxy ({e}) — the stale \
+                                 proxy entry keeps blocking traffic (fail-closed)"
+                            )));
+                        }
+                    }
+                } else {
+                    // S20 (Copilot round 2): port 9 is owned by someone else
+                    // (or unbindable) — do NOT point the system proxy at
+                    // it; the stale entry keeps refusing (fail-closed).
                     core.emit(ServiceEvent::LogLine(
-                        "KILL SWITCH engaged: the tunnel died on its own — system traffic is \
-                         now BLOCKED (fail-closed) instead of leaking direct. Connect again \
-                         to resume, or toggle the kill switch off in Settings."
+                        "KILL SWITCH: could not own the black-hole port — the stale proxy entry \
+                         keeps blocking traffic (fail-closed; nothing is routed to a foreign \
+                         listener)"
                             .into(),
                     ));
                 }
-                Err(e) => {
-                    // The registry still points at OUR dead local listener
-                    // — still fail-closed, just less tidy.
-                    core.emit(ServiceEvent::LogLine(format!(
-                        "KILL SWITCH: could not engage the black-hole proxy ({e}) — the stale \
-                         proxy entry keeps blocking traffic (fail-closed)"
-                    )));
+            } else if core.sysproxy_set.swap(false, Ordering::SeqCst) {
+                if let Err(e) = crate::sysproxy::restore(&core.paths.state_dir) {
+                    tracing::warn!("system proxy restore failed: {e}");
                 }
             }
-        } else if core.sysproxy_set.swap(false, Ordering::SeqCst) {
-            if let Err(e) = crate::sysproxy::restore(&core.paths.state_dir) {
-                tracing::warn!("system proxy restore failed: {e}");
-            }
         }
     }
     // Retire the session so status/profiles views stop reporting the dead
@@ -1499,6 +1588,31 @@ pub fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
     Some((host.to_string(), port, path))
 }
 
+/// S20 (Copilot round 2): a support-file-safe rendering of a
+/// user-configurable URL — scheme + host + port + path ONLY. Userinfo
+/// (`http://user:pass@host/`), query strings (`?token=...`) and
+/// fragments are stripped: those slots routinely carry API keys in
+/// custom probe/speed endpoints, and the diagnostics file is designed
+/// to be pasted into a chat verbatim. An unparseable URL renders as a
+/// fixed placeholder (never echoed — the raw value may embed a secret).
+fn redact_url(url: &str) -> String {
+    match parse_http_url(url) {
+        Some((host, port, path)) => {
+            // parse_http_url leaves userinfo attached to the host
+            // ("user:pass@host") — strip everything up to the last '@'.
+            let host = match host.rsplit_once('@') {
+                Some((_, h)) => h.to_string(),
+                None => host,
+            };
+            // Path only up to the query ('?' — parse_http_url keeps it in
+            // the path) and the fragment ('#').
+            let path = path.split(['?', '#']).next().unwrap_or("/").to_string();
+            format!("http://{host}:{port}{path}")
+        }
+        None => "<redacted (invalid URL)>".to_string(),
+    }
+}
+
 /// `host:port` → [`TargetAddr`] (IP literals become [`Host::Ip`]; brackets
 /// for IPv6 are unwrapped by the v0.2 parser we reuse).
 pub fn parse_probe_target(spec: &str) -> Option<TargetAddr> {
@@ -1613,13 +1727,30 @@ impl MtService for RealService {
             // black-hole (or any proxy state we still own from a session
             // that already died) — the user asked to stop tunneling, not
             // to keep the network blocked.
-            if self.core.sysproxy_set.swap(false, Ordering::SeqCst) {
-                if let Err(e) = crate::sysproxy::restore(&self.core.paths.state_dir) {
-                    tracing::warn!("system proxy restore failed: {e}");
+            // S20 (Copilot round 2): under the proxy lock, and the
+            // ownership/listener are only released when the restore
+            // SUCCEEDED — a failed restore keeps the black-hole engaged
+            // (fail-closed) and this same path retries on the next
+            // disconnect; the "restored" log line is only truthful then.
+            {
+                let _guard = lock(&self.core.proxy_mtx);
+                if self.core.sysproxy_set.load(Ordering::SeqCst) {
+                    match crate::sysproxy::restore(&self.core.paths.state_dir) {
+                        Ok(()) => {
+                            self.core.sysproxy_set.store(false, Ordering::SeqCst);
+                            stop_blackhole(&self.core);
+                            self.core
+                                .emit(ServiceEvent::LogLine("system proxy restored".into()));
+                        }
+                        Err(e) => {
+                            tracing::warn!("system proxy restore failed: {e}");
+                            self.core.emit(ServiceEvent::LogLine(format!(
+                                "ERROR system proxy restore failed: {e} — the kill switch stays \
+                                 engaged; disconnect again to retry"
+                            )));
+                        }
+                    }
                 }
-                stop_blackhole(&self.core);
-                self.core
-                    .emit(ServiceEvent::LogLine("system proxy restored".into()));
             }
             return;
         };
@@ -1631,11 +1762,25 @@ impl MtService for RealService {
         drop(session.shutdown); // run() also exits when the sender drops
         let _ = session.run_handle.await;
         let _ = session.pump_handle.await;
-        if self.core.sysproxy_set.swap(false, Ordering::SeqCst) {
-            if let Err(e) = crate::sysproxy::restore(&self.core.paths.state_dir) {
-                tracing::warn!("system proxy restore failed: {e}");
+        // S20 (Copilot round 2): same locked, restore-first discipline as
+        // the no-session branch above.
+        {
+            let _guard = lock(&self.core.proxy_mtx);
+            if self.core.sysproxy_set.load(Ordering::SeqCst) {
+                match crate::sysproxy::restore(&self.core.paths.state_dir) {
+                    Ok(()) => {
+                        self.core.sysproxy_set.store(false, Ordering::SeqCst);
+                        stop_blackhole(&self.core);
+                    }
+                    Err(e) => {
+                        tracing::warn!("system proxy restore failed: {e}");
+                        self.core.emit(ServiceEvent::LogLine(format!(
+                            "ERROR system proxy restore failed: {e} — the kill switch stays \
+                             engaged; disconnect again to retry"
+                        )));
+                    }
+                }
             }
-            stop_blackhole(&self.core);
         }
         self.core.bytes_reset();
         self.core.set_state(ServiceState::Disconnected);
@@ -1711,13 +1856,16 @@ impl MtService for RealService {
             &settings.settings.bonded_adapters,
             &mt_platform::enumerate_adapters(),
         );
-        let mut links = self
+        let links = self
             .core
             .factory
             .build(&profile, &bonded)
             .await
             .map_err(|e| e.to_string())?;
-        let Some(link) = links.pop() else {
+        // S20 (Copilot round 2): the FIRST link is the top-priority
+        // adapter (engine_specs_for preserves the user's order) — pop()
+        // used to probe through the lowest-priority one.
+        let Some(link) = links.first().cloned() else {
             return Err("link factory produced no links".to_string());
         };
         let target = parse_probe_target(&settings.probe_target)
@@ -1814,19 +1962,22 @@ impl MtService for RealService {
         // on them AFTER the (successful) persist below.
         let diagnostics_changed = file.settings.diagnostics != settings.diagnostics;
         let kill_switch_disabled = file.settings.kill_switch && !settings.kill_switch;
+        // S20 (Copilot round 2): the engines only pick up the debug log
+        // level at SPAWN time, so enabling diagnostics while CONNECTED
+        // must reconnect — otherwise the file silently misses the engine
+        // debug output the UI promises.
+        let diag_enabled_live = !file.settings.diagnostics
+            && settings.diagnostics
+            && self.core.get_state() == ServiceState::Connected;
+        // S20: only set when the sink actually started (a failed + reverted
+        // enable must NOT trigger the live reconnect below).
+        let mut diag_started = false;
         let mut settings = settings;
         settings.bonded_adapters = file.settings.bonded_adapters.clone();
         file.settings = settings.clone();
         file.sync_legacy();
         file.save(&self.core.paths.settings_file())
             .map_err(|e| e.to_string())?;
-        if dataplane_changed && self.core.get_state() == ServiceState::Connected {
-            self.core.emit(ServiceEvent::LogLine(
-                "data-plane settings changed — reconnecting".into(),
-            ));
-            self.disconnect().await;
-            self.connect().await;
-        }
         // S19: diagnostics mode transitions (only after a successful save —
         // a rejected settings change never touches the log file).
         if diagnostics_changed {
@@ -1834,14 +1985,27 @@ impl MtService for RealService {
                 let header = self.build_diag_header(&settings);
                 match crate::diagnostics::enable(&self.core.paths.log_dir, &header) {
                     Ok(p) => {
+                        diag_started = true;
                         self.core.emit(ServiceEvent::LogLine(format!(
                             "INFO  diagnostics mode ON — writing {}",
                             p.display()
                         )));
                     }
                     Err(e) => {
+                        // S20 (Copilot round 2): the toggle must not stay
+                        // persisted-ON with no sink behind it (the UI
+                        // would show the mode enabled while nothing is
+                        // written, and a later settings save would skip
+                        // the enable because the diff vanished). Revert
+                        // the persisted flag and say so.
+                        file.settings.diagnostics = false;
+                        file.sync_legacy();
+                        if let Err(save_err) = file.save(&self.core.paths.settings_file()) {
+                            tracing::warn!("reverting the diagnostics toggle failed: {save_err}");
+                        }
                         self.core.emit(ServiceEvent::LogLine(format!(
-                            "WARN  diagnostics mode could not start: {e}"
+                            "ERROR diagnostics mode could not start: {e} — the toggle was \
+                             reverted; fix the cause and enable it again"
                         )));
                     }
                 }
@@ -1852,18 +2016,49 @@ impl MtService for RealService {
                 )));
             }
         }
+        // Reconnect when the LIVE data plane changed, or when diagnostics
+        // was enabled on a live session (S20: the engines re-read their
+        // log level only at spawn — see diag_enabled_live above). Both
+        // AFTER the diagnostics transition so connect_flow sees the new
+        // sink state.
+        if (dataplane_changed || (diag_enabled_live && diag_started))
+            && self.core.get_state() == ServiceState::Connected
+        {
+            self.core.emit(ServiceEvent::LogLine(if dataplane_changed {
+                "data-plane settings changed — reconnecting".into()
+            } else {
+                "diagnostics on — reconnecting so the engines log at debug".into()
+            }));
+            self.disconnect().await;
+            self.connect().await;
+        }
         // S19: turning the kill switch off is an explicit escape hatch for
         // an engaged black-hole (the give_up path keeps the proxy blocked
-        // while the switch stays on).
-        if kill_switch_disabled
-            && self.core.get_state() != ServiceState::Connected
-            && self.core.sysproxy_set.swap(false, Ordering::SeqCst)
-        {
-            let _ = crate::sysproxy::restore(&self.core.paths.state_dir);
-            stop_blackhole(&self.core);
-            self.core.emit(ServiceEvent::LogLine(
-                "kill switch disabled — system proxy restored".into(),
-            ));
+        // while the switch stays on). S20 (Copilot round 2): under the
+        // proxy lock, restore-first — ownership and the listener are only
+        // released on a successful restore, so a failed escape hatch can
+        // be retried instead of stranding a black-holed proxy with no
+        // owner.
+        if kill_switch_disabled && self.core.get_state() != ServiceState::Connected {
+            let _guard = lock(&self.core.proxy_mtx);
+            if self.core.sysproxy_set.load(Ordering::SeqCst) {
+                match crate::sysproxy::restore(&self.core.paths.state_dir) {
+                    Ok(()) => {
+                        self.core.sysproxy_set.store(false, Ordering::SeqCst);
+                        stop_blackhole(&self.core);
+                        self.core.emit(ServiceEvent::LogLine(
+                            "kill switch disabled — system proxy restored".into(),
+                        ));
+                    }
+                    Err(e) => {
+                        tracing::warn!("system proxy restore failed: {e}");
+                        self.core.emit(ServiceEvent::LogLine(format!(
+                            "ERROR system proxy restore failed: {e} — the black hole stays \
+                             engaged; toggle the kill switch off again to retry"
+                        )));
+                    }
+                }
+            }
         }
         if autostart_changed {
             let exe = std::env::current_exe()
@@ -1946,13 +2141,10 @@ impl MtService for RealService {
         //    session, fall back to a TEMPORARY probe tunnel through the
         //    active profile — same exit identity, works while the app is
         //    disconnected, no session left behind.
-        let socks_addr = {
-            let guard = lock(&self.core.session);
-            match guard.as_ref() {
-                Some(s) => s.server.socks_local_addr(),
-                None => None,
-            }
-        };
+        //    S20 (Copilot round 2): the live listeners only count when the
+        //    session was BUILT from this profile — the active profile can
+        //    be switched without reconnecting, and the old tunnel's exit
+        //    must never be reported against the new profile's endpoint.
         let store = self.core.load_store_lossy();
         let Some(profile) = store
             .active
@@ -1962,6 +2154,13 @@ impl MtService for RealService {
         else {
             return Err("no active profile — pick a server first".to_string());
         };
+        let socks_addr = {
+            let guard = lock(&self.core.session);
+            match guard.as_ref() {
+                Some(s) if s.profile_id == profile.id => s.server.socks_local_addr(),
+                _ => None,
+            }
+        };
         let settings = SettingsFile::load(&self.core.paths.settings_file());
         let url = settings.settings.exit_probe_url;
         let (host, port, path) =
@@ -2047,17 +2246,15 @@ impl MtService for RealService {
             },
             port,
         };
-        // 2. The LIVE session only counts when it belongs to the target
-        //    (the active profile) — otherwise a temporary tunnel through
-        //    the target profile.
-        let is_active = store.active.as_deref() == Some(profile.id.as_str());
-        let live_socks = if is_active {
-            lock(&self.core.session)
-                .as_ref()
-                .and_then(|s| s.server.socks_local_addr())
-        } else {
-            None
-        };
+        // 2. The LIVE session only counts when it was BUILT from the
+        //    target profile (S20, Copilot round 2 — the active profile can
+        //    be switched without reconnecting, so matching on "is the
+        //    target the active one" is not enough); otherwise a temporary
+        //    tunnel through the target profile.
+        let live_socks = lock(&self.core.session)
+            .as_ref()
+            .filter(|s| s.profile_id == profile.id)
+            .and_then(|s| s.server.socks_local_addr());
         let via_temp = live_socks.is_none();
         let connect = async {
             Ok::<_, anyhow::Error>(if let Some(socks) = live_socks {
@@ -2122,6 +2319,17 @@ impl MtService for RealService {
                     // when the head straddled reads).
                     total += head.len() as u64 - split as u64;
                     first = false;
+                } else if head.len() > SPEED_TEST_MAX_HEAD {
+                    // S20 (Copilot round 2): still inside the head past
+                    // 64 KiB — no legitimate HTTP head is this big. Cap
+                    // INDEPENDENTLY of the payload limit: until the
+                    // terminator is found nothing counts toward `total`,
+                    // so a header-less byte flood would otherwise grow
+                    // this buffer for the whole 30 s budget.
+                    return Err(format!(
+                        "speed test: malformed response (no header end within {} KiB)",
+                        SPEED_TEST_MAX_HEAD / 1024
+                    ));
                 }
                 // Still inside the head (a slow server dribbling headers):
                 // keep buffering, nothing counts yet.
@@ -2194,7 +2402,16 @@ impl MtService for RealService {
             .port
             .trim()
             .parse::<u16>()
-            .map_err(|_| format!("port must be a number 0-65535 ({:?})", edit.port))?;
+            .map_err(|_| format!("port must be a number 1-65535 ({:?})", edit.port))?;
+        // S20 (Copilot round 2): u16::parse happily accepts 0, but a
+        // server port of 0 is meaningless (imported links reject it; the
+        // engine would only fail later at connect time).
+        if updated.port == 0 {
+            return Err(format!(
+                "port 0 is not a valid server port ({:?}) — use 1-65535",
+                edit.port
+            ));
+        }
         let identity = edit.identity.trim().to_string();
         if identity.is_empty() {
             return Err("identity (UUID/password) cannot be empty".to_string());
@@ -2220,9 +2437,38 @@ impl MtService for RealService {
         // Capability regression check: if the ORIGINAL could drive the
         // engine, the edited one must still be able to (fail loud BEFORE
         // the save; a profile that never could connect may be edited
-        // freely — its inability predates the edit).
+        // freely — its inability predates the edit). S20 (Copilot round
+        // 2): for convertible profiles the edited REALITY material gets
+        // the SAME structural validation the link parser enforces — an
+        // empty SNI, a wrong-length/non-base64url public key or an
+        // invalid short ID must not silently reach the Xray config.
         if endpoint_from_profile(&original).is_ok() {
             endpoint_from_profile(&updated).map_err(|e| e.to_string())?;
+            if let Security::Reality {
+                sni,
+                fingerprint: _,
+                public_key,
+                short_id,
+                spider_x: _,
+            } = &updated.security
+            {
+                if sni.is_empty() {
+                    return Err("sni cannot be empty (the REALITY serverName)".to_string());
+                }
+                if public_key.len() != 43 || !public_key.bytes().all(crate::vless::is_base64url) {
+                    return Err(format!(
+                        "public key must be 43 unpadded base64url characters \
+                         (a 32-byte X25519 key); got {}",
+                        public_key.len()
+                    ));
+                }
+                if short_id.len() > 16 || !short_id.bytes().all(|c| c.is_ascii_hexdigit()) {
+                    return Err(format!(
+                        "short id must be empty or 1-16 hex characters; got {}",
+                        short_id.len()
+                    ));
+                }
+            }
         }
         if !store.update(&target, updated.clone()) {
             return Err(format!("no profile matches {id:?}"));
@@ -2325,20 +2571,24 @@ fn parse_exit_info(body: &[u8]) -> Result<ExitDoc, String> {
 /// S19: build a one-shot link for `profile` (offline probes: exit-IP,
 /// speed test). Mirrors `test_latency`'s factory use — engines spawn
 /// lazily and die with the returned Arcs. With several bonded adapters
-/// the FIRST link (the top-priority adapter) carries the probe.
+/// the FIRST link (the top-priority adapter) carries the probe — S20
+/// (Copilot round 2): `engine_specs_for` preserves the user's priority
+/// order, and `pop()` used to silently select the LAST (lowest-priority)
+/// link instead.
 async fn temp_link_for(core: &Core, profile: &Profile) -> Result<Arc<dyn Outbound>, String> {
     let settings = SettingsFile::load(&core.paths.settings_file());
     let bonded = resolve_bonded(
         &settings.settings.bonded_adapters,
         &mt_platform::enumerate_adapters(),
     );
-    let mut links = core
+    let links = core
         .factory
         .build(profile, &bonded)
         .await
         .map_err(|e| e.to_string())?;
     links
-        .pop()
+        .into_iter()
+        .next()
         .ok_or_else(|| "link factory produced no links".to_string())
 }
 
@@ -2435,7 +2685,9 @@ impl RealService {
         ));
         out.push_str(&format!(
             "probe_target={} exit_probe_url={} speed_test_url={}\n",
-            settings.probe_target, settings.exit_probe_url, settings.speed_test_url
+            settings.probe_target,
+            redact_url(&settings.exit_probe_url),
+            redact_url(&settings.speed_test_url)
         ));
         let store = self.core.load_store_lossy();
         out.push_str("=== profiles (endpoints only — ids/secrets redacted) ===\n");
@@ -3099,12 +3351,15 @@ mod tests {
         assert!(saw_restore, "the restore is announced in the GUI log");
     }
 
-    /// S19 (review fix): spawn_blackhole OWNS the port while engaged (a
+    /// S19 (review fix): the black-hole OWNERS the port while engaged (a
     /// local process cannot pre-bind it) and stop_blackhole releases it —
     /// the accept loop swallows connections (an open TCP dial to it lands
     /// and closes immediately instead of being answered by anyone else).
-    /// On hosts where binding port 9 is not permitted the spawn is a
+    /// On hosts where binding port 9 is not permitted the bind is a
     /// documented no-op fallback (dev/linux root-only port).
+    /// S20 (Copilot round 2): the bind and the engage are separate steps —
+    /// give_up binds FIRST and only points the proxy at the port once
+    /// ownership is proven.
     #[tokio::test]
     async fn blackhole_listener_owns_and_releases_the_port() {
         let exit = spawn_exit_echo("127.0.0.1").await;
@@ -3117,10 +3372,12 @@ mod tests {
                 fail: false,
             }),
         );
-        // Engage.
-        spawn_blackhole(&svc.core).await;
-        let guard_taken = svc.core.blackhole.lock().unwrap().is_some();
-        if guard_taken {
+        // Engage (bind first, then install the accept loop — the give_up
+        // order).
+        if let Some(listener) = bind_blackhole().await {
+            engage_blackhole(&svc.core, listener);
+            let guard_taken = svc.core.blackhole.lock().unwrap().is_some();
+            assert!(guard_taken, "engage records the listener");
             // The port answers to US: a connect lands (accepted + dropped).
             let dial = tokio::net::TcpStream::connect("127.0.0.1:9").await;
             assert!(dial.is_ok(), "our own listener owns the port");
@@ -3137,6 +3394,393 @@ mod tests {
         }
     }
 
+    /// S20 (Copilot round 2, high finding): the connect step-0 self-heal
+    /// must CLEAR `sysproxy_set` together with the restore — after a
+    /// kill-switch give-up + reconnect, a stale-true flag made a later
+    /// no-session disconnect "restore" with no snapshot (which DISABLES
+    /// the user's pre-existing proxy instead of leaving it untouched).
+    #[tokio::test]
+    async fn connect_selfheal_clears_stale_proxy_ownership() {
+        let exit = spawn_exit_echo("127.0.0.1").await;
+        let paths = scratch("selfheal");
+        write_settings(&paths, exit.port(), false);
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: exit,
+                fail: false,
+            }),
+        );
+        let view = svc.import_link(VLESS_REALITY).await.expect("import");
+        svc.set_active_profile(&view.tag).await.expect("active");
+        // Simulate the post-give-up state: ownership armed, no session.
+        svc.core.sysproxy_set.store(true, Ordering::SeqCst);
+        svc.connect().await;
+        assert_eq!(svc.status().await.state, ServiceState::Connected);
+        assert!(
+            !svc.core.sysproxy_set.load(Ordering::SeqCst),
+            "the step-0 self-heal consumed the snapshot — ownership MUST clear"
+        );
+        svc.disconnect().await;
+        assert!(!svc.core.sysproxy_set.load(Ordering::SeqCst));
+    }
+
+    /// S20 (Copilot round 2, medium findings): the edit dialog must reject
+    /// port 0 and REALITY material the link parser would refuse — an empty
+    /// SNI, a wrong-length/non-base64url public key or an invalid short ID
+    /// must not silently reach the Xray config.
+    #[tokio::test]
+    async fn update_profile_rejects_port_zero_and_invalid_reality() {
+        let exit = spawn_exit_echo("127.0.0.1").await;
+        let paths = scratch("editreality");
+        write_settings(&paths, exit.port(), false);
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: exit,
+                fail: false,
+            }),
+        );
+        let view = svc.import_link(VLESS_REALITY).await.expect("import");
+        svc.set_active_profile(&view.tag).await.expect("active");
+
+        // Port 0 parses as u16 but is not a server port.
+        let mut d = svc.profile_details(&view.id).await.expect("details");
+        d.port = "0".into();
+        let err = svc.update_profile(&view.id, d).await.unwrap_err();
+        assert!(err.contains("port 0"), "{err}");
+        // Empty SNI.
+        let mut d = svc.profile_details(&view.id).await.expect("details");
+        d.sni = "   ".into();
+        let err = svc.update_profile(&view.id, d).await.unwrap_err();
+        assert!(err.contains("sni"), "{err}");
+        // Wrong-length public key.
+        let mut d = svc.profile_details(&view.id).await.expect("details");
+        d.public_key = "short".into();
+        let err = svc.update_profile(&view.id, d).await.unwrap_err();
+        assert!(err.contains("public key"), "{err}");
+        // Non-hex short id.
+        let mut d = svc.profile_details(&view.id).await.expect("details");
+        d.short_id = "zzzz".into();
+        let err = svc.update_profile(&view.id, d).await.unwrap_err();
+        assert!(err.contains("short id"), "{err}");
+        // A fully valid edit still saves; an EMPTY short id is legal
+        // (the parser allows `sid=` empty).
+        let mut d = svc.profile_details(&view.id).await.expect("details");
+        d.short_id = "".into();
+        d.tag = "still fine".into();
+        let updated = svc.update_profile(&view.id, d).await.expect("valid edit");
+        assert_eq!(updated.tag, "still fine");
+        // Nothing from the rejected edits leaked into the store.
+        let store = svc.core.load_store().expect("reload");
+        assert_eq!(store.profiles[0].tag, "still fine");
+        assert_eq!(store.profiles[0].port, 443);
+    }
+
+    /// S20 (Copilot round 2, high finding): user-configurable URLs carry
+    /// credentials in userinfo/query — the diagnostics header must render
+    /// host:port:path only (the file is pasted into chats verbatim).
+    #[tokio::test]
+    async fn diag_header_redacts_url_credentials() {
+        let exit = spawn_exit_echo("127.0.0.1").await;
+        let paths = scratch("diagredact");
+        write_settings(&paths, exit.port(), false);
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: exit,
+                fail: false,
+            }),
+        );
+        let mut s = svc.settings().await;
+        s.exit_probe_url = "http://user:hunter2@probe.example.com:8080/path?token=T0PS3CRET".into();
+        s.speed_test_url = "http://speed.example.com/down?key=abc123#frag".into();
+        let header = svc.build_diag_header(&s);
+        assert!(header.contains("probe.example.com:8080/path"), "{header}");
+        assert!(header.contains("speed.example.com:80/down"), "{header}");
+        for secret in ["hunter2", "user:", "T0PS3CRET", "abc123", "#frag"] {
+            assert!(!header.contains(secret), "leaked {secret:?} in:\n{header}");
+        }
+        // Unparseable URLs never echo the raw value.
+        s.exit_probe_url = "not a url at all".into();
+        let header = svc.build_diag_header(&s);
+        assert!(header.contains("<redacted (invalid URL)>"), "{header}");
+        assert!(!header.contains("not a url"), "{header}");
+    }
+
+    /// S20 (Copilot round 2, medium finding): until `\r\n\r\n` is found
+    /// nothing counts toward the 64 MiB payload cap — an endpoint flooding
+    /// header-less bytes must hit the independent 64 KiB head cap fast
+    /// instead of growing the buffer for the whole 30 s budget.
+    #[tokio::test]
+    async fn speed_test_rejects_a_header_flood() {
+        // A server that answers with 256 KiB of header-less bytes and then
+        // holds the connection open.
+        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
+            .await
+            .expect("bind flood");
+        let flood = listener.local_addr().expect("flood addr");
+        tokio::spawn(async move {
+            loop {
+                let Ok((mut stream, _)) = listener.accept().await else {
+                    break;
+                };
+                tokio::spawn(async move {
+                    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
+                    let mut buf = vec![0u8; 1024];
+                    let _ = stream.read(&mut buf).await; // drain the request
+                    let flood = vec![b'x'; 256 * 1024];
+                    let _ = stream.write_all(&flood).await;
+                    // Keep the connection open (no EOF, no \r\n\r\n).
+                    loop {
+                        tokio::time::sleep(Duration::from_secs(60)).await;
+                    }
+                });
+            }
+        });
+        let paths = scratch("flood");
+        write_settings(&paths, flood.port(), false);
+        let mut file = SettingsFile::load(&paths.settings_file());
+        file.settings.speed_test_url = format!("http://127.0.0.1:{}/down", flood.port());
+        file.save(&paths.settings_file()).expect("save settings");
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: flood,
+                fail: false,
+            }),
+        );
+        let view = svc.import_link(VLESS_REALITY).await.expect("import");
+        svc.set_active_profile(&view.tag).await.expect("active");
+        let started = Instant::now();
+        let err = svc.speed_test(None).await.unwrap_err();
+        assert!(err.contains("malformed response"), "{err}");
+        assert!(
+            started.elapsed() < Duration::from_secs(10),
+            "the head cap must fire long before the 30 s budget"
+        );
+    }
+
+    /// S20 (Copilot round 2, medium finding): the exit probe and speed test
+    /// may only ride the LIVE listeners when the session was built from the
+    /// target profile — switching the active profile without reconnecting
+    /// must fall back to the temporary tunnel (not report the old tunnel's
+    /// exit against the new profile's endpoint).
+    #[tokio::test]
+    async fn probes_fall_back_when_active_profile_switched() {
+        let exit = spawn_exit_echo("198.51.100.7").await;
+        let echo = spawn_echo().await;
+        let paths = scratch("switched");
+        // Probe target = echo (so connect() succeeds); exit URL = exit echo.
+        write_settings(&paths, echo.port(), false);
+        let mut file = SettingsFile::load(&paths.settings_file());
+        file.settings.exit_probe_url = format!("http://127.0.0.1:{}/json", exit.port());
+        file.settings.speed_test_url = format!("http://127.0.0.1:{}/down", exit.port());
+        file.save(&paths.settings_file()).expect("save settings");
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: exit,
+                fail: false,
+            }),
+        );
+        let a = svc.import_link(VLESS_REALITY).await.expect("import A");
+        let b = svc
+            .import_link(
+                &VLESS_REALITY
+                    .replace("203.0.113.10", "203.0.113.11")
+                    .replace("synthetic-reality", "second-server"),
+            )
+            .await
+            .expect("import B");
+        svc.set_active_profile(&a.tag).await.expect("active A");
+        svc.connect().await;
+        assert_eq!(svc.status().await.state, ServiceState::Connected);
+        // Matching session → the probe rides the live listeners.
+        let live = svc.exit_probe().await.expect("live probe");
+        assert!(!live.via_temp_tunnel, "session matches the active profile");
+        // Switch the active profile WITHOUT reconnecting → both probes
+        // must report the temporary-tunnel path.
+        svc.set_active_profile(&b.tag).await.expect("active B");
+        let temp = svc.exit_probe().await.expect("temp probe");
+        assert!(
+            temp.via_temp_tunnel,
+            "stale session must not pose as B's tunnel"
+        );
+        let speed = svc
+            .speed_test(Some(b.id.clone()))
+            .await
+            .expect("temp speed");
+        assert!(speed.via_temp_tunnel, "speed test ditto");
+        // And an explicit target that DOES match the session still rides it.
+        let live_speed = svc
+            .speed_test(Some(a.id.clone()))
+            .await
+            .expect("live speed");
+        assert!(!live_speed.via_temp_tunnel);
+        svc.disconnect().await;
+    }
+
+    /// S20 (Copilot round 2, medium finding): `engine_specs_for` preserves
+    /// the user's bonded-adapter priority order — the one-shot probe link
+    /// must be the FIRST (top-priority), not whatever `pop()` produced.
+    #[tokio::test]
+    async fn temp_link_uses_the_top_priority_first_link() {
+        let first_exit = spawn_exit_echo("203.0.113.7").await;
+        let second_exit = spawn_exit_echo("203.0.113.9").await;
+        let paths = scratch("templink");
+        write_settings(&paths, first_exit.port(), false);
+        struct TwoLinkFactory {
+            first: SocketAddr,
+            second: SocketAddr,
+        }
+        #[async_trait::async_trait]
+        impl LinkFactory for TwoLinkFactory {
+            async fn build(
+                &self,
+                _profile: &Profile,
+                _bonded: &[BondedAdapter],
+            ) -> anyhow::Result<Vec<Arc<dyn Outbound>>> {
+                Ok(vec![
+                    Arc::new(FakeLinkNamed {
+                        name: "first [Wi-Fi]".into(),
+                        echo: self.first,
+                    }) as Arc<dyn Outbound>,
+                    Arc::new(FakeLinkNamed {
+                        name: "second [LTE]".into(),
+                        echo: self.second,
+                    }),
+                ])
+            }
+        }
+        let svc = RealService::with(
+            paths,
+            Arc::new(TwoLinkFactory {
+                first: first_exit,
+                second: second_exit,
+            }),
+        );
+        let view = svc.import_link(VLESS_REALITY).await.expect("import");
+        let store = svc.core.load_store().expect("store");
+        let profile = store
+            .profiles
+            .iter()
+            .find(|p| p.id == view.id)
+            .cloned()
+            .expect("profile");
+        let link = temp_link_for(&svc.core, &profile)
+            .await
+            .expect("one-shot link");
+        assert_eq!(
+            link.name(),
+            "first [Wi-Fi]",
+            "the FIRST link carries the probe"
+        );
+        // Prove it end-to-end: the chosen link dials the FIRST exit echo.
+        let target = TargetAddr {
+            host: Host::Domain("irrelevant.example".into()),
+            port: 80,
+        };
+        let mut stream = link.connect(&target).await.expect("connect");
+        let body = http_get(&mut stream, "irrelevant.example", "/", EXIT_PROBE_MAX_BYTES)
+            .await
+            .expect("http get");
+        let exit = parse_exit_info(&body).expect("identity");
+        assert_eq!(exit.exit_ip, "203.0.113.7", "rode the top-priority link");
+    }
+
+    /// S20 (Copilot round 2, medium finding): a failed `diagnostics::enable`
+    /// (e.g. the log dir is unwritable) must REVERT the persisted toggle —
+    /// the UI must never show the mode enabled with no file being written
+    /// and no retry path.
+    #[tokio::test]
+    async fn diagnostics_enable_failure_reverts_the_toggle() {
+        let _guard = crate::diagnostics::TEST_LOCK.lock().await;
+        let _ = crate::diagnostics::disable();
+        let exit = spawn_exit_echo("127.0.0.1").await;
+        let paths = scratch("diagfail");
+        write_settings(&paths, exit.port(), false);
+        // Make the log dir unusable: a FILE where the dir should be.
+        std::fs::create_dir_all(&paths.config_dir).expect("base");
+        std::fs::write(&paths.log_dir, b"not a directory").expect("block log dir");
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: exit,
+                fail: false,
+            }),
+        );
+        let mut events = svc.events();
+        let mut s = svc.settings().await;
+        s.diagnostics = true;
+        svc.set_settings(s).await.expect("settings still save");
+        // The persisted toggle is back OFF.
+        let reloaded = SettingsFile::load(&svc.core.paths.settings_file());
+        assert!(!reloaded.settings.diagnostics, "toggle reverted on failure");
+        assert!(svc.diagnostics_path().await.is_none());
+        let mut saw_revert = false;
+        while let Ok(ev) = events.try_recv() {
+            if let ServiceEvent::LogLine(l) = ev {
+                if l.contains("could not start") && l.contains("reverted") {
+                    saw_revert = true;
+                }
+            }
+        }
+        assert!(
+            saw_revert,
+            "the failure + revert is announced in the GUI log"
+        );
+    }
+
+    /// S20 (Copilot round 2, medium finding): engines only read the debug
+    /// log level at SPAWN time — enabling diagnostics on a LIVE session
+    /// must reconnect, or the file silently misses the engine debug output
+    /// the UI promises.
+    #[tokio::test]
+    async fn diagnostics_enable_while_connected_reconnects() {
+        let _guard = crate::diagnostics::TEST_LOCK.lock().await;
+        let _ = crate::diagnostics::disable();
+        let exit = spawn_exit_echo("127.0.0.1").await;
+        let echo = spawn_echo().await;
+        let paths = scratch("diaglive");
+        write_settings(&paths, echo.port(), false);
+        let svc = RealService::with(
+            paths,
+            Arc::new(FakeFactory {
+                echo: exit,
+                fail: false,
+            }),
+        );
+        let view = svc.import_link(VLESS_REALITY).await.expect("import");
+        svc.set_active_profile(&view.tag).await.expect("active");
+        svc.connect().await;
+        assert_eq!(svc.status().await.state, ServiceState::Connected);
+        let restarts_before = svc.core.restarts.load(Ordering::SeqCst);
+
+        let mut s = svc.settings().await;
+        s.diagnostics = true;
+        svc.set_settings(s).await.expect("enable diagnostics");
+
+        // The session was rebuilt at debug level.
+        assert_eq!(
+            svc.status().await.state,
+            ServiceState::Connected,
+            "the reconnect restores the session"
+        );
+        assert!(
+            svc.core.restarts.load(Ordering::SeqCst) > restarts_before,
+            "the live session reconnected"
+        );
+        let level = wlock(&svc.core.engine_log_level).clone();
+        assert_eq!(level.as_deref(), Some("debug"));
+        assert!(svc.diagnostics_path().await.is_some());
+        svc.disconnect().await;
+        let mut s = svc.settings().await;
+        s.diagnostics = false;
+        svc.set_settings(s).await.expect("disable diagnostics");
+        assert!(svc.diagnostics_path().await.is_none());
+    }
+
     #[tokio::test]
     async fn stream_bond_mode_connects_and_carries_traffic() {
         let (daemon, _guard) = spawn_merge_daemon().await;
diff --git a/src/vless.rs b/src/vless.rs
index 27f7c34..65826f0 100755
--- a/src/vless.rs
+++ b/src/vless.rs
@@ -152,7 +152,9 @@ fn valid_uuid(s: &str) -> bool {
 }
 
 /// base64url alphabet WITHOUT padding: A-Z a-z 0-9 '-' '_'.
-fn is_base64url(c: u8) -> bool {
+/// S20: crate-visible — the profile EDIT path reuses the exact structural
+/// rule the link parser enforces on `pbk=`.
+pub(crate) fn is_base64url(c: u8) -> bool {
     c.is_ascii_alphanumeric() || c == b'-' || c == b'_'
 }
 

### Commit: 52f1d3c - backup(s19-close-v0.13.0): 2026-11-21T15:07:33Z (Z User, 2026-11-21)
diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
index bcb7935..ee6160f 100755
--- a/harness/memory/LEDGER.md
+++ b/harness/memory/LEDGER.md
@@ -215,3 +215,4 @@
 2026-11-21T07:17Z | s18 | v0.12.0 RELEASED: push attempt-1 REJECTED by GitHub — PROJECT_MEMORY.md had crossed the 100 MB per-file hard limit (the update_memory.py self-embedding recursion: each ritual run committed its own previous section into the file, so the next diff contained it — 81 MB at S17 close → 134 MB after this session's two runs; detected via the LFS-error push message, NOTHING reached origin). FIX: update_memory.py now excludes PROJECT_MEMORY.md's own diff (pathspec :(exclude) + an elision note), local history rewritten pre-push (reset --soft to origin/main, regenerated the section → 77 KB, re-committed s18 + re-tagged v0.12.0 — the oversized blobs never left the sandbox and are now unreachable). Attempt-2 push OK → release run 44879546925 SUCCESS on the first try (no cold-cache stall this time) + ci 44879546922 running. Assets verified by download+unzip: multitunnel-v0.12.0-windows-x64.zip 6,430,469 B integrity OK (multitunnel.exe PE32+ console + multitunnel-tray.exe PE32+ GUI + README v0.9–v0.12 section + QUICKSTART v0.12 note + server scripts + LICENSE + THIRD_PARTY_NOTICES + config.example) and mt-merge-server {x86_64 1,826,272 B, aarch64 1,860,808 B} ELFs.
 2026-11-21T08:30Z | s18 | OPS: the ci run on the s18 commit (44879546922) — lint/cross-check/test(ubuntu) all green; the windows test leg sat 69 min inside cargo test with no live logs fetchable mid-run → CANCELLED per the S11/S17 ops protocol. Verdict: 100%-cold rust-cache, not a wedge — the s18 commit changed Cargo.lock (version bumps + windows-sys feature), no windows job with that lock had ever completed, so Swatinem/rust-cache had nothing to restore and the leg compiled ~450 crates from zero (the exact S17 attempt-1 signature, cancelled at 68 min). Same-commit green proof on a REAL windows-latest runner exists: release run 44879546925's build-windows leg ran fmt + clippy + cargo test + packaging and PASSED (its zip is the verified v0.12.0 artifact). No rerun: the next ci.yml run would face the same cold cache (the release workflow's cache is keyed to a different workflow file); the S13 precedent (slow-cold cancelled, green proof elsewhere, no rerun) applies. The NEXT stage's ci will find the cache the release job saved for ITS key only — expect one more cold windows leg on the next Cargo.lock change.
 2026-11-21T14:20Z | s19 | IMPLEMENTED (six asks, v0.13.0): (1) server editing — Servers per-card Edit → full-window modal (S16 overlay discipline; dim backdrop cancels), mt-service ProfileDetails round-trip, ProfileStore::update by id (id/position/active preserved, no dedupe on explicit edit), primitive + capability-regression validation (a never-convertible profile edits freely); (2) exit-IP while OFFLINE — temp_link_for() one-shot factory link through the active profile, ExitInfo.via_temp_tunnel chip; (3) speed test — live session for the active profile else temp tunnel, settings.speed_test_url (cloudflare __down 25MB default, plain-HTTP verified), clock starts after connect, 5s stall/30s budget/64MiB caps, ≥400 rejected, SpeedTestInfo chips; (4) diagnostics mode — src/diagnostics.rs redacted paste-ready file (header + LogLine tee + hand-rolled global tracing subscriber, no tracing-subscriber dep, engine level forced debug, redaction test-pinned), Settings card + Open log folder; (5) kill switch — give_up() fail-closed at 127.0.0.1:9, review fix: app BINDS the port while engaged (spawn_blackhole swallows connections; dead-port fallback documented), stop_blackhole on every restore path (both disconnect branches, toggle-off, connect step-0), explicit disconnect restores with no session, Status "KILL SWITCH ACTIVE" banner (Failed+armed, pure gate tested); (6) code review — Copilot unavailable (no entitlement → reviewer silently dropped; models.github.ai proxy-intercepted "OK"), 3-round discussion-based review on the strongest reachable model instead, 4 valid findings fixed (port pre-bind ownership, banner, edit-failure logging, chained diag-path refresh), 7 assessed non-issues documented (transcript /tmp/s19-review-transcript.md, copied to harness/memory/agent-reports/s19-review-transcript.md); review PR #1 (base v0.12-line) left open for an entitled-machine Copilot pass. 466/0 (+19); gates fmt/clippy host+windows-gnu; iced 6/0; smoke 0; rig sf=1.0+1.5 (52.0 Mbps chips, offline exit-IP chip, modal fields, rename→save, Cancel+backdrop, kill-switch + diagnostics toggles, banner gate unit-tested); PE32+ ×4 (multitunnel 19.2MB, mt-app 14.9MB, tray 10.1MB, merge-server 2.4MB) + ELF ×2. LESSONS: z-ai CLI --thinking broken; window is 1080x720 (clicks beyond x=1080 miss); locate-then-click per scroll position; ENOSPC — stage binaries before wiping target trees.
+2026-11-21T15:10Z | s19 | RELEASED v0.13.0 (tag run 44910934439 SUCCESS ~7 min; assets verified by download+unzip: multitunnel-v0.13.0-windows-x64.zip 6,506,263 B, 11-entry manifest integrity OK, README/QUICKSTART/server scripts carried; mt-merge-server x86_64 1,826,272 B + aarch64 1,860,808 B; zip staged in sandbox download/v0.13.0-staging/). CI SAGA: the push leg failed on mt-bond carrier_death (documented runner flake, 4th occurrence) → rerun wedged 54 min inside domain_targets_dial (log captured: harness/memory/agent-reports/s19-ci-wedge-job-136868435499.log — 5/6 tests ok, domain_targets 'over 60 seconds', cargo+integration orphans killed) → HARDENED both tests with 60-90 s deadlines (bodies untouched; a wedge now fails in a minute with a LEDGER pointer) → next push run: BOTH tests failed loudly on runners (windows carrier_death 'no live bond carrier', ubuntu domain_targets deadline) while the same-commit PR run was fully green and local 2-core taskset stress (8× integration at test-threads=6) stayed 0-fail → rerun: ALL FOUR JOBS GREEN. ROOT-CAUSE LEAD (backlog, top priority): pick_carrier's 'no live bond carrier' = usable() empty = Wi-Fi dead + LTE flagged !alive mid-death-replay — a carrier-lifecycle race in PRE-EXISTING S11/S18 mt-bond code, GitHub-runner-only (slower I/O scheduling), never reproduced locally; investigate the death handler's replay path + LTE alive-flag transitions on a runner-like environment. Review PR #1 commented + left open.
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index 14882ff..1747d50 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "702293b 2026-11-21T14:58:33Z",
+  "as_of": "666bc16 2026-11-21T14:58:33Z",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 273,
-  "edge_count": 279,
+  "node_count": 274,
+  "edge_count": 281,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -915,6 +915,26 @@
         }
       ]
     },
+    {
+      "id": "commit:666bc16",
+      "type": "commit",
+      "title": "backup(s19-ci-hardening): 2026-11-21T14:58:33Z",
+      "tags": [
+        "2026-11-21"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "666bc16"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s19-ci-hardening): 2026-11-21T14:58:33Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:6a4bd7c",
       "type": "commit",
@@ -4576,6 +4596,10 @@
         {
           "text": "2026-11-21T14:20Z | IMPLEMENTED (six asks, v0.13.0): (1) server editing — Servers per-card Edit → full-window modal (S16 overlay discipline; dim backdrop cancels), mt-service ProfileDetails round-trip, ProfileStore::update by id (id/position/active preserved, no dedupe on explicit edit), primitive + capability-regression validation (a never-convertible profile edits freely); (2) exit-IP while OFFLINE — temp_link_for() one-shot factory link through the active profile, ExitInfo.via_temp_tunnel chip; (3) speed test — live session for the active profile else temp tunnel, settings.speed_test_url (cloudflare __down 25MB default, plain-HTTP verified), clock starts after connect, 5s stall/30s budget/64MiB caps, ≥400 rejected, SpeedTestInfo chips; (4) diagnostics mode — src/diagnostics.rs redacted paste-ready file (header + LogLine tee + hand-rolled global tracing subscriber, no tracing-subscriber dep, engine level forced debug, redaction test-pinned), Settings card + Open log folder; (5) kill switch — give_up() fail-closed at 127.0.0.1:9, review fix: app BINDS the port while engaged (spawn_blackhole swallows connections; dead-port fallback documented), stop_blackhole on every restore path (both disconnect branches, toggle-off, connect step-0), explicit disconnect restores with no session, Status \"KILL SWITCH ACTIVE\" banner (Failed+armed, pure gate tested); (6) code review — Copilot unavailable (no entitlement → reviewer silently dropped; models.github.ai proxy-intercepted \"OK\"), 3-round discussion-based review on the strongest reachable model instead, 4 valid findings fixed (port ",
           "kind": "body"
+        },
+        {
+          "text": "2026-11-21T15:10Z | RELEASED v0.13.0 (tag run 44910934439 SUCCESS ~7 min; assets verified by download+unzip: multitunnel-v0.13.0-windows-x64.zip 6,506,263 B, 11-entry manifest integrity OK, README/QUICKSTART/server scripts carried; mt-merge-server x86_64 1,826,272 B + aarch64 1,860,808 B; zip staged in sandbox download/v0.13.0-staging/). CI SAGA: the push leg failed on mt-bond carrier_death (documented runner flake, 4th occurrence) → rerun wedged 54 min inside domain_targets_dial (log captured: harness/memory/agent-reports/s19-ci-wedge-job-136868435499.log — 5/6 tests ok, domain_targets 'over 60 seconds', cargo+integration orphans killed) → HARDENED both tests with 60-90 s deadlines (bodies untouched; a wedge now fails in a minute with a LEDGER pointer) → next push run: BOTH tests failed loudly on runners (windows carrier_death 'no live bond carrier', ubuntu domain_targets deadline) while the same-commit PR run was fully green and local 2-core taskset stress (8× integration at test-threads=6) stayed 0-fail → rerun: ALL FOUR JOBS GREEN. ROOT-CAUSE LEAD (backlog, top priority): pick_carrier's 'no live bond carrier' = usable() empty = Wi-Fi dead + LTE flagged !alive mid-death-replay — a carrier-lifecycle race in PRE-EXISTING S11/S18 mt-bond code, GitHub-runner-only (slower I/O scheduling), never reproduced locally; investigate the death handler's replay path + LTE alive-flag transitions on a runner-like environment. Review PR #1 commented + left open.",
+          "kind": "body"
         }
       ]
     },
@@ -5379,7 +5403,7 @@
       ],
       "passages": [
         {
-          "text": "- **Stage**: S19 IMPLEMENTED, gates green, v0.13.0 releasing — the six asks shipped: (1) server EDITING (Servers → per-card Edit button → full-window modal; mt-service ProfileDetails round-trip via profile_details/update_profile; ProfileStore::update replaces BY ID in place — id/position/active preserved, no dedupe on explicit edits; primitive + capability-regression validation; editing the active server logs \"reconnect to apply\"); (2) exit-IP while OFFLINE (exit_probe falls back to temp_link_for — a one-shot factory link through the active profile, same engine path as test_latency; ExitInfo.via_temp_tunnel → \"temporary probe tunnel\" chip); (3) SPEED TEST (Status card; rides the live session for the active profile else the temp tunnel; settings.speed_test_url default http://speed.cloudflare.com/__down?bytes=25000000; clock starts AFTER connect so engine spawn never pollutes the rate; 5 s stall / 30 s budget / 64 MiB caps; HTTP ≥400 rejected); (4) DIAGNOSTICS MODE (SettingsView.diagnostics; src/diagnostics.rs writes one redacted paste-ready <log_dir>/diagnostics-*.log — header + GUI LogLine tee + ALL tracing events via a hand-rolled process-global subscriber (the e2e PrintSubscriber pattern, no tracing-subscriber dep); engine log level forced debug while ON; redaction test-pinned; Settings card + Open log folder); (5) KILL SWITCH (SettingsView.kill_switch; give_up() is fail-CLOSED: system proxy → 127.0.0.1:9 via sysproxy::set (keep-first snapshot survives for the true restore); REVIEW FIX: the app BINDS port 9 itself while engaged (spawn_blackhole accept loop swallows ",
+          "text": "- **Stage**: S19 CLOSED — v0.13.0 released and verified (tag run 44910934439 SUCCESS ~7 min; CI all-green on the rerun after the documented mt-bond runner-flake round — LEDGER 15:10Z).",
           "kind": "body"
         }
       ]
@@ -8046,6 +8070,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s17)"
     },
+    {
+      "src": "commit:666bc16",
+      "dst": "ledger:s19",
+      "rel": "part_of",
+      "evidence": "subject stage label (s19)"
+    },
     {
       "src": "commit:6a4bd7c",
       "dst": "ledger:s9.1",
@@ -8910,6 +8940,12 @@
       "rel": "part_of",
       "evidence": "project ledger"
     },
+    {
+      "src": "ledger:s19",
+      "dst": "release:v0.13.0",
+      "rel": "released_in",
+      "evidence": "2026-11-21T15:10Z | RELEASED v0.13.0 (tag run 44910934439 SUCCESS ~7 min; assets verified by download+unzip: multitunnel-v0.13.0-windows-x64.zip 6,506,263 B, 11-entry manifest integrity OK, README/QUICKSTART/server scripts carried; mt-merge-server x86_64 1,826,272 B + aarch64 1,860,808 B; zip staged in sandbox download/v0.13.0-staging/). CI SAGA: the push leg failed on mt-bond carrier_death (documented runner flake, 4th occurrence) → rerun wedged 54 min inside domain_targets_dial (log captured: harness/memory/agent-reports/s19-ci-wedge-job-136868435499.log — 5/6 tests ok, domain_targets 'over 60 seconds', cargo+integration orphans killed) → HARDENED both tests with 60-90 s deadlines (bodies untouched; a wedge now fails in a minute with a LEDGER pointer) → next push run: BOTH tests failed loudly on runners (windows carrier_death 'no live bond carrier', ubuntu domain_targets deadline) while the same-commit PR run was fully green and local 2-core taskset stress (8× integration at test-threads=6) stayed 0-fail → rerun: ALL FOUR JOBS GREEN. ROOT-CAUSE LEAD (backlog, top priority): pick_carrier's 'no live bond carrier' = usable() empty = Wi-Fi dead + LTE flagged !alive mid-death-replay — a carrier-lifecycle race in PRE-EXISTING S11/S18 mt-bond code, GitHub-runner-only (slower I/O scheduling), never reproduced locally; investigate the death handler's replay path + LTE alive-flag transitions on a runner-like environment. Review PR #1 commented + left open."
+    },
     {
       "src": "ledger:s2",
       "dst": "project:multitunnel",

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

( PROJECT_MEMORY.md's own diff is elided here — the durable
  sections above are hand-maintained; see harness/memory/LEDGER.md
  for the full stage history. )
```

