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

- **Stage**: S23 CLOSED — v0.16.0 released and verified (PR #3 merged as
  0b684f1, tag v0.16.0 on fe3f0e1, release run 46101413593 SUCCESS
  first try, ci on the same commit ALL GREEN first try — no flake
  rounds; assets download-verified: multitunnel-v0.16.0-windows-x64.zip
  6,641,511 B integrity OK [multitunnel.exe PE32+ console +
  multitunnel-tray.exe PE32+ GUI + README v0.16 section + QUICKSTART
  note + server scripts + LICENSE + THIRD_PARTY_NOTICES +
  config.example] + mt-merge-server ELFs {x86_64 1,849,496 B, aarch64
  1,866,560 B}; release notes hand-patched via API). COPILOT REVIEW
  FLOW (the user's explicit ask): @copilot reviewer requested via
  POST /pulls/3/requested_reviewers {"reviewers":["copilot"]} (the
  gh-pr-edit equivalent) — accepted, silently dropped (entitlement);
  the cloud code-review endpoint then triggered at max/balanced/lite
  — ALL THREE quota-rejected today (reviews 6088319039/6088442391/
  6088493808; S20 got lite to run — quota resets over days); re-run
  command documented in the PR. Per the S19/S20 precedent a substitute
  critical review of every safety-relevant section ran and is recorded
  in PR #3 (comment 7232494998): ShellExecuteW contract, Default
  single-source, reset flow, window persistence, tune_wan_stream,
  sweep_old_logs — no findings. MERGED with all four required checks
  green. The user's three asks shipped:
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
  zigbuild PE32+ ×3 + ELF ×2). The release-close continuation ran
  the 8 gates again at 0.16.0 (linux 512/0 + windows-CI 8 = 520;
  mt-bond flaked once locally — the documented runner signature,
  passes in isolation) and verified every release asset by download.
  Sandbox-recycle artifacts hit again (stale s13-era clone + 4th
  incomplete ziglang wheel + ENOSPC at 94% — all resolved per the
  ritual; pip --force-reinstall does NOT replace an existing
  --target dir: rm -rf it first).

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
### Commit: fe3f0e1 - backup(s23-release-prep): 2026-11-28T03:08:22Z (Z User, 2026-11-28)
diff --git a/Cargo.lock b/Cargo.lock
index 539fbf6..5071046 100755
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1962,7 +1962,7 @@ dependencies = [
 
 [[package]]
 name = "mt-app"
-version = "0.15.0"
+version = "0.16.0"
 dependencies = [
  "async-trait",
  "iced",
@@ -2074,7 +2074,7 @@ dependencies = [
 
 [[package]]
 name = "multitunnel"
-version = "0.15.0"
+version = "0.16.0"
 dependencies = [
  "anyhow",
  "async-trait",
diff --git a/Cargo.toml b/Cargo.toml
index e386a49..c85366a 100755
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -18,7 +18,7 @@ members = ["crates/*"]
 resolver = "2"
 
 [workspace.package]
-version = "0.15.0"
+version = "0.16.0"
 
 [lib]
 name = "multitunnel"
diff --git a/QUICKSTART.md b/QUICKSTART.md
index 94e9118..951543e 100755
--- a/QUICKSTART.md
+++ b/QUICKSTART.md
@@ -118,6 +118,15 @@ needed for the usual per-user system-proxy writes (expect the SmartScreen
 > writes again after a restart); the app icon fills its tile — the
 > glyph grew ~24% in the tray.
 >
+> v0.16: the **Open log folder** button works on real Windows again —
+> the old spawn hit os error 50 (a GUI-subsystem binary's null handles
+> inherited by explorer.exe); it now opens via ShellExecuteW and any
+> failure says exactly what was tried. Settings has a **Reset all
+> settings to defaults** card (two-click confirm; servers and routing
+> rules are kept). Also: stale log files are swept after 7 days at
+> startup, the window size is remembered across restarts, and the merge
+> daemon's WAN sockets use 1 MiB buffers for high-bandwidth carriers.
+>
 > v0.13.1 (Copilot code-review round): kill-switch hardening — the
 > black-hole port is owned BEFORE the proxy points at it, the proxy
 > hand-off is serialized with disconnect, a failed restore keeps the
diff --git a/README.md b/README.md
index e3ef43d..05d3c34 100755
--- a/README.md
+++ b/README.md
@@ -464,6 +464,26 @@ GUI, no new dependencies):
   size) was regenerated so the glyph fills the tile — the mark grew
   ~24% in the tray after a dead gradient band was cropped out of the
   source and the margins tightened (93–97% width, 75–77% height fill).
+- **v0.16.0 — Open log folder truly fixed, reset-to-defaults, and a
+  changelog-driven stability pass**: the "Open log folder" button failed
+  on real Windows with *"The request is not supported. (os error 50)"*
+  — the app is a GUI-subsystem binary whose null standard handles were
+  inherited by the spawned `explorer.exe` (a trap console apps never
+  hit). It now opens the folder via **`ShellExecuteW`**, the shell's
+  own open verb (with an `explorer.exe` fallback whose stdio is null on
+  all three streams), and any failure names everything that was tried —
+  never a bare error code. Settings gained a **Reset all settings to
+  defaults** card (two-click confirm; every setting returns to stock
+  while **servers and routing rules are kept** — the caption says so
+  and round-trip tests pin it). Three improvements adopted from the
+  changelogs of similar clients (v2rayN, Clash Verge Rev, Hiddify,
+  Speedify — survey in `docs/research/`): stale log files are swept
+  automatically at startup (7-day retention, newest 40 always kept),
+  the **window size is remembered** across restarts (clamped to the
+  880×580 minimums, debounced writes), and the merge daemon sizes its
+  WAN socket buffers to 1 MiB (≈ the bandwidth-delay product of a
+  150 Mbps × 50 ms carrier) so a single high-BDP path is not capped by
+  the OS default 128–256 KiB buffers.
 - **Quick Settings on right-click**: the tray menu is built fresh from
   live state — Connect/Disconnect (state-reflecting), the server list
   (radio-checked on the active profile), Open MultiTunnel and Exit.
diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
index ea6d60e..b103ee7 100755
--- a/harness/memory/LEDGER.md
+++ b/harness/memory/LEDGER.md
@@ -222,3 +222,4 @@
 2026-11-26T03:30Z | s22 | IMPLEMENTED (five user asks; v0.15.0). Sandbox recycled AGAIN (rustup 1.100.0 + ziglang 0.14.1 + cargo-zigbuild 0.24.1 + xkblibs + python-xlib rebuilt per the ritual; the pypkgs ziglang wheel was INCOMPLETE AGAIN mid-gates — ring stddef.h — reinstalled from PyPI, S20/S21 pattern). Local clone was stale at s13 with a dirty (stale script-generated) PROJECT_MEMORY.md — discarded, fast-forwarded to origin (S21 had already shipped v0.14.0 with the P0/P1/QoS/merged-panel brief; the FIVE asks below landed after that close, so they were all pending). (1) TWO-COLUMN Exit & speed split (user: "exit ip outputs on the left side and the speed test outputs on the right side"): screens.rs status() now builds exit_out/speed_out columns (headings "Exit IP"/"Speed test", quiet placeholders while idle) pushed as row![exit_out, speed_out] — verdict card + IP/chips rows stacked LEFT, Mbps + chips + note RIGHT; pure view change, D24-safe in the page scrollable. (2) UPLOAD SPEED TEST (user: "After the speed test has concluded, show another button to test upload speed"): mt-service contract + SettingsView.speed_test_upload_url (serde default http://speed.cloudflare.com/__up, D8 migration), RealService::speed_test_upload POSTs a fixed 25 MiB body through the same tunnel selection (S20 profile_id rule), clock after connect, 64 KiB chunks, same 30 s/5 s stall/64 MiB caps, partial-measurement-stands semantics, status guard ONLY on clean completion (a truncated body may never get a response); SpeedTestUpload/SpeedUploadTested messages + speed_up/speed_up_testing state + the button appears ONLY after the download concluded (gate app.speed.is_some() && !speed_testing), mutual exclusion across all three triggers; upload URL field in Settings → Advanced; redact_url in the diag header; MockService simulated. Tests: temp-tunnel measurement + 403-rejection (spawn_upload_server fixture drains Content-Length then answers), mock message flow, mt-service serde migration/roundtrip. (3) PER-LINK ADAPTER (user: "Under the links, show which adapter each link is going through"): mt-service::adapter_of_link_name parses the engine's "<tag> [adapter]" grammar (dedup " #N" stripped for display; rate-sampler's tricky cases pinned: "Wi-Fi Direct"/"Ethernet 2" verbatim), LinkStatus.adapter_name (serde default) populated in link_status_of (per-connection) + carrier_snapshot map (stream mode) + mock seeds; the Links card renders a dim `via <adapter>` line UNDER each link row — survives util::short's 32-char truncation that previously hid the [adapter] suffix. (4) OPEN LOG FOLDER FIX (user: "clicking the 'Open log folder button' does not open the log folder"): ROOT CAUSE was the diagnostics-file gate — the button resolved diag_path (None unless the sink started THIS process lifetime) so after every restart it printed "turn diagnostics on first"; FIXED by a new MtService::log_dir() (always resolvable), the handler create_dir_all's + opens the folder itself via open_folder_command (explorer/open/xdg-open per platform, spawn-result reported, never silent — Settings status + Logs-tab WARN), fetched at boot (fetch_log_dir in BOTH init paths — the iced init batch was the second gap the rig caught); ADJACENT ROOT CAUSE fixed: a diagnostics toggle persisted ON now re-enables the sink AT CONSTRUCTION (RealService::with) — the toggle used to lie after a restart; diagnostics_persisted_on_starts_at_construction pins it. (5) ICON FILL (user: "have the icon utilize as much of the available space as possible"): make_logo_assets.py re-crops emblem+lockup to their ALPHA bboxes after key-out (a dead ~71px left gradient band made the mark render off-center at 62% height fill), margins 0.04→0.02 (16/32/48/24/64) + 0.10→0.03 (256 lockup); measured fills 93.8-96.9%w × 75.0-76.6%h (was 75-81%w × 61-64%h — glyph ~24% larger in the tray's 32px tile); all test-pinned invariants hold (4-entry ICOs 16/32/48/256, biWidth=32/biHeight=64, logo_64=16384 B, logo_24=2304 B, deterministic re-runs, banner/logo/color_wheel untouched). RIG (sf=1.0 + 1.5, mock): panel split + placeholders, download result right-column, upload button appears → "testing upload…" → result rows, exit probe left-column verdict card, via-adapter lines, Open-log-folder click → folder created + honest per-platform failure surfaced (no xdg-open in bare Xvfb — the Windows path spawns explorer; spawn success unit-pinned via open_folder_command_targets_the_os_file_manager). LESSONS: (a) gui_drive click on a button that APPEARED after a reflow needs a pointer-jiggle first (no motion event → iced hover stale → first click no-op); (b) content appended BELOW a scrollable's initial viewport looks like a stale frame in shots — damage-instrumented the vendored compositor (reverted byte-identical) to prove the renderer was right: the rows were below the fold, scroll at a y INSIDE the viewport (400 was above it, 500 works). TESTS 497/0 (+7). GATES: fmt clean; clippy host + windows-gnu -D warnings clean (zig shims + fresh wheel); vendored iced 6/0; mt-app --smoke 0; zigbuild PE32+ x4 (multitunnel 19.3MB, mt-app 14.9MB, tray 10.2MB, merge-server 2.4MB) + ELF x2 (x86_64 1.25MB, aarch64 1.23MB); version 0.15.0; README v0.15 section + QUICKSTART note. DISK: 9.9G rootfs hit 99% mid-gates — rm -rf target/debug + target/x86_64-pc-windows-gnu between phases (S15 sequence discipline).
 2026-11-27T12:10Z | s23 | IMPLEMENTED (three asks; PR-flow session). Sandbox recycled AGAIN (rustup 1.100.0 + clippy/rustfmt components + ziglang 0.14.1 re-PyPI'd — the wheel was INCOMPLETE for the THIRD time, no lib/libc/include; xkblibs re-extracted to /tmp/xkblibs; python-xlib to tools/pypkgs-xlib). (1) OPEN LOG FOLDER, OS ERROR 50 (user: "could not open the log folder: The request is not supported. (os error 50)"): root cause pinned — v0.15.0 spawned explorer.exe with DEFAULT (inherited) stdio; mt-app is a Windows GUI-subsystem binary whose standard handles are NULL, Rust's spawn duplicates them into the child (STARTF_USESTDHANDLES) and CreateProcessW fails with ERROR_NOT_SUPPORTED; console-subsystem parents never hit it (why every manual probe passed while the user's GUI failed). FIX: crates/mt-app/src/shell_open.rs — Windows ShellExecuteW primary (windows-sys Win32_UI_Shell, already a tray dep; verb "open", rc>32 = ok, documented SE_* code table) + explorer.exe fallback now with Stdio::null() on ALL THREE streams (the inheritance trap is unreachable by construction); macOS open(1)/other xdg-open(1), all NUL-stdio; failures name every mechanism tried + the folder (never silent; Settings status + Logs WARN). Rig: click → "could not open the log folder: xdg-open is not installed (tried to open /tmp/MultiTunnel-mock-logs)" — the actionable format, vs the old bare "(os error 50)". (2) RESET ALL SETTINGS (user ask): Settings "Reset" card, ErrorText, TWO-CLICK confirm (first press arms: button relabels + status line names the exact scope; second executes; any other settings mutation disarms via save_settings). mt_service::SettingsView::default() is now the SINGLE stock source (impl Default; five diverging literals collapsed: SettingsFile, GUI placeholder, mock, theme/real_service test fixtures; serde-defaults == Default pinned by test, theme_mode's absent-key legacy semantics documented). Scope: every Settings field; profiles + routing rules untouched (stated in the card caption); bonded_adapters cleared through set_bonded_adapters([]) — the real set_settings deliberately preserves server-side bonded selection (stale-GUI-copy guard); rows reload selected=false; always-on-top UN-applies at stock(false) via apply_window_level both directions; reset-specific SettingsSaved note. (3) RESEARCH PASS (user: "check for improvements… by checking the changelogs of similar applications"): docs/research/2026-11-27-stability-changelog-survey.md — v2rayN 7.25.6/Clash Verge 2.5.10/Hiddify/Speedify → adopted: 7-day stale-log sweep (v2rayN's headline hygiene item; diagnostics::sweep_old_logs: retention 7d, newest-40 floor, live diag file protected, best-effort, runs at every RealService::with with an INFO line), remembered window size (desktop-client convention; window_w/h serde-default 1080x720 D8, debounced 2-quiet-tick save — a drag never writes per event, clamped 880x580, restored via iced::window::resize on load, reset restores stock), 1 MiB SO_SNDBUF/SO_RCVBUF + existing NODELAY on the merge daemon's two WAN socket populations via socket2 (transitive dep — zero new lockfile entries; BDP 150 Mbps × 50 ms ≈ 0.94 MiB; loopback engine hops intentionally OS-default). Confirmed parity: real-delay latency (already through-tunnel), balancer policies (5 merge policies superset), connection-info card, core pinning (D14). Rejected with reasons: mux knobs, MASQUE, redundant mode (doubles traffic), TUN, subscription headers. TESTS 520/0 (+10: shell_open ×4, stock-defaults ×2, GUI reset ×1 + window ×1, real_service reset round-trip + window round-trip + construction-sweep ×3, daemon tune ×1, diagnostics sweep ×3). GATES all green: fmt; clippy host + windows-gnu (zig shims, fresh wheel) -D warnings; cargo test --workspace 510/0 + s23 subset; vendored iced 6/0 (cargo test -p iced_graphics -p iced_tiny_skia --lib from the workspace root — the standalone-manifest route errors on the parent workspace); mt-app --smoke 0; zigbuild PE32+ ×3 (multitunnel 19.2MB, tray 10.2MB, mt-merge-server) + ELF ×2; rig sf=1.0 + 1.5 (reset two-click arm→confirm→"settings reset to defaults — servers and routing rules untouched", seed reverted 6770852/#6750A4/purple-selected, log-folder honest failure, 0 panics). DELIVERY FLOW CHANGED THIS SESSION per the user's ask: PR + @copilot reviewer instead of direct-to-main; NO tag/release (review first). RIG LESSONS: the Settings page scroll needs ONE wheel tick per verb invocation with ≥1 s between (consecutive ticks in one scroll:X,Y,N verb coalesce — only ~1 lands; a tick over a pick_list is eaten; (1070,400) — the right card-padding strip — is widget-free); VLM on crops beats whole-shot pixel archaeology; row-profile dy alignment can hallucinate (err=0 at impossible shifts on sparse profiles) — track a distinctive feature (the red swatch row) instead. DISK: hit 100% twice — rm -rf target/debug + the /tmp/mt-base worktree between phases (S15/S22 discipline).
 2026-11-27T12:40Z | s23-pr | PR FLOW EXECUTED (the user's delivery instruction): branch s23-open-folder-reset-perf pushed (5 feature commits + backup), PR #3 opened via REST (https://github.com/maniesh-NR/MultiTunnel/pull/3 — full root-cause/write-up body; the plugin create_pull_request tool errored twice, curl worked first try). @copilot REVIEWER REQUESTED exactly as asked — POST /pulls/3/requested_reviewers {"reviewers":["copilot"]} (the API equivalent of gh pr edit --add-reviewer @copilot; no gh binary in the sandbox) — GitHub ACCEPTED the call but SILENTLY DROPPED the reviewer (requested_reviewers returns empty; same entitlement finding as S19/S20/S21 — no Copilot code review on this repo/PAT). Documented in a PR comment with the re-request command for an entitled machine. CI kicked off on the PR: lint + test(ubuntu) + test(windows) + cross-check all running. NO tag/release this session by design (review first); v0.16.0 material is ready when the PR merges.
+2026-11-28T03:10Z | s23-cont | PR #3 MERGED (0b684f1) + v0.16.0 RELEASE-PREP + GATES. The continuation session (context handoff) executed the review-then-merge flow the s23-pr session had staged: CI on the PR head (1eaf80f) verified all four required checks GREEN (run 45995734937: test ubuntu 64s / test windows 150s / lint 47s / cross-check 117s — zero flake rounds this time). COPILOT CODE REVIEW: triggered via the S20 REST method (POST /pulls/3/copilot-code-review) — max effort ACCEPTED (201/PENDING) but landed quota-rejected (review 6088319039), balanced the same (6088442391), lite the same (6088493808) — the account's Copilot review quota is exhausted at EVERY tier today (S20 got lite to run; quota resets over days). The reviewer REQUEST via requested_reviewers remains in place exactly as the user asked (documented in the PR). Per the S19/S20 precedent a substitute critical review ran and was recorded in the PR (comment 7232494998): shell_open.rs ShellExecuteW contract (NUL-terminated wide strings, >32 success, SE_* table, NUL-stdio fallback), SettingsView::default single-source (serde-defaults parity pinned), reset two-click flow (validated save path, profiles/rules pinned to survive, bonded_adapters via set_bonded_adapters([])), window persistence (debounced, clamped, D8-additive), tune_wan_stream (BDP-sized, advisory, loopback-exempt), sweep_old_logs (newest-40 floor, protected live file, clock-skew safe) — no findings. MERGED via API (enforce_admins=false, all required checks green). SANDBOX RECYCLED AGAIN mid-session (local clone reverted to the s13-era snapshot + dirty script-generated PROJECT_MEMORY.md — the S21/S22/S23 signature; remote authoritative: fetch + reset --hard origin/main; the reflog anomaly "checkout: moving from main to main" is the recycled-clone artifact). TOOLCHAIN REBUILT per the ritual: rustup 1.100.0 + rustfmt/clippy, cargo-zigbuild 0.24.1, xkblibs re-extracted, xkblibs/xlib ok — and the ziglang wheel was INCOMPLETE for the FOURTH time (libc only musl/wasi → ring E0463/build-script failure on the windows clippy leg); clean-reinstalled 0.14.1 from PyPI (rm -rf the dir first — --force-reinstall alone does NOT replace an existing --target dir). VERSION BUMP 0.15.0→0.16.0 (one workspace line + cargo update --workspace; lock: multitunnel+mt-app at 0.16.0, lib crates stay 0.1.0 per convention) + README v0.16.0 highlight bullet + QUICKSTART v0.16 note. GATES: fmt clean; clippy host 1m18s + windows-gnu -D warnings clean (fresh wheel + rustup target add x86_64-pc-windows-gnu+aarch64-unknown-linux-gnu); cargo test --workspace 512/0 LINUX (mt-bond flaked ONCE locally — carrier_death first, domain_targets 60s-deadline second, PASSES in isolation 3.1s — the documented runner signature, this sandbox is as constrained as a runner; the PR CI legs were green at 520 incl. the 8 windows-only tests); vendored iced 6/0; mt-app --smoke 0; zigbuild PE32+ x4 (multitunnel 19.3MB, mt-app 14.9MB, tray 10.2MB, merge-server 2.4MB) + ELF x2 (x86_64 1.85MB, aarch64 1.23MB). DISK hit 94% mid-gates (ENOSPC on the smoke build) — rm -rf target/debug/{incremental,build,deps} freed 5G. Tagging v0.16.0 next.
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index bf657e1..aaa03e6 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "21fd59f 2026-11-25T19:24:03Z",
+  "as_of": "0b684f1 2026-11-28T09:08:39+06:30",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 300,
-  "edge_count": 308,
+  "node_count": 313,
+  "edge_count": 314,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -255,6 +255,30 @@
         }
       ]
     },
+    {
+      "id": "commit:0b684f1",
+      "type": "commit",
+      "title": "Merge PR #3: S23 — open log folder fix, reset-to-defaults, changelog research pass (v0.16.0 material)",
+      "tags": [
+        "2026-11-28"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "0b684f1"
+        }
+      ],
+      "passages": [
+        {
+          "text": "Merge PR #3: S23 — open log folder fix, reset-to-defaults, changelog research pass (v0.16.0 material)",
+          "kind": "title"
+        },
+        {
+          "text": "Merge S23: ShellExecuteW open-folder fix (os error 50), reset-all-settings, changelog-driven stability/UI pass. CI green on all four required checks; substitute critical review recorded in PR comments (Copilot quota-rejected at max/balanced/lite).",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "commit:0bdfaf4",
       "type": "commit",
@@ -455,6 +479,26 @@
         }
       ]
     },
+    {
+      "id": "commit:1eaf80f",
+      "type": "commit",
+      "title": "backup(s23-pr-open): 2026-11-27 PR #3 opened, copilot reviewer requested (entitlement-dropped, documented)",
+      "tags": [
+        "2026-11-27"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "1eaf80f"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s23-pr-open): 2026-11-27 PR #3 opened, copilot reviewer requested (entitlement-dropped, documented)",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:21fd59f",
       "type": "commit",
@@ -555,6 +599,30 @@
         }
       ]
     },
+    {
+      "id": "commit:2b5b38b",
+      "type": "commit",
+      "title": "feat: reset-all-settings button + remembered window size",
+      "tags": [
+        "2026-11-27"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "2b5b38b"
+        }
+      ],
+      "passages": [
+        {
+          "text": "feat: reset-all-settings button + remembered window size",
+          "kind": "title"
+        },
+        {
+          "text": "RESET TO DEFAULTS (user ask): a Settings 'Reset' card with a two-click confirm (first press arms — button relabels, status line names the exact scope; second press executes; any other settings mutation disarms). The reset applies mt_service::SettingsView::default() — now the SINGLE source of truth for stock values (impl Default, previously spelled out in five diverging literals: real_service's SettingsFile, the GUI placeholder, the mock, theme/real_service test fixtures). - Scope: every Settings-screen field; servers (profiles.json) and routing rules are explicitly NOT touched — stated in the card caption. - bonded_adapters is cleared through set_bonded_adapters([]) — the real service's set_settings deliberately never clobbers it (stale-GUI-copy guard); the Adapters tab rows reload selected=false. - Always-on-top is UN-applied when stock(false) — apply_window_level handles both directions now (the old load path only raised). - SettingsSaved shows a reset-specific completion note.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "commit:34b56ba",
       "type": "commit",
@@ -1635,6 +1703,30 @@
         }
       ]
     },
