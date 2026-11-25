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
### Commit: 033c009 - backup(s21-close-v0.14.0): 2026-11-24T20:57:46Z (Z User, 2026-11-24)
diff --git a/Cargo.lock b/Cargo.lock
index 91f9783..c32d817 100755
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1962,7 +1962,7 @@ dependencies = [
 
 [[package]]
 name = "mt-app"
-version = "0.13.1"
+version = "0.14.0"
 dependencies = [
  "async-trait",
  "iced",
@@ -2073,7 +2073,7 @@ dependencies = [
 
 [[package]]
 name = "multitunnel"
-version = "0.13.1"
+version = "0.14.0"
 dependencies = [
  "anyhow",
  "async-trait",
diff --git a/harness/memory/DECISIONS.md b/harness/memory/DECISIONS.md
index a3f7c46..7949054 100755
--- a/harness/memory/DECISIONS.md
+++ b/harness/memory/DECISIONS.md
@@ -512,3 +512,76 @@ apps' changelogs.
    Verge Rev / v2rayN / FlClash / Hiddify / Speedify changelogs mapped
    to shipped vs proposed (next: per-link live status badges, exit-IP
    auto-refresh, empty states).
+
+## D29 — S21: P0 reconnect fix — tracked force-close at drain end + bind retry (2026-11-24)
+
+User report: "The connection works for some time, but it stops working
+when a second connection is added. The application must be restart for it
+to work again."
+
+1. ROOT CAUSE (H1): connection tasks spawned per accepted proxy
+   connection were UNTRACKED; a shutdown drained for 3 s and returned
+   WITH LIVE CONNECTIONS. Each lingering socket pins the listener port's
+   local endpoint; the next connect re-binds 10808/10809. Linux tolerates
+   a listen re-bind over non-listening sockets (proven with a standalone
+   std probe — scripts/rebind_probe.rs); WINDOWS does NOT
+   (WSAEADDRINUSE), so every picker switch after a busy session landed
+   Failed and only process death (RST) freed the ports. Reproduced
+   first: the regression test fails pre-fix (EOF never arrives) and
+   passes post-fix (bug-first contract).
+2. FIX: track every connection task's AbortHandle in `Shared.conns`
+   (pruned at 4096 pushes; aborting finished tasks is a no-op), and at
+   the END of the drain grace FORCE-CLOSE whatever wait_idle could not
+   reap. Listeners re-bind after a `bind_with_retry` (5 s budget / 250 ms
+   interval; BIND_WAIT 2→8 s) absorbing the kernel teardown race.
+3. PUMP DETACHMENT: `tunnel` spawns two pump tasks; aborting the outer
+   connection task merely DROPS their JoinHandles (detach), leaving the
+   socket halves open — the EOF assertion caught it. `AbortPumpsOnDrop`
+   aborts the pumps whenever the tunnel future dies (normal completion
+   too: no-op).
+4. H2 (same report family): engine config/log filenames keyed by tag
+   alone collided across profiles sharing a `#fragment` — a temp
+   probe/speed-test engine REWROTE the live engine's config and its next
+   respawn dialed the wrong server into DEGRADED while the GUI showed
+   Connected. `engine_file_stem` appends a deterministic FNV-1a hash of
+   the full endpoint identity (same profile → same file across
+   reconnects; different profiles → different files). Wire files gain a
+   hash suffix; only mt-engine's lifecycle-test helper had to follow.
+5. H3: profiles.json load→mutate→save cycles are now serialized by
+   `Core::store_mtx` (GUI iced tasks run concurrently; the CLI can race
+   the GUI; shared per-PID temp name collided on Windows).
+
+## D30 — S21: QoS fair share by destination + live toggle; merged Status panel (2026-11-24)
+
+User asks (verbatim): "Include a QoS toggle that can be turned on to
+prevent a single application from dominating all the bandwidth." / "Have
+the 'check exit ip' and 'speed test' in the same UI block, as there is a
+lot of blank space in their individual blocks."
+
+1. GROUPING: local flows all originate from 127.0.0.1 ephemeral ports —
+   per-PROCESS attribution is impossible at the proxy layer. Flows group
+   by TARGET (`host:port`): one site/app = one group regardless of how
+   many connections it opens (a download manager spreading over many
+   servers counts as several groups — documented in the Settings copy).
+2. SHAPING: the existing 1 s ticker measures aggregate up+down and
+   refills per-group token buckets at `aggregate/groups` ONLY under real
+   contention (≥2 active groups AND aggregate ≥ 2 MiB/s floor). Pumps
+   acquire a `PUMP_BUF`-sized budget before each read (20 ms retry until
+   the next refill). One group alone / idle tunnels are UNLIMITED — no
+   throughput tax without contention.
+3. LIVE TOGGLE: the scheduler is ALWAYS attached at connect (initialized
+   from settings), so `set_settings` flips an AtomicBool — NO reconnect
+   (unlike diagnostics/kill-switch, nothing in the data plane changes).
+   Disabling unblocks waiters and clears the table (tested).
+4. MERGED PANEL: exit-IP + speed-test share ONE Status card (shared
+   header, both triggers side-by-side Fill-width, result rows stacked
+   with label prefixes "exit"/"speed"); all messages/state unchanged
+   (pure view merge inside the D24 page scrollable).
+5. P1 PARITY (same batch): TCP_NODELAY on all four carrier legs; ACK
+   coalescing (8 frames / 20 ms, CLOSE+PING flush, cumulative semantics
+   unchanged); PONG echoes on the receiving carrier (RTT was
+   cross-contaminated); send-path copies 5→3 per frame (single-pass
+   encoder, wire-identity test); carrier writer batches ≤16 frames per
+   write_vectored; quarantine last-resort fallback (a probe can no
+   longer blackhole the only link); production prober uses the
+   user-configured probe target.
diff --git a/harness/memory/LEDGER.md b/harness/memory/LEDGER.md
index 62d858e..2fef1de 100755
--- a/harness/memory/LEDGER.md
+++ b/harness/memory/LEDGER.md
@@ -218,3 +218,4 @@
 2026-11-21T15:10Z | s19 | RELEASED v0.13.0 (tag run 44910934439 SUCCESS ~7 min; assets verified by download+unzip: multitunnel-v0.13.0-windows-x64.zip 6,506,263 B, 11-entry manifest integrity OK, README/QUICKSTART/server scripts carried; mt-merge-server x86_64 1,826,272 B + aarch64 1,860,808 B; zip staged in sandbox download/v0.13.0-staging/). CI SAGA: the push leg failed on mt-bond carrier_death (documented runner flake, 4th occurrence) → rerun wedged 54 min inside domain_targets_dial (log captured: harness/memory/agent-reports/s19-ci-wedge-job-136868435499.log — 5/6 tests ok, domain_targets 'over 60 seconds', cargo+integration orphans killed) → HARDENED both tests with 60-90 s deadlines (bodies untouched; a wedge now fails in a minute with a LEDGER pointer) → next push run: BOTH tests failed loudly on runners (windows carrier_death 'no live bond carrier', ubuntu domain_targets deadline) while the same-commit PR run was fully green and local 2-core taskset stress (8× integration at test-threads=6) stayed 0-fail → rerun: ALL FOUR JOBS GREEN. ROOT-CAUSE LEAD (backlog, top priority): pick_carrier's 'no live bond carrier' = usable() empty = Wi-Fi dead + LTE flagged !alive mid-death-replay — a carrier-lifecycle race in PRE-EXISTING S11/S18 mt-bond code, GitHub-runner-only (slower I/O scheduling), never reproduced locally; investigate the death handler's replay path + LTE alive-flag transitions on a runner-like environment. Review PR #1 commented + left open.
 2026-11-22T04:20Z | s20 | COPILOT CODE REVIEW COMPLETED (the S19 leftover): the cloud Copilot reviewer finally ran from this sandbox via POST /repos/maniesh-NR/MultiTunnel/pulls/1/copilot-code-review — the user's runbook pointed at the standalone @github/copilot CLI, which is installed and network-verified but hard-blocked by the PAT: api.individual.githubcopilot.com answers 401 "Personal Access Token does not have \"Copilot Requests\" permission" (authentic server response, NOT a proxy block; the permission can only be added by editing the PAT in the GitHub web UI — told the user exactly that). The REST code-review endpoint needs only pull-requests:write, which the PAT has. Effort ladder per the fallback instruction: max → quota-rejected ("insufficient remaining quota"), balanced → quota-rejected, LITE → ran (review 6010002263 on 52f1d3c): 12 findings (5 High: step-0 self-heal never cleared sysproxy_set → later no-snapshot restore DISABLES a user's pre-existing proxy; give_up check-and-set unserialized with disconnect → black-hole re-installed AFTER an explicit disconnect; proxy pointed at 127.0.0.1:9 BEFORE the bind → a foreign port-9 owner receives the blocked traffic; black-hole listener stopped even when restore FAILED → stranded blocked proxy, no retry, false "restored" log; diag header wrote user URLs verbatim → credential leak in the paste-ready file; 7 Medium: diag-on-while-connected misses engine debug, failed enable leaves persisted toggle on, exit probe/speed test ride a session that is not the target profile, unbounded pre-\r\n\r\n head buffer, port-0 edit accepted, edited REALITY material unvalidated, temp_link_for pop() = lowest-priority adapter). ALL 12 fixed in 4923566: step-0 clears ownership on successful restore (keeps flag+listener on failure for retry); new Core::proxy_mtx serializes every proxy hand-off (give_up/disconnect×2/toggle-off/step-0/arming; the black-hole BIND hoisted outside the lock so no std lock crosses an await — spawn_blackhole split into bind_blackhole+engage_blackhole); bind-BEFORE-set with no-set-on-bind-failure (stale refused entry keeps blocking, nothing routes to a foreign listener); restore-first release on all paths with ERROR log + retry on failure; redact_url() (scheme://host:port/path, userinfo/query/fragment stripped, invalid → placeholder); diag-on-live reconnects (engine level is spawn-time-only); failed enable reverts the persisted toggle; Session.profile_id pins the live-session match for BOTH probes; SPEED_TEST_MAX_HEAD 64 KiB independent cap; port-0 + REALITY structural validation (vless parser rules, is_base64url now crate-visible; fingerprint deliberately unchecked = import parity); first-link selection (temp_link_for AND test_latency). ROUND 2 (lite, on 4923566): CLEAN PASS — review 6010239912, zero inline comments. PR #1: 12 per-finding maintainer replies + summary, CLOSED. Transcript: harness/memory/agent-reports/s20-copilot-review-transcript.md. TESTS 474/0 (+8: connect_selfheal_clears_stale_proxy_ownership, update_profile_rejects_port_zero_and_invalid_reality, diag_header_redacts_url_credentials, speed_test_rejects_a_header_flood, probes_fall_back_when_active_profile_switched, temp_link_uses_the_top_priority_first_link, diagnostics_enable_failure_reverts_the_toggle, diagnostics_enable_while_connected_reconnects). Gates: fmt clean; clippy host + windows-gnu -D warnings clean; vendored iced 6/0; mt-app --smoke 0; rig launch+alive+render-healthy (82% non-white, 1738 colors — zero GUI code changed this round); zigbuild PE32+ ×4 + ELF ×2. SANDBOX RESTORE NOTES: rustc 1.100.0 reinstalled (rustup minimal), tools/pypkgs ziglang wheel was INCOMPLETE (libc only musl/wasi — stddef.h compile failure; reinstalled 0.14.1 from PyPI), python-xlib reinstalled, /tmp/xkblibs re-extracted from the my-project root debs; DISK: /tmp/mt-target debug trees wiped twice (ENOSPC at 94% — stage release binaries before wiping, per the S19 lesson).
 2026-11-22T04:16Z | s20 | v0.13.1 RELEASED: tag on 4923566 → release run 44983522874 SUCCESS (~4 min, no cold-cache stall); ci ×2 on main (44983512609/44983518575) ALL GREEN first try. Assets verified by download+unzip: multitunnel-v0.13.1-windows-x64.zip 6,511,395 B integrity OK (multitunnel.exe PE32+ console 13.8 MB + multitunnel-tray.exe PE32+ GUI 2.95 MB + README v0.13.1 section + QUICKSTART v0.13.1 note + server scripts + LICENSE + THIRD_PARTY_NOTICES + config.example) + mt-merge-server {x86_64 1,826,272 B, aarch64 1,860,808 B} ELFs. Verified copies staged in sandbox download/ (v0.13.1-verify-*). S20 CLOSED.
+2026-11-24T21:00Z | s21 | IMPLEMENTED (four asks: P0 bug, P1 perf parity, QoS toggle, merged panel; v0.14.0). Sandbox recycled AGAIN (rustup/zigbuild/xkblibs gone; tools/pypkgs ziglang wheel INCOMPLETE again — no lib/libc/include → ring stddef.h failure mid-gates; reinstalled 0.14.1 from PyPI per the S20 note; local clone was STALE at s13 with a dirty PROJECT_MEMORY.md — remote authoritative at s20/v0.13.1, snapshot→/tmp then fast-forward). P0 (user: "stops working when a second connection is added, restart required"): 3 Explore agents mapped the paths; H1 = mid-traffic reconnect re-binds 10808/10809 while untracked in-flight connection tasks pin their sockets (no SO_REUSEADDR anywhere) → bind WSAEADDRINUSE → Failed; PROVEN platform-split: Linux tolerates a listen re-bind over non-listening sockets (std rebind probe in scripts/rebind_probe.rs) while Windows does NOT — the sandbox cannot reproduce the bind refusal, so the regression test asserts the platform-independent observable (lingering client sees EOF after the switch) and pre-fix it FAILS (8.0 s timeout) / post-fix passes. FIX: Shared.conns tracks every connection task's AbortHandle (pruned at 4096 push), run() force-closes whatever the 3 s drain could not reap; AbortPumpsOnDrop aborts the DETACHED pump children too (dropping their JoinHandles leaves them running — found when the first EOF assertion failed; pumps held the socket halves forever); bind_with_retry (5 s budget, 250 ms interval; BIND_WAIT 2→8 s). H2: engine config/log stems now carry a deterministic FNV-1a hash of the FULL endpoint identity (engine_file_stem; same profile keeps its file across reconnects, same-tag different-endpoint profiles never share engine-<tag>.json again — temp probe/speed-test tunnels could rewrite the LIVE engine's config → phantom-live respawn budget exhaustion → DEGRADED while GUI showed Connected). H3: Core.store_mtx serializes profiles.json read-modify-write (import/remove/set_active/update; no await inside the critical sections). H1+H3 tests: picker_switch_with_lingering_client_reconnects, shutdown_force_closes_lingering_connections_and_frees_the_port (mt-net level: EOF + immediate re-bind). P1 PERF (user: v2rayN faster on ONE adapter): (1) TCP_NODELAY on all four legs (client→engine loopback socks5.rs establish; daemon accepted carrier; daemon dial_target ×2 — Nagle+delayed-ACK stalled each 16 KiB frame's tail segment ~40 ms on Linux peers ≈ Mbps-per-carrier stop-and-wait); (2) quarantine can no longer BLACKHOLE the sole link (pick_excluding last-resort fallback over quarantined/down links when no healthy candidate; the dial decides, fail-open; retry-exclusion contract kept; module doc + contract test updated, new sole-link test); (3) production background prober now uses the user-configured probe_target (was hardcoded 1.1.1.1:443); (4) ACK coalescing (ACK_COALESCE_FRAMES=8 / 20 ms window, per-session AckCoalescer, flushed by CLOSE handling + the 1 s PING heartbeat → ≤1 s ACK latency; cumulative semantics unchanged; guard scoped before the await for Send); (5) PONG echoes on the RECEIVING carrier (schedule_bytes re-picked — rtt_ms was cross-contaminated across carriers feeding ECF/Balanced wrong data); (6) send-path copies 5→3 per frame (encode_session_borrowed encodes header+payload in one pass from the slice; send_data sends slices; wire-format identity test); (7) carrier writer batches up to 16 co-queued frames into ONE write_vectored (write_all_vectored with partial-write advance). O — QoS FAIR SHARE toggle (Settings → Network, qos_fair serde-default bool): new mt-net/src/qos.rs QosScheduler groups flows by TARGET, 1 s tick refills buckets at aggregate/groups ONLY under real contention (≥2 groups AND aggregate ≥ 2 MiB/s floor; single group/idle = unlimited), pump read gate blocks until budget (20 ms retry), ALWAYS attached at connect so set_settings flips it LIVE (no reconnect — scheduler flag atomic; log line emitted); 5 scheduler unit tests + live-flip persistence test + GUI draft test. P — merged Exit & speed panel: one Status card (shared header, both triggers side-by-side Fill-width, exit row + verdict + speed row + hint stacked compactly; all messages/state unchanged; D24-safe inside the page scroll), rig-verified sf=1.0 (VLM + knob-pixel-diff: 1957 px changed, knob center 39→55) + sf=1.5. TESTS 490/0 (+16: mt-engine stem ×4, merge sole-link ×1, mt-net shutdown-force-close ×1 + qos ×5, frame wire-identity ×1, root P0 ×1 + qos ×1, mt-app qos ×1, mt-service serde ×1). GATES: fmt clean; clippy host + windows-gnu -D warnings clean (zig shims + fresh ziglang wheel); cargo test --workspace 490/0; vendored iced 6/0; mt-app --smoke 0; rig sf=1.0+1.5; zigbuild PE32+ ×4 + ELF ×2. RELEASE: v0.14.0 tag → run 45477702090 — attempt-1 ubuntu leg carrier_death 'no live bond carrier' (documented S11/S18/S19 pre-existing runner race, signature exactly as backlogged), attempt-2 windows leg domain_targets 60 s deadline, attempt-3 SUCCESS; same-commit ci.yml runs green on BOTH OS legs throughout + local 2-core taskset stress 8/8; all four logs archived under harness/memory/agent-reports/s21-ci-*.log. Assets download-verified: multitunnel-v0.14.0-windows-x64.zip 6,540,352 B (11-entry manifest identical shape to v0.13.1: multitunnel.exe + tray + docs + server scripts) + mt-merge-server x86_64 1,845,824 B + aarch64 1,864,608 B. PARKED (needs user input or new scope): single-instance guard (H4 — two app instances race the sysproxy snapshot; needs a CreateMutexW design decision), BufReader-style carrier read batching (2 read_exact syscalls/frame), MAX_PAYLOAD 64 KiB protocol bump (needs a coordinated mt-merge-server update on the user's VM), carrier-lifecycle race root-cause (S19 backlog — this session hit it 2/3 release legs; reruns shipped), upstream-iced migration, subscription auto-update, vmess validation, profiles.json locking (partially addressed by store_mtx in-process; cross-process locking still open), engine log rotation, listener max_conns. Copilot CLI still blocked by PAT (S20 finding unchanged — cloud route works, needs only pull-requests:write).
diff --git a/harness/memory/agent-reports/s21-ci-carrier-death-release-run1.log b/harness/memory/agent-reports/s21-ci-carrier-death-release-run1.log
new file mode 100644
index 0000000..c42ab0b
--- /dev/null
+++ b/harness/memory/agent-reports/s21-ci-carrier-death-release-run1.log
@@ -0,0 +1,591 @@
+﻿2026-11-24T20:08:59.0729386Z Current runner version: '2.338.2'
+2026-11-24T20:08:59.0755442Z ##[group]Runner Image Provisioner
+2026-11-24T20:08:59.0756501Z Hosted Compute Agent
+2026-11-24T20:08:59.0757129Z Version: 20261111.635
+2026-11-24T20:08:59.0757806Z Commit: b8a594ea514ff44cadb5342986c68b1eac6e22ed
+2026-11-24T20:08:59.0758608Z Build Date: 2026-11-11T01:49:01Z
+2026-11-24T20:08:59.0759371Z Worker ID: {e19d26c1-8e95-4353-9497-e891587fab3c}
+2026-11-24T20:08:59.0760131Z Region: westus
+2026-11-24T20:08:59.0760708Z Cloud: Azure
+2026-11-24T20:08:59.0761289Z ##[endgroup]
+2026-11-24T20:08:59.0762874Z ##[group]Operating System
+2026-11-24T20:08:59.0763546Z Ubuntu
+2026-11-24T20:08:59.0764061Z 26.04.1
+2026-11-24T20:08:59.0764858Z LTS
+2026-11-24T20:08:59.0765436Z ##[endgroup]
+2026-11-24T20:08:59.0766039Z ##[group]Runner Image
+2026-11-24T20:08:59.0766813Z Image: ubuntu-26.04
+2026-11-24T20:08:59.0767371Z Version: 20261116.202.1
+2026-11-24T20:08:59.0768753Z Included Software: https://github.com/actions/runner-images/blob/ubuntu26/20261116.202/images/ubuntu/Ubuntu2604-Readme.md
+2026-11-24T20:08:59.0770350Z Image Release: https://github.com/actions/runner-images/releases/tag/ubuntu26%2F20261116.202
+2026-11-24T20:08:59.0771391Z ##[endgroup]
+2026-11-24T20:08:59.0772861Z ##[group]GITHUB_TOKEN Permissions
+2026-11-24T20:08:59.0775265Z Contents: write
+2026-11-24T20:08:59.0775871Z Metadata: read
+2026-11-24T20:08:59.0776555Z ##[endgroup]
+2026-11-24T20:08:59.0778699Z Secret source: Actions
+2026-11-24T20:08:59.0779791Z Cache mode: write
+2026-11-24T20:08:59.0780491Z Prepare workflow directory
+2026-11-24T20:08:59.1174316Z Prepare all required actions
+2026-11-24T20:08:59.1243861Z Getting action download info
+2026-11-24T20:08:59.4882166Z Download action repository 'actions/checkout@v4' (SHA:11d5960a326750d5838078e36cf38b85af677262)
+2026-11-24T20:08:59.5855381Z Download action repository 'dtolnay/rust-toolchain@stable' (SHA:39008827e6b4319a349e9a42dfc90579dd25f1ca)
+2026-11-24T20:08:59.8078102Z Download action repository 'Swatinem/rust-cache@v2' (SHA:d1527c8f67ef9af357290475d4af4b0fab8f12e0)
+2026-11-24T20:09:00.1496465Z Download action repository 'actions/upload-artifact@v4' (SHA:ea165f8d65b6e75b540449e92b4886f43607fa02)
+2026-11-24T20:09:00.2327639Z Download action repository 'softprops/action-gh-release@v2' (SHA:3bb12739c298aeb8a4eeaf626c5b8d85266b0e65)
+2026-11-24T20:09:00.6818899Z Complete job name: build-linux-daemon
+2026-11-24T20:09:00.7762510Z ##[group]Run actions/checkout@v4
+2026-11-24T20:09:00.7763485Z with:
+2026-11-24T20:09:00.7763970Z   repository: maniesh-NR/MultiTunnel
+2026-11-24T20:09:00.7768172Z   token: ***
+2026-11-24T20:09:00.7768642Z   ssh-strict: true
+2026-11-24T20:09:00.7769105Z   ssh-user: git
+2026-11-24T20:09:00.7769582Z   persist-credentials: true
+2026-11-24T20:09:00.7770087Z   clean: true
+2026-11-24T20:09:00.7770560Z   sparse-checkout-cone-mode: true
+2026-11-24T20:09:00.7771100Z   fetch-depth: 1
+2026-11-24T20:09:00.7771563Z   fetch-tags: false
+2026-11-24T20:09:00.7772036Z   show-progress: true
+2026-11-24T20:09:00.7772511Z   lfs: false
+2026-11-24T20:09:00.7772974Z   submodules: false
+2026-11-24T20:09:00.7773442Z   set-safe-directory: true
+2026-11-24T20:09:00.7773965Z   allow-unsafe-pr-checkout: false
+2026-11-24T20:09:00.7774945Z ##[endgroup]
+2026-11-24T20:09:01.0725321Z Syncing repository: maniesh-NR/MultiTunnel
+2026-11-24T20:09:01.0728317Z ##[group]Getting Git version info
+2026-11-24T20:09:01.0729533Z Working directory is '/home/runner/work/MultiTunnel/MultiTunnel'
+2026-11-24T20:09:01.0731210Z [command]/usr/bin/git version
+2026-11-24T20:09:01.0765382Z git version 2.56.1
+2026-11-24T20:09:01.0788788Z ##[endgroup]
+2026-11-24T20:09:01.0805088Z Temporarily overriding HOME='/home/runner/work/_temp/0ee5723b-6558-4d98-b163-e8c4c43f2b23' before making global git config changes
+2026-11-24T20:09:01.0807588Z Adding repository directory to the temporary git global config as a safe directory
+2026-11-24T20:09:01.0811738Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/MultiTunnel/MultiTunnel
+2026-11-24T20:09:01.0880888Z Deleting the contents of '/home/runner/work/MultiTunnel/MultiTunnel'
+2026-11-24T20:09:01.0884842Z ##[group]Initializing the repository
+2026-11-24T20:09:01.0889953Z [command]/usr/bin/git init /home/runner/work/MultiTunnel/MultiTunnel
+2026-11-24T20:09:01.1021879Z hint: Using 'master' as the name for the initial branch. This default branch name
+2026-11-24T20:09:01.1023932Z hint: will change to "main" in Git 3.0. To configure the initial branch name
+2026-11-24T20:09:01.1025881Z hint: to use in all of your new repositories, which will suppress this warning,
+2026-11-24T20:09:01.1027116Z hint: call:
+2026-11-24T20:09:01.1027747Z hint:
+2026-11-24T20:09:01.1028506Z hint: 	git config --global init.defaultBranch <name>
+2026-11-24T20:09:01.1029719Z hint:
+2026-11-24T20:09:01.1030577Z hint: Names commonly chosen instead of 'master' are 'main', 'trunk' and
+2026-11-24T20:09:01.1032432Z hint: 'development'. The just-created branch can be renamed via this command:
+2026-11-24T20:09:01.1034008Z hint:
+2026-11-24T20:09:01.1034939Z hint: 	git branch -m <name>
+2026-11-24T20:09:01.1035816Z hint:
+2026-11-24T20:09:01.1036973Z hint: Disable this message with "git config set advice.defaultBranchName false"
+2026-11-24T20:09:01.1038359Z Initialized empty Git repository in /home/runner/work/MultiTunnel/MultiTunnel/.git/
+2026-11-24T20:09:01.1049924Z [command]/usr/bin/git remote add origin https://github.com/maniesh-NR/MultiTunnel
+2026-11-24T20:09:01.1112076Z ##[endgroup]
+2026-11-24T20:09:01.1113003Z ##[group]Disabling automatic garbage collection
+2026-11-24T20:09:01.1117696Z [command]/usr/bin/git config --local gc.auto 0
+2026-11-24T20:09:01.1154456Z ##[endgroup]
+2026-11-24T20:09:01.1155601Z ##[group]Setting up auth
+2026-11-24T20:09:01.1163253Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
+2026-11-24T20:09:01.1200437Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
+2026-11-24T20:09:01.1829030Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
+2026-11-24T20:09:01.1868560Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
+2026-11-24T20:09:01.2173357Z [command]/usr/bin/git config --local --name-only --get-regexp ^includeIf\.gitdir:
+2026-11-24T20:09:01.2213363Z [command]/usr/bin/git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url
+2026-11-24T20:09:01.2504832Z [command]/usr/bin/git config --local http.https://github.com/.extraheader AUTHORIZATION: basic ***
+2026-11-24T20:09:01.2549715Z ##[endgroup]
+2026-11-24T20:09:01.2551454Z ##[group]Fetching the repository
+2026-11-24T20:09:01.2562088Z [command]/usr/bin/git -c protocol.version=2 fetch --no-tags --prune --no-recurse-submodules --depth=1 origin +e510e439249fe4072477c05040bd5b9d0678db6a:refs/tags/v0.14.0
+2026-11-24T20:09:01.8095159Z From https://github.com/maniesh-NR/MultiTunnel
+2026-11-24T20:09:01.8096932Z  * [new ref]         e510e439249fe4072477c05040bd5b9d0678db6a -> v0.14.0
+2026-11-24T20:09:01.8101801Z ##[endgroup]
+2026-11-24T20:09:01.8104116Z ##[group]Determining the checkout info
+2026-11-24T20:09:01.8106792Z ##[endgroup]
+2026-11-24T20:09:01.8111131Z [command]/usr/bin/git sparse-checkout disable
+2026-11-24T20:09:01.8172199Z [command]/usr/bin/git config --local --unset-all extensions.worktreeConfig
+2026-11-24T20:09:01.8205255Z ##[group]Checking out the ref
+2026-11-24T20:09:01.8210966Z [command]/usr/bin/git checkout --progress --force refs/tags/v0.14.0
+2026-11-24T20:09:01.8583679Z Note: switching to 'refs/tags/v0.14.0'.
+2026-11-24T20:09:01.8584787Z 
+2026-11-24T20:09:01.8585540Z You are in 'detached HEAD' state. You can look around, make experimental
+2026-11-24T20:09:01.8587284Z changes and commit them, and you can discard any commits you make in this
+2026-11-24T20:09:01.8588999Z state without impacting any branches by switching back to a branch.
+2026-11-24T20:09:01.8590563Z 
+2026-11-24T20:09:01.8591347Z If you want to create a new branch to retain commits you create, you may
+2026-11-24T20:09:01.8594121Z do so (now or later) by using -c with the switch command. Example:
+2026-11-24T20:09:01.8596063Z 
+2026-11-24T20:09:01.8596978Z   git switch -c <new-branch-name>
+2026-11-24T20:09:01.8598226Z 
+2026-11-24T20:09:01.8599026Z Or undo this operation with:
+2026-11-24T20:09:01.8600172Z 
+2026-11-24T20:09:01.8600878Z   git switch -
+2026-11-24T20:09:01.8601874Z 
+2026-11-24T20:09:01.8603372Z Turn off this advice by setting config variable advice.detachedHead to false
+2026-11-24T20:09:01.8605802Z 
+2026-11-24T20:09:01.8607054Z HEAD is now at e510e43 backup(s21-release-prep-2): 2026-11-24T20:08:15Z
+2026-11-24T20:09:01.8613092Z ##[endgroup]
+2026-11-24T20:09:01.8656923Z [command]/usr/bin/git log -1 --format=%H
+2026-11-24T20:09:01.8684695Z e510e439249fe4072477c05040bd5b9d0678db6a
+2026-11-24T20:09:01.9431291Z ##[group]Run dtolnay/rust-toolchain@stable
+2026-11-24T20:09:01.9432625Z with:
+2026-11-24T20:09:01.9433555Z   components: rustfmt, clippy
+2026-11-24T20:09:01.9434803Z   toolchain: stable
+2026-11-24T20:09:01.9435745Z ##[endgroup]
+2026-11-24T20:09:01.9526187Z ##[start-action display=Parse toolchain version;id=__dtolnay_rust-toolchain.parse]
+2026-11-24T20:09:01.9608440Z ##[group]Run if [[ -z $toolchain ]]; then
+2026-11-24T20:09:01.9609827Z [36;1mif [[ -z $toolchain ]]; then[0m
+2026-11-24T20:09:01.9611866Z [36;1m  # GitHub does not enforce `required: true` inputs itself. https://github.com/actions/runner/issues/1070[0m
+2026-11-24T20:09:01.9614067Z [36;1m  echo "'toolchain' is a required input" >&2[0m
+2026-11-24T20:09:01.9615483Z [36;1m  exit 1[0m
+2026-11-24T20:09:01.9616888Z [36;1melif [[ $toolchain =~ ^stable' '[0-9]+' '(year|month|week|day)s?' 'ago$ ]]; then[0m
+2026-11-24T20:09:01.9618560Z [36;1m  if [[ Linux == macOS ]]; then[0m
+2026-11-24T20:09:01.9620814Z [36;1m    echo "toolchain=1.$((($(date -v-$(sed 's/stable \([0-9]*\) \(.\).*/\1\2/' <<< $toolchain) +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:09:01.9622963Z [36;1m  else[0m
+2026-11-24T20:09:01.9624729Z [36;1m    echo "toolchain=1.$((($(date --date "${toolchain#stable }" +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:09:01.9626670Z [36;1m  fi[0m
+2026-11-24T20:09:01.9627926Z [36;1melif [[ $toolchain =~ ^stable' 'minus' '[0-9]+' 'releases?$ ]]; then[0m
+2026-11-24T20:09:01.9630091Z [36;1m  echo "toolchain=1.$((($(date +%s)/60/60/24-16569)/7/6-${toolchain//[^0-9]/}))" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:09:01.9632126Z [36;1melif [[ $toolchain =~ ^1\.[0-9]+$ ]]; then[0m
+2026-11-24T20:09:01.9634264Z [36;1m  echo "toolchain=1.$((i=${toolchain#1.}, c=($(date +%s)/60/60/24-16569)/7/6, i+9*i*(10*i<=c)+90*i*(100*i<=c)))" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:09:01.9636407Z [36;1melse[0m
+2026-11-24T20:09:01.9637284Z [36;1m  delimiter=EOF[0m
+2026-11-24T20:09:01.9638594Z [36;1m  while [[ $toolchain == *$delimiter* ]]; do delimiter+=_; done[0m
+2026-11-24T20:09:01.9640672Z [36;1m  printf 'toolchain<<%s\n%s\n%s\n' "$delimiter" "$toolchain" "$delimiter" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:09:01.9642409Z [36;1mfi[0m
+2026-11-24T20:09:01.9683303Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:09:01.9684830Z env:
+2026-11-24T20:09:01.9685667Z   toolchain: stable
+2026-11-24T20:09:01.9686573Z ##[endgroup]
+2026-11-24T20:09:01.9822111Z ##[end-action id=__dtolnay_rust-toolchain.parse;outcome=success;conclusion=success;duration_ms=29]
+2026-11-24T20:09:01.9831824Z ##[start-action display=Set $CARGO_HOME;id=__dtolnay_rust-toolchain.__run]
+2026-11-24T20:09:01.9889017Z ##[group]Run echo CARGO_HOME=${CARGO_HOME:-"$HOME/.cargo"} >> $GITHUB_ENV
+2026-11-24T20:09:01.9891016Z [36;1mecho CARGO_HOME=${CARGO_HOME:-"$HOME/.cargo"} >> $GITHUB_ENV[0m
+2026-11-24T20:09:01.9928506Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:09:01.9929943Z ##[endgroup]
+2026-11-24T20:09:02.0017022Z ##[end-action id=__dtolnay_rust-toolchain.__run;outcome=success;conclusion=success;duration_ms=18]
+2026-11-24T20:09:02.0024024Z ##[start-action display=Install rustup if needed;id=__dtolnay_rust-toolchain.__run_2]
+2026-11-24T20:09:02.0081548Z ##[group]Run if ! command -v rustup &>/dev/null; then
+2026-11-24T20:09:02.0082992Z [36;1mif ! command -v rustup &>/dev/null; then[0m
+2026-11-24T20:09:02.0086004Z [36;1m  curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused --location --silent --show-error --fail https://sh.rustup.rs | sh -s -- --default-toolchain none -y[0m
+2026-11-24T20:09:02.0088830Z [36;1m  echo "$CARGO_HOME/bin" >> $GITHUB_PATH[0m
+2026-11-24T20:09:02.0090053Z [36;1mfi[0m
+2026-11-24T20:09:02.0126566Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:09:02.0127916Z env:
+2026-11-24T20:09:02.0128777Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:02.0129929Z ##[endgroup]
+2026-11-24T20:09:02.0216981Z ##[end-action id=__dtolnay_rust-toolchain.__run_2;outcome=success;conclusion=success;duration_ms=18]
+2026-11-24T20:09:02.0224148Z ##[start-action display=Install rustup if needed on windows;id=__dtolnay_rust-toolchain.__run_3]
+2026-11-24T20:09:02.0241199Z ##[end-action id=__dtolnay_rust-toolchain.__run_3;outcome=skipped;conclusion=skipped;duration_ms=0]
+2026-11-24T20:09:02.0262344Z ##[start-action display=rustup toolchain install stable;id=__dtolnay_rust-toolchain.__run_4]
+2026-11-24T20:09:02.0304922Z ##[group]Run set -f  # split targets and components on commas and whitespace, without glob expansion
+2026-11-24T20:09:02.0307422Z [36;1mset -f  # split targets and components on commas and whitespace, without glob expansion[0m
+2026-11-24T20:09:02.0309196Z [36;1mflags=()[0m
+2026-11-24T20:09:02.0310348Z [36;1mfor t in ${targets//,/ }; do flags+=(--target "$t"); done[0m
+2026-11-24T20:09:02.0311983Z [36;1mfor c in ${components//,/ }; do flags+=(--component "$c"); done[0m
+2026-11-24T20:09:02.0313931Z [36;1mif [[ $toolchain == nightly && -n $components ]]; then flags+=(--allow-downgrade); fi[0m
+2026-11-24T20:09:02.0315777Z [36;1mlog=$(mktemp)[0m
+2026-11-24T20:09:02.0316733Z [36;1mtrap 'rm -f "$log"' EXIT[0m
+2026-11-24T20:09:02.0317797Z [36;1mfor attempt in 1 2 3 4 5; do[0m
+2026-11-24T20:09:02.0319985Z [36;1m  if rustup toolchain install "$toolchain" "${flags[@]}" --profile minimal --no-self-update --force-non-host 2>&1 | tee "$log"; then[0m
+2026-11-24T20:09:02.0322235Z [36;1m    break[0m
+2026-11-24T20:09:02.0323136Z [36;1m  else[0m
+2026-11-24T20:09:02.0323976Z [36;1m    status=$?[0m
+2026-11-24T20:09:02.0324993Z [36;1m  fi[0m
+2026-11-24T20:09:02.0326149Z [36;1m  if [[ $attempt == 5 ]] || ! grep -Fq 'checksum failed for ' "$log" \[0m
+2026-11-24T20:09:02.0328272Z [36;1m      || ! grep -Fq 'this is likely due to an ongoing update of the official release server' "$log"; then[0m
+2026-11-24T20:09:02.0330095Z [36;1m    exit "$status"[0m
+2026-11-24T20:09:02.0331045Z [36;1m  fi[0m
+2026-11-24T20:09:02.0332472Z [36;1m  echo "Retrying rustup toolchain install in 30 seconds (retry $attempt of 4)" >&2[0m
+2026-11-24T20:09:02.0334151Z [36;1m  sleep 30[0m
+2026-11-24T20:09:02.0335107Z [36;1mdone[0m
+2026-11-24T20:09:02.0373235Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:09:02.0374784Z env:
+2026-11-24T20:09:02.0375640Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:02.0376681Z   toolchain: stable
+2026-11-24T20:09:02.0377538Z   targets: 
+2026-11-24T20:09:02.0378383Z   components: rustfmt, clippy
+2026-11-24T20:09:02.0379424Z   RUSTUP_PERMIT_COPY_RENAME: 1
+2026-11-24T20:09:02.0380409Z ##[endgroup]
+2026-11-24T20:09:02.2313103Z info: syncing channel updates for stable-x86_64-unknown-linux-gnu
+2026-11-24T20:09:02.3246052Z info: latest update on 2026-11-12 for version 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-24T20:09:02.3393971Z info: component clippy is up to date
+2026-11-24T20:09:02.3396420Z info: component rustfmt is up to date
+2026-11-24T20:09:02.3402647Z 
+2026-11-24T20:09:02.3490310Z   stable-x86_64-unknown-linux-gnu unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-24T20:09:02.3492270Z 
+2026-11-24T20:09:02.3552437Z ##[end-action id=__dtolnay_rust-toolchain.__run_4;outcome=success;conclusion=success;duration_ms=328]
+2026-11-24T20:09:02.3563566Z ##[start-action display=rustup default stable;id=__dtolnay_rust-toolchain.__run_5]
+2026-11-24T20:09:02.3605716Z ##[group]Run rustup default "$toolchain" --force-non-host
+2026-11-24T20:09:02.3607197Z [36;1mrustup default "$toolchain" --force-non-host[0m
+2026-11-24T20:09:02.3646525Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:09:02.3674243Z env:
+2026-11-24T20:09:02.3675758Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:02.3677456Z   toolchain: stable
+2026-11-24T20:09:02.3678887Z ##[endgroup]
+2026-11-24T20:09:02.3800027Z info: using existing install for stable-x86_64-unknown-linux-gnu
+2026-11-24T20:09:02.3810232Z info: default toolchain set to stable-x86_64-unknown-linux-gnu
+2026-11-24T20:09:02.3811674Z 
+2026-11-24T20:09:02.3894417Z   stable-x86_64-unknown-linux-gnu unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-24T20:09:02.3896832Z 
+2026-11-24T20:09:02.3919415Z ##[end-action id=__dtolnay_rust-toolchain.__run_5;outcome=success;conclusion=success;duration_ms=35]
+2026-11-24T20:09:02.3927824Z ##[start-action display=Create cachekey;id=__dtolnay_rust-toolchain.rustc-version]
+2026-11-24T20:09:02.3969921Z ##[group]Run DATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')
+2026-11-24T20:09:02.3972999Z [36;1mDATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')[0m
+2026-11-24T20:09:02.3975729Z [36;1mHASH=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-hash: //p')[0m
+2026-11-24T20:09:02.3977688Z [36;1mecho "cachekey=$(echo $DATE$HASH | head -c12)" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:09:02.4016387Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:09:02.4017680Z env:
+2026-11-24T20:09:02.4018519Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:02.4019546Z   toolchain: stable
+2026-11-24T20:09:02.4020398Z ##[endgroup]
+2026-11-24T20:09:02.4415951Z ##[end-action id=__dtolnay_rust-toolchain.rustc-version;outcome=success;conclusion=success;duration_ms=48]
+2026-11-24T20:09:02.4423124Z ##[start-action display=Disable incremental compilation;id=__dtolnay_rust-toolchain.__run_6]
+2026-11-24T20:09:02.4466215Z ##[group]Run if [ -z "${CARGO_INCREMENTAL+set}" ]; then
+2026-11-24T20:09:02.4467602Z [36;1mif [ -z "${CARGO_INCREMENTAL+set}" ]; then[0m
+2026-11-24T20:09:02.4468913Z [36;1m  echo CARGO_INCREMENTAL=0 >> $GITHUB_ENV[0m
+2026-11-24T20:09:02.4470085Z [36;1mfi[0m
+2026-11-24T20:09:02.4508904Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:09:02.4510206Z env:
+2026-11-24T20:09:02.4511037Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:02.4512062Z ##[endgroup]
+2026-11-24T20:09:02.4596458Z ##[end-action id=__dtolnay_rust-toolchain.__run_6;outcome=success;conclusion=success;duration_ms=17]
+2026-11-24T20:09:02.4602909Z ##[start-action display=Enable colors in Cargo output;id=__dtolnay_rust-toolchain.__run_7]
+2026-11-24T20:09:02.4643502Z ##[group]Run if [ -z "${CARGO_TERM_COLOR+set}" ]; then
+2026-11-24T20:09:02.4645090Z [36;1mif [ -z "${CARGO_TERM_COLOR+set}" ]; then[0m
+2026-11-24T20:09:02.4646438Z [36;1m  echo CARGO_TERM_COLOR=always >> $GITHUB_ENV[0m
+2026-11-24T20:09:02.4647651Z [36;1mfi[0m
+2026-11-24T20:09:02.4684366Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:09:02.4685880Z env:
+2026-11-24T20:09:02.4686722Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:02.4687755Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:09:02.4688647Z ##[endgroup]
+2026-11-24T20:09:02.4768601Z ##[end-action id=__dtolnay_rust-toolchain.__run_7;outcome=success;conclusion=success;duration_ms=16]
+2026-11-24T20:09:02.4776399Z ##[start-action display=Enable Cargo sparse registry;id=__dtolnay_rust-toolchain.__run_8]
+2026-11-24T20:09:02.4818114Z ##[group]Run # implemented in 1.66, stabilized in 1.68, made default in 1.70
+2026-11-24T20:09:02.4820146Z [36;1m# implemented in 1.66, stabilized in 1.68, made default in 1.70[0m
+2026-11-24T20:09:02.4822722Z [36;1mif [ -z "${CARGO_REGISTRIES_CRATES_IO_PROTOCOL+set}" -o -f "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol ]; then[0m
+2026-11-24T20:09:02.4825585Z [36;1m  if rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[89]\.'; then[0m
+2026-11-24T20:09:02.4827808Z [36;1m    touch "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
+2026-11-24T20:09:02.4829874Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse >> $GITHUB_ENV[0m
+2026-11-24T20:09:02.4831868Z [36;1m  elif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[67]\.'; then[0m
+2026-11-24T20:09:02.4834113Z [36;1m    touch "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
+2026-11-24T20:09:02.4836480Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git >> $GITHUB_ENV[0m
+2026-11-24T20:09:02.4837932Z [36;1m  fi[0m
+2026-11-24T20:09:02.4838742Z [36;1mfi[0m
+2026-11-24T20:09:02.4873824Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:09:02.4875259Z env:
+2026-11-24T20:09:02.4876093Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:02.4877123Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:09:02.4878046Z   CARGO_TERM_COLOR: always
+2026-11-24T20:09:02.4879002Z   toolchain: stable
+2026-11-24T20:09:02.4879860Z ##[endgroup]
+2026-11-24T20:09:02.5227951Z ##[end-action id=__dtolnay_rust-toolchain.__run_8;outcome=success;conclusion=success;duration_ms=44]
+2026-11-24T20:09:02.5236988Z ##[start-action display=Work around spurious network errors in curl 8.0;id=__dtolnay_rust-toolchain.__run_9]
+2026-11-24T20:09:02.5279870Z ##[group]Run # https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation
+2026-11-24T20:09:02.5282504Z [36;1m# https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation[0m
+2026-11-24T20:09:02.5285085Z [36;1mif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.7[01]\.'; then[0m
+2026-11-24T20:09:02.5286896Z [36;1m  echo CARGO_HTTP_MULTIPLEXING=false >> $GITHUB_ENV[0m
+2026-11-24T20:09:02.5288139Z [36;1mfi[0m
+2026-11-24T20:09:02.5327256Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:09:02.5328516Z env:
+2026-11-24T20:09:02.5329393Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:02.5330403Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:09:02.5331299Z   CARGO_TERM_COLOR: always
+2026-11-24T20:09:02.5332204Z   toolchain: stable
+2026-11-24T20:09:02.5333030Z ##[endgroup]
+2026-11-24T20:09:02.5561980Z ##[end-action id=__dtolnay_rust-toolchain.__run_9;outcome=success;conclusion=success;duration_ms=32]
+2026-11-24T20:09:02.5572379Z ##[start-action display=rustc +stable --version --verbose;id=__dtolnay_rust-toolchain.__run_10]
+2026-11-24T20:09:02.5613985Z ##[group]Run rustc +"$toolchain" --version --verbose
+2026-11-24T20:09:02.5615889Z [36;1mrustc +"$toolchain" --version --verbose[0m
+2026-11-24T20:09:02.5655288Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:09:02.5656556Z env:
+2026-11-24T20:09:02.5657384Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:02.5658410Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:09:02.5659315Z   CARGO_TERM_COLOR: always
+2026-11-24T20:09:02.5660233Z   toolchain: stable
+2026-11-24T20:09:02.5661076Z ##[endgroup]
+2026-11-24T20:09:02.5847271Z rustc 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-24T20:09:02.5849398Z binary: rustc
+2026-11-24T20:09:02.5851520Z commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a
+2026-11-24T20:09:02.5853668Z commit-date: 2026-11-09
+2026-11-24T20:09:02.5855499Z host: x86_64-unknown-linux-gnu
+2026-11-24T20:09:02.5857212Z release: 1.100.0
+2026-11-24T20:09:02.5858712Z LLVM version: 23.1.5
+2026-11-24T20:09:02.5881370Z ##[end-action id=__dtolnay_rust-toolchain.__run_10;outcome=success;conclusion=success;duration_ms=30]
+2026-11-24T20:09:02.6099273Z ##[group]Run Swatinem/rust-cache@v2
+2026-11-24T20:09:02.6100614Z with:
+2026-11-24T20:09:02.6101398Z   prefix-key: v0-rust
+2026-11-24T20:09:02.6102280Z   add-job-id-key: true
+2026-11-24T20:09:02.6103226Z   add-rust-environment-hash-key: true
+2026-11-24T20:09:02.6104289Z   cache-targets: true
+2026-11-24T20:09:02.6105482Z   cache-all-crates: false
+2026-11-24T20:09:02.6106456Z   cache-workspace-crates: false
+2026-11-24T20:09:02.6107425Z   save-if: true
+2026-11-24T20:09:02.6108266Z   cache-provider: github
+2026-11-24T20:09:02.6109168Z   cache-bin: true
+2026-11-24T20:09:02.6109987Z   lookup-only: false
+2026-11-24T20:09:02.6110834Z   cmd-format: {0}
+2026-11-24T20:09:02.6111632Z env:
+2026-11-24T20:09:02.6112426Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:02.6113426Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:09:02.6114312Z   CARGO_TERM_COLOR: always
+2026-11-24T20:09:02.6115489Z ##[endgroup]
+2026-11-24T20:09:03.0504720Z ##[group]Cache Configuration
+2026-11-24T20:09:03.0507633Z Cache Provider:
+2026-11-24T20:09:03.0509759Z     github
+2026-11-24T20:09:03.0511923Z Workspaces:
+2026-11-24T20:09:03.0514826Z     /home/runner/work/MultiTunnel/MultiTunnel
+2026-11-24T20:09:03.0517348Z Cache Paths:
+2026-11-24T20:09:03.0519749Z     /home/runner/.cargo/bin
+2026-11-24T20:09:03.0522303Z     /home/runner/.cargo/.crates.toml
+2026-11-24T20:09:03.0525194Z     /home/runner/.cargo/.crates2.json
+2026-11-24T20:09:03.0528148Z     /home/runner/.cargo/registry
+2026-11-24T20:09:03.0530807Z     /home/runner/.cargo/git
+2026-11-24T20:09:03.0533519Z     /home/runner/work/MultiTunnel/MultiTunnel/target
+2026-11-24T20:09:03.0536365Z Restore Key:
+2026-11-24T20:09:03.0538453Z     v0-rust-build-linux-daemon-Linux-x64-2fd77104
+2026-11-24T20:09:03.0541024Z Cache Key:
+2026-11-24T20:09:03.0543088Z     v0-rust-build-linux-daemon-Linux-x64-2fd77104-39107a77
+2026-11-24T20:09:03.0545811Z .. Prefix:
+2026-11-24T20:09:03.0547455Z   - v0-rust-build-linux-daemon-Linux-x64
+2026-11-24T20:09:03.0549496Z .. Environment considered:
+2026-11-24T20:09:03.0551335Z   - Rust Versions:
+2026-11-24T20:09:03.0553619Z     - 1.100.0 x86_64-unknown-linux-gnu 0ef1bc386059596dada8fb65b50425aab2ea637a
+2026-11-24T20:09:03.0556520Z   - CARGO_HOME
+2026-11-24T20:09:03.0558508Z   - CARGO_INCREMENTAL
+2026-11-24T20:09:03.0638794Z   - CARGO_TERM_COLOR
+2026-11-24T20:09:03.0640600Z .. Lockfiles considered:
+2026-11-24T20:09:03.0642672Z   - /home/runner/work/MultiTunnel/MultiTunnel/Cargo.lock
+2026-11-24T20:09:03.0645555Z   - /home/runner/work/MultiTunnel/MultiTunnel/Cargo.toml
+2026-11-24T20:09:03.0648459Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-app/Cargo.toml
+2026-11-24T20:09:03.0651701Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond/Cargo.toml
+2026-11-24T20:09:03.0655129Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-engine/Cargo.toml
+2026-11-24T20:09:03.0658441Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-links/Cargo.toml
+2026-11-24T20:09:03.0661548Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-m3/Cargo.toml
+2026-11-24T20:09:03.0664792Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net/Cargo.toml
+2026-11-24T20:09:03.0667987Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-platform/Cargo.toml
+2026-11-24T20:09:03.0672305Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-service/Cargo.toml
+2026-11-24T20:09:03.0676043Z ##[endgroup]
+2026-11-24T20:09:03.0676794Z 
+2026-11-24T20:09:03.0677522Z ... Restoring cache ...
+2026-11-24T20:09:03.3061004Z No cache found.
+2026-11-24T20:09:03.3203938Z ##[group]Run cargo fmt --all --check
+2026-11-24T20:09:03.3205302Z [36;1mcargo fmt --all --check[0m
+2026-11-24T20:09:03.3245610Z shell: /usr/bin/bash -e {0}
+2026-11-24T20:09:03.3246549Z env:
+2026-11-24T20:09:03.3247344Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:03.3248343Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:09:03.3249232Z   CARGO_TERM_COLOR: always
+2026-11-24T20:09:03.3250157Z   CACHE_ON_FAILURE: false
+2026-11-24T20:09:03.3251053Z ##[endgroup]
+2026-11-24T20:09:04.1474369Z ##[group]Run cargo clippy -p mt-bond --all-targets -- -D warnings
+2026-11-24T20:09:04.1475406Z [36;1mcargo clippy -p mt-bond --all-targets -- -D warnings[0m
+2026-11-24T20:09:04.1507893Z shell: /usr/bin/bash -e {0}
+2026-11-24T20:09:04.1508177Z env:
+2026-11-24T20:09:04.1508402Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:04.1508704Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:09:04.1508956Z   CARGO_TERM_COLOR: always
+2026-11-24T20:09:04.1509217Z   CACHE_ON_FAILURE: false
+2026-11-24T20:09:04.1509462Z ##[endgroup]
+2026-11-24T20:09:04.4199487Z [1m[92m    Updating[0m crates.io index
+2026-11-24T20:09:05.0420797Z [1m[92m Downloading[0m crates ...
+2026-11-24T20:09:05.0949849Z [1m[92m  Downloaded[0m futures v0.3.34
+2026-11-24T20:09:05.1051390Z [1m[92m  Downloaded[0m signal-hook-registry v1.4.8
+2026-11-24T20:09:05.1069684Z [1m[92m  Downloaded[0m thiserror-impl v2.0.21
+2026-11-24T20:09:05.1086486Z [1m[92m  Downloaded[0m tokio-macros v2.7.2
+2026-11-24T20:09:05.1101083Z [1m[92m  Downloaded[0m futures-core v0.3.34
+2026-11-24T20:09:05.1122824Z [1m[92m  Downloaded[0m futures-macro v0.3.34
+2026-11-24T20:09:05.1136132Z [1m[92m  Downloaded[0m bytes v1.12.1
+2026-11-24T20:09:05.1186982Z [1m[92m  Downloaded[0m futures-io v0.3.34
+2026-11-24T20:09:05.1205935Z [1m[92m  Downloaded[0m futures-task v0.3.34
+2026-11-24T20:09:05.1215990Z [1m[92m  Downloaded[0m tracing-attributes v0.1.31
+2026-11-24T20:09:05.1253890Z [1m[92m  Downloaded[0m anyhow v1.0.104
+2026-11-24T20:09:05.1319569Z [1m[92m  Downloaded[0m errno v0.3.15
+2026-11-24T20:09:05.1338364Z [1m[92m  Downloaded[0m quote v1.0.47
+2026-11-24T20:09:05.1375541Z [1m[92m  Downloaded[0m async-trait v0.1.92
+2026-11-24T20:09:05.1411246Z [1m[92m  Downloaded[0m pin-project-lite v0.2.17
+2026-11-24T20:09:05.1466974Z [1m[92m  Downloaded[0m futures-channel v0.3.34
+2026-11-24T20:09:05.1485467Z [1m[92m  Downloaded[0m once_cell v1.21.4
+2026-11-24T20:09:05.1509357Z [1m[92m  Downloaded[0m serde_derive v1.0.229
+2026-11-24T20:09:05.1538960Z [1m[92m  Downloaded[0m tracing-core v0.1.36
+2026-11-24T20:09:05.1564687Z [1m[92m  Downloaded[0m serde_core v1.0.229
+2026-11-24T20:09:05.1591168Z [1m[92m  Downloaded[0m serde v1.0.229
+2026-11-24T20:09:05.1625060Z [1m[92m  Downloaded[0m proc-macro2 v1.0.107
+2026-11-24T20:09:05.1660108Z [1m[92m  Downloaded[0m slab v0.4.12
+2026-11-24T20:09:05.1673370Z [1m[92m  Downloaded[0m syn v3.0.10
+2026-11-24T20:09:05.1776488Z [1m[92m  Downloaded[0m socket2 v0.6.5
+2026-11-24T20:09:05.1794782Z [1m[92m  Downloaded[0m futures-executor v0.3.34
+2026-11-24T20:09:05.1808291Z [1m[92m  Downloaded[0m mio v1.2.5
+2026-11-24T20:09:05.1869070Z [1m[92m  Downloaded[0m futures-util v0.3.34
+2026-11-24T20:09:05.1996443Z [1m[92m  Downloaded[0m memchr v2.8.3
+2026-11-24T20:09:05.2049887Z [1m[92m  Downloaded[0m futures-sink v0.3.34
+2026-11-24T20:09:05.2058553Z [1m[92m  Downloaded[0m syn v2.0.119
+2026-11-24T20:09:05.2160068Z [1m[92m  Downloaded[0m ipnet v2.12.2
+2026-11-24T20:09:05.2176083Z [1m[92m  Downloaded[0m thiserror v2.0.21
+2026-11-24T20:09:05.2237425Z [1m[92m  Downloaded[0m tracing v0.1.44
+2026-11-24T20:09:05.2389977Z [1m[92m  Downloaded[0m unicode-ident v1.0.27
+2026-11-24T20:09:05.2416330Z [1m[92m  Downloaded[0m tokio v1.54.1
+2026-11-24T20:09:05.2866433Z [1m[92m  Downloaded[0m libc v0.2.191
+2026-11-24T20:09:05.3585629Z [1m[92m   Compiling[0m proc-macro2 v1.0.107
+2026-11-24T20:09:05.3586616Z [1m[92m   Compiling[0m unicode-ident v1.0.27
+2026-11-24T20:09:05.3586988Z [1m[92m   Compiling[0m quote v1.0.47
+2026-11-24T20:09:05.3587316Z [1m[92m   Compiling[0m libc v0.2.191
+2026-11-24T20:09:05.6296812Z [1m[92m    Checking[0m pin-project-lite v0.2.17
+2026-11-24T20:09:05.6714720Z [1m[92m    Checking[0m futures-sink v0.3.34
+2026-11-24T20:09:05.7130425Z [1m[92m    Checking[0m futures-core v0.3.34
+2026-11-24T20:09:05.7720540Z [1m[92m    Checking[0m futures-channel v0.3.34
+2026-11-24T20:09:05.8836341Z [1m[92m    Checking[0m slab v0.4.12
+2026-11-24T20:09:05.9306922Z [1m[92m    Checking[0m futures-task v0.3.34
+2026-11-24T20:09:05.9815900Z [1m[92m    Checking[0m futures-io v0.3.34
+2026-11-24T20:09:06.0275980Z [1m[92m    Checking[0m memchr v2.8.3
+2026-11-24T20:09:06.0426057Z [1m[92m   Compiling[0m serde_core v1.0.229
+2026-11-24T20:09:06.2576054Z [1m[92m   Compiling[0m serde v1.0.229
+2026-11-24T20:09:06.4293048Z [1m[92m    Checking[0m once_cell v1.21.4
+2026-11-24T20:09:06.4446180Z [1m[92m   Compiling[0m thiserror v2.0.21
+2026-11-24T20:09:06.6628915Z [1m[92m   Compiling[0m anyhow v1.0.104
+2026-11-24T20:09:06.7226178Z [1m[92m   Compiling[0m syn v3.0.10
+2026-11-24T20:09:06.7539455Z [1m[92m   Compiling[0m syn v2.0.119
+2026-11-24T20:09:06.9175929Z [1m[92m    Checking[0m tracing-core v0.1.36
+2026-11-24T20:09:07.0576607Z [1m[92m    Checking[0m errno v0.3.15
+2026-11-24T20:09:07.1506316Z [1m[92m    Checking[0m signal-hook-registry v1.4.8
+2026-11-24T20:09:07.2215903Z [1m[92m    Checking[0m mio v1.2.5
+2026-11-24T20:09:07.2580545Z [1m[92m    Checking[0m socket2 v0.6.5
+2026-11-24T20:09:07.5315982Z [1m[92m    Checking[0m bytes v1.12.1
+2026-11-24T20:09:08.1125932Z [1m[92m    Checking[0m ipnet v2.12.2
+2026-11-24T20:09:10.0375344Z [1m[92m   Compiling[0m futures-macro v0.3.34
+2026-11-24T20:09:10.0475977Z [1m[92m   Compiling[0m serde_derive v1.0.229
+2026-11-24T20:09:10.1578331Z [1m[92m   Compiling[0m thiserror-impl v2.0.21
+2026-11-24T20:09:10.4068942Z [1m[92m   Compiling[0m tracing-attributes v0.1.31
+2026-11-24T20:09:10.6985937Z [1m[92m    Checking[0m futures-util v0.3.34
+2026-11-24T20:09:11.2909737Z [1m[92m   Compiling[0m tokio-macros v2.7.2
+2026-11-24T20:09:11.4496058Z [1m[92m    Checking[0m tracing v0.1.44
+2026-11-24T20:09:11.6477982Z [1m[92m   Compiling[0m async-trait v0.1.92
+2026-11-24T20:09:11.8751186Z [1m[92m    Checking[0m tokio v1.54.1
+2026-11-24T20:09:13.1396206Z [1m[92m    Checking[0m futures-executor v0.3.34
+2026-11-24T20:09:13.2229599Z [1m[92m    Checking[0m futures v0.3.34
+2026-11-24T20:09:14.7440419Z [1m[92m    Checking[0m mt-net v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net)
+2026-11-24T20:09:15.8142721Z [1m[92m    Checking[0m mt-bond v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond)
+2026-11-24T20:09:16.5977534Z [1m[92m    Finished[0m `dev` profile [unoptimized + debuginfo] target(s) in 12.39s
+2026-11-24T20:09:16.6833256Z ##[group]Run cargo test -p mt-bond
+2026-11-24T20:09:16.6833618Z [36;1mcargo test -p mt-bond[0m
+2026-11-24T20:09:16.6865979Z shell: /usr/bin/bash -e {0}
+2026-11-24T20:09:16.6866246Z env:
+2026-11-24T20:09:16.6866460Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:09:16.6866746Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:09:16.6866989Z   CARGO_TERM_COLOR: always
+2026-11-24T20:09:16.6867242Z   CACHE_ON_FAILURE: false
+2026-11-24T20:09:16.6867477Z ##[endgroup]
+2026-11-24T20:09:16.8166140Z [1m[92m   Compiling[0m pin-project-lite v0.2.17
+2026-11-24T20:09:16.8167016Z [1m[92m   Compiling[0m futures-sink v0.3.34
+2026-11-24T20:09:16.8167554Z [1m[92m   Compiling[0m libc v0.2.191
+2026-11-24T20:09:16.8168064Z [1m[92m   Compiling[0m futures-core v0.3.34
+2026-11-24T20:09:16.8616860Z [1m[92m   Compiling[0m memchr v2.8.3
+2026-11-24T20:09:16.8730412Z [1m[92m   Compiling[0m futures-channel v0.3.34
+2026-11-24T20:09:16.9035996Z [1m[92m   Compiling[0m slab v0.4.12
+2026-11-24T20:09:17.0352571Z [1m[92m   Compiling[0m futures-task v0.3.34
+2026-11-24T20:09:17.0515601Z [1m[92m   Compiling[0m futures-io v0.3.34
+2026-11-24T20:09:17.1486377Z [1m[92m   Compiling[0m once_cell v1.21.4
+2026-11-24T20:09:17.1736314Z [1m[92m   Compiling[0m serde_core v1.0.229
+2026-11-24T20:09:17.3121390Z [1m[92m   Compiling[0m tracing-core v0.1.36
+2026-11-24T20:09:17.5846701Z [1m[92m   Compiling[0m futures-util v0.3.34
+2026-11-24T20:09:17.9552679Z [1m[92m   Compiling[0m bytes v1.12.1
+2026-11-24T20:09:18.0576488Z [1m[92m   Compiling[0m errno v0.3.15
+2026-11-24T20:09:18.1466415Z [1m[92m   Compiling[0m signal-hook-registry v1.4.8
+2026-11-24T20:09:18.4015859Z [1m[92m   Compiling[0m socket2 v0.6.5
+2026-11-24T20:09:18.6357696Z [1m[92m   Compiling[0m mio v1.2.5
+2026-11-24T20:09:18.9355795Z [1m[92m   Compiling[0m thiserror v2.0.21
+2026-11-24T20:09:18.9895886Z [1m[92m   Compiling[0m tokio v1.54.1
+2026-11-24T20:09:19.1736144Z [1m[92m   Compiling[0m tracing v0.1.44
+2026-11-24T20:09:19.3466061Z [1m[92m   Compiling[0m anyhow v1.0.104
+2026-11-24T20:09:19.6516463Z [1m[92m   Compiling[0m serde v1.0.229
+2026-11-24T20:09:19.6785815Z [1m[92m   Compiling[0m ipnet v2.12.2
+2026-11-24T20:09:20.3058768Z [1m[92m   Compiling[0m futures-executor v0.3.34
+2026-11-24T20:09:20.3939955Z [1m[92m   Compiling[0m futures v0.3.34
+2026-11-24T20:09:22.3696055Z [1m[92m   Compiling[0m mt-net v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net)
+2026-11-24T20:09:23.2475437Z [1m[92m   Compiling[0m mt-bond v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond)
+2026-11-24T20:09:26.1377441Z [1m[92m    Finished[0m `test` profile [unoptimized + debuginfo] target(s) in 9.43s
+2026-11-24T20:09:26.1451912Z [1m[92m     Running[0m unittests src/lib.rs (target/debug/build/mt-bond/a79068680de7bb57/out/mt_bond-a79068680de7bb57)
+2026-11-24T20:09:26.1465366Z 
+2026-11-24T20:09:26.1465575Z running 33 tests
+2026-11-24T20:09:26.1476825Z test endpoint::scheduling_tests::balanced_blest_guard_never_bypasses_the_score_leader ... ok
+2026-11-24T20:09:26.1478660Z test endpoint::scheduling_tests::balanced_capacity_aware_bypasses_the_saturated_adapter ... ok
+2026-11-24T20:09:26.1480483Z test endpoint::scheduling_tests::balanced_blest_guard_skips_slow_draining_tied_carrier ... ok
+2026-11-24T20:09:26.1482137Z test endpoint::scheduling_tests::balanced_picks_the_least_backlogged_carrier ... ok
+2026-11-24T20:09:26.1483733Z test endpoint::scheduling_tests::balanced_spreads_proportionally_to_reported_capacity ... ok
+2026-11-24T20:09:26.1485006Z test endpoint::scheduling_tests::balanced_ties_rotate ... ok
+2026-11-24T20:09:26.1486106Z test endpoint::scheduling_tests::carrier_score_is_relative_load_with_probe_floor ... ok
+2026-11-24T20:09:26.1486998Z test endpoint::scheduling_tests::ecf_completion_is_rtt_plus_drain_time ... ok
+2026-11-24T20:09:26.1487683Z test endpoint::scheduling_tests::failover_takes_the_first_live_carrier ... ok
+2026-11-24T20:09:26.1488701Z test endpoint::scheduling_tests::lowest_latency_ecf_prefers_earliest_completion ... ok
+2026-11-24T20:09:26.1489474Z test endpoint::scheduling_tests::lowest_latency_prefers_sampled_rtt_then_known_over_unknown ... ok
+2026-11-24T20:09:26.1490191Z test endpoint::scheduling_tests::mixed_reports_probe_the_unreported_carrier ... ok
+2026-11-24T20:09:26.1490872Z test endpoint::scheduling_tests::priority_fills_in_order_until_the_queue_backs_up ... ok
+2026-11-24T20:09:26.1491575Z test endpoint::scheduling_tests::priority_spills_when_relative_load_exceeds_ratio ... ok
+2026-11-24T20:09:26.1492216Z test endpoint::scheduling_tests::round_robin_cycles_the_cursor ... ok
+2026-11-24T20:09:26.1492774Z test frame::tests::encode_session_borrowed_is_wire_identical ... ok
+2026-11-24T20:09:26.1493334Z test frame::tests::header_rejects_bad_magic_version_kind_and_size ... ok
+2026-11-24T20:09:26.1493880Z test frame::tests::empty_and_max_payloads_roundtrip ... ok
+2026-11-24T20:09:26.1494393Z test frame::tests::roundtrip_all_kinds ... ok
+2026-11-24T20:09:26.1495316Z test frame::tests::target_rejects_truncated_and_garbage ... ok
+2026-11-24T20:09:26.1495997Z test frame::tests::read_write_frame_over_duplex ... ok
+2026-11-24T20:09:26.1496446Z test frame::tests::target_roundtrip_domain_ipv4_ipv6 ... ok
+2026-11-24T20:09:26.1496831Z test reorder::tests::cap_zero_still_delivers_in_order ... ok
+2026-11-24T20:09:26.1497259Z test outbound::tests::ensure_carriers_fails_cleanly_with_no_links ... ok
+2026-11-24T20:09:26.1497717Z test reorder::tests::duplicates_and_replays_are_idempotent ... ok
+2026-11-24T20:09:26.1498145Z test outbound::tests::ensure_carriers_reports_all_dead_links ... ok
+2026-11-24T20:09:26.1498540Z test reorder::tests::gap_buffers_then_flushes ... ok
+2026-11-24T20:09:26.1498896Z test reorder::tests::in_order_delivery ... ok
+2026-11-24T20:09:26.1499265Z test reorder::tests::long_out_of_order_run_reorders_completely ... ok
+2026-11-24T20:09:26.1499879Z test reorder::tests::overflow_is_an_error ... ok
+2026-11-24T20:09:26.1500260Z test reorder::tests::would_overflow_mirrors_the_insert_boundary ... ok
+2026-11-24T20:09:26.1500646Z test tests::constants_are_coherent ... ok
+2026-11-24T20:09:26.3491361Z test daemon::tests::serve_binds_and_shuts_down ... ok
+2026-11-24T20:09:26.3491883Z 
+2026-11-24T20:09:26.3492405Z test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
+2026-11-24T20:09:26.3493160Z 
+2026-11-24T20:09:26.3498383Z [1m[92m     Running[0m unittests src/bin/mt_merge_server.rs (target/debug/build/mt-bond/9915eef574e68264/out/mt_merge_server-9915eef574e68264)
+2026-11-24T20:09:26.3511273Z 
+2026-11-24T20:09:26.3511508Z running 0 tests
+2026-11-24T20:09:26.3511853Z 
+2026-11-24T20:09:26.3512506Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
+2026-11-24T20:09:26.3513211Z 
+2026-11-24T20:09:26.3515021Z [1m[92m     Running[0m tests/integration.rs (target/debug/build/mt-bond/3d8a2918f951a063/out/integration-3d8a2918f951a063)
+2026-11-24T20:09:26.3526299Z 
+2026-11-24T20:09:26.3526541Z running 6 tests
+2026-11-24T20:09:27.4199444Z test echo_roundtrip_1mib_across_two_carriers ... ok
+2026-11-24T20:09:29.4418224Z test domain_targets_dial_through_the_daemon ... ok
+2026-11-24T20:09:29.4817493Z test concurrent_sessions_multiplex_over_the_same_carriers ... ok
+2026-11-24T20:09:29.5169266Z test refused_target_surfaces_as_connection_reset ... ok
+2026-11-24T20:09:30.5467404Z test one_dead_link_still_yields_a_working_bond ... ok
+2026-11-24T20:09:31.6307616Z test carrier_death_mid_stream_fails_over_without_data_loss ... FAILED
+2026-11-24T20:09:31.6308662Z 
+2026-11-24T20:09:31.6308957Z failures:
+2026-11-24T20:09:31.6309288Z 
+2026-11-24T20:09:31.6309847Z ---- carrier_death_mid_stream_fails_over_without_data_loss stdout ----
+2026-11-24T20:09:31.6310998Z [mt-merge-server] listening on 127.0.0.1:45149
+2026-11-24T20:09:31.6311626Z 
+2026-11-24T20:09:31.6312740Z thread 'carrier_death_mid_stream_fails_over_without_data_loss' (3542) panicked at crates/mt-bond/tests/integration.rs:304:14:
+2026-11-24T20:09:31.6315688Z second half: Custom { kind: ConnectionReset, error: "bond send failed: no live bond carrier" }
+2026-11-24T20:09:31.6317404Z note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
+2026-11-24T20:09:31.6318316Z 
+2026-11-24T20:09:31.6318326Z 
+2026-11-24T20:09:31.6318579Z failures:
+2026-11-24T20:09:31.6319273Z     carrier_death_mid_stream_fails_over_without_data_loss
+2026-11-24T20:09:31.6319839Z 
+2026-11-24T20:09:31.6320349Z test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.28s
+2026-11-24T20:09:31.6321078Z 
+2026-11-24T20:09:31.6324651Z [1m[91merror[0m: test failed, to rerun pass `-p mt-bond --test integration`
+2026-11-24T20:09:31.6401326Z ##[error]Process completed with exit code 101.
+2026-11-24T20:09:31.6571586Z Post job cleanup.
+2026-11-24T20:09:31.7450136Z [command]/usr/bin/git version
+2026-11-24T20:09:31.7512057Z git version 2.56.1
+2026-11-24T20:09:31.7554957Z Temporarily overriding HOME='/home/runner/work/_temp/d3b4da46-a437-4e58-ab1a-ca98ecca3f9e' before making global git config changes
+2026-11-24T20:09:31.7556453Z Adding repository directory to the temporary git global config as a safe directory
+2026-11-24T20:09:31.7562113Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/MultiTunnel/MultiTunnel
+2026-11-24T20:09:31.7605965Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
+2026-11-24T20:09:31.7646163Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
+2026-11-24T20:09:31.7965867Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
+2026-11-24T20:09:31.7994975Z http.https://github.com/.extraheader
+2026-11-24T20:09:31.8007863Z [command]/usr/bin/git config --local --unset-all http.https://github.com/.extraheader
+2026-11-24T20:09:31.8057730Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
+2026-11-24T20:09:31.8386796Z [command]/usr/bin/git config --local --name-only --get-regexp ^includeIf\.gitdir:
+2026-11-24T20:09:31.8435711Z [command]/usr/bin/git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url
+2026-11-24T20:09:31.8947033Z Cleaning up orphan processes
+2026-11-24T20:09:31.9349207Z ##[warning]Node.js 20 is deprecated. The following actions target Node.js 20 but are being forced to run on Node.js 24: actions/checkout@v4. For more information see: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/
diff --git a/harness/memory/agent-reports/s21-ci-domain-targets-release-run2.log b/harness/memory/agent-reports/s21-ci-domain-targets-release-run2.log
new file mode 100644
index 0000000..579fcaf
--- /dev/null
+++ b/harness/memory/agent-reports/s21-ci-domain-targets-release-run2.log
@@ -0,0 +1,591 @@
+﻿2026-11-24T20:27:03.9790698Z Current runner version: '2.338.2'
+2026-11-24T20:27:03.9812772Z ##[group]Runner Image Provisioner
+2026-11-24T20:27:03.9813661Z Hosted Compute Agent
+2026-11-24T20:27:03.9814101Z Version: 20261111.635
+2026-11-24T20:27:03.9814631Z Commit: b8a594ea514ff44cadb5342986c68b1eac6e22ed
+2026-11-24T20:27:03.9815216Z Build Date: 2026-11-11T01:49:01Z
+2026-11-24T20:27:03.9815719Z Worker ID: {12b39b16-eba8-4485-ad1f-90f29aae1e1f}
+2026-11-24T20:27:03.9816253Z Region: centralus
+2026-11-24T20:27:03.9816666Z Cloud: Azure
+2026-11-24T20:27:03.9817067Z ##[endgroup]
+2026-11-24T20:27:03.9818161Z ##[group]Operating System
+2026-11-24T20:27:03.9818615Z Microsoft Windows Server 2025
+2026-11-24T20:27:03.9819094Z 10.0.26100
+2026-11-24T20:27:03.9819484Z Datacenter
+2026-11-24T20:27:03.9819868Z ##[endgroup]
+2026-11-24T20:27:03.9820310Z ##[group]Runner Image
+2026-11-24T20:27:03.9820733Z Image: windows-2025-vs2026
+2026-11-24T20:27:03.9821198Z Version: 20261116.315.1
+2026-11-24T20:27:03.9822346Z Included Software: https://github.com/actions/runner-images/blob/win25-vs2026/20261116.315/images/windows/Windows2025-VS2026-Readme.md
+2026-11-24T20:27:03.9823698Z Image Release: https://github.com/actions/runner-images/releases/tag/win25-vs2026%2F20261116.315
+2026-11-24T20:27:03.9824437Z ##[endgroup]
+2026-11-24T20:27:03.9825301Z ##[group]GITHUB_TOKEN Permissions
+2026-11-24T20:27:03.9826790Z Contents: write
+2026-11-24T20:27:03.9827262Z Metadata: read
+2026-11-24T20:27:03.9827668Z ##[endgroup]
+2026-11-24T20:27:03.9829799Z Secret source: Actions
+2026-11-24T20:27:03.9830511Z Cache mode: write
+2026-11-24T20:27:03.9831054Z Prepare workflow directory
+2026-11-24T20:27:04.0176609Z Prepare all required actions
+2026-11-24T20:27:04.0216180Z Getting action download info
+2026-11-24T20:27:04.2490281Z Download action repository 'actions/checkout@v4' (SHA:11d5960a326750d5838078e36cf38b85af677262)
+2026-11-24T20:27:04.4910712Z Download action repository 'dtolnay/rust-toolchain@stable' (SHA:39008827e6b4319a349e9a42dfc90579dd25f1ca)
+2026-11-24T20:27:04.6743741Z Download action repository 'Swatinem/rust-cache@v2' (SHA:d1527c8f67ef9af357290475d4af4b0fab8f12e0)
+2026-11-24T20:27:05.1650630Z Download action repository 'actions/upload-artifact@v4' (SHA:ea165f8d65b6e75b540449e92b4886f43607fa02)
+2026-11-24T20:27:05.3360084Z Download action repository 'softprops/action-gh-release@v2' (SHA:3bb12739c298aeb8a4eeaf626c5b8d85266b0e65)
+2026-11-24T20:27:05.7911518Z Complete job name: build-windows
+2026-11-24T20:27:05.9187811Z ##[group]Run actions/checkout@v4
+2026-11-24T20:27:05.9188592Z with:
+2026-11-24T20:27:05.9188899Z   repository: maniesh-NR/MultiTunnel
+2026-11-24T20:27:05.9191377Z   token: ***
+2026-11-24T20:27:05.9191647Z   ssh-strict: true
+2026-11-24T20:27:05.9191925Z   ssh-user: git
+2026-11-24T20:27:05.9192213Z   persist-credentials: true
+2026-11-24T20:27:05.9192519Z   clean: true
+2026-11-24T20:27:05.9192792Z   sparse-checkout-cone-mode: true
+2026-11-24T20:27:05.9193103Z   fetch-depth: 1
+2026-11-24T20:27:05.9193372Z   fetch-tags: false
+2026-11-24T20:27:05.9193672Z   show-progress: true
+2026-11-24T20:27:05.9194264Z   lfs: false
+2026-11-24T20:27:05.9194529Z   submodules: false
+2026-11-24T20:27:05.9194796Z   set-safe-directory: true
+2026-11-24T20:27:05.9195091Z   allow-unsafe-pr-checkout: false
+2026-11-24T20:27:05.9195626Z ##[endgroup]
+2026-11-24T20:27:06.0381862Z Syncing repository: maniesh-NR/MultiTunnel
+2026-11-24T20:27:06.0383813Z ##[group]Getting Git version info
+2026-11-24T20:27:06.0384514Z Working directory is 'D:\a\MultiTunnel\MultiTunnel'
+2026-11-24T20:27:06.1002394Z [command]"C:\Program Files\Git\bin\git.exe" version
+2026-11-24T20:27:06.4246221Z git version 2.56.1.windows.1
+2026-11-24T20:27:06.4288495Z ##[endgroup]
+2026-11-24T20:27:06.4304931Z Temporarily overriding HOME='D:\a\_temp\24fc1558-ebb6-443f-a71c-62afb36e780e' before making global git config changes
+2026-11-24T20:27:06.4305750Z Adding repository directory to the temporary git global config as a safe directory
+2026-11-24T20:27:06.4313445Z [command]"C:\Program Files\Git\bin\git.exe" config --global --add safe.directory D:\a\MultiTunnel\MultiTunnel
+2026-11-24T20:27:06.4892256Z Deleting the contents of 'D:\a\MultiTunnel\MultiTunnel'
+2026-11-24T20:27:06.4897344Z ##[group]Initializing the repository
+2026-11-24T20:27:06.4904340Z [command]"C:\Program Files\Git\bin\git.exe" init D:\a\MultiTunnel\MultiTunnel
+2026-11-24T20:27:06.5658949Z Initialized empty Git repository in D:/a/MultiTunnel/MultiTunnel/.git/
+2026-11-24T20:27:06.5699847Z [command]"C:\Program Files\Git\bin\git.exe" remote add origin https://github.com/maniesh-NR/MultiTunnel
+2026-11-24T20:27:06.6939564Z ##[endgroup]
+2026-11-24T20:27:06.6940074Z ##[group]Disabling automatic garbage collection
+2026-11-24T20:27:06.6948134Z [command]"C:\Program Files\Git\bin\git.exe" config --local gc.auto 0
+2026-11-24T20:27:06.7263166Z ##[endgroup]
+2026-11-24T20:27:06.7263664Z ##[group]Setting up auth
+2026-11-24T20:27:06.7273773Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp core\.sshCommand
+2026-11-24T20:27:06.7588759Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "sh -c \"git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :\""
+2026-11-24T20:27:08.0124739Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
+2026-11-24T20:27:08.0421099Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "sh -c \"git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :\""
+2026-11-24T20:27:08.4870329Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp ^includeIf\.gitdir:
+2026-11-24T20:27:08.5176924Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "git config --local --show-origin --name-only --get-regexp remote.origin.url"
+2026-11-24T20:27:08.9819633Z [command]"C:\Program Files\Git\bin\git.exe" config --local http.https://github.com/.extraheader "AUTHORIZATION: basic ***"
+2026-11-24T20:27:09.0134250Z ##[endgroup]
+2026-11-24T20:27:09.0144912Z ##[group]Fetching the repository
+2026-11-24T20:27:09.0148856Z [command]"C:\Program Files\Git\bin\git.exe" -c protocol.version=2 fetch --no-tags --prune --no-recurse-submodules --depth=1 origin +e510e439249fe4072477c05040bd5b9d0678db6a:refs/tags/v0.14.0
+2026-11-24T20:27:11.2409285Z From https://github.com/maniesh-NR/MultiTunnel
+2026-11-24T20:27:11.2442167Z  * [new ref]         e510e439249fe4072477c05040bd5b9d0678db6a -> v0.14.0
+2026-11-24T20:27:11.2443768Z ##[endgroup]
+2026-11-24T20:27:11.2444209Z ##[group]Determining the checkout info
+2026-11-24T20:27:11.2445819Z ##[endgroup]
+2026-11-24T20:27:11.2454617Z [command]"C:\Program Files\Git\bin\git.exe" sparse-checkout disable
+2026-11-24T20:27:11.2880326Z [command]"C:\Program Files\Git\bin\git.exe" config --local --unset-all extensions.worktreeConfig
+2026-11-24T20:27:11.3189140Z ##[group]Checking out the ref
+2026-11-24T20:27:11.3196664Z [command]"C:\Program Files\Git\bin\git.exe" checkout --progress --force refs/tags/v0.14.0
+2026-11-24T20:27:11.4353574Z Note: switching to 'refs/tags/v0.14.0'.
+2026-11-24T20:27:11.4353996Z 
+2026-11-24T20:27:11.4354210Z You are in 'detached HEAD' state. You can look around, make experimental
+2026-11-24T20:27:11.4354838Z changes and commit them, and you can discard any commits you make in this
+2026-11-24T20:27:11.4355386Z state without impacting any branches by switching back to a branch.
+2026-11-24T20:27:11.4355706Z 
+2026-11-24T20:27:11.4355918Z If you want to create a new branch to retain commits you create, you may
+2026-11-24T20:27:11.4356437Z do so (now or later) by using -c with the switch command. Example:
+2026-11-24T20:27:11.4356747Z 
+2026-11-24T20:27:11.4356882Z   git switch -c <new-branch-name>
+2026-11-24T20:27:11.4357115Z 
+2026-11-24T20:27:11.4357221Z Or undo this operation with:
+2026-11-24T20:27:11.4357451Z 
+2026-11-24T20:27:11.4358570Z   git switch -
+2026-11-24T20:27:11.4358761Z 
+2026-11-24T20:27:11.4359094Z Turn off this advice by setting config variable advice.detachedHead to false
+2026-11-24T20:27:11.4359498Z 
+2026-11-24T20:27:11.4359731Z HEAD is now at e510e43 backup(s21-release-prep-2): 2026-11-24T20:08:15Z
+2026-11-24T20:27:11.4413922Z ##[endgroup]
+2026-11-24T20:27:11.4783798Z [command]"C:\Program Files\Git\bin\git.exe" log -1 --format=%H
+2026-11-24T20:27:11.5055448Z e510e439249fe4072477c05040bd5b9d0678db6a
+2026-11-24T20:27:11.5504985Z ##[group]Run dtolnay/rust-toolchain@stable
+2026-11-24T20:27:11.5505268Z with:
+2026-11-24T20:27:11.5505435Z   components: rustfmt, clippy
+2026-11-24T20:27:11.5505621Z   toolchain: stable
+2026-11-24T20:27:11.5505775Z ##[endgroup]
+2026-11-24T20:27:11.5558247Z ##[start-action display=Parse toolchain version;id=__dtolnay_rust-toolchain.parse]
+2026-11-24T20:27:11.5694646Z ##[group]Run if [[ -z $toolchain ]]; then
+2026-11-24T20:27:11.5694931Z [36;1mif [[ -z $toolchain ]]; then[0m
+2026-11-24T20:27:11.5695368Z [36;1m  # GitHub does not enforce `required: true` inputs itself. https://github.com/actions/runner/issues/1070[0m
+2026-11-24T20:27:11.5695802Z [36;1m  echo "'toolchain' is a required input" >&2[0m
+2026-11-24T20:27:11.5696038Z [36;1m  exit 1[0m
+2026-11-24T20:27:11.5696288Z [36;1melif [[ $toolchain =~ ^stable' '[0-9]+' '(year|month|week|day)s?' 'ago$ ]]; then[0m
+2026-11-24T20:27:11.5696591Z [36;1m  if [[ Windows == macOS ]]; then[0m
+2026-11-24T20:27:11.5697022Z [36;1m    echo "toolchain=1.$((($(date -v-$(sed 's/stable \([0-9]*\) \(.\).*/\1\2/' <<< $toolchain) +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:27:11.5697398Z [36;1m  else[0m
+2026-11-24T20:27:11.5697690Z [36;1m    echo "toolchain=1.$((($(date --date "${toolchain#stable }" +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:27:11.5698022Z [36;1m  fi[0m
+2026-11-24T20:27:11.5698236Z [36;1melif [[ $toolchain =~ ^stable' 'minus' '[0-9]+' 'releases?$ ]]; then[0m
+2026-11-24T20:27:11.5698634Z [36;1m  echo "toolchain=1.$((($(date +%s)/60/60/24-16569)/7/6-${toolchain//[^0-9]/}))" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:27:11.5698973Z [36;1melif [[ $toolchain =~ ^1\.[0-9]+$ ]]; then[0m
+2026-11-24T20:27:11.5699354Z [36;1m  echo "toolchain=1.$((i=${toolchain#1.}, c=($(date +%s)/60/60/24-16569)/7/6, i+9*i*(10*i<=c)+90*i*(100*i<=c)))" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:27:11.5699705Z [36;1melse[0m
+2026-11-24T20:27:11.5699854Z [36;1m  delimiter=EOF[0m
+2026-11-24T20:27:11.5700087Z [36;1m  while [[ $toolchain == *$delimiter* ]]; do delimiter+=_; done[0m
+2026-11-24T20:27:11.5700465Z [36;1m  printf 'toolchain<<%s\n%s\n%s\n' "$delimiter" "$toolchain" "$delimiter" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:27:11.5700773Z [36;1mfi[0m
+2026-11-24T20:27:11.5715861Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:27:11.5716173Z env:
+2026-11-24T20:27:11.5716327Z   toolchain: stable
+2026-11-24T20:27:11.5716490Z ##[endgroup]
+2026-11-24T20:27:11.7484880Z ##[end-action id=__dtolnay_rust-toolchain.parse;outcome=success;conclusion=success;duration_ms=192]
+2026-11-24T20:27:11.7489065Z ##[start-action display=Set $CARGO_HOME;id=__dtolnay_rust-toolchain.__run]
+2026-11-24T20:27:11.7601834Z ##[group]Run echo CARGO_HOME=${CARGO_HOME:-"$USERPROFILE\.cargo"} >> $GITHUB_ENV
+2026-11-24T20:27:11.7602287Z [36;1mecho CARGO_HOME=${CARGO_HOME:-"$USERPROFILE\.cargo"} >> $GITHUB_ENV[0m
+2026-11-24T20:27:11.7613427Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:27:11.7613736Z ##[endgroup]
+2026-11-24T20:27:11.8053001Z ##[end-action id=__dtolnay_rust-toolchain.__run;outcome=success;conclusion=success;duration_ms=56]
+2026-11-24T20:27:11.8055028Z ##[start-action display=Install rustup if needed;id=__dtolnay_rust-toolchain.__run_2]
+2026-11-24T20:27:11.8064233Z ##[end-action id=__dtolnay_rust-toolchain.__run_2;outcome=skipped;conclusion=skipped;duration_ms=0]
+2026-11-24T20:27:11.8066138Z ##[start-action display=Install rustup if needed on windows;id=__dtolnay_rust-toolchain.__run_3]
+2026-11-24T20:27:11.8168176Z ##[group]Run if ! command -v rustup &>/dev/null; then
+2026-11-24T20:27:11.8168455Z [36;1mif ! command -v rustup &>/dev/null; then[0m
+2026-11-24T20:27:11.8169029Z [36;1m  curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused --location --silent --show-error --fail https://win.rustup.rs/x86_64 --output 'D:\a\_temp\rustup-init.exe'[0m
+2026-11-24T20:27:11.8171080Z [36;1m  'D:\a\_temp\rustup-init.exe' --default-toolchain none --no-modify-path -y[0m
+2026-11-24T20:27:11.8171459Z [36;1m  echo "$CARGO_HOME\bin" >> $GITHUB_PATH[0m
+2026-11-24T20:27:11.8171669Z [36;1mfi[0m
+2026-11-24T20:27:11.8181957Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:27:11.8182253Z env:
+2026-11-24T20:27:11.8182407Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:27:11.8182616Z ##[endgroup]
+2026-11-24T20:27:11.8589763Z ##[end-action id=__dtolnay_rust-toolchain.__run_3;outcome=success;conclusion=success;duration_ms=52]
+2026-11-24T20:27:11.8603691Z ##[start-action display=rustup toolchain install stable;id=__dtolnay_rust-toolchain.__run_4]
+2026-11-24T20:27:11.8708203Z ##[group]Run set -f  # split targets and components on commas and whitespace, without glob expansion
+2026-11-24T20:27:11.8708725Z [36;1mset -f  # split targets and components on commas and whitespace, without glob expansion[0m
+2026-11-24T20:27:11.8709045Z [36;1mflags=()[0m
+2026-11-24T20:27:11.8709272Z [36;1mfor t in ${targets//,/ }; do flags+=(--target "$t"); done[0m
+2026-11-24T20:27:11.8709573Z [36;1mfor c in ${components//,/ }; do flags+=(--component "$c"); done[0m
+2026-11-24T20:27:11.8709943Z [36;1mif [[ $toolchain == nightly && -n $components ]]; then flags+=(--allow-downgrade); fi[0m
+2026-11-24T20:27:11.8710248Z [36;1mlog=$(mktemp)[0m
+2026-11-24T20:27:11.8710427Z [36;1mtrap 'rm -f "$log"' EXIT[0m
+2026-11-24T20:27:11.8710615Z [36;1mfor attempt in 1 2 3 4 5; do[0m
+2026-11-24T20:27:11.8711046Z [36;1m  if rustup toolchain install "$toolchain" "${flags[@]}" --profile minimal --no-self-update --force-non-host 2>&1 | tee "$log"; then[0m
+2026-11-24T20:27:11.8711451Z [36;1m    break[0m
+2026-11-24T20:27:11.8711593Z [36;1m  else[0m
+2026-11-24T20:27:11.8711734Z [36;1m    status=$?[0m
+2026-11-24T20:27:11.8711887Z [36;1m  fi[0m
+2026-11-24T20:27:11.8712100Z [36;1m  if [[ $attempt == 5 ]] || ! grep -Fq 'checksum failed for ' "$log" \[0m
+2026-11-24T20:27:11.8712507Z [36;1m      || ! grep -Fq 'this is likely due to an ongoing update of the official release server' "$log"; then[0m
+2026-11-24T20:27:11.8712861Z [36;1m    exit "$status"[0m
+2026-11-24T20:27:11.8713047Z [36;1m  fi[0m
+2026-11-24T20:27:11.8713320Z [36;1m  echo "Retrying rustup toolchain install in 30 seconds (retry $attempt of 4)" >&2[0m
+2026-11-24T20:27:11.8713646Z [36;1m  sleep 30[0m
+2026-11-24T20:27:11.8713804Z [36;1mdone[0m
+2026-11-24T20:27:11.8739839Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:27:11.8740152Z env:
+2026-11-24T20:27:11.8740306Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:27:11.8740501Z   toolchain: stable
+2026-11-24T20:27:11.8740660Z   targets: 
+2026-11-24T20:27:11.8740808Z   components: rustfmt, clippy
+2026-11-24T20:27:11.8740979Z   RUSTUP_PERMIT_COPY_RENAME: 1
+2026-11-24T20:27:11.8741144Z ##[endgroup]
+2026-11-24T20:27:15.3536443Z info: syncing channel updates for stable-x86_64-pc-windows-msvc
+2026-11-24T20:27:15.9083691Z info: latest update on 2026-11-12 for version 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-24T20:27:15.9237794Z info: component clippy is up to date
+2026-11-24T20:27:15.9238549Z info: component rustfmt is up to date
+2026-11-24T20:27:15.9394695Z 
+2026-11-24T20:27:15.9555014Z   stable-x86_64-pc-windows-msvc unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-24T20:27:15.9555307Z 
+2026-11-24T20:27:15.9818562Z ##[end-action id=__dtolnay_rust-toolchain.__run_4;outcome=success;conclusion=success;duration_ms=4121]
+2026-11-24T20:27:15.9824832Z ##[start-action display=rustup default stable;id=__dtolnay_rust-toolchain.__run_5]
+2026-11-24T20:27:15.9920751Z ##[group]Run rustup default "$toolchain" --force-non-host
+2026-11-24T20:27:15.9921060Z [36;1mrustup default "$toolchain" --force-non-host[0m
+2026-11-24T20:27:15.9931362Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:27:15.9931657Z env:
+2026-11-24T20:27:15.9932390Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:27:15.9932602Z   toolchain: stable
+2026-11-24T20:27:15.9932751Z ##[endgroup]
+2026-11-24T20:27:16.0489845Z info: using existing install for stable-x86_64-pc-windows-msvc
+2026-11-24T20:27:16.0555213Z info: default toolchain set to stable-x86_64-pc-windows-msvc
+2026-11-24T20:27:16.0555480Z 
+2026-11-24T20:27:16.0740959Z   stable-x86_64-pc-windows-msvc unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-24T20:27:16.0806652Z 
+2026-11-24T20:27:16.0812759Z ##[end-action id=__dtolnay_rust-toolchain.__run_5;outcome=success;conclusion=success;duration_ms=98]
+2026-11-24T20:27:16.0816126Z ##[start-action display=Create cachekey;id=__dtolnay_rust-toolchain.rustc-version]
+2026-11-24T20:27:16.0922483Z ##[group]Run DATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')
+2026-11-24T20:27:16.0923281Z [36;1mDATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')[0m
+2026-11-24T20:27:16.0923777Z [36;1mHASH=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-hash: //p')[0m
+2026-11-24T20:27:16.0924158Z [36;1mecho "cachekey=$(echo $DATE$HASH | head -c12)" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:27:16.0933561Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:27:16.0933864Z env:
+2026-11-24T20:27:16.0934021Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:27:16.0934222Z   toolchain: stable
+2026-11-24T20:27:16.0934387Z ##[endgroup]
+2026-11-24T20:27:16.8931708Z ##[end-action id=__dtolnay_rust-toolchain.rustc-version;outcome=success;conclusion=success;duration_ms=811]
+2026-11-24T20:27:16.8933795Z ##[start-action display=Disable incremental compilation;id=__dtolnay_rust-toolchain.__run_6]
+2026-11-24T20:27:16.9027647Z ##[group]Run if [ -z "${CARGO_INCREMENTAL+set}" ]; then
+2026-11-24T20:27:16.9027943Z [36;1mif [ -z "${CARGO_INCREMENTAL+set}" ]; then[0m
+2026-11-24T20:27:16.9028206Z [36;1m  echo CARGO_INCREMENTAL=0 >> $GITHUB_ENV[0m
+2026-11-24T20:27:16.9028409Z [36;1mfi[0m
+2026-11-24T20:27:16.9037398Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:27:16.9037690Z env:
+2026-11-24T20:27:16.9037849Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:27:16.9038050Z ##[endgroup]
+2026-11-24T20:27:16.9356585Z ##[end-action id=__dtolnay_rust-toolchain.__run_6;outcome=success;conclusion=success;duration_ms=42]
+2026-11-24T20:27:16.9359237Z ##[start-action display=Enable colors in Cargo output;id=__dtolnay_rust-toolchain.__run_7]
+2026-11-24T20:27:16.9460010Z ##[group]Run if [ -z "${CARGO_TERM_COLOR+set}" ]; then
+2026-11-24T20:27:16.9460307Z [36;1mif [ -z "${CARGO_TERM_COLOR+set}" ]; then[0m
+2026-11-24T20:27:16.9460561Z [36;1m  echo CARGO_TERM_COLOR=always >> $GITHUB_ENV[0m
+2026-11-24T20:27:16.9460779Z [36;1mfi[0m
+2026-11-24T20:27:16.9469884Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:27:16.9470193Z env:
+2026-11-24T20:27:16.9470352Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:27:16.9470564Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:27:16.9470722Z ##[endgroup]
+2026-11-24T20:27:16.9774396Z ##[end-action id=__dtolnay_rust-toolchain.__run_7;outcome=success;conclusion=success;duration_ms=41]
+2026-11-24T20:27:16.9777663Z ##[start-action display=Enable Cargo sparse registry;id=__dtolnay_rust-toolchain.__run_8]
+2026-11-24T20:27:16.9869729Z ##[group]Run # implemented in 1.66, stabilized in 1.68, made default in 1.70
+2026-11-24T20:27:16.9870687Z [36;1m# implemented in 1.66, stabilized in 1.68, made default in 1.70[0m
+2026-11-24T20:27:16.9871180Z [36;1mif [ -z "${CARGO_REGISTRIES_CRATES_IO_PROTOCOL+set}" -o -f "D:\a\_temp"/.implicit_cargo_registries_crates_io_protocol ]; then[0m
+2026-11-24T20:27:16.9871685Z [36;1m  if rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[89]\.'; then[0m
+2026-11-24T20:27:16.9872184Z [36;1m    touch "D:\a\_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
+2026-11-24T20:27:16.9872539Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse >> $GITHUB_ENV[0m
+2026-11-24T20:27:16.9872911Z [36;1m  elif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[67]\.'; then[0m
+2026-11-24T20:27:16.9873284Z [36;1m    touch "D:\a\_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
+2026-11-24T20:27:16.9873624Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git >> $GITHUB_ENV[0m
+2026-11-24T20:27:16.9873879Z [36;1m  fi[0m
+2026-11-24T20:27:16.9874361Z [36;1mfi[0m
+2026-11-24T20:27:16.9883549Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:27:16.9883843Z env:
+2026-11-24T20:27:16.9884001Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:27:16.9884212Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:27:16.9884374Z   CARGO_TERM_COLOR: always
+2026-11-24T20:27:16.9884535Z   toolchain: stable
+2026-11-24T20:27:16.9884685Z ##[endgroup]
+2026-11-24T20:27:17.1800627Z ##[end-action id=__dtolnay_rust-toolchain.__run_8;outcome=success;conclusion=success;duration_ms=202]
+2026-11-24T20:27:17.1804033Z ##[start-action display=Work around spurious network errors in curl 8.0;id=__dtolnay_rust-toolchain.__run_9]
+2026-11-24T20:27:17.1910078Z ##[group]Run # https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation
+2026-11-24T20:27:17.1910627Z [36;1m# https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation[0m
+2026-11-24T20:27:17.1911114Z [36;1mif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.7[01]\.'; then[0m
+2026-11-24T20:27:17.1911465Z [36;1m  echo CARGO_HTTP_MULTIPLEXING=false >> $GITHUB_ENV[0m
+2026-11-24T20:27:17.1911692Z [36;1mfi[0m
+2026-11-24T20:27:17.1921566Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:27:17.1921870Z env:
+2026-11-24T20:27:17.1922026Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:27:17.1922237Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:27:17.1922400Z   CARGO_TERM_COLOR: always
+2026-11-24T20:27:17.1922569Z   toolchain: stable
+2026-11-24T20:27:17.1922717Z ##[endgroup]
+2026-11-24T20:27:17.2733961Z ##[end-action id=__dtolnay_rust-toolchain.__run_9;outcome=success;conclusion=success;duration_ms=92]
+2026-11-24T20:27:17.2737754Z ##[start-action display=rustc +stable --version --verbose;id=__dtolnay_rust-toolchain.__run_10]
+2026-11-24T20:27:17.2840444Z ##[group]Run rustc +"$toolchain" --version --verbose
+2026-11-24T20:27:17.2840740Z [36;1mrustc +"$toolchain" --version --verbose[0m
+2026-11-24T20:27:17.2864872Z shell: C:\Program Files\Git\bin\bash.EXE --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:27:17.2865321Z env:
+2026-11-24T20:27:17.2865533Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:27:17.2865809Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:27:17.2866042Z   CARGO_TERM_COLOR: always
+2026-11-24T20:27:17.2866349Z   toolchain: stable
+2026-11-24T20:27:17.2866612Z ##[endgroup]
+2026-11-24T20:27:17.3542348Z rustc 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-24T20:27:17.3542742Z binary: rustc
+2026-11-24T20:27:17.3543036Z commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a
+2026-11-24T20:27:17.3543397Z commit-date: 2026-11-09
+2026-11-24T20:27:17.3543660Z host: x86_64-pc-windows-msvc
+2026-11-24T20:27:17.3543928Z release: 1.100.0
+2026-11-24T20:27:17.3544170Z LLVM version: 23.1.5
+2026-11-24T20:27:17.3650648Z ##[end-action id=__dtolnay_rust-toolchain.__run_10;outcome=success;conclusion=success;duration_ms=91]
+2026-11-24T20:27:17.3827594Z ##[group]Run Swatinem/rust-cache@v2
+2026-11-24T20:27:17.3827861Z with:
+2026-11-24T20:27:17.3828011Z   prefix-key: v0-rust
+2026-11-24T20:27:17.3828184Z   add-job-id-key: true
+2026-11-24T20:27:17.3828359Z   add-rust-environment-hash-key: true
+2026-11-24T20:27:17.3828563Z   cache-targets: true
+2026-11-24T20:27:17.3828725Z   cache-all-crates: false
+2026-11-24T20:27:17.3828903Z   cache-workspace-crates: false
+2026-11-24T20:27:17.3829083Z   save-if: true
+2026-11-24T20:27:17.3829235Z   cache-provider: github
+2026-11-24T20:27:17.3829396Z   cache-bin: true
+2026-11-24T20:27:17.3829538Z   lookup-only: false
+2026-11-24T20:27:17.3829684Z   cmd-format: {0}
+2026-11-24T20:27:17.3829814Z env:
+2026-11-24T20:27:17.3829957Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:27:17.3830157Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:27:17.3830313Z   CARGO_TERM_COLOR: always
+2026-11-24T20:27:17.3830480Z ##[endgroup]
+2026-11-24T20:27:19.4290079Z ##[group]Cache Configuration
+2026-11-24T20:27:19.4290901Z Cache Provider:
+2026-11-24T20:27:19.4291313Z     github
+2026-11-24T20:27:19.4298199Z Workspaces:
+2026-11-24T20:27:19.4298635Z     D:\a\MultiTunnel\MultiTunnel
+2026-11-24T20:27:19.4299065Z Cache Paths:
+2026-11-24T20:27:19.4299444Z     C:\Users\runneradmin\.cargo\bin
+2026-11-24T20:27:19.4299981Z     C:\Users\runneradmin\.cargo\.crates.toml
+2026-11-24T20:27:19.4307915Z     C:\Users\runneradmin\.cargo\.crates2.json
+2026-11-24T20:27:19.4314335Z     C:\Users\runneradmin\.cargo\registry
+2026-11-24T20:27:19.4324500Z     C:\Users\runneradmin\.cargo\git
+2026-11-24T20:27:19.4325343Z     D:\a\MultiTunnel\MultiTunnel\target
+2026-11-24T20:27:19.4325960Z Restore Key:
+2026-11-24T20:27:19.4326325Z     v0-rust-build-windows-Windows_NT-x64-ea627442
+2026-11-24T20:27:19.4326783Z Cache Key:
+2026-11-24T20:27:19.4327074Z     v0-rust-build-windows-Windows_NT-x64-ea627442-39107a77
+2026-11-24T20:27:19.4327443Z .. Prefix:
+2026-11-24T20:27:19.4327688Z   - v0-rust-build-windows-Windows_NT-x64
+2026-11-24T20:27:19.4328762Z .. Environment considered:
+2026-11-24T20:27:19.4329040Z   - Rust Versions:
+2026-11-24T20:27:19.4329400Z     - 1.100.0 x86_64-pc-windows-msvc 0ef1bc386059596dada8fb65b50425aab2ea637a
+2026-11-24T20:27:19.4329836Z   - CARGO_HOME
+2026-11-24T20:27:19.4330036Z   - CARGO_INCREMENTAL
+2026-11-24T20:27:19.4330197Z   - CARGO_TERM_COLOR
+2026-11-24T20:27:19.4330393Z .. Lockfiles considered:
+2026-11-24T20:27:19.4330604Z   - D:\a\MultiTunnel\MultiTunnel\Cargo.lock
+2026-11-24T20:27:19.4330831Z   - D:\a\MultiTunnel\MultiTunnel\Cargo.toml
+2026-11-24T20:27:19.4331088Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-app\Cargo.toml
+2026-11-24T20:27:19.4331558Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-bond\Cargo.toml
+2026-11-24T20:27:19.4331999Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-engine\Cargo.toml
+2026-11-24T20:27:19.4332360Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-links\Cargo.toml
+2026-11-24T20:27:19.4332775Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-m3\Cargo.toml
+2026-11-24T20:27:19.4333212Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-net\Cargo.toml
+2026-11-24T20:27:19.4333678Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-platform\Cargo.toml
+2026-11-24T20:27:19.4334144Z   - D:\a\MultiTunnel\MultiTunnel\crates\mt-service\Cargo.toml
+2026-11-24T20:27:19.4334786Z ##[endgroup]
+2026-11-24T20:27:19.4334922Z 
+2026-11-24T20:27:19.4335023Z ... Restoring cache ...
+2026-11-24T20:27:19.6581004Z Cache hit for: v0-rust-build-windows-Windows_NT-x64-ea627442-39107a77
+2026-11-24T20:27:21.1462642Z Received 134217728 of 598956331 (22.4%), 93.2 MBs/sec
+2026-11-24T20:27:22.1482439Z Received 268435456 of 598956331 (44.8%), 107.8 MBs/sec
+2026-11-24T20:27:23.1897918Z Received 402653184 of 598956331 (67.2%), 112.4 MBs/sec
+2026-11-24T20:27:24.1926266Z Received 536870912 of 598956331 (89.6%), 115.8 MBs/sec
+2026-11-24T20:27:24.6811307Z Received 598956331 of 598956331 (100.0%), 116.5 MBs/sec
+2026-11-24T20:27:24.6811725Z Cache Size: ~571 MB (598956331 B)
+2026-11-24T20:27:24.6814940Z [command]"C:\Program Files\Git\usr\bin\tar.exe" -xf D:/a/_temp/f5c90829-09f3-4bf1-bc31-4605e71c97e5/cache.tzst -P -C D:/a/MultiTunnel/MultiTunnel --force-local --use-compress-program "zstd -d"
+2026-11-24T20:27:38.6100704Z Cache restored successfully
+2026-11-24T20:27:38.6394092Z Restored from cache key "v0-rust-build-windows-Windows_NT-x64-ea627442-39107a77" full match: true.
+2026-11-24T20:27:38.6642626Z ##[group]Run cargo fmt --all --check
+2026-11-24T20:27:38.6642965Z [36;1mcargo fmt --all --check[0m
+2026-11-24T20:27:38.7326909Z shell: C:\Program Files\PowerShell\7\pwsh.EXE -command ". '{0}'"
+2026-11-24T20:27:38.7327205Z env:
+2026-11-24T20:27:38.7327388Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:27:38.7327613Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:27:38.7327788Z   CARGO_TERM_COLOR: always
+2026-11-24T20:27:38.7327967Z   CACHE_ON_FAILURE: false
+2026-11-24T20:27:38.7328146Z ##[endgroup]
+2026-11-24T20:27:41.3404215Z ##[group]Run cargo clippy --workspace --all-targets -- -D warnings
+2026-11-24T20:27:41.3404896Z [36;1mcargo clippy --workspace --all-targets -- -D warnings[0m
+2026-11-24T20:27:41.3480180Z shell: C:\Program Files\PowerShell\7\pwsh.EXE -command ". '{0}'"
+2026-11-24T20:27:41.3480470Z env:
+2026-11-24T20:27:41.3480659Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:27:41.3480889Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:27:41.3481077Z   CARGO_TERM_COLOR: always
+2026-11-24T20:27:41.3481264Z   CACHE_ON_FAILURE: false
+2026-11-24T20:27:41.3481443Z ##[endgroup]
+2026-11-24T20:27:42.8608792Z [1m[92m    Updating[0m crates.io index
+2026-11-24T20:27:58.2375009Z [1m[92m    Checking[0m iced_graphics v0.13.0 (D:\a\MultiTunnel\MultiTunnel\vendor\iced_graphics)
+2026-11-24T20:27:58.2377458Z [1m[92m    Checking[0m mt-net v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-net)
+2026-11-24T20:27:58.2413766Z [1m[92m    Checking[0m mt-links v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-links)
+2026-11-24T20:27:58.2414619Z [1m[92m    Checking[0m mt-service v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-service)
+2026-11-24T20:28:06.5980044Z [1m[92m   Compiling[0m mt-platform v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-platform)
+2026-11-24T20:28:06.6264720Z [1m[92m    Checking[0m iced_tiny_skia v0.13.0 (D:\a\MultiTunnel\MultiTunnel\vendor\iced_tiny_skia)
+2026-11-24T20:28:07.0559391Z [1m[92m    Checking[0m iced_renderer v0.13.0
+2026-11-24T20:28:07.0900169Z [1m[92m    Checking[0m iced_winit v0.13.0
+2026-11-24T20:28:07.2132871Z [1m[92m    Checking[0m iced_widget v0.13.4
+2026-11-24T20:28:07.5124473Z [1m[92m    Checking[0m mt-engine v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-engine)
+2026-11-24T20:28:07.6436646Z [1m[92m    Checking[0m mt-bond v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-bond)
+2026-11-24T20:28:08.6358638Z [1m[92m    Checking[0m iced v0.13.1
+2026-11-24T20:28:08.9119916Z [1m[92m    Checking[0m mt-m3 v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-m3)
+2026-11-24T20:28:09.2452422Z [1m[92m    Checking[0m mt-app v0.14.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-app)
+2026-11-24T20:28:11.2203316Z [1m[92m    Checking[0m multitunnel v0.14.0 (D:\a\MultiTunnel\MultiTunnel)
+2026-11-24T20:28:13.7062184Z [1m[92m    Finished[0m `dev` profile [unoptimized + debuginfo] target(s) in 31.49s
+2026-11-24T20:28:13.8503479Z ##[group]Run cargo test --workspace
+2026-11-24T20:28:13.8503820Z [36;1mcargo test --workspace[0m
+2026-11-24T20:28:13.8556810Z shell: C:\Program Files\PowerShell\7\pwsh.EXE -command ". '{0}'"
+2026-11-24T20:28:13.8557097Z env:
+2026-11-24T20:28:13.8557275Z   CARGO_HOME: C:\Users\runneradmin\.cargo
+2026-11-24T20:28:13.8557505Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:28:13.8557684Z   CARGO_TERM_COLOR: always
+2026-11-24T20:28:13.8557867Z   CACHE_ON_FAILURE: false
+2026-11-24T20:28:13.8558050Z ##[endgroup]
+2026-11-24T20:28:14.4870720Z [1m[92m   Compiling[0m mt-net v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-net)
+2026-11-24T20:28:14.4929622Z [1m[92m   Compiling[0m iced_graphics v0.13.0 (D:\a\MultiTunnel\MultiTunnel\vendor\iced_graphics)
+2026-11-24T20:28:14.4977014Z [1m[92m   Compiling[0m mt-links v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-links)
+2026-11-24T20:28:14.4977972Z [1m[92m   Compiling[0m mt-service v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-service)
+2026-11-24T20:28:15.7009512Z [1m[92m   Compiling[0m iced_tiny_skia v0.13.0 (D:\a\MultiTunnel\MultiTunnel\vendor\iced_tiny_skia)
+2026-11-24T20:28:16.7186690Z [1m[92m   Compiling[0m iced_renderer v0.13.0
+2026-11-24T20:28:16.7819101Z [1m[92m   Compiling[0m iced_winit v0.13.0
+2026-11-24T20:28:16.9037426Z [1m[92m   Compiling[0m iced_widget v0.13.4
+2026-11-24T20:28:16.9424326Z [1m[92m   Compiling[0m mt-engine v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-engine)
+2026-11-24T20:28:17.3547602Z [1m[92m   Compiling[0m mt-bond v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-bond)
+2026-11-24T20:28:17.4927874Z [1m[92m   Compiling[0m mt-platform v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-platform)
+2026-11-24T20:28:18.6486951Z [1m[92m   Compiling[0m iced v0.13.1
+2026-11-24T20:28:18.9856217Z [1m[92m   Compiling[0m mt-m3 v0.1.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-m3)
+2026-11-24T20:28:19.4128979Z [1m[92m   Compiling[0m mt-app v0.14.0 (D:\a\MultiTunnel\MultiTunnel\crates\mt-app)
+2026-11-24T20:28:21.9836900Z [1m[92m   Compiling[0m multitunnel v0.14.0 (D:\a\MultiTunnel\MultiTunnel)
+2026-11-24T20:28:47.7056064Z [1m[92m    Finished[0m `test` profile [unoptimized + debuginfo] target(s) in 33.61s
+2026-11-24T20:28:47.7304869Z [1m[92m     Running[0m unittests src\lib.rs (target\debug\build\mt-app/4c747a949941af2f\out\mt_app-4c747a949941af2f.exe)
+2026-11-24T20:28:47.7422691Z 
+2026-11-24T20:28:47.7422949Z running 64 tests
+2026-11-24T20:28:47.7469233Z test app::tests::app_version_is_release_triplet ... ok
+2026-11-24T20:28:47.7525534Z test app::tests::adapter_rate_sampling_deltas_and_resets ... ok
+2026-11-24T20:28:47.7528579Z test app::tests::close_action_decision ... ok
+2026-11-24T20:28:47.7535407Z test app::tests::close_to_tray_settings_roundtrip ... ok
+2026-11-24T20:28:47.7582713Z test app::tests::begin_exit_is_idempotent_without_tray ... ok
+2026-11-24T20:28:47.7584250Z test app::tests::always_on_top_menu_item_roundtrips ... ok
+2026-11-24T20:28:47.7587230Z test app::tests::fmt_helpers ... ok
+2026-11-24T20:28:47.7589828Z test app::tests::kill_switch_banner_gate ... ok
+2026-11-24T20:28:47.7592637Z test app::tests::link_matches_adapter_shapes ... ok
+2026-11-24T20:28:47.7595031Z test app::tests::log_filter_matches ... ok
+2026-11-24T20:28:47.7649839Z test app::tests::adapters_screen_toggle_move_apply_revert ... ok
+2026-11-24T20:28:47.7663482Z test app::tests::logs_autoscroll_default_and_toggle ... ok
+2026-11-24T20:28:47.7666209Z test app::tests::logs_scroll_id_stable ... ok
+2026-11-24T20:28:47.7668799Z test app::tests::rule_editor_add_edit_remove ... ok
+2026-11-24T20:28:47.7671289Z test app::tests::edit_dialog_open_type_save_close ... ok
+2026-11-24T20:28:47.7719549Z test app::tests::edit_save_failure_is_logged ... ok
+2026-11-24T20:28:47.7722296Z test app::tests::logs_autoscroll_gated_on_screen ... ok
+2026-11-24T20:28:47.7723257Z test app::tests::seed_parse ... ok
+2026-11-24T20:28:47.7725640Z test app::tests::smoke_flag_parse ... ok
+2026-11-24T20:28:47.7794320Z test app::tests::session_totals_accumulate_and_reset ... ok
+2026-11-24T20:28:47.7798110Z test app::tests::sort_adapter_rows_selected_first_in_saved_order ... ok
+2026-11-24T20:28:47.7803222Z test app::tests::s21_qos_toggle_updates_draft ... ok
+2026-11-24T20:28:47.7810735Z test app::tests::s19_settings_toggles_and_modal_gating ... ok
+2026-11-24T20:28:47.7877916Z test app::tests::title_menu_anchor_clamps ... ok
+2026-11-24T20:28:47.7879763Z test app::tests::titlebar_double_click_window ... ok
+2026-11-24T20:28:47.8190012Z test app::tests::theme_reroutes_on_settings_messages ... ok
+2026-11-24T20:28:47.8383462Z test app::tests::wheel_drag_updates_seed_live_and_persists_on_release ... ok
+2026-11-24T20:28:47.8385577Z test graph::tests::local_bounds_strips_widget_offset_regression ... ok
+2026-11-24T20:28:47.8387971Z test graph::tests::nice_ceiling_covers_and_rounds ... ok
+2026-11-24T20:28:47.8388968Z test graph::tests::points_stay_inside_bounds_and_right_align ... ok
+2026-11-24T20:28:47.8390877Z test graph::tests::rates_format_human ... ok
+2026-11-24T20:28:47.8392446Z test mock::tests::fmt_utc_known_values ... ok
+2026-11-24T20:28:47.8491041Z test app::tests::smoke_run_drives_app_with_mock ... ok
+2026-11-24T20:28:47.9125940Z test mock::tests::mock_latency_test ... ok
+2026-11-24T20:28:47.9130914Z test mock::tests::mock_profiles_import_remove ... ok
+2026-11-24T20:28:47.9135058Z test mock::tests::mock_settings_and_rules ... ok
+2026-11-24T20:28:47.9185289Z test theme::tests::button_states_use_state_layers ... ok
+2026-11-24T20:28:47.9187455Z test theme::tests::hsv_conversions_roundtrip_primaries ... ok
+2026-11-24T20:28:47.9235858Z test theme::tests::switch_styles_differ_by_state ... ok
+2026-11-24T20:28:47.9239314Z test theme::tests::system_prefers_dark_env_override_wins ... ok
+2026-11-24T20:28:47.9287743Z test theme::tests::text_color_from_m3 ... ok
+2026-11-24T20:28:47.9474295Z test theme::tests::theme_derivation_seed_changes_scheme ... ok
+2026-11-24T20:28:47.9570948Z test theme::tests::theme_from_settings_and_rederive ... ok
+2026-11-24T20:28:47.9572777Z test theme::tests::wheel_asset_dimensions ... ok
+2026-11-24T20:28:47.9574559Z test theme::tests::wheel_marker_roundtrips_picked_points ... ok
+2026-11-24T20:28:47.9576103Z test theme::tests::wheel_seed_clamps_outside_points_to_the_rim ... ok
+2026-11-24T20:28:47.9577842Z test theme::tests::wheel_seed_matches_cardinal_hues ... ok
+2026-11-24T20:28:47.9579918Z test tray::tests::ico_embedded_asset_parses_with_32_entry ... ok
+2026-11-24T20:28:47.9581761Z test tray::tests::ico_embedded_mono_asset_parses_with_32_entry ... ok
+2026-11-24T20:28:47.9583455Z test tray::tests::ico_rejects_bad_directories ... ok
+2026-11-24T20:28:47.9585485Z test tray::tests::ids_roundtrip_and_skip_zero ... ok
+2026-11-24T20:28:47.9587248Z test tray::tests::menu_connected_shows_disconnect_and_servers ... ok
+2026-11-24T20:28:47.9588831Z test tray::tests::menu_disconnected_shows_connect ... ok
+2026-11-24T20:28:47.9590771Z test tray::tests::menu_failed_is_disconnected_shape_with_honest_header ... ok
+2026-11-24T20:28:47.9592444Z test tray::tests::menu_transition_disables_the_toggle ... ok
+2026-11-24T20:28:47.9594219Z test tray::tests::model_from_snapshot_states ... ok
+2026-11-24T20:28:47.9596393Z test tray::tests::tip_array_never_overruns_or_splits_surrogates ... ok
+2026-11-24T20:28:47.9598174Z test tray::tests::to_wide_is_nul_terminated ... ok
+2026-11-24T20:28:47.9600246Z test tray::tests::tooltip_live_formats_speeds_and_totals ... ok
+2026-11-24T20:28:47.9601929Z test tray::tests::tooltip_states ... ok
+2026-11-24T20:28:47.9603396Z test tray::tests::tray_icon_follows_connection_state ... ok
+2026-11-24T20:28:48.6071109Z test app::tests::speed_test_message_flow ... ok
+2026-11-24T20:28:48.7042689Z test app::tests::update_loop_screens_and_rules ... ok
+2026-11-24T20:28:48.7488505Z test mock::tests::mock_connect_disconnect_lifecycle ... ok
+2026-11-24T20:28:48.7488757Z 
+2026-11-24T20:28:48.7490045Z test result: ok. 64 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.01s
+2026-11-24T20:28:48.7490335Z 
+2026-11-24T20:28:48.7514433Z [1m[92m     Running[0m unittests src\main.rs (target\debug\build\mt-app/7c07808781ddcf4a\out\mt_app-7c07808781ddcf4a.exe)
+2026-11-24T20:28:48.7597526Z 
+2026-11-24T20:28:48.7598025Z running 0 tests
+2026-11-24T20:28:48.7598427Z 
+2026-11-24T20:28:48.7598894Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
+2026-11-24T20:28:48.7599186Z 
+2026-11-24T20:28:48.7612126Z [1m[92m     Running[0m unittests src\lib.rs (target\debug\build\mt-bond/6410ae8cc506944c\out\mt_bond-6410ae8cc506944c.exe)
+2026-11-24T20:28:48.7693636Z 
+2026-11-24T20:28:48.7693889Z running 33 tests
+2026-11-24T20:28:48.7702553Z test endpoint::scheduling_tests::balanced_blest_guard_never_bypasses_the_score_leader ... ok
+2026-11-24T20:28:48.7704685Z test endpoint::scheduling_tests::balanced_blest_guard_skips_slow_draining_tied_carrier ... ok
+2026-11-24T20:28:48.7706230Z test endpoint::scheduling_tests::balanced_capacity_aware_bypasses_the_saturated_adapter ... ok
+2026-11-24T20:28:48.7707790Z test endpoint::scheduling_tests::balanced_picks_the_least_backlogged_carrier ... ok
+2026-11-24T20:28:48.7709175Z test endpoint::scheduling_tests::balanced_ties_rotate ... ok
+2026-11-24T20:28:48.7709873Z test endpoint::scheduling_tests::balanced_spreads_proportionally_to_reported_capacity ... ok
+2026-11-24T20:28:48.7710688Z test endpoint::scheduling_tests::carrier_score_is_relative_load_with_probe_floor ... ok
+2026-11-24T20:28:48.7711868Z test endpoint::scheduling_tests::ecf_completion_is_rtt_plus_drain_time ... ok
+2026-11-24T20:28:48.7713487Z test endpoint::scheduling_tests::failover_takes_the_first_live_carrier ... ok
+2026-11-24T20:28:48.7714936Z test endpoint::scheduling_tests::lowest_latency_ecf_prefers_earliest_completion ... ok
+2026-11-24T20:28:48.7716089Z test endpoint::scheduling_tests::lowest_latency_prefers_sampled_rtt_then_known_over_unknown ... ok
+2026-11-24T20:28:48.7716905Z test endpoint::scheduling_tests::priority_fills_in_order_until_the_queue_backs_up ... ok
+2026-11-24T20:28:48.7717342Z test endpoint::scheduling_tests::mixed_reports_probe_the_unreported_carrier ... ok
+2026-11-24T20:28:48.7720163Z test endpoint::scheduling_tests::priority_spills_when_relative_load_exceeds_ratio ... ok
+2026-11-24T20:28:48.7721507Z test endpoint::scheduling_tests::round_robin_cycles_the_cursor ... ok
+2026-11-24T20:28:48.7722915Z test frame::tests::empty_and_max_payloads_roundtrip ... ok
+2026-11-24T20:28:48.7724555Z test frame::tests::encode_session_borrowed_is_wire_identical ... ok
+2026-11-24T20:28:48.7725304Z test frame::tests::header_rejects_bad_magic_version_kind_and_size ... ok
+2026-11-24T20:28:48.7727569Z test frame::tests::target_rejects_truncated_and_garbage ... ok
+2026-11-24T20:28:48.7728468Z test frame::tests::roundtrip_all_kinds ... ok
+2026-11-24T20:28:48.7729888Z test frame::tests::read_write_frame_over_duplex ... ok
+2026-11-24T20:28:48.7731741Z test frame::tests::target_roundtrip_domain_ipv4_ipv6 ... ok
+2026-11-24T20:28:48.7733892Z test reorder::tests::cap_zero_still_delivers_in_order ... ok
+2026-11-24T20:28:48.7735473Z test outbound::tests::ensure_carriers_fails_cleanly_with_no_links ... ok
+2026-11-24T20:28:48.7736825Z test outbound::tests::ensure_carriers_reports_all_dead_links ... ok
+2026-11-24T20:28:48.7737790Z test reorder::tests::duplicates_and_replays_are_idempotent ... ok
+2026-11-24T20:28:48.7738442Z test reorder::tests::gap_buffers_then_flushes ... ok
+2026-11-24T20:28:48.7739131Z test reorder::tests::in_order_delivery ... ok
+2026-11-24T20:28:48.7740453Z test reorder::tests::long_out_of_order_run_reorders_completely ... ok
+2026-11-24T20:28:48.7741474Z test reorder::tests::overflow_is_an_error ... ok
+2026-11-24T20:28:48.7742112Z test reorder::tests::would_overflow_mirrors_the_insert_boundary ... ok
+2026-11-24T20:28:48.7742757Z test tests::constants_are_coherent ... ok
+2026-11-24T20:28:48.9861447Z test daemon::tests::serve_binds_and_shuts_down ... ok
+2026-11-24T20:28:48.9861761Z 
+2026-11-24T20:28:48.9862615Z test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
+2026-11-24T20:28:48.9862903Z 
+2026-11-24T20:28:48.9883143Z [1m[92m     Running[0m unittests src\bin\mt_merge_server.rs (target\debug\build\mt-bond/fbee07725169cdd9\out\mt_merge_server-fbee07725169cdd9.exe)
+2026-11-24T20:28:48.9964542Z 
+2026-11-24T20:28:48.9964886Z running 0 tests
+2026-11-24T20:28:48.9965095Z 
+2026-11-24T20:28:48.9965742Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
+2026-11-24T20:28:48.9966196Z 
+2026-11-24T20:28:48.9977426Z [1m[92m     Running[0m tests\integration.rs (target\debug\build\mt-bond/38eeb9b5e0d206b1\out\integration-38eeb9b5e0d206b1.exe)
+2026-11-24T20:28:49.0060098Z 
+2026-11-24T20:28:49.0060574Z running 6 tests
+2026-11-24T20:28:49.3648070Z test carrier_death_mid_stream_fails_over_without_data_loss ... ok
+2026-11-24T20:28:50.0910437Z test echo_roundtrip_1mib_across_two_carriers ... ok
+2026-11-24T20:28:52.1268799Z test concurrent_sessions_multiplex_over_the_same_carriers ... ok
+2026-11-24T20:28:52.1737792Z test refused_target_surfaces_as_connection_reset ... ok
+2026-11-24T20:28:52.4742712Z test one_dead_link_still_yields_a_working_bond ... ok
+2026-11-24T20:29:49.0111481Z test domain_targets_dial_through_the_daemon has been running for over 60 seconds
+2026-11-24T20:29:49.0113106Z test domain_targets_dial_through_the_daemon ... FAILED
+2026-11-24T20:29:49.0113628Z 
+2026-11-24T20:29:49.0113723Z failures:
+2026-11-24T20:29:49.0113811Z 
+2026-11-24T20:29:49.0113939Z ---- domain_targets_dial_through_the_daemon stdout ----
+2026-11-24T20:29:49.0114203Z [mt-merge-server] listening on 127.0.0.1:52542
+2026-11-24T20:29:49.0114368Z 
+2026-11-24T20:29:49.0114632Z thread 'domain_targets_dial_through_the_daemon' (6544) panicked at crates\mt-bond\tests\integration.rs:382:6:
+2026-11-24T20:29:49.0115484Z deadline: domain_targets wedged (documented runner flake — see LEDGER): Elapsed(())
+2026-11-24T20:29:49.0115906Z note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
+2026-11-24T20:29:49.0116137Z 
+2026-11-24T20:29:49.0116140Z 
+2026-11-24T20:29:49.0116199Z failures:
+2026-11-24T20:29:49.0116386Z     domain_targets_dial_through_the_daemon
+2026-11-24T20:29:49.0116551Z 
+2026-11-24T20:29:49.0116780Z test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 60.01s
+2026-11-24T20:29:49.0117091Z 
+2026-11-24T20:29:49.0146157Z [1m[91merror[0m: test failed, to rerun pass `-p mt-bond --test integration`
+2026-11-24T20:29:49.1071748Z ##[error]Process completed with exit code 1.
+2026-11-24T20:29:49.1284324Z Post job cleanup.
+2026-11-24T20:29:49.2584864Z [command]"C:\Program Files\Git\bin\git.exe" version
+2026-11-24T20:29:49.2874683Z git version 2.56.1.windows.1
+2026-11-24T20:29:49.2933790Z Temporarily overriding HOME='D:\a\_temp\2d504455-8d0b-4102-8e16-e59c48f8f959' before making global git config changes
+2026-11-24T20:29:49.2934448Z Adding repository directory to the temporary git global config as a safe directory
+2026-11-24T20:29:49.2942313Z [command]"C:\Program Files\Git\bin\git.exe" config --global --add safe.directory D:\a\MultiTunnel\MultiTunnel
+2026-11-24T20:29:49.3324160Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp core\.sshCommand
+2026-11-24T20:29:49.3661191Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "sh -c \"git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :\""
+2026-11-24T20:29:49.8475726Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
+2026-11-24T20:29:49.8752733Z http.https://github.com/.extraheader
+2026-11-24T20:29:49.8800726Z [command]"C:\Program Files\Git\bin\git.exe" config --local --unset-all http.https://github.com/.extraheader
+2026-11-24T20:29:49.9109184Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "sh -c \"git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :\""
+2026-11-24T20:29:50.4182064Z [command]"C:\Program Files\Git\bin\git.exe" config --local --name-only --get-regexp ^includeIf\.gitdir:
+2026-11-24T20:29:50.4534671Z [command]"C:\Program Files\Git\bin\git.exe" submodule foreach --recursive "git config --local --show-origin --name-only --get-regexp remote.origin.url"
+2026-11-24T20:29:50.9242819Z Cleaning up orphan processes
+2026-11-24T20:29:50.9732521Z Terminate orphan process: pid (7108) (vctip)
+2026-11-24T20:29:51.0355927Z ##[warning]Node.js 20 is deprecated. The following actions target Node.js 20 but are being forced to run on Node.js 24: actions/checkout@v4. For more information see: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/
diff --git a/harness/memory/agent-reports/s21-ci-domain-targets-release-run3.log b/harness/memory/agent-reports/s21-ci-domain-targets-release-run3.log
new file mode 100644
index 0000000..c743b0f
--- /dev/null
+++ b/harness/memory/agent-reports/s21-ci-domain-targets-release-run3.log
@@ -0,0 +1,497 @@
+﻿2026-11-24T20:36:56.7468464Z Current runner version: '2.338.2'
+2026-11-24T20:36:56.7492979Z ##[group]Runner Image Provisioner
+2026-11-24T20:36:56.7493795Z Hosted Compute Agent
+2026-11-24T20:36:56.7494430Z Version: 20261111.635
+2026-11-24T20:36:56.7495023Z Commit: b8a594ea514ff44cadb5342986c68b1eac6e22ed
+2026-11-24T20:36:56.7495709Z Build Date: 2026-11-11T01:49:01Z
+2026-11-24T20:36:56.7496378Z Worker ID: {8af1622b-9c23-4c29-a178-070895d02a66}
+2026-11-24T20:36:56.7497063Z Region: westus3
+2026-11-24T20:36:56.7497598Z Cloud: Azure
+2026-11-24T20:36:56.7498097Z ##[endgroup]
+2026-11-24T20:36:56.7499542Z ##[group]Operating System
+2026-11-24T20:36:56.7500177Z Ubuntu
+2026-11-24T20:36:56.7500717Z 26.04.1
+2026-11-24T20:36:56.7501209Z LTS
+2026-11-24T20:36:56.7501705Z ##[endgroup]
+2026-11-24T20:36:56.7502231Z ##[group]Runner Image
+2026-11-24T20:36:56.7502952Z Image: ubuntu-26.04
+2026-11-24T20:36:56.7503506Z Version: 20261116.202.1
+2026-11-24T20:36:56.7504661Z Included Software: https://github.com/actions/runner-images/blob/ubuntu26/20261116.202/images/ubuntu/Ubuntu2604-Readme.md
+2026-11-24T20:36:56.7506075Z Image Release: https://github.com/actions/runner-images/releases/tag/ubuntu26%2F20261116.202
+2026-11-24T20:36:56.7506969Z ##[endgroup]
+2026-11-24T20:36:56.7508008Z ##[group]GITHUB_TOKEN Permissions
+2026-11-24T20:36:56.7510328Z Contents: write
+2026-11-24T20:36:56.7510904Z Metadata: read
+2026-11-24T20:36:56.7511502Z ##[endgroup]
+2026-11-24T20:36:56.7513820Z Secret source: Actions
+2026-11-24T20:36:56.7514667Z Cache mode: write
+2026-11-24T20:36:56.7515443Z Prepare workflow directory
+2026-11-24T20:36:56.7905686Z Prepare all required actions
+2026-11-24T20:36:56.7954522Z Getting action download info
+2026-11-24T20:36:57.1280272Z Download action repository 'actions/checkout@v4' (SHA:11d5960a326750d5838078e36cf38b85af677262)
+2026-11-24T20:36:57.2637874Z Download action repository 'dtolnay/rust-toolchain@stable' (SHA:39008827e6b4319a349e9a42dfc90579dd25f1ca)
+2026-11-24T20:36:57.4927585Z Download action repository 'Swatinem/rust-cache@v2' (SHA:d1527c8f67ef9af357290475d4af4b0fab8f12e0)
+2026-11-24T20:36:58.1017173Z Download action repository 'actions/upload-artifact@v4' (SHA:ea165f8d65b6e75b540449e92b4886f43607fa02)
+2026-11-24T20:36:58.1655955Z Download action repository 'softprops/action-gh-release@v2' (SHA:3bb12739c298aeb8a4eeaf626c5b8d85266b0e65)
+2026-11-24T20:36:58.8080048Z Complete job name: build-linux-daemon
+2026-11-24T20:36:58.9025651Z ##[group]Run actions/checkout@v4
+2026-11-24T20:36:58.9026900Z with:
+2026-11-24T20:36:58.9027700Z   repository: maniesh-NR/MultiTunnel
+2026-11-24T20:36:58.9035332Z   token: ***
+2026-11-24T20:36:58.9036083Z   ssh-strict: true
+2026-11-24T20:36:58.9036876Z   ssh-user: git
+2026-11-24T20:36:58.9037674Z   persist-credentials: true
+2026-11-24T20:36:58.9038534Z   clean: true
+2026-11-24T20:36:58.9039329Z   sparse-checkout-cone-mode: true
+2026-11-24T20:36:58.9040259Z   fetch-depth: 1
+2026-11-24T20:36:58.9041030Z   fetch-tags: false
+2026-11-24T20:36:58.9041817Z   show-progress: true
+2026-11-24T20:36:58.9042739Z   lfs: false
+2026-11-24T20:36:58.9043529Z   submodules: false
+2026-11-24T20:36:58.9044322Z   set-safe-directory: true
+2026-11-24T20:36:58.9045223Z   allow-unsafe-pr-checkout: false
+2026-11-24T20:36:58.9046429Z ##[endgroup]
+2026-11-24T20:36:59.1725168Z Syncing repository: maniesh-NR/MultiTunnel
+2026-11-24T20:36:59.1729448Z ##[group]Getting Git version info
+2026-11-24T20:36:59.1731074Z Working directory is '/home/runner/work/MultiTunnel/MultiTunnel'
+2026-11-24T20:36:59.1734140Z [command]/usr/bin/git version
+2026-11-24T20:36:59.1761131Z git version 2.56.1
+2026-11-24T20:36:59.1789132Z ##[endgroup]
+2026-11-24T20:36:59.1801509Z Temporarily overriding HOME='/home/runner/work/_temp/a5f6893c-8af6-4f36-8c6d-8cf562b6250a' before making global git config changes
+2026-11-24T20:36:59.1806263Z Adding repository directory to the temporary git global config as a safe directory
+2026-11-24T20:36:59.1810131Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/MultiTunnel/MultiTunnel
+2026-11-24T20:36:59.1880833Z Deleting the contents of '/home/runner/work/MultiTunnel/MultiTunnel'
+2026-11-24T20:36:59.1888576Z ##[group]Initializing the repository
+2026-11-24T20:36:59.1893336Z [command]/usr/bin/git init /home/runner/work/MultiTunnel/MultiTunnel
+2026-11-24T20:36:59.1990347Z hint: Using 'master' as the name for the initial branch. This default branch name
+2026-11-24T20:36:59.1994769Z hint: will change to "main" in Git 3.0. To configure the initial branch name
+2026-11-24T20:36:59.1998370Z hint: to use in all of your new repositories, which will suppress this warning,
+2026-11-24T20:36:59.2001132Z hint: call:
+2026-11-24T20:36:59.2002978Z hint:
+2026-11-24T20:36:59.2004836Z hint: 	git config --global init.defaultBranch <name>
+2026-11-24T20:36:59.2007042Z hint:
+2026-11-24T20:36:59.2009430Z hint: Names commonly chosen instead of 'master' are 'main', 'trunk' and
+2026-11-24T20:36:59.2013021Z hint: 'development'. The just-created branch can be renamed via this command:
+2026-11-24T20:36:59.2015820Z hint:
+2026-11-24T20:36:59.2017774Z hint: 	git branch -m <name>
+2026-11-24T20:36:59.2019563Z hint:
+2026-11-24T20:36:59.2021847Z hint: Disable this message with "git config set advice.defaultBranchName false"
+2026-11-24T20:36:59.2026091Z Initialized empty Git repository in /home/runner/work/MultiTunnel/MultiTunnel/.git/
+2026-11-24T20:36:59.2032669Z [command]/usr/bin/git remote add origin https://github.com/maniesh-NR/MultiTunnel
+2026-11-24T20:36:59.2061599Z ##[endgroup]
+2026-11-24T20:36:59.2064025Z ##[group]Disabling automatic garbage collection
+2026-11-24T20:36:59.2066119Z [command]/usr/bin/git config --local gc.auto 0
+2026-11-24T20:36:59.2098365Z ##[endgroup]
+2026-11-24T20:36:59.2100852Z ##[group]Setting up auth
+2026-11-24T20:36:59.2106726Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
+2026-11-24T20:36:59.2141603Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
+2026-11-24T20:36:59.2569880Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
+2026-11-24T20:36:59.2617242Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
+2026-11-24T20:36:59.2851867Z [command]/usr/bin/git config --local --name-only --get-regexp ^includeIf\.gitdir:
+2026-11-24T20:36:59.2885867Z [command]/usr/bin/git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url
+2026-11-24T20:36:59.3109341Z [command]/usr/bin/git config --local http.https://github.com/.extraheader AUTHORIZATION: basic ***
+2026-11-24T20:36:59.3145197Z ##[endgroup]
+2026-11-24T20:36:59.3153778Z ##[group]Fetching the repository
+2026-11-24T20:36:59.3156162Z [command]/usr/bin/git -c protocol.version=2 fetch --no-tags --prune --no-recurse-submodules --depth=1 origin +e510e439249fe4072477c05040bd5b9d0678db6a:refs/tags/v0.14.0
+2026-11-24T20:37:00.1205240Z From https://github.com/maniesh-NR/MultiTunnel
+2026-11-24T20:37:00.1205777Z  * [new ref]         e510e439249fe4072477c05040bd5b9d0678db6a -> v0.14.0
+2026-11-24T20:37:00.1209105Z ##[endgroup]
+2026-11-24T20:37:00.1209755Z ##[group]Determining the checkout info
+2026-11-24T20:37:00.1211716Z ##[endgroup]
+2026-11-24T20:37:00.1217697Z [command]/usr/bin/git sparse-checkout disable
+2026-11-24T20:37:00.1265386Z [command]/usr/bin/git config --local --unset-all extensions.worktreeConfig
+2026-11-24T20:37:00.1291344Z ##[group]Checking out the ref
+2026-11-24T20:37:00.1296969Z [command]/usr/bin/git checkout --progress --force refs/tags/v0.14.0
+2026-11-24T20:37:00.1614021Z Note: switching to 'refs/tags/v0.14.0'.
+2026-11-24T20:37:00.1614457Z 
+2026-11-24T20:37:00.1614846Z You are in 'detached HEAD' state. You can look around, make experimental
+2026-11-24T20:37:00.1615654Z changes and commit them, and you can discard any commits you make in this
+2026-11-24T20:37:00.1616130Z state without impacting any branches by switching back to a branch.
+2026-11-24T20:37:00.1616887Z 
+2026-11-24T20:37:00.1617209Z If you want to create a new branch to retain commits you create, you may
+2026-11-24T20:37:00.1617904Z do so (now or later) by using -c with the switch command. Example:
+2026-11-24T20:37:00.1618327Z 
+2026-11-24T20:37:00.1618534Z   git switch -c <new-branch-name>
+2026-11-24T20:37:00.1618831Z 
+2026-11-24T20:37:00.1618994Z Or undo this operation with:
+2026-11-24T20:37:00.1619251Z 
+2026-11-24T20:37:00.1619384Z   git switch -
+2026-11-24T20:37:00.1619607Z 
+2026-11-24T20:37:00.1620019Z Turn off this advice by setting config variable advice.detachedHead to false
+2026-11-24T20:37:00.1620487Z 
+2026-11-24T20:37:00.1620771Z HEAD is now at e510e43 backup(s21-release-prep-2): 2026-11-24T20:08:15Z
+2026-11-24T20:37:00.1622574Z ##[endgroup]
+2026-11-24T20:37:00.1667398Z [command]/usr/bin/git log -1 --format=%H
+2026-11-24T20:37:00.1687967Z e510e439249fe4072477c05040bd5b9d0678db6a
+2026-11-24T20:37:00.2067272Z ##[group]Run dtolnay/rust-toolchain@stable
+2026-11-24T20:37:00.2067607Z with:
+2026-11-24T20:37:00.2067828Z   components: rustfmt, clippy
+2026-11-24T20:37:00.2068080Z   toolchain: stable
+2026-11-24T20:37:00.2068290Z ##[endgroup]
+2026-11-24T20:37:00.2142933Z ##[start-action display=Parse toolchain version;id=__dtolnay_rust-toolchain.parse]
+2026-11-24T20:37:00.2201582Z ##[group]Run if [[ -z $toolchain ]]; then
+2026-11-24T20:37:00.2202059Z [36;1mif [[ -z $toolchain ]]; then[0m
+2026-11-24T20:37:00.2202927Z [36;1m  # GitHub does not enforce `required: true` inputs itself. https://github.com/actions/runner/issues/1070[0m
+2026-11-24T20:37:00.2203585Z [36;1m  echo "'toolchain' is a required input" >&2[0m
+2026-11-24T20:37:00.2203950Z [36;1m  exit 1[0m
+2026-11-24T20:37:00.2204353Z [36;1melif [[ $toolchain =~ ^stable' '[0-9]+' '(year|month|week|day)s?' 'ago$ ]]; then[0m
+2026-11-24T20:37:00.2204859Z [36;1m  if [[ Linux == macOS ]]; then[0m
+2026-11-24T20:37:00.2205551Z [36;1m    echo "toolchain=1.$((($(date -v-$(sed 's/stable \([0-9]*\) \(.\).*/\1\2/' <<< $toolchain) +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:37:00.2206178Z [36;1m  else[0m
+2026-11-24T20:37:00.2206660Z [36;1m    echo "toolchain=1.$((($(date --date "${toolchain#stable }" +%s)/60/60/24-16569)/7/6))" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:37:00.2207197Z [36;1m  fi[0m
+2026-11-24T20:37:00.2207501Z [36;1melif [[ $toolchain =~ ^stable' 'minus' '[0-9]+' 'releases?$ ]]; then[0m
+2026-11-24T20:37:00.2208031Z [36;1m  echo "toolchain=1.$((($(date +%s)/60/60/24-16569)/7/6-${toolchain//[^0-9]/}))" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:37:00.2208494Z [36;1melif [[ $toolchain =~ ^1\.[0-9]+$ ]]; then[0m
+2026-11-24T20:37:00.2209016Z [36;1m  echo "toolchain=1.$((i=${toolchain#1.}, c=($(date +%s)/60/60/24-16569)/7/6, i+9*i*(10*i<=c)+90*i*(100*i<=c)))" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:37:00.2209504Z [36;1melse[0m
+2026-11-24T20:37:00.2209751Z [36;1m  delimiter=EOF[0m
+2026-11-24T20:37:00.2210070Z [36;1m  while [[ $toolchain == *$delimiter* ]]; do delimiter+=_; done[0m
+2026-11-24T20:37:00.2210575Z [36;1m  printf 'toolchain<<%s\n%s\n%s\n' "$delimiter" "$toolchain" "$delimiter" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:37:00.2210984Z [36;1mfi[0m
+2026-11-24T20:37:00.2234897Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:37:00.2235277Z env:
+2026-11-24T20:37:00.2235501Z   toolchain: stable
+2026-11-24T20:37:00.2235722Z ##[endgroup]
+2026-11-24T20:37:00.2338971Z ##[end-action id=__dtolnay_rust-toolchain.parse;outcome=success;conclusion=success;duration_ms=19]
+2026-11-24T20:37:00.2343707Z ##[start-action display=Set $CARGO_HOME;id=__dtolnay_rust-toolchain.__run]
+2026-11-24T20:37:00.2378357Z ##[group]Run echo CARGO_HOME=${CARGO_HOME:-"$HOME/.cargo"} >> $GITHUB_ENV
+2026-11-24T20:37:00.2378874Z [36;1mecho CARGO_HOME=${CARGO_HOME:-"$HOME/.cargo"} >> $GITHUB_ENV[0m
+2026-11-24T20:37:00.2398282Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:37:00.2398636Z ##[endgroup]
+2026-11-24T20:37:00.2446765Z ##[end-action id=__dtolnay_rust-toolchain.__run;outcome=success;conclusion=success;duration_ms=10]
+2026-11-24T20:37:00.2449440Z ##[start-action display=Install rustup if needed;id=__dtolnay_rust-toolchain.__run_2]
+2026-11-24T20:37:00.2476211Z ##[group]Run if ! command -v rustup &>/dev/null; then
+2026-11-24T20:37:00.2476596Z [36;1mif ! command -v rustup &>/dev/null; then[0m
+2026-11-24T20:37:00.2477338Z [36;1m  curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused --location --silent --show-error --fail https://sh.rustup.rs | sh -s -- --default-toolchain none -y[0m
+2026-11-24T20:37:00.2478043Z [36;1m  echo "$CARGO_HOME/bin" >> $GITHUB_PATH[0m
+2026-11-24T20:37:00.2478407Z [36;1mfi[0m
+2026-11-24T20:37:00.2496949Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:37:00.2497303Z env:
+2026-11-24T20:37:00.2497523Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:00.2498031Z ##[endgroup]
+2026-11-24T20:37:00.2541904Z ##[end-action id=__dtolnay_rust-toolchain.__run_2;outcome=success;conclusion=success;duration_ms=9]
+2026-11-24T20:37:00.2544742Z ##[start-action display=Install rustup if needed on windows;id=__dtolnay_rust-toolchain.__run_3]
+2026-11-24T20:37:00.2552121Z ##[end-action id=__dtolnay_rust-toolchain.__run_3;outcome=skipped;conclusion=skipped;duration_ms=0]
+2026-11-24T20:37:00.2566771Z ##[start-action display=rustup toolchain install stable;id=__dtolnay_rust-toolchain.__run_4]
+2026-11-24T20:37:00.2585619Z ##[group]Run set -f  # split targets and components on commas and whitespace, without glob expansion
+2026-11-24T20:37:00.2586378Z [36;1mset -f  # split targets and components on commas and whitespace, without glob expansion[0m
+2026-11-24T20:37:00.2586858Z [36;1mflags=()[0m
+2026-11-24T20:37:00.2587155Z [36;1mfor t in ${targets//,/ }; do flags+=(--target "$t"); done[0m
+2026-11-24T20:37:00.2587567Z [36;1mfor c in ${components//,/ }; do flags+=(--component "$c"); done[0m
+2026-11-24T20:37:00.2588078Z [36;1mif [[ $toolchain == nightly && -n $components ]]; then flags+=(--allow-downgrade); fi[0m
+2026-11-24T20:37:00.2588513Z [36;1mlog=$(mktemp)[0m
+2026-11-24T20:37:00.2588805Z [36;1mtrap 'rm -f "$log"' EXIT[0m
+2026-11-24T20:37:00.2589075Z [36;1mfor attempt in 1 2 3 4 5; do[0m
+2026-11-24T20:37:00.2589642Z [36;1m  if rustup toolchain install "$toolchain" "${flags[@]}" --profile minimal --no-self-update --force-non-host 2>&1 | tee "$log"; then[0m
+2026-11-24T20:37:00.2590197Z [36;1m    break[0m
+2026-11-24T20:37:00.2590408Z [36;1m  else[0m
+2026-11-24T20:37:00.2590622Z [36;1m    status=$?[0m
+2026-11-24T20:37:00.2590836Z [36;1m  fi[0m
+2026-11-24T20:37:00.2591130Z [36;1m  if [[ $attempt == 5 ]] || ! grep -Fq 'checksum failed for ' "$log" \[0m
+2026-11-24T20:37:00.2591681Z [36;1m      || ! grep -Fq 'this is likely due to an ongoing update of the official release server' "$log"; then[0m
+2026-11-24T20:37:00.2592211Z [36;1m    exit "$status"[0m
+2026-11-24T20:37:00.2592724Z [36;1m  fi[0m
+2026-11-24T20:37:00.2593186Z [36;1m  echo "Retrying rustup toolchain install in 30 seconds (retry $attempt of 4)" >&2[0m
+2026-11-24T20:37:00.2593610Z [36;1m  sleep 30[0m
+2026-11-24T20:37:00.2593817Z [36;1mdone[0m
+2026-11-24T20:37:00.2612945Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:37:00.2613298Z env:
+2026-11-24T20:37:00.2613525Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:00.2613780Z   toolchain: stable
+2026-11-24T20:37:00.2614002Z   targets: 
+2026-11-24T20:37:00.2614212Z   components: rustfmt, clippy
+2026-11-24T20:37:00.2614473Z   RUSTUP_PERMIT_COPY_RENAME: 1
+2026-11-24T20:37:00.2614716Z ##[endgroup]
+2026-11-24T20:37:00.4365927Z info: syncing channel updates for stable-x86_64-unknown-linux-gnu
+2026-11-24T20:37:00.5959286Z info: latest update on 2026-11-12 for version 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-24T20:37:00.6094391Z info: component clippy is up to date
+2026-11-24T20:37:00.6094800Z info: component rustfmt is up to date
+2026-11-24T20:37:00.6106568Z 
+2026-11-24T20:37:00.6179683Z   stable-x86_64-unknown-linux-gnu unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-24T20:37:00.6180593Z 
+2026-11-24T20:37:00.6217254Z ##[end-action id=__dtolnay_rust-toolchain.__run_4;outcome=success;conclusion=success;duration_ms=364]
+2026-11-24T20:37:00.6223008Z ##[start-action display=rustup default stable;id=__dtolnay_rust-toolchain.__run_5]
+2026-11-24T20:37:00.6240520Z ##[group]Run rustup default "$toolchain" --force-non-host
+2026-11-24T20:37:00.6240931Z [36;1mrustup default "$toolchain" --force-non-host[0m
+2026-11-24T20:37:00.6348310Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:37:00.6348676Z env:
+2026-11-24T20:37:00.6348905Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:00.6349168Z   toolchain: stable
+2026-11-24T20:37:00.6349385Z ##[endgroup]
+2026-11-24T20:37:00.6429037Z info: using existing install for stable-x86_64-unknown-linux-gnu
+2026-11-24T20:37:00.6442747Z info: default toolchain set to stable-x86_64-unknown-linux-gnu
+2026-11-24T20:37:00.6443352Z 
+2026-11-24T20:37:00.6508825Z   stable-x86_64-unknown-linux-gnu unchanged - rustc 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-24T20:37:00.6509233Z 
+2026-11-24T20:37:00.6523348Z ##[end-action id=__dtolnay_rust-toolchain.__run_5;outcome=success;conclusion=success;duration_ms=29]
+2026-11-24T20:37:00.6527294Z ##[start-action display=Create cachekey;id=__dtolnay_rust-toolchain.rustc-version]
+2026-11-24T20:37:00.6546468Z ##[group]Run DATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')
+2026-11-24T20:37:00.6547330Z [36;1mDATE=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-date: \(20[0-9][0-9]\)-\([01][0-9]\)-\([0-3][0-9]\)$/\1\2\3/p')[0m
+2026-11-24T20:37:00.6547997Z [36;1mHASH=$(rustc +"$toolchain" --version --verbose | sed -ne 's/^commit-hash: //p')[0m
+2026-11-24T20:37:00.6548515Z [36;1mecho "cachekey=$(echo $DATE$HASH | head -c12)" >> $GITHUB_OUTPUT[0m
+2026-11-24T20:37:00.6567966Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:37:00.6568325Z env:
+2026-11-24T20:37:00.6568559Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:00.6568814Z   toolchain: stable
+2026-11-24T20:37:00.6569027Z ##[endgroup]
+2026-11-24T20:37:00.6884625Z ##[end-action id=__dtolnay_rust-toolchain.rustc-version;outcome=success;conclusion=success;duration_ms=35]
+2026-11-24T20:37:00.6887330Z ##[start-action display=Disable incremental compilation;id=__dtolnay_rust-toolchain.__run_6]
+2026-11-24T20:37:00.6906299Z ##[group]Run if [ -z "${CARGO_INCREMENTAL+set}" ]; then
+2026-11-24T20:37:00.6906702Z [36;1mif [ -z "${CARGO_INCREMENTAL+set}" ]; then[0m
+2026-11-24T20:37:00.6907038Z [36;1m  echo CARGO_INCREMENTAL=0 >> $GITHUB_ENV[0m
+2026-11-24T20:37:00.6907336Z [36;1mfi[0m
+2026-11-24T20:37:00.6926533Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:37:00.6926885Z env:
+2026-11-24T20:37:00.6927101Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:00.6927354Z ##[endgroup]
+2026-11-24T20:37:00.6970856Z ##[end-action id=__dtolnay_rust-toolchain.__run_6;outcome=success;conclusion=success;duration_ms=8]
+2026-11-24T20:37:00.6973429Z ##[start-action display=Enable colors in Cargo output;id=__dtolnay_rust-toolchain.__run_7]
+2026-11-24T20:37:00.6991114Z ##[group]Run if [ -z "${CARGO_TERM_COLOR+set}" ]; then
+2026-11-24T20:37:00.6991501Z [36;1mif [ -z "${CARGO_TERM_COLOR+set}" ]; then[0m
+2026-11-24T20:37:00.6991834Z [36;1m  echo CARGO_TERM_COLOR=always >> $GITHUB_ENV[0m
+2026-11-24T20:37:00.6992134Z [36;1mfi[0m
+2026-11-24T20:37:00.7009764Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:37:00.7010111Z env:
+2026-11-24T20:37:00.7010327Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:00.7010593Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:37:00.7010813Z ##[endgroup]
+2026-11-24T20:37:00.7053835Z ##[end-action id=__dtolnay_rust-toolchain.__run_7;outcome=success;conclusion=success;duration_ms=7]
+2026-11-24T20:37:00.7057382Z ##[start-action display=Enable Cargo sparse registry;id=__dtolnay_rust-toolchain.__run_8]
+2026-11-24T20:37:00.7076335Z ##[group]Run # implemented in 1.66, stabilized in 1.68, made default in 1.70
+2026-11-24T20:37:00.7076970Z [36;1m# implemented in 1.66, stabilized in 1.68, made default in 1.70[0m
+2026-11-24T20:37:00.7077651Z [36;1mif [ -z "${CARGO_REGISTRIES_CRATES_IO_PROTOCOL+set}" -o -f "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol ]; then[0m
+2026-11-24T20:37:00.7078371Z [36;1m  if rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[89]\.'; then[0m
+2026-11-24T20:37:00.7078949Z [36;1m    touch "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
+2026-11-24T20:37:00.7079478Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse >> $GITHUB_ENV[0m
+2026-11-24T20:37:00.7079986Z [36;1m  elif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.6[67]\.'; then[0m
+2026-11-24T20:37:00.7080556Z [36;1m    touch "/home/runner/work/_temp"/.implicit_cargo_registries_crates_io_protocol || true[0m
+2026-11-24T20:37:00.7081170Z [36;1m    echo CARGO_REGISTRIES_CRATES_IO_PROTOCOL=git >> $GITHUB_ENV[0m
+2026-11-24T20:37:00.7081520Z [36;1m  fi[0m
+2026-11-24T20:37:00.7081720Z [36;1mfi[0m
+2026-11-24T20:37:00.7100009Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:37:00.7100350Z env:
+2026-11-24T20:37:00.7100562Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:00.7100831Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:37:00.7101062Z   CARGO_TERM_COLOR: always
+2026-11-24T20:37:00.7101294Z   toolchain: stable
+2026-11-24T20:37:00.7101499Z ##[endgroup]
+2026-11-24T20:37:00.7364040Z ##[end-action id=__dtolnay_rust-toolchain.__run_8;outcome=success;conclusion=success;duration_ms=30]
+2026-11-24T20:37:00.7368082Z ##[start-action display=Work around spurious network errors in curl 8.0;id=__dtolnay_rust-toolchain.__run_9]
+2026-11-24T20:37:00.7389896Z ##[group]Run # https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation
+2026-11-24T20:37:00.7390663Z [36;1m# https://rust-lang.zulipchat.com/#narrow/stream/246057-t-cargo/topic/timeout.20investigation[0m
+2026-11-24T20:37:00.7391311Z [36;1mif rustc +"$toolchain" --version --verbose | grep -q '^release: 1\.7[01]\.'; then[0m
+2026-11-24T20:37:00.7391773Z [36;1m  echo CARGO_HTTP_MULTIPLEXING=false >> $GITHUB_ENV[0m
+2026-11-24T20:37:00.7392084Z [36;1mfi[0m
+2026-11-24T20:37:00.7411234Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:37:00.7411572Z env:
+2026-11-24T20:37:00.7411791Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:00.7412056Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:37:00.7412282Z   CARGO_TERM_COLOR: always
+2026-11-24T20:37:00.7412855Z   toolchain: stable
+2026-11-24T20:37:00.7413075Z ##[endgroup]
+2026-11-24T20:37:00.7571328Z ##[end-action id=__dtolnay_rust-toolchain.__run_9;outcome=success;conclusion=success;duration_ms=20]
+2026-11-24T20:37:00.7576602Z ##[start-action display=rustc +stable --version --verbose;id=__dtolnay_rust-toolchain.__run_10]
+2026-11-24T20:37:00.7595992Z ##[group]Run rustc +"$toolchain" --version --verbose
+2026-11-24T20:37:00.7596410Z [36;1mrustc +"$toolchain" --version --verbose[0m
+2026-11-24T20:37:00.7615959Z shell: /usr/bin/bash --noprofile --norc -e -o pipefail {0}
+2026-11-24T20:37:00.7616300Z env:
+2026-11-24T20:37:00.7616516Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:00.7616773Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:37:00.7617011Z   CARGO_TERM_COLOR: always
+2026-11-24T20:37:00.7617245Z   toolchain: stable
+2026-11-24T20:37:00.7617456Z ##[endgroup]
+2026-11-24T20:37:00.7778121Z rustc 1.100.0 (0ef1bc386 2026-11-09)
+2026-11-24T20:37:00.7778726Z binary: rustc
+2026-11-24T20:37:00.7779172Z commit-hash: 0ef1bc386059596dada8fb65b50425aab2ea637a
+2026-11-24T20:37:00.7779705Z commit-date: 2026-11-09
+2026-11-24T20:37:00.7780109Z host: x86_64-unknown-linux-gnu
+2026-11-24T20:37:00.7780476Z release: 1.100.0
+2026-11-24T20:37:00.7780797Z LLVM version: 23.1.5
+2026-11-24T20:37:00.7800035Z ##[end-action id=__dtolnay_rust-toolchain.__run_10;outcome=success;conclusion=success;duration_ms=22]
+2026-11-24T20:37:00.7913091Z ##[group]Run Swatinem/rust-cache@v2
+2026-11-24T20:37:00.7913559Z with:
+2026-11-24T20:37:00.7913767Z   prefix-key: v0-rust
+2026-11-24T20:37:00.7913997Z   add-job-id-key: true
+2026-11-24T20:37:00.7914246Z   add-rust-environment-hash-key: true
+2026-11-24T20:37:00.7914523Z   cache-targets: true
+2026-11-24T20:37:00.7914755Z   cache-all-crates: false
+2026-11-24T20:37:00.7915015Z   cache-workspace-crates: false
+2026-11-24T20:37:00.7915262Z   save-if: true
+2026-11-24T20:37:00.7915476Z   cache-provider: github
+2026-11-24T20:37:00.7915709Z   cache-bin: true
+2026-11-24T20:37:00.7915920Z   lookup-only: false
+2026-11-24T20:37:00.7916138Z   cmd-format: {0}
+2026-11-24T20:37:00.7916334Z env:
+2026-11-24T20:37:00.7916536Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:00.7916791Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:37:00.7917017Z   CARGO_TERM_COLOR: always
+2026-11-24T20:37:00.7917252Z ##[endgroup]
+2026-11-24T20:37:01.1525055Z ##[group]Cache Configuration
+2026-11-24T20:37:01.1527013Z Cache Provider:
+2026-11-24T20:37:01.1527662Z     github
+2026-11-24T20:37:01.1528162Z Workspaces:
+2026-11-24T20:37:01.1528788Z     /home/runner/work/MultiTunnel/MultiTunnel
+2026-11-24T20:37:01.1529553Z Cache Paths:
+2026-11-24T20:37:01.1530104Z     /home/runner/.cargo/bin
+2026-11-24T20:37:01.1530747Z     /home/runner/.cargo/.crates.toml
+2026-11-24T20:37:01.1531458Z     /home/runner/.cargo/.crates2.json
+2026-11-24T20:37:01.1532309Z     /home/runner/.cargo/registry
+2026-11-24T20:37:01.1540315Z     /home/runner/.cargo/git
+2026-11-24T20:37:01.1541149Z     /home/runner/work/MultiTunnel/MultiTunnel/target
+2026-11-24T20:37:01.1541866Z Restore Key:
+2026-11-24T20:37:01.1542615Z     v0-rust-build-linux-daemon-Linux-x64-2fd77104
+2026-11-24T20:37:01.1543844Z Cache Key:
+2026-11-24T20:37:01.1544463Z     v0-rust-build-linux-daemon-Linux-x64-2fd77104-39107a77
+2026-11-24T20:37:01.1545058Z .. Prefix:
+2026-11-24T20:37:01.1545485Z   - v0-rust-build-linux-daemon-Linux-x64
+2026-11-24T20:37:01.1546035Z .. Environment considered:
+2026-11-24T20:37:01.1546515Z   - Rust Versions:
+2026-11-24T20:37:01.1547248Z     - 1.100.0 x86_64-unknown-linux-gnu 0ef1bc386059596dada8fb65b50425aab2ea637a
+2026-11-24T20:37:01.1547920Z   - CARGO_HOME
+2026-11-24T20:37:01.1548282Z   - CARGO_INCREMENTAL
+2026-11-24T20:37:01.1548663Z   - CARGO_TERM_COLOR
+2026-11-24T20:37:01.1549081Z .. Lockfiles considered:
+2026-11-24T20:37:01.1549630Z   - /home/runner/work/MultiTunnel/MultiTunnel/Cargo.lock
+2026-11-24T20:37:01.1550311Z   - /home/runner/work/MultiTunnel/MultiTunnel/Cargo.toml
+2026-11-24T20:37:01.1551088Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-app/Cargo.toml
+2026-11-24T20:37:01.1551982Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond/Cargo.toml
+2026-11-24T20:37:01.1553065Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-engine/Cargo.toml
+2026-11-24T20:37:01.1553910Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-links/Cargo.toml
+2026-11-24T20:37:01.1554668Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-m3/Cargo.toml
+2026-11-24T20:37:01.1555461Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net/Cargo.toml
+2026-11-24T20:37:01.1556339Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-platform/Cargo.toml
+2026-11-24T20:37:01.1557245Z   - /home/runner/work/MultiTunnel/MultiTunnel/crates/mt-service/Cargo.toml
+2026-11-24T20:37:01.1558412Z ##[endgroup]
+2026-11-24T20:37:01.1558670Z 
+2026-11-24T20:37:01.1558857Z ... Restoring cache ...
+2026-11-24T20:37:01.3827302Z Cache hit for: v0-rust-build-linux-daemon-Linux-x64-2fd77104-39107a77
+2026-11-24T20:37:02.5997494Z Received 71303168 of 217294952 (32.8%), 68.0 MBs/sec
+2026-11-24T20:37:03.5204106Z Received 217294952 of 217294952 (100.0%), 107.9 MBs/sec
+2026-11-24T20:37:03.5205574Z Cache Size: ~207 MB (217294952 B)
+2026-11-24T20:37:03.5323042Z [command]/usr/bin/tar -xf /home/runner/work/_temp/9bedf71f-3618-4c40-ac45-6cc06b5374fa/cache.tzst -P -C /home/runner/work/MultiTunnel/MultiTunnel --use-compress-program unzstd
+2026-11-24T20:37:04.4208774Z Cache restored successfully
+2026-11-24T20:37:04.4278133Z Restored from cache key "v0-rust-build-linux-daemon-Linux-x64-2fd77104-39107a77" full match: true.
+2026-11-24T20:37:04.4386296Z ##[group]Run cargo fmt --all --check
+2026-11-24T20:37:04.4386657Z [36;1mcargo fmt --all --check[0m
+2026-11-24T20:37:04.4407912Z shell: /usr/bin/bash -e {0}
+2026-11-24T20:37:04.4408222Z env:
+2026-11-24T20:37:04.4408438Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:04.4408703Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:37:04.4408930Z   CARGO_TERM_COLOR: always
+2026-11-24T20:37:04.4409188Z   CACHE_ON_FAILURE: false
+2026-11-24T20:37:04.4409438Z ##[endgroup]
+2026-11-24T20:37:04.9116989Z ##[group]Run cargo clippy -p mt-bond --all-targets -- -D warnings
+2026-11-24T20:37:04.9117488Z [36;1mcargo clippy -p mt-bond --all-targets -- -D warnings[0m
+2026-11-24T20:37:04.9136654Z shell: /usr/bin/bash -e {0}
+2026-11-24T20:37:04.9136936Z env:
+2026-11-24T20:37:04.9137144Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:04.9137410Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:37:04.9137651Z   CARGO_TERM_COLOR: always
+2026-11-24T20:37:04.9137912Z   CACHE_ON_FAILURE: false
+2026-11-24T20:37:04.9138142Z ##[endgroup]
+2026-11-24T20:37:05.2628623Z [1m[92m    Updating[0m crates.io index
+2026-11-24T20:37:05.5636947Z [1m[92m    Checking[0m mt-net v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net)
+2026-11-24T20:37:07.8990819Z [1m[92m    Checking[0m mt-bond v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond)
+2026-11-24T20:37:08.7755798Z [1m[92m    Finished[0m `dev` profile [unoptimized + debuginfo] target(s) in 3.76s
+2026-11-24T20:37:08.7919048Z ##[group]Run cargo test -p mt-bond
+2026-11-24T20:37:08.7919467Z [36;1mcargo test -p mt-bond[0m
+2026-11-24T20:37:08.7938957Z shell: /usr/bin/bash -e {0}
+2026-11-24T20:37:08.7939232Z env:
+2026-11-24T20:37:08.7939443Z   CARGO_HOME: /home/runner/.cargo
+2026-11-24T20:37:08.7939716Z   CARGO_INCREMENTAL: 0
+2026-11-24T20:37:08.7939952Z   CARGO_TERM_COLOR: always
+2026-11-24T20:37:08.7940203Z   CACHE_ON_FAILURE: false
+2026-11-24T20:37:08.7940430Z ##[endgroup]
+2026-11-24T20:37:08.9032319Z [1m[92m   Compiling[0m mt-net v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-net)
+2026-11-24T20:37:09.5953828Z [1m[92m   Compiling[0m mt-bond v0.1.0 (/home/runner/work/MultiTunnel/MultiTunnel/crates/mt-bond)
+2026-11-24T20:37:12.4119615Z [1m[92m    Finished[0m `test` profile [unoptimized + debuginfo] target(s) in 3.60s
+2026-11-24T20:37:12.4188313Z [1m[92m     Running[0m unittests src/lib.rs (target/debug/build/mt-bond/a79068680de7bb57/out/mt_bond-a79068680de7bb57)
+2026-11-24T20:37:12.4201804Z 
+2026-11-24T20:37:12.4202011Z running 33 tests
+2026-11-24T20:37:12.4214075Z test endpoint::scheduling_tests::balanced_blest_guard_skips_slow_draining_tied_carrier ... ok
+2026-11-24T20:37:12.4215740Z test endpoint::scheduling_tests::balanced_picks_the_least_backlogged_carrier ... ok
+2026-11-24T20:37:12.4216943Z test endpoint::scheduling_tests::balanced_blest_guard_never_bypasses_the_score_leader ... ok
+2026-11-24T20:37:12.4218203Z test endpoint::scheduling_tests::balanced_capacity_aware_bypasses_the_saturated_adapter ... ok
+2026-11-24T20:37:12.4219625Z test endpoint::scheduling_tests::balanced_spreads_proportionally_to_reported_capacity ... ok
+2026-11-24T20:37:12.4220692Z test endpoint::scheduling_tests::balanced_ties_rotate ... ok
+2026-11-24T20:37:12.4221633Z test endpoint::scheduling_tests::ecf_completion_is_rtt_plus_drain_time ... ok
+2026-11-24T20:37:12.4223363Z test endpoint::scheduling_tests::carrier_score_is_relative_load_with_probe_floor ... ok
+2026-11-24T20:37:12.4224284Z test endpoint::scheduling_tests::failover_takes_the_first_live_carrier ... ok
+2026-11-24T20:37:12.4224859Z test endpoint::scheduling_tests::lowest_latency_prefers_sampled_rtt_then_known_over_unknown ... ok
+2026-11-24T20:37:12.4225492Z test endpoint::scheduling_tests::lowest_latency_ecf_prefers_earliest_completion ... ok
+2026-11-24T20:37:12.4226035Z test endpoint::scheduling_tests::mixed_reports_probe_the_unreported_carrier ... ok
+2026-11-24T20:37:12.4226606Z test endpoint::scheduling_tests::priority_fills_in_order_until_the_queue_backs_up ... ok
+2026-11-24T20:37:12.4227456Z test endpoint::scheduling_tests::priority_spills_when_relative_load_exceeds_ratio ... ok
+2026-11-24T20:37:12.4227949Z test frame::tests::empty_and_max_payloads_roundtrip ... ok
+2026-11-24T20:37:12.4228361Z test endpoint::scheduling_tests::round_robin_cycles_the_cursor ... ok
+2026-11-24T20:37:12.4228797Z test frame::tests::encode_session_borrowed_is_wire_identical ... ok
+2026-11-24T20:37:12.4229249Z test frame::tests::header_rejects_bad_magic_version_kind_and_size ... ok
+2026-11-24T20:37:12.4229642Z test frame::tests::roundtrip_all_kinds ... ok
+2026-11-24T20:37:12.4229983Z test frame::tests::read_write_frame_over_duplex ... ok
+2026-11-24T20:37:12.4230362Z test frame::tests::target_rejects_truncated_and_garbage ... ok
+2026-11-24T20:37:12.4230762Z test frame::tests::target_roundtrip_domain_ipv4_ipv6 ... ok
+2026-11-24T20:37:12.4231194Z test outbound::tests::ensure_carriers_fails_cleanly_with_no_links ... ok
+2026-11-24T20:37:12.4231648Z test outbound::tests::ensure_carriers_reports_all_dead_links ... ok
+2026-11-24T20:37:12.4232067Z test reorder::tests::cap_zero_still_delivers_in_order ... ok
+2026-11-24T20:37:12.4232724Z test reorder::tests::duplicates_and_replays_are_idempotent ... ok
+2026-11-24T20:37:12.4233317Z test reorder::tests::in_order_delivery ... ok
+2026-11-24T20:37:12.4233885Z test reorder::tests::gap_buffers_then_flushes ... ok
+2026-11-24T20:37:12.4234271Z test reorder::tests::overflow_is_an_error ... ok
+2026-11-24T20:37:12.4235042Z test reorder::tests::would_overflow_mirrors_the_insert_boundary ... ok
+2026-11-24T20:37:12.4235504Z test reorder::tests::long_out_of_order_run_reorders_completely ... ok
+2026-11-24T20:37:12.4235879Z test tests::constants_are_coherent ... ok
+2026-11-24T20:37:12.6229878Z test daemon::tests::serve_binds_and_shuts_down ... ok
+2026-11-24T20:37:12.6230460Z 
+2026-11-24T20:37:12.6230967Z test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
+2026-11-24T20:37:12.6231708Z 
+2026-11-24T20:37:12.6233749Z [1m[92m     Running[0m unittests src/bin/mt_merge_server.rs (target/debug/build/mt-bond/9915eef574e68264/out/mt_merge_server-9915eef574e68264)
+2026-11-24T20:37:12.6245243Z 
+2026-11-24T20:37:12.6245788Z running 0 tests
+2026-11-24T20:37:12.6246188Z 
+2026-11-24T20:37:12.6247128Z test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
+2026-11-24T20:37:12.6248057Z 
+2026-11-24T20:37:12.6249472Z [1m[92m     Running[0m tests/integration.rs (target/debug/build/mt-bond/3d8a2918f951a063/out/integration-3d8a2918f951a063)
+2026-11-24T20:37:12.6260511Z 
+2026-11-24T20:37:12.6261246Z running 6 tests
+2026-11-24T20:37:13.6939582Z test carrier_death_mid_stream_fails_over_without_data_loss ... ok
+2026-11-24T20:37:15.7584266Z test concurrent_sessions_multiplex_over_the_same_carriers ... ok
+2026-11-24T20:37:15.7672272Z test echo_roundtrip_1mib_across_two_carriers ... ok
+2026-11-24T20:37:15.8110694Z test refused_target_surfaces_as_connection_reset ... ok
+2026-11-24T20:37:16.7880589Z test one_dead_link_still_yields_a_working_bond ... ok
+2026-11-24T20:38:12.6273497Z test domain_targets_dial_through_the_daemon has been running for over 60 seconds
+2026-11-24T20:38:12.6297928Z test domain_targets_dial_through_the_daemon ... FAILED
+2026-11-24T20:38:12.6298455Z 
+2026-11-24T20:38:12.6298633Z failures:
+2026-11-24T20:38:12.6298834Z 
+2026-11-24T20:38:12.6299016Z ---- domain_targets_dial_through_the_daemon stdout ----
+2026-11-24T20:38:12.6299427Z [mt-merge-server] listening on 127.0.0.1:38591
+2026-11-24T20:38:12.6299711Z 
+2026-11-24T20:38:12.6300085Z thread 'domain_targets_dial_through_the_daemon' (2745) panicked at crates/mt-bond/tests/integration.rs:382:6:
+2026-11-24T20:38:12.6301055Z deadline: domain_targets wedged (documented runner flake — see LEDGER): Elapsed(())
+2026-11-24T20:38:12.6301730Z note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
+2026-11-24T20:38:12.6302085Z 
+2026-11-24T20:38:12.6302090Z 
+2026-11-24T20:38:12.6302197Z failures:
+2026-11-24T20:38:12.6303339Z     domain_targets_dial_through_the_daemon
+2026-11-24T20:38:12.6303574Z 
+2026-11-24T20:38:12.6303881Z test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 60.00s
+2026-11-24T20:38:12.6304291Z 
+2026-11-24T20:38:12.6307998Z [1m[91merror[0m: test failed, to rerun pass `-p mt-bond --test integration`
+2026-11-24T20:38:12.6362351Z ##[error]Process completed with exit code 101.
+2026-11-24T20:38:12.6527178Z Post job cleanup.
+2026-11-24T20:38:12.7379358Z [command]/usr/bin/git version
+2026-11-24T20:38:12.7422773Z git version 2.56.1
+2026-11-24T20:38:12.7460068Z Temporarily overriding HOME='/home/runner/work/_temp/58dd0450-01a4-4e8a-8b78-e4f4f99e7daa' before making global git config changes
+2026-11-24T20:38:12.7461536Z Adding repository directory to the temporary git global config as a safe directory
+2026-11-24T20:38:12.7466435Z [command]/usr/bin/git config --global --add safe.directory /home/runner/work/MultiTunnel/MultiTunnel
+2026-11-24T20:38:12.7504488Z [command]/usr/bin/git config --local --name-only --get-regexp core\.sshCommand
+2026-11-24T20:38:12.7538378Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'core\.sshCommand' && git config --local --unset-all 'core.sshCommand' || :"
+2026-11-24T20:38:12.7787549Z [command]/usr/bin/git config --local --name-only --get-regexp http\.https\:\/\/github\.com\/\.extraheader
+2026-11-24T20:38:12.7810765Z http.https://github.com/.extraheader
+2026-11-24T20:38:12.7821251Z [command]/usr/bin/git config --local --unset-all http.https://github.com/.extraheader
+2026-11-24T20:38:12.7853240Z [command]/usr/bin/git submodule foreach --recursive sh -c "git config --local --name-only --get-regexp 'http\.https\:\/\/github\.com\/\.extraheader' && git config --local --unset-all 'http.https://github.com/.extraheader' || :"
+2026-11-24T20:38:12.8139962Z [command]/usr/bin/git config --local --name-only --get-regexp ^includeIf\.gitdir:
+2026-11-24T20:38:12.8177570Z [command]/usr/bin/git submodule foreach --recursive git config --local --show-origin --name-only --get-regexp remote.origin.url
+2026-11-24T20:38:12.8581674Z Cleaning up orphan processes
+2026-11-24T20:38:12.8877184Z ##[warning]Node.js 20 is deprecated. The following actions target Node.js 20 but are being forced to run on Node.js 24: actions/checkout@v4. For more information see: https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/
diff --git a/harness/memory/curated/edges.json b/harness/memory/curated/edges.json
index f04fdfa..2251d71 100644
--- a/harness/memory/curated/edges.json
+++ b/harness/memory/curated/edges.json
@@ -60,6 +60,18 @@
       "rel": "part_of",
       "evidence": "workspace crate"
     },
+    {
+      "src": "decision:D29",
+      "dst": "ledger:s21",
+      "rel": "decided_in",
+      "evidence": "agent-curated"
+    },
+    {
+      "src": "decision:D30",
+      "dst": "ledger:s21",
+      "rel": "decided_in",
+      "evidence": "agent-curated"
+    },
     {
       "src": "protocol:memory-graph",
       "dst": "protocol:pstack",
diff --git a/harness/memory/curated/nodes.json b/harness/memory/curated/nodes.json
index 577db45..df12b95 100644
--- a/harness/memory/curated/nodes.json
+++ b/harness/memory/curated/nodes.json
@@ -221,6 +221,60 @@
         }
       ]
     },
+    {
+      "id": "decision:D29",
+      "type": "decision",
+      "title": "D29 P0 reconnect force-close + bind retry",
+      "tags": [],
+      "sources": [
+        {
+          "file": "harness/memory/curated/",
+          "anchor": ""
+        }
+      ],
+      "passages": [
+        {
+          "text": "Second-connection bug root cause: untracked connection tasks pinned listener-port sockets past the drain; Windows re-bind WSAEADDRINUSE (Linux tolerates re-bind over non-listening sockets — proven by scripts/rebind_probe.rs). Fix: Shared.conns AbortHandles + end-of-drain force-close + AbortPumpsOnDrop (pumps detach on JoinHandle drop) + bind_with_retry 5s. engine_file_stem FNV identity hash separates same-tag profiles (H2); store_mtx serializes profiles.json RMW (H3). source=src/real_service.rs#connect",
+          "kind": "body"
+        }
+      ]
+    },
+    {
+      "id": "decision:D30",
+      "type": "decision",
+      "title": "D30 QoS fair share + merged Exit&speed panel",
+      "tags": [],
+      "sources": [
+        {
+          "file": "harness/memory/curated/",
+          "anchor": ""
+        }
+      ],
+      "passages": [
+        {
+          "text": "QoS: per-destination groups, 1 s tick refills aggregate/groups only under contention (>=2 groups AND >=2 MiB/s aggregate floor), pump read gate blocks until budget, scheduler ALWAYS attached so the Settings qos_fair toggle flips live (no reconnect). Merged Exit & speed Status panel (view-only merge, D24-safe, both triggers side-by-side). P1 parity: TCP_NODELAY x4 legs, ACK coalescing 8 frames/20ms with CLOSE+PING flush, same-carrier PONG echo (RTT was cross-contaminated), send-path copies 5->3 (encode_session_borrowed, wire-identity test), writev batching (<=16 frames), quarantine last-resort fallback (no sole-link blackout), production prober uses user probe target. source=crates/mt-net/src/qos.rs",
+          "kind": "body"
+        }
+      ]
+    },
+    {
+      "id": "ledger:s21",
+      "type": "ledger",
+      "title": "S21: P0 second-connection fix, perf parity, QoS toggle, merged panel (v0.14.0)",
+      "tags": [],
+      "sources": [
+        {
+          "file": "harness/memory/curated/",
+          "anchor": ""
+        }
+      ],
+      "passages": [
+        {
+          "text": "2026-11-24T21:00Z | s21 | IMPLEMENTED + RELEASED v0.14.0: P0 = tracked connection tasks + end-of-drain force-close + pump-abort guard + bind retry (Windows WSAEADDRINUSE root cause; Linux-tolerant re-bind proven; regression test fails pre-fix) + engine_file_stem FNV identity hash + store_mtx; P1 = TCP_NODELAY x4 legs, ACK coalescing 8/20ms, same-carrier PONG echo, copies 5->3, writev <=16, quarantine last-resort fallback, user probe target; o = QoS fair share (per-destination groups, >=2 groups + >=2 MiB/s floor, live toggle, no reconnect); p = merged Exit & speed panel. 490/0; all 8 gates; release run 45477702090 SUCCESS attempt-3 (2 documented runner-flake legs rerun; logs archived s21-ci-*); assets verified. source=harness/memory/LEDGER.md#s21",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "project:multitunnel",
       "type": "project",
diff --git a/harness/memory/store/memory_graph.json b/harness/memory/store/memory_graph.json
index f5a4fba..8477f2b 100644
--- a/harness/memory/store/memory_graph.json
+++ b/harness/memory/store/memory_graph.json
@@ -1,6 +1,6 @@
 {
   "schema_version": 1,
-  "as_of": "c0d8f15 2026-11-22T04:16:12Z",
+  "as_of": "e510e43 2026-11-24T20:08:15Z",
   "sources": [
     "PROJECT_MEMORY.md",
     "harness/memory/STATE.md",
@@ -11,8 +11,8 @@
     "git (tags + full log)",
     "harness/memory/curated/"
   ],
-  "node_count": 281,
-  "edge_count": 287,
+  "node_count": 290,
+  "edge_count": 298,
   "nodes": [
     {
       "id": "bug:freedom-loopback-blackhole",
@@ -335,6 +335,26 @@
         }
       ]
     },
+    {
+      "id": "commit:1a9849f",
+      "type": "commit",
+      "title": "backup(s21-release-prep): 2026-11-24T20:08:00Z",
+      "tags": [
+        "2026-11-24"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "1a9849f"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s21-release-prep): 2026-11-24T20:08:00Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:1b2fe0b",
       "type": "commit",
@@ -555,6 +575,26 @@
         }
       ]
     },
+    {
+      "id": "commit:3cd904d",
+      "type": "commit",
+      "title": "backup(s21-p0-second-connection): 2026-11-24T18:40:42Z",
+      "tags": [
+        "2026-11-24"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "3cd904d"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s21-p0-second-connection): 2026-11-24T18:40:42Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:3da17ad",
       "type": "commit",
@@ -995,6 +1035,26 @@
         }
       ]
     },
+    {
+      "id": "commit:685e95c",
+      "type": "commit",
+      "title": "backup(s21-features-qos-merged-panel): 2026-11-24T20:07:19Z",
+      "tags": [
+        "2026-11-24"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "685e95c"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s21-features-qos-merged-panel): 2026-11-24T20:07:19Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:6a4bd7c",
       "type": "commit",
@@ -1835,6 +1895,26 @@
         }
       ]
     },
+    {
+      "id": "commit:cccc77a",
+      "type": "commit",
+      "title": "backup(s21-p1-perf-stability): 2026-11-24T18:51:55Z",
+      "tags": [
+        "2026-11-24"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "cccc77a"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s21-p1-perf-stability): 2026-11-24T18:51:55Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:ce6af07",
       "type": "commit",
@@ -2075,6 +2155,26 @@
         }
       ]
     },
+    {
+      "id": "commit:e510e43",
+      "type": "commit",
+      "title": "backup(s21-release-prep-2): 2026-11-24T20:08:15Z",
+      "tags": [
+        "2026-11-24"
+      ],
+      "sources": [
+        {
+          "file": "git:log",
+          "anchor": "e510e43"
+        }
+      ],
+      "passages": [
+        {
+          "text": "backup(s21-release-prep-2): 2026-11-24T20:08:15Z",
+          "kind": "title"
+        }
+      ]
+    },
     {
       "id": "commit:eaaa8c3",
       "type": "commit",
@@ -3271,6 +3371,24 @@
         }
       ]
     },
+    {
+      "id": "decision:D29",
+      "type": "decision",
+      "title": "D29 P0 reconnect force-close + bind retry",
+      "tags": [],
+      "sources": [
+        {
+          "file": "harness/memory/curated/",
+          "anchor": ""
+        }
+      ],
+      "passages": [
+        {
+          "text": "Second-connection bug root cause: untracked connection tasks pinned listener-port sockets past the drain; Windows re-bind WSAEADDRINUSE (Linux tolerates re-bind over non-listening sockets — proven by scripts/rebind_probe.rs). Fix: Shared.conns AbortHandles + end-of-drain force-close + AbortPumpsOnDrop (pumps detach on JoinHandle drop) + bind_with_retry 5s. engine_file_stem FNV identity hash separates same-tag profiles (H2); store_mtx serializes profiles.json RMW (H3). source=src/real_service.rs#connect",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "decision:D3",
       "type": "decision",
@@ -3291,6 +3409,24 @@
         }
       ]
     },
+    {
+      "id": "decision:D30",
+      "type": "decision",
+      "title": "D30 QoS fair share + merged Exit&speed panel",
+      "tags": [],
+      "sources": [
+        {
+          "file": "harness/memory/curated/",
+          "anchor": ""
+        }
+      ],
+      "passages": [
+        {
+          "text": "QoS: per-destination groups, 1 s tick refills aggregate/groups only under contention (>=2 groups AND >=2 MiB/s aggregate floor), pump read gate blocks until budget, scheduler ALWAYS attached so the Settings qos_fair toggle flips live (no reconnect). Merged Exit & speed Status panel (view-only merge, D24-safe, both triggers side-by-side). P1 parity: TCP_NODELAY x4 legs, ACK coalescing 8 frames/20ms with CLOSE+PING flush, same-carrier PONG echo (RTT was cross-contaminated), send-path copies 5->3 (encode_session_borrowed, wire-identity test), writev batching (<=16 frames), quarantine last-resort fallback (no sole-link blackout), production prober uses user probe target. source=crates/mt-net/src/qos.rs",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "decision:D4",
       "type": "decision",
@@ -4828,6 +4964,24 @@
         }
       ]
     },
+    {
+      "id": "ledger:s21",
+      "type": "ledger",
+      "title": "S21: P0 second-connection fix, perf parity, QoS toggle, merged panel (v0.14.0)",
+      "tags": [],
+      "sources": [
+        {
+          "file": "harness/memory/curated/",
+          "anchor": ""
+        }
+      ],
+      "passages": [
+        {
+          "text": "2026-11-24T21:00Z | s21 | IMPLEMENTED + RELEASED v0.14.0: P0 = tracked connection tasks + end-of-drain force-close + pump-abort guard + bind retry (Windows WSAEADDRINUSE root cause; Linux-tolerant re-bind proven; regression test fails pre-fix) + engine_file_stem FNV identity hash + store_mtx; P1 = TCP_NODELAY x4 legs, ACK coalescing 8/20ms, same-carrier PONG echo, copies 5->3, writev <=16, quarantine last-resort fallback, user probe target; o = QoS fair share (per-destination groups, >=2 groups + >=2 MiB/s floor, live toggle, no reconnect); p = merged Exit & speed panel. 490/0; all 8 gates; release run 45477702090 SUCCESS attempt-3 (2 documented runner-flake legs rerun; logs archived s21-ci-*); assets verified. source=harness/memory/LEDGER.md#s21",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "ledger:s3",
       "type": "ledger",
@@ -5507,6 +5661,10 @@
         }
       ],
       "passages": [
+        {
+          "text": "- **Stage**: S21 CLOSED — v0.14.0 released and verified (tag on e510e43, release run 45477702090 SUCCESS on attempt 3, same-commit ci green on both OS legs, assets download-verified). The user's four asks shipped: (1) P0 SECOND-CONNECTION BUG — mid-traffic reconnect re-binds the fixed listener ports while untracked in-flight connection tasks pin their sockets (no SO_REUSEADDR); on Windows the re-bind lands WSAEADDRINUSE → connect Failed → dead tunnel until restart. Linux tolerates the re-bind (std rebind probe), so the regression test asserts the platform-independent observable: the lingering client connection sees EOF after the picker switch (pre-fix FAILS, post-fix passes). Fix: Shared.conns tracks every connection task's AbortHandle; run() force-closes whatever the 3 s drain could not reap; AbortPumpsOnDrop aborts the DETACHED pump children (dropping JoinHandles leaves them running — found when the first EOF assertion failed); bind_with_retry 5 s (BIND_WAIT 2→8 s). H2: engine config/log stems carry a deterministic FNV-1a hash of the full endpoint identity (engine_file_stem) — same-tag different-endpoint profiles can no longer overwrite each other's Xray config (temp probe tunnels corrupting the LIVE engine → phantom respawn → DEGRADED while GUI showed Connected). H3: Core.store_mtx serializes profiles.json read-modify-write.",
+          "kind": "body"
+        },
         {
           "text": "- **Stage**: S20 CLOSED — the S19 leftover (Copilot code review) COMPLETED and shipped as v0.13.1 (tag on 4923566, release run 44983522874 SUCCESS, ci ×2 green, assets download-verified). The cloud Copilot reviewer ran via `POST /repos/maniesh-NR/MultiTunnel/pulls/1/copilot-code-review` (needs only pull-requests:write, which the PAT has): effort ladder per the fallback instruction — max → quota-rejected, balanced → quota-rejected, **lite → ran**. Round 1 (on 52f1d3c): 12 findings (5 High: step-0 self-heal never cleared sysproxy_set → no-snapshot restore disables a user's pre-existing proxy; give_up unserialized with disconnect → black-hole re-installed AFTER an explicit disconnect; set-BEFORE-bind → a foreign port-9 owner receives the blocked traffic; listener stopped even when restore failed → stranded block, no retry, false \"restored\" log; diag header wrote user URLs verbatim → credential leak. 7 Medium: live diag toggle misses engine debug, failed enable leaves the toggle on, probes ride a session that is not the target profile, unbounded head buffer, port-0 edit, unvalidated REALITY edits, pop() = lowest-priority adapter). All 12 fixed (Core::proxy_mtx serializes every proxy hand-off with the bind hoisted out of the lock; bind-before-set with no-set-on-bind- failure; restore-first release + retry; redact_url; diag-on-live reconnect; failed-enable revert; Session.profile_id pins both probes; 64 KiB head cap; port-0 + REALITY structural validation; first-link selection). Round 2 (lite, on the fixes): CLEAN PASS, zero comments.",
           "kind": "body"
@@ -5725,6 +5883,26 @@
         }
       ]
     },
+    {
+      "id": "release:v0.14.0",
+      "type": "release",
+      "title": "Release v0.14.0 (tagged e510e43)",
+      "tags": [
+        "v0.14.0"
+      ],
+      "sources": [
+        {
+          "file": "git:tags/v0.14.0",
+          "anchor": "e510e43"
+        }
+      ],
+      "passages": [
+        {
+          "text": "Release v0.14.0: tag points at commit e510e43.",
+          "kind": "body"
+        }
+      ]
+    },
     {
       "id": "release:v0.2.0",
       "type": "release",
@@ -8159,6 +8337,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (v0.3.1)"
     },
+    {
+      "src": "commit:1a9849f",
+      "dst": "ledger:s21",
+      "rel": "part_of",
+      "evidence": "subject stage label (s21)"
+    },
     {
       "src": "commit:1b2fe0b",
       "dst": "ledger:s6-wave1",
@@ -8213,6 +8397,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s2)"
     },
+    {
+      "src": "commit:3cd904d",
+      "dst": "ledger:s21",
+      "rel": "part_of",
+      "evidence": "subject stage label (s21)"
+    },
     {
       "src": "commit:3da17ad",
       "dst": "ledger:s19",
@@ -8327,6 +8517,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s19)"
     },
+    {
+      "src": "commit:685e95c",
+      "dst": "ledger:s21",
+      "rel": "part_of",
+      "evidence": "subject stage label (s21)"
+    },
     {
       "src": "commit:6a4bd7c",
       "dst": "ledger:s9.1",
@@ -8519,6 +8715,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s13)"
     },
+    {
+      "src": "commit:cccc77a",
+      "dst": "ledger:s21",
+      "rel": "part_of",
+      "evidence": "subject stage label (s21)"
+    },
     {
       "src": "commit:ce6af07",
       "dst": "ledger:s17",
@@ -8579,6 +8781,12 @@
       "rel": "part_of",
       "evidence": "subject stage label (s2)"
     },
+    {
+      "src": "commit:e510e43",
+      "dst": "ledger:s21",
+      "rel": "part_of",
+      "evidence": "subject stage label (s21)"
+    },
     {
       "src": "commit:eb1235a",
       "dst": "ledger:s2-fix",
@@ -8825,12 +9033,36 @@
       "rel": "part_of",
       "evidence": "project decision log"
     },
+    {
+      "src": "decision:D29",
+      "dst": "ledger:s21",
+      "rel": "decided_in",
+      "evidence": "agent-curated"
+    },
+    {
+      "src": "decision:D29",
+      "dst": "project:multitunnel",
+      "rel": "part_of",
+      "evidence": "project decision log"
+    },
     {
       "src": "decision:D3",
       "dst": "project:multitunnel",
       "rel": "part_of",
       "evidence": "project decision log"
     },
+    {
+      "src": "decision:D30",
+      "dst": "ledger:s21",
+      "rel": "decided_in",
+      "evidence": "agent-curated"
+    },
+    {
+      "src": "decision:D30",
+      "dst": "project:multitunnel",
+      "rel": "part_of",
+      "evidence": "project decision log"
+    },
     {
       "src": "decision:D4",
       "dst": "project:multitunnel",
@@ -9239,6 +9471,12 @@
       "rel": "released_in",
       "evidence": "2026-11-22T04:16Z | v0.13.1 RELEASED: tag on 4923566 → release run 44983522874 SUCCESS (~4 min, no cold-cache stall); ci ×2 on main (44983512609/44983518575) ALL GREEN first try. Assets verified by download+unzip: multitunnel-v0.13.1-windows-x64.zip 6,511,395 B integrity OK (multitunnel.exe PE32+ console 13.8 MB + multitunnel-tray.exe PE32+ GUI 2.95 MB + README v0.13.1 section + QUICKSTART v0.13.1 note + server scripts + LICENSE + THIRD_PARTY_NOTICES + config.example) + mt-merge-server {x86_64 1,826,272 B, aarch64 1,860,808 B} ELFs. Verified copies staged in sandbox download/ (v0.13.1-verify-*). S20 CLOSED."
     },
+    {
+      "src": "ledger:s21",
+      "dst": "project:multitunnel",
+      "rel": "part_of",
+      "evidence": "project ledger"
+    },
     {
       "src": "ledger:s3",
       "dst": "project:multitunnel",
@@ -9449,6 +9687,12 @@
       "rel": "built_from",
       "evidence": "git tag v0.13.1"
     },
+    {
+      "src": "release:v0.14.0",
+      "dst": "commit:e510e43",
+      "rel": "built_from",
+      "evidence": "git tag v0.14.0"
+    },
     {
       "src": "release:v0.2.0",
       "dst": "commit:06881a6",

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

( PROJECT_MEMORY.md's own diff is elided here — the durable
  sections above are hand-maintained; see harness/memory/LEDGER.md
  for the full stage history. )
```