+    {
+      "id": "commit:8949522",
+      "type": "commit",
+      "title": "build(deps): bump zip",
+      "tags": [
+        "2026-11-26"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "8949522"
+        }
+      ],
+      "passages": [
+        {
+          "text": "build(deps): bump zip",
+          "kind": "title"
+        },
+        {
+          "text": "Bumps the cargo group with 1 update in the / directory: [zip](https://github.com/zip-rs/zip2).",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "commit:8fdf7d1",
       "type": "commit",
@@ -1655,6 +1747,26 @@
         }
       ]
     },
+    {
+      "id": "commit:978da2b",
+      "type": "commit",
+      "title": "backup(s23-pr-ready): 2026-11-27 memory + gates + rig evidence",
+      "tags": [
+        "2026-11-27"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "978da2b"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s23-pr-ready): 2026-11-27 memory + gates + rig evidence",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:9d4111b",
       "type": "commit",
@@ -1695,6 +1807,26 @@
         }
       ]
     },
+    {
+      "id": "commit:a0e0ccb",
+      "type": "commit",
+      "title": "chore: clippy/fmt pass (struct-update idioms, sort_by_key, drop dead default fns)",
+      "tags": [
+        "2026-11-27"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "a0e0ccb"
+        }
+      ],
+      "passages": [
+        {
+          "text": "chore: clippy/fmt pass (struct-update idioms, sort_by_key, drop dead default fns)",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:a3b9e5f",
       "type": "commit",
@@ -1715,6 +1847,26 @@
         }
       ]
     },
+    {
+      "id": "commit:a5ca2f7",
+      "type": "commit",
+      "title": "backup(s22-close-v0.15.0): 2026-11-25T19:41:20Z",
+      "tags": [
+        "2026-11-25"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "a5ca2f7"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s22-close-v0.15.0): 2026-11-25T19:41:20Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:a5f4864",
       "type": "commit",
@@ -1795,6 +1947,30 @@
         }
       ]
     },
+    {
+      "id": "commit:ab91166",
+      "type": "commit",
+      "title": "perf(mt-bond): 1 MiB WAN socket buffers on the merge daemon + research doc",
+      "tags": [
+        "2026-11-27"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "ab91166"
+        }
+      ],
+      "passages": [
+        {
+          "text": "perf(mt-bond): 1 MiB WAN socket buffers on the merge daemon + research doc",
+          "kind": "title"
+        },
+        {
+          "text": "Speedify-style per-path window sizing applied to the two WAN socket populations MultiTunnel owns (the Xray engine legs stay on OS defaults — loopback buffers only cost memory): - accepted carrier streams (client ⇄ daemon); - dialed target streams (daemon ⇄ real target).",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "commit:ad550fa",
       "type": "commit",
@@ -2255,6 +2431,30 @@
         }
       ]
     },
+    {
+      "id": "commit:e1ec5cc",
+      "type": "commit",
+      "title": "feat: automatic stale-log cleanup at startup (v2rayN 7.25.6 parity)",
+      "tags": [
+        "2026-11-27"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "e1ec5cc"
+        }
+      ],
+      "passages": [
+        {
+          "text": "feat: automatic stale-log cleanup at startup (v2rayN 7.25.6 parity)",
+          "kind": "title"
+        },
+        {
+          "text": "v2rayN 7.25.6 made 'automatic cleanup time for logs and temporary files default to 7 days' — the same hygiene for MultiTunnel's log dir, whose engine logs and diagnostics files otherwise accumulate forever.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "commit:e2b9fa2",
       "type": "commit",
@@ -2515,6 +2715,30 @@
         }
       ]
     },
+    {
+      "id": "commit:fc90e47",
+      "type": "commit",
+      "title": "fix(mt-app): open log folder via ShellExecuteW, NUL-stdio fallback (os error 50)",
+      "tags": [
+        "2026-11-27"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "fc90e47"
+        }
+      ],
+      "passages": [
+        {
+          "text": "fix(mt-app): open log folder via ShellExecuteW, NUL-stdio fallback (os error 50)",
+          "kind": "title"
+        },
+        {
+          "text": "v0.15.0 spawned explorer.exe through std::process::Command with default (inherited) stdio. mt-app is a Windows GUI-subsystem binary — its standard handles are NULL, so Rust's spawn duplicates null handles into the child and CreateProcessW fails with ERROR_NOT_SUPPORTED: 'The request is not supported. (os error 50)' — exactly the reported Settings failure.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "commit:fe613ab",
       "type": "commit",
@@ -5163,6 +5387,69 @@
         }
       ]
     },
+    {
+      "id": "ledger:s23",
+      "type": "ledger",
+      "title": "Stage s23 ledger",
+      "tags": [
+        "s23",
+        "append-only"
+      ],
+      "sources": [
+        {
+          "file": "harness/memory/LEDGER.md",
+          "anchor": "s23"
+        }
+      ],
+      "passages": [
+        {
+          "text": "2026-11-27T12:10Z | IMPLEMENTED (three asks; PR-flow session). Sandbox recycled AGAIN (rustup 1.100.0 + clippy/rustfmt components + ziglang 0.14.1 re-PyPI'd — the wheel was INCOMPLETE for the THIRD time, no lib/libc/include; xkblibs re-extracted to /tmp/xkblibs; python-xlib to tools/pypkgs-xlib). (1) OPEN LOG FOLDER, OS ERROR 50 (user: \"could not open the log folder: The request is not supported. (os error 50)\"): root cause pinned — v0.15.0 spawned explorer.exe with DEFAULT (inherited) stdio; mt-app is a Windows GUI-subsystem binary whose standard handles are NULL, Rust's spawn duplicates them into the child (STARTF_USESTDHANDLES) and CreateProcessW fails with ERROR_NOT_SUPPORTED; console-subsystem parents never hit it (why every manual probe passed while the user's GUI failed). FIX: crates/mt-app/src/shell_open.rs — Windows ShellExecuteW primary (windows-sys Win32_UI_Shell, already a tray dep; verb \"open\", rc>32 = ok, documented SE_* code table) + explorer.exe fallback now with Stdio::null() on ALL THREE streams (the inheritance trap is unreachable by construction); macOS open(1)/other xdg-open(1), all NUL-stdio; failures name every mechanism tried + the folder (never silent; Settings status + Logs WARN). Rig: click → \"could not open the log folder: xdg-open is not installed (tried to open /tmp/MultiTunnel-mock-logs)\" — the actionable format, vs the old bare \"(os error 50)\".",
+          "kind": "body"
+        }
+      ]
+    },
+    {
+      "id": "ledger:s23-cont",
+      "type": "ledger",
+      "title": "LEDGER stream: s23-cont",
+      "tags": [
+        "s23-cont",
+        "append-only"
+      ],
+      "sources": [
+        {
+          "file": "harness/memory/LEDGER.md",
+          "anchor": "s23-cont"
+        }
+      ],
+      "passages": [
+        {
+          "text": "2026-11-28T03:10Z | PR #3 MERGED (0b684f1) + v0.16.0 RELEASE-PREP + GATES. The continuation session (context handoff) executed the review-then-merge flow the s23-pr session had staged: CI on the PR head (1eaf80f) verified all four required checks GREEN (run 45995734937: test ubuntu 64s / test windows 150s / lint 47s / cross-check 117s — zero flake rounds this time). COPILOT CODE REVIEW: triggered via the S20 REST method (POST /pulls/3/copilot-code-review) — max effort ACCEPTED (201/PENDING) but landed quota-rejected (review 6088319039), balanced the same (6088442391), lite the same (6088493808) — the account's Copilot review quota is exhausted at EVERY tier today (S20 got lite to run; quota resets over days). The reviewer REQUEST via requested_reviewers remains in place exactly as the user asked (documented in the PR). Per the S19/S20 precedent a substitute critical review ran and was recorded in the PR (comment 7232494998): shell_open.rs ShellExecuteW contract (NUL-terminated wide strings, >32 success, SE_* table, NUL-stdio fallback), SettingsView::default single-source (serde-defaults parity pinned), reset two-click flow (validated save path, profiles/rules pinned to survive, bonded_adapters via set_bonded_adapters([])), window persistence (debounced, clamped, D8-additive), tune_wan_stream (BDP-sized, advisory, loopback-exempt), sweep_old_logs (newest-40 floor, protected live file, clock-skew safe) — no findings. MERGED via API (enforce_admins=false, all required checks green).",
+          "kind": "body"
+        }
+      ]
+    },
+    {
+      "id": "ledger:s23-pr",
+      "type": "ledger",
+      "title": "LEDGER stream: s23-pr",
+      "tags": [
+        "s23-pr",
+        "append-only"
+      ],
+      "sources": [
+        {
+          "file": "harness/memory/LEDGER.md",
+          "anchor": "s23-pr"
+        }
+      ],
+      "passages": [
+        {
+          "text": "2026-11-27T12:40Z | PR FLOW EXECUTED (the user's delivery instruction): branch s23-open-folder-reset-perf pushed (5 feature commits + backup), PR #3 opened via REST (https://github.com/maniesh-NR/MultiTunnel/pull/3 — full root-cause/write-up body; the plugin create_pull_request tool errored twice, curl worked first try). @copilot REVIEWER REQUESTED exactly as asked — POST /pulls/3/requested_reviewers {\"reviewers\":[\"copilot\"]} (the API equivalent of gh pr edit --add-reviewer @copilot; no gh binary in the sandbox) — GitHub ACCEPTED the call but SILENTLY DROPPED the reviewer (requested_reviewers returns empty; same entitlement finding as S19/S20/S21 — no Copilot code review on this repo/PAT). Documented in a PR comment with the re-request command for an entitled machine. CI kicked off on the PR: lint + test(ubuntu) + test(windows) + cross-check all running. NO tag/release this session by design (review first); v0.16.0 material is ready when the PR merges.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "ledger:s3",
       "type": "ledger",
@@ -5843,7 +6130,7 @@
       ],
       "passages": [
         {
-          "text": "- **Stage**: S22 CLOSED — v0.15.0 released (tag on 21fd59f). The user's five asks shipped: (1) Exit & speed panel split into TWO columns — exit-IP outputs LEFT (heading, IP, location/ISP chips, verdict card), speed-test outputs RIGHT (heading, Mbps, volume chips, download note), quiet placeholders while idle; pure view change, D24-safe. (2) UPLOAD SPEED TEST: \"Test upload speed\" button appears ONLY after the download test concluded, result in the right column; speed_test_upload_url setting (serde default cloudflare __up, D8), RealService::speed_test_upload POSTs 25 MiB through the same tunnel selection (S20 profile_id rule; clock after connect; 30 s/5 s stall/ 64 MiB caps; partial stands; status guard only on clean completion); Settings → Advanced \"upload test URL\" field; diag header redacts it. (3) PER-LINK ADAPTER: `via <adapter>` dim line UNDER each link row — mt_service::adapter_of_link_name parses \"<tag> [adapter]\" (dedup \" #N\" stripped; \"Wi-Fi Direct\"/\"Ethernet 2\" verbatim), populated in link_status_of + carrier_snapshot map + mock; survives util::short truncation. (4) OPEN LOG FOLDER FIXED: root cause = the diag-file gate (diag_path None after restart → \"turn diagnostics on first\"); new MtService::log_dir() (always resolvable) + handler create_dir_all + opens the folder itself via open_folder_command (explorer/open/xdg-open; spawn-result reported; Settings status + Logs-tab WARN, never silent); fetched at boot in BOTH init paths; ADJACENT: diagnostics persisted ON re-enables the sink AT CONSTRUCTION (RealService::with) — the toggle no longer lies after a restart.",
+          "text": "- **Stage**: S23 IMPLEMENTED, delivered as PR `s23-open-folder-reset-perf` with @copilot requested as reviewer (the user's explicit delivery instruction — PR + review instead of direct-to-main; NO tag/release until the review lands). The user's three asks shipped on the branch: (1) OPEN LOG FOLDER, OS ERROR 50 FIXED — root cause: v0.15.0 spawned explorer.exe with inherited stdio; mt-app is a Windows GUI-subsystem binary (NULL standard handles) and Rust's spawn passes them with STARTF_USESTDHANDLES → CreateProcessW fails ERROR_NOT_SUPPORTED (\"os error 50\") — console parents never hit it, which is why every manual probe passed while the user's GUI failed. FIX: mt-app/src/shell_open.rs — Windows ShellExecuteW (shell open-verb, no handle inheritance; windows-sys Win32_UI_Shell was already the tray dep; documented SE_* ≤32 code table) + explorer.exe fallback with Stdio::null() on ALL three streams; macOS open(1)/other xdg-open(1), NUL-stdio; failures name every mechanism tried + the folder (never silent). (2) RESET ALL SETTINGS — Settings \"Reset\" card (ErrorText, TWO-CLICK confirm; first press arms: relabel + scope status; second executes; any other settings mutation disarms). Stock source is now ONE: mt_service::SettingsView::default() (five diverging literals collapsed; serde-defaults == Default test-pinned; theme_mode's absent-key legacy semantics documented).",
           "kind": "body"
         },
         {
@@ -8227,7 +8514,7 @@
     {
       "id": "state:current",
       "type": "state",
-      "title": "STATE — current position (last update: S19 (server editing + offline probes + speed test + diagnostics + kill switch, v0.13.0)",
+      "title": "STATE — current position (last update: S23 (open-log-folder os-error-50 fix + reset-to-defaults + changelog-research pass; PR flow)",
       "tags": [
         "durable"
       ],
@@ -8239,15 +8526,15 @@
       ],
       "passages": [
         {
-          "text": "# STATE — rewritten at the end of every stage. Last update: S19 (server editing + offline probes + speed test + diagnostics + kill switch, v0.13.0)",
+          "text": "# STATE — rewritten at the end of every stage. Last update: S23 (open-log-folder os-error-50 fix + reset-to-defaults + changelog-research pass; PR flow)",
           "kind": "title"
         },
         {
-          "text": "## Current position - Stage: S19 IMPLEMENTED, all local gates green, review fixes in, releasing v0.13.0. - The user's six asks shipped: (1) SERVER EDITING — Servers tab per-card Edit button → full-window modal (name/remark/address/port/UUID + REALITY fields for reality profiles); mt-service ProfileDetails round-trip (profile_details → update_profile); ProfileStore::update replaces BY ID in place (id/position/active preserved, no dedupe on explicit edit); RealService validates primitives + a capability-regression check (an edit that would break a convertible profile is rejected with the reason). (2) EXIT IP WHILE OFFLINE — exit_probe prefers the live session, falls back to temp_link_for() (the same factory path test_latency uses); ExitInfo.via_temp_tunnel drives a \"temporary probe tunnel\" chip. (3) SPEED TEST — Status card; speed_test(None) rides the live session for the active profile (else the temp tunnel); downloads settings.speed_test_url (default http://speed.cloudflare.com/__down?bytes=25000000, plain-HTTP verified); clock starts after connect (engine spawn excluded); 5 s stall / 30 s budget / 64 MiB caps; ≥400 rejected. (4) DIAGNOSTICS MODE — src/diagnostics.rs: one redacted paste-ready <log_dir>/diagnostics-YYYYMMDD-HHMMSS.log (header + GUI LogLine tee + ALL tracing events via a hand-rolled global subscriber; engine level forced debug; UUID/pbk never written — test-pinned); Settings card with the path + \"Open log folder\".",
+          "text": "## Current position - Stage: S23 IMPLEMENTED, all local gates green, delivered as the PR `s23-open-folder-reset-perf` requesting @copilot review (the user's explicit delivery instruction this session — NO tag/release until the review lands). - The user's three asks shipped on the branch: (1) OPEN LOG FOLDER FIXED FOR REAL — the v0.15.0 explorer.exe spawn hit ERROR_NOT_SUPPORTED (os error 50) because a GUI-subsystem binary passes NULL stdio handles into CreateProcessW; the fix is crates/mt-app/src/shell_open.rs: ShellExecuteW (shell's open-verb path, no handle inheritance) with an explorer.exe fallback that now uses Stdio::null() on all three streams; failures name every mechanism tried + the folder. (2) RESET ALL SETTINGS — a Settings \"Reset\" card (ErrorText, two-click confirm, scope caption: servers and routing rules untouched; bonded_adapters cleared through set_bonded_adapters([]); mt_service::SettingsView::default() is the single stock source — five diverging literals collapsed; reset note in SettingsSaved). (3) CHANGELOG RESEARCH APPLIED — docs/research/2026-11-27-stability-changelog-survey.md: adopted the v2rayN 7.25.6 7-day stale-log sweep (diagnostics::sweep_old_logs at every RealService::with), the desktop-client remembered window size (window_w/h serde-default, debounced save, restored on load), and 1 MiB SO_SNDBUF/SO_RCVBUF on the merge daemon's WAN streams via socket2 (transitive dep, zero new lockfile entries).",
           "kind": "title"
         },
         {
-          "text": "## Next actions 1. Tag v0.13.0 → release.yml; watch the run; verify assets by download+unzip (PE32+ in zip, ELFs, README/QUICKSTART carried). 2. User-side Windows verification: Edit dialog against a real profile (save + reconnect-apply), speed test + offline exit IP against the real server, diagnostics file paste-readiness (turn on, connect, reproduce an issue, paste), kill switch behavior after killing the VM side. 3. Review PR #1: request Copilot from an entitled machine (`gh pr edit 1 --add-reviewer Copilot`) if the account gains a Copilot seat; otherwise close it after the release.",
+          "text": "## Prior stages (compressed) ## Next actions 1. Tag v0.13.0 → release.yml; watch the run; verify assets by download+unzip (PE32+ in zip, ELFs, README/QUICKSTART carried). 2. User-side Windows verification: Edit dialog against a real profile (save + reconnect-apply), speed test + offline exit IP against the real server, diagnostics file paste-readiness (turn on, connect, reproduce an issue, paste), kill switch behavior after killing the VM side. 3. Review PR #1: request Copilot from an entitled machine (`gh pr edit 1 --add-reviewer Copilot`) if the account gains a Copilot seat; otherwise close it after the release.",
           "kind": "title"
         },
         {
@@ -8578,6 +8865,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s10)"
     },
+    {
+      "src": "commit:1eaf80f",
+      "dst": "ledger:s23-pr",
+      "rel": "part_of",
+      "evidence": "subject stage label (s23-pr)"
+    },
     {
       "src": "commit:21fd59f",
       "dst": "ledger:s22",
@@ -8872,6 +9165,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s22)"
     },
+    {
+      "src": "commit:978da2b",
+      "dst": "ledger:s23-pr",
+      "rel": "part_of",
+      "evidence": "subject stage label (s23-pr)"
+    },
     {
       "src": "commit:9d4111b",
       "dst": "ledger:s18",
@@ -8890,6 +9189,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (recovery)"
     },
+    {
+      "src": "commit:a5ca2f7",
+      "dst": "ledger:s22",
+      "rel": "part_of",
+      "evidence": "subject stage label (s22)"
+    },
     {
       "src": "commit:a7f5caa",
       "dst": "ledger:s2-review",
@@ -9736,6 +10041,24 @@
       "rel": "part_of",
       "evidence": "project ledger"
     },
+    {
+      "src": "ledger:s23",
+      "dst": "project:multitunnel",
+      "rel": "part_of",
+      "evidence": "project ledger"
+    },
+    {
+      "src": "ledger:s23-cont",
+      "dst": "project:multitunnel",
+      "rel": "part_of",
+      "evidence": "project ledger"
+    },
+    {
+      "src": "ledger:s23-pr",
+      "dst": "project:multitunnel",
+      "rel": "part_of",
+      "evidence": "project ledger"
+    },
     {
       "src": "ledger:s3",
       "dst": "project:multitunnel",

### Commit: 1eaf80f - backup(s23-pr-open): 2026-11-27 PR #3 opened, copilot reviewer requested (entitlement-dropped, documented) (Z User, 2026-11-27)
diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
index f73ad73..ea6d60e 100755
--- a/harness/memory/LEDGER.md
+++ b/harness/memory/LEDGER.md
@@ -221,3 +221,4 @@
 2026-11-24T21:00Z | s21 | IMPLEMENTED (four asks: P0 bug, P1 perf parity, QoS toggle, merged panel; v0.14.0). Sandbox recycled AGAIN (rustup/zigbuild/xkblibs gone; tools/pypkgs ziglang wheel INCOMPLETE again — no lib/libc/include → ring stddef.h failure mid-gates; reinstalled 0.14.1 from PyPI per the S20 note; local clone was STALE at s13 with a dirty PROJECT_MEMORY.md — remote authoritative at s20/v0.13.1, snapshot→/tmp then fast-forward). P0 (user: "stops working when a second connection is added, restart required"): 3 Explore agents mapped the paths; H1 = mid-traffic reconnect re-binds 10808/10809 while untracked in-flight connection tasks pin their sockets (no SO_REUSEADDR anywhere) → bind WSAEADDRINUSE → Failed; PROVEN platform-split: Linux tolerates a listen re-bind over non-listening sockets (std rebind probe in scripts/rebind_probe.rs) while Windows does NOT — the sandbox cannot reproduce the bind refusal, so the regression test asserts the platform-independent observable (lingering client sees EOF after the switch) and pre-fix it FAILS (8.0 s timeout) / post-fix passes. FIX: Shared.conns tracks every connection task's AbortHandle (pruned at 4096 push), run() force-closes whatever the 3 s drain could not reap; AbortPumpsOnDrop aborts the DETACHED pump children too (dropping their JoinHandles leaves them running — found when the first EOF assertion failed; pumps held the socket halves forever); bind_with_retry (5 s budget, 250 ms interval; BIND_WAIT 2→8 s). H2: engine config/log stems now carry a deterministic FNV-1a hash of the FULL endpoint identity (engine_file_stem; same profile keeps its file across reconnects, same-tag different-endpoint profiles never share engine-<tag>.json again — temp probe/speed-test tunnels could rewrite the LIVE engine's config → phantom-live respawn budget exhaustion → DEGRADED while GUI showed Connected). H3: Core.store_mtx serializes profiles.json read-modify-write (import/remove/set_active/update; no await inside the critical sections). H1+H3 tests: picker_switch_with_lingering_client_reconnects, shutdown_force_closes_lingering_connections_and_frees_the_port (mt-net level: EOF + immediate re-bind). P1 PERF (user: v2rayN faster on ONE adapter): (1) TCP_NODELAY on all four legs (client→engine loopback socks5.rs establish; daemon accepted carrier; daemon dial_target ×2 — Nagle+delayed-ACK stalled each 16 KiB frame's tail segment ~40 ms on Linux peers ≈ Mbps-per-carrier stop-and-wait); (2) quarantine can no longer BLACKHOLE the sole link (pick_excluding last-resort fallback over quarantined/down links when no healthy candidate; the dial decides, fail-open; retry-exclusion contract kept; module doc + contract test updated, new sole-link test); (3) production background prober now uses the user-configured probe_target (was hardcoded 1.1.1.1:443); (4) ACK coalescing (ACK_COALESCE_FRAMES=8 / 20 ms window, per-session AckCoalescer, flushed by CLOSE handling + the 1 s PING heartbeat → ≤1 s ACK latency; cumulative semantics unchanged; guard scoped before the await for Send); (5) PONG echoes on the RECEIVING carrier (schedule_bytes re-picked — rtt_ms was cross-contaminated across carriers feeding ECF/Balanced wrong data); (6) send-path copies 5→3 per frame (encode_session_borrowed encodes header+payload in one pass from the slice; send_data sends slices; wire-format identity test); (7) carrier writer batches up to 16 co-queued frames into ONE write_vectored (write_all_vectored with partial-write advance). O — QoS FAIR SHARE toggle (Settings → Network, qos_fair serde-default bool): new mt-net/src/qos.rs QosScheduler groups flows by TARGET, 1 s tick refills buckets at aggregate/groups ONLY under real contention (≥2 groups AND aggregate ≥ 2 MiB/s floor; single group/idle = unlimited), pump read gate blocks until budget (20 ms retry), ALWAYS attached at connect so set_settings flips it LIVE (no reconnect — scheduler flag atomic; log line emitted); 5 scheduler unit tests + live-flip persistence test + GUI draft test. P — merged Exit & speed panel: one Status card (shared header, both triggers side-by-side Fill-width, exit row + verdict + speed row + hint stacked compactly; all messages/state unchanged; D24-safe inside the page scroll), rig-verified sf=1.0 (VLM + knob-pixel-diff: 1957 px changed, knob center 39→55) + sf=1.5. TESTS 490/0 (+16: mt-engine stem ×4, merge sole-link ×1, mt-net shutdown-force-close ×1 + qos ×5, frame wire-identity ×1, root P0 ×1 + qos ×1, mt-app qos ×1, mt-service serde ×1). GATES: fmt clean; clippy host + windows-gnu -D warnings clean (zig shims + fresh ziglang wheel); cargo test --workspace 490/0; vendored iced 6/0; mt-app --smoke 0; rig sf=1.0+1.5; zigbuild PE32+ ×4 + ELF ×2. RELEASE: v0.14.0 tag → run 45477702090 — attempt-1 ubuntu leg carrier_death 'no live bond carrier' (documented S11/S18/S19 pre-existing runner race, signature exactly as backlogged), attempt-2 windows leg domain_targets 60 s deadline, attempt-3 SUCCESS; same-commit ci.yml runs green on BOTH OS legs throughout + local 2-core taskset stress 8/8; all four logs archived under harness/memory/agent-reports/s21-ci-*.log. Assets download-verified: multitunnel-v0.14.0-windows-x64.zip 6,540,352 B (11-entry manifest identical shape to v0.13.1: multitunnel.exe + tray + docs + server scripts) + mt-merge-server x86_64 1,845,824 B + aarch64 1,864,608 B. PARKED (needs user input or new scope): single-instance guard (H4 — two app instances race the sysproxy snapshot; needs a CreateMutexW design decision), BufReader-style carrier read batching (2 read_exact syscalls/frame), MAX_PAYLOAD 64 KiB protocol bump (needs a coordinated mt-merge-server update on the user's VM), carrier-lifecycle race root-cause (S19 backlog — this session hit it 2/3 release legs; reruns shipped), upstream-iced migration, subscription auto-update, vmess validation, profiles.json locking (partially addressed by store_mtx in-process; cross-process locking still open), engine log rotation, listener max_conns. Copilot CLI still blocked by PAT (S20 finding unchanged — cloud route works, needs only pull-requests:write).
 2026-11-26T03:30Z | s22 | IMPLEMENTED (five user asks; v0.15.0). Sandbox recycled AGAIN (rustup 1.100.0 + ziglang 0.14.1 + cargo-zigbuild 0.24.1 + xkblibs + python-xlib rebuilt per the ritual; the pypkgs ziglang wheel was INCOMPLETE AGAIN mid-gates — ring stddef.h — reinstalled from PyPI, S20/S21 pattern). Local clone was stale at s13 with a dirty (stale script-generated) PROJECT_MEMORY.md — discarded, fast-forwarded to origin (S21 had already shipped v0.14.0 with the P0/P1/QoS/merged-panel brief; the FIVE asks below landed after that close, so they were all pending). (1) TWO-COLUMN Exit & speed split (user: "exit ip outputs on the left side and the speed test outputs on the right side"): screens.rs status() now builds exit_out/speed_out columns (headings "Exit IP"/"Speed test", quiet placeholders while idle) pushed as row![exit_out, speed_out] — verdict card + IP/chips rows stacked LEFT, Mbps + chips + note RIGHT; pure view change, D24-safe in the page scrollable. (2) UPLOAD SPEED TEST (user: "After the speed test has concluded, show another button to test upload speed"): mt-service contract + SettingsView.speed_test_upload_url (serde default http://speed.cloudflare.com/__up, D8 migration), RealService::speed_test_upload POSTs a fixed 25 MiB body through the same tunnel selection (S20 profile_id rule), clock after connect, 64 KiB chunks, same 30 s/5 s stall/64 MiB caps, partial-measurement-stands semantics, status guard ONLY on clean completion (a truncated body may never get a response); SpeedTestUpload/SpeedUploadTested messages + speed_up/speed_up_testing state + the button appears ONLY after the download concluded (gate app.speed.is_some() && !speed_testing), mutual exclusion across all three triggers; upload URL field in Settings → Advanced; redact_url in the diag header; MockService simulated. Tests: temp-tunnel measurement + 403-rejection (spawn_upload_server fixture drains Content-Length then answers), mock message flow, mt-service serde migration/roundtrip. (3) PER-LINK ADAPTER (user: "Under the links, show which adapter each link is going through"): mt-service::adapter_of_link_name parses the engine's "<tag> [adapter]" grammar (dedup " #N" stripped for display; rate-sampler's tricky cases pinned: "Wi-Fi Direct"/"Ethernet 2" verbatim), LinkStatus.adapter_name (serde default) populated in link_status_of (per-connection) + carrier_snapshot map (stream mode) + mock seeds; the Links card renders a dim `via <adapter>` line UNDER each link row — survives util::short's 32-char truncation that previously hid the [adapter] suffix. (4) OPEN LOG FOLDER FIX (user: "clicking the 'Open log folder button' does not open the log folder"): ROOT CAUSE was the diagnostics-file gate — the button resolved diag_path (None unless the sink started THIS process lifetime) so after every restart it printed "turn diagnostics on first"; FIXED by a new MtService::log_dir() (always resolvable), the handler create_dir_all's + opens the folder itself via open_folder_command (explorer/open/xdg-open per platform, spawn-result reported, never silent — Settings status + Logs-tab WARN), fetched at boot (fetch_log_dir in BOTH init paths — the iced init batch was the second gap the rig caught); ADJACENT ROOT CAUSE fixed: a diagnostics toggle persisted ON now re-enables the sink AT CONSTRUCTION (RealService::with) — the toggle used to lie after a restart; diagnostics_persisted_on_starts_at_construction pins it. (5) ICON FILL (user: "have the icon utilize as much of the available space as possible"): make_logo_assets.py re-crops emblem+lockup to their ALPHA bboxes after key-out (a dead ~71px left gradient band made the mark render off-center at 62% height fill), margins 0.04→0.02 (16/32/48/24/64) + 0.10→0.03 (256 lockup); measured fills 93.8-96.9%w × 75.0-76.6%h (was 75-81%w × 61-64%h — glyph ~24% larger in the tray's 32px tile); all test-pinned invariants hold (4-entry ICOs 16/32/48/256, biWidth=32/biHeight=64, logo_64=16384 B, logo_24=2304 B, deterministic re-runs, banner/logo/color_wheel untouched). RIG (sf=1.0 + 1.5, mock): panel split + placeholders, download result right-column, upload button appears → "testing upload…" → result rows, exit probe left-column verdict card, via-adapter lines, Open-log-folder click → folder created + honest per-platform failure surfaced (no xdg-open in bare Xvfb — the Windows path spawns explorer; spawn success unit-pinned via open_folder_command_targets_the_os_file_manager). LESSONS: (a) gui_drive click on a button that APPEARED after a reflow needs a pointer-jiggle first (no motion event → iced hover stale → first click no-op); (b) content appended BELOW a scrollable's initial viewport looks like a stale frame in shots — damage-instrumented the vendored compositor (reverted byte-identical) to prove the renderer was right: the rows were below the fold, scroll at a y INSIDE the viewport (400 was above it, 500 works). TESTS 497/0 (+7). GATES: fmt clean; clippy host + windows-gnu -D warnings clean (zig shims + fresh wheel); vendored iced 6/0; mt-app --smoke 0; zigbuild PE32+ x4 (multitunnel 19.3MB, mt-app 14.9MB, tray 10.2MB, merge-server 2.4MB) + ELF x2 (x86_64 1.25MB, aarch64 1.23MB); version 0.15.0; README v0.15 section + QUICKSTART note. DISK: 9.9G rootfs hit 99% mid-gates — rm -rf target/debug + target/x86_64-pc-windows-gnu between phases (S15 sequence discipline).
 2026-11-27T12:10Z | s23 | IMPLEMENTED (three asks; PR-flow session). Sandbox recycled AGAIN (rustup 1.100.0 + clippy/rustfmt components + ziglang 0.14.1 re-PyPI'd — the wheel was INCOMPLETE for the THIRD time, no lib/libc/include; xkblibs re-extracted to /tmp/xkblibs; python-xlib to tools/pypkgs-xlib). (1) OPEN LOG FOLDER, OS ERROR 50 (user: "could not open the log folder: The request is not supported. (os error 50)"): root cause pinned — v0.15.0 spawned explorer.exe with DEFAULT (inherited) stdio; mt-app is a Windows GUI-subsystem binary whose standard handles are NULL, Rust's spawn duplicates them into the child (STARTF_USESTDHANDLES) and CreateProcessW fails with ERROR_NOT_SUPPORTED; console-subsystem parents never hit it (why every manual probe passed while the user's GUI failed). FIX: crates/mt-app/src/shell_open.rs — Windows ShellExecuteW primary (windows-sys Win32_UI_Shell, already a tray dep; verb "open", rc>32 = ok, documented SE_* code table) + explorer.exe fallback now with Stdio::null() on ALL THREE streams (the inheritance trap is unreachable by construction); macOS open(1)/other xdg-open(1), all NUL-stdio; failures name every mechanism tried + the folder (never silent; Settings status + Logs WARN). Rig: click → "could not open the log folder: xdg-open is not installed (tried to open /tmp/MultiTunnel-mock-logs)" — the actionable format, vs the old bare "(os error 50)". (2) RESET ALL SETTINGS (user ask): Settings "Reset" card, ErrorText, TWO-CLICK confirm (first press arms: button relabels + status line names the exact scope; second executes; any other settings mutation disarms via save_settings). mt_service::SettingsView::default() is now the SINGLE stock source (impl Default; five diverging literals collapsed: SettingsFile, GUI placeholder, mock, theme/real_service test fixtures; serde-defaults == Default pinned by test, theme_mode's absent-key legacy semantics documented). Scope: every Settings field; profiles + routing rules untouched (stated in the card caption); bonded_adapters cleared through set_bonded_adapters([]) — the real set_settings deliberately preserves server-side bonded selection (stale-GUI-copy guard); rows reload selected=false; always-on-top UN-applies at stock(false) via apply_window_level both directions; reset-specific SettingsSaved note. (3) RESEARCH PASS (user: "check for improvements… by checking the changelogs of similar applications"): docs/research/2026-11-27-stability-changelog-survey.md — v2rayN 7.25.6/Clash Verge 2.5.10/Hiddify/Speedify → adopted: 7-day stale-log sweep (v2rayN's headline hygiene item; diagnostics::sweep_old_logs: retention 7d, newest-40 floor, live diag file protected, best-effort, runs at every RealService::with with an INFO line), remembered window size (desktop-client convention; window_w/h serde-default 1080x720 D8, debounced 2-quiet-tick save — a drag never writes per event, clamped 880x580, restored via iced::window::resize on load, reset restores stock), 1 MiB SO_SNDBUF/SO_RCVBUF + existing NODELAY on the merge daemon's two WAN socket populations via socket2 (transitive dep — zero new lockfile entries; BDP 150 Mbps × 50 ms ≈ 0.94 MiB; loopback engine hops intentionally OS-default). Confirmed parity: real-delay latency (already through-tunnel), balancer policies (5 merge policies superset), connection-info card, core pinning (D14). Rejected with reasons: mux knobs, MASQUE, redundant mode (doubles traffic), TUN, subscription headers. TESTS 520/0 (+10: shell_open ×4, stock-defaults ×2, GUI reset ×1 + window ×1, real_service reset round-trip + window round-trip + construction-sweep ×3, daemon tune ×1, diagnostics sweep ×3). GATES all green: fmt; clippy host + windows-gnu (zig shims, fresh wheel) -D warnings; cargo test --workspace 510/0 + s23 subset; vendored iced 6/0 (cargo test -p iced_graphics -p iced_tiny_skia --lib from the workspace root — the standalone-manifest route errors on the parent workspace); mt-app --smoke 0; zigbuild PE32+ ×3 (multitunnel 19.2MB, tray 10.2MB, mt-merge-server) + ELF ×2; rig sf=1.0 + 1.5 (reset two-click arm→confirm→"settings reset to defaults — servers and routing rules untouched", seed reverted 6770852/#6750A4/purple-selected, log-folder honest failure, 0 panics). DELIVERY FLOW CHANGED THIS SESSION per the user's ask: PR + @copilot reviewer instead of direct-to-main; NO tag/release (review first). RIG LESSONS: the Settings page scroll needs ONE wheel tick per verb invocation with ≥1 s between (consecutive ticks in one scroll:X,Y,N verb coalesce — only ~1 lands; a tick over a pick_list is eaten; (1070,400) — the right card-padding strip — is widget-free); VLM on crops beats whole-shot pixel archaeology; row-profile dy alignment can hallucinate (err=0 at impossible shifts on sparse profiles) — track a distinctive feature (the red swatch row) instead. DISK: hit 100% twice — rm -rf target/debug + the /tmp/mt-base worktree between phases (S15/S22 discipline).
+2026-11-27T12:40Z | s23-pr | PR FLOW EXECUTED (the user's delivery instruction): branch s23-open-folder-reset-perf pushed (5 feature commits + backup), PR #3 opened via REST (https://github.com/maniesh-NR/MultiTunnel/pull/3 — full root-cause/write-up body; the plugin create_pull_request tool errored twice, curl worked first try). @copilot REVIEWER REQUESTED exactly as asked — POST /pulls/3/requested_reviewers {"reviewers":["copilot"]} (the API equivalent of gh pr edit --add-reviewer @copilot; no gh binary in the sandbox) — GitHub ACCEPTED the call but SILENTLY DROPPED the reviewer (requested_reviewers returns empty; same entitlement finding as S19/S20/S21 — no Copilot code review on this repo/PAT). Documented in a PR comment with the re-request command for an entitled machine. CI kicked off on the PR: lint + test(ubuntu) + test(windows) + cross-check all running. NO tag/release this session by design (review first); v0.16.0 material is ready when the PR merges.

### Commit: 978da2b - backup(s23-pr-ready): 2026-11-27 memory + gates + rig evidence (Z User, 2026-11-27)
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

( PROJECT_MEMORY.md's own diff is elided here — the durable
  sections above are hand-maintained; see harness/memory/LEDGER.md
  for the full stage history. )
```

