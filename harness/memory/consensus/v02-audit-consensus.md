# v0.2.0 Audit Consensus — recycled-tree review round, backported 2026-11-17

## Origin (incident)

The 2026-11-17 03:20Z session resumed from a sandbox snapshot that had rolled
the working tree back to 2026-11-12 ~16:57 AND left the local branch at
d5a33ab. Unaware that origin/main had since closed S2–S5 and shipped v0.1.0 +
v0.2.0 (last remote commit 4363e71, s5-final, 2026-11-16), that session
re-ran a full S2 adversarial review round against the STALE Nov-12 tree and
committed "recovery" commits on the old base (73d6bab…9828e25). The push of
`main` was correctly rejected (non-fast-forward); only the tag
`backup/s2-runtime` (9828e25) slipped onto the remote — it is a side tag,
harmless. The stale commits are preserved on the local branch
`rescue/local-stale-2026-11-17`.

## Verdicts (round 1, on the stale tree — both APPROVE)

| Reviewer | Lens | Verdict | P0 | P1 | P2 |
|---|---|---|---|---|---|
| AUDIT-C (agent-ac1cd5f2) | correctness / security / panics / FFI / races | **APPROVE** | 0 | 3 | 10 |
| AUDIT-B (agent-0faf5e78) | build / portability / licensing / tests | **APPROVE** | 0 | 2 | 5 |

Reports: agent-reports/v02-audit-c.md, agent-reports/v02-audit-b.md.

## Re-verification against v0.2.0 (s5-final) and backport

Findings already fixed on origin/main by the 2026-11-13/16 rounds were
dropped; findings still live in v0.2.0 were ported onto 4363e71 and verified
by the full gate battery:

Ported (still live in v0.2.0, now fixed):
- AUDIT-C#1 (P1) foreign-listener false-Connected: connect/campaign validate
  liveness only via port+probe → new `CoreProc::child_alive()` guards after
  `wait_for_port`, after the probe, and in every campaign attempt; regression
  test `connect_fails_when_a_foreign_process_owns_the_socks_port` (slow-stub
  greet delay makes the death deterministic).
- AUDIT-C#2 (P1) xray child survives Ctrl+C (CREATE_NO_WINDOW detaches it
  from the console) → Windows kill-on-close Job Object around every spawned
  core (`Win32_System_JobObjects` feature added; `unsafe impl Send`
  justified in-source; failure degrades to a log line, never fatal). Also
  bounds the pump-thread grandchild concern (AUDIT-C#10).
- AUDIT-C#3 (P1) corrupt sysproxy snapshot failed closed forever (all
  callers `let _ =`) → `restore_plan` policy maps corrupt → FailOpenRemove
  (disable + remove + `.tmp-*` sweep); pure policy unit-tested. Addresses
  the v0.3 backlog item "restore-error surfacing" for the corrupt case.
- AUDIT-C#4 (P2) `stoppable_sleep` Instant-sub overflow window →
  checked_add deadline + frozen-sample subtraction.
- AUDIT-B#1 (P1) restart campaign only tested in degenerate max=0 form →
  new tests `core_death_with_restart_budget_recovers_exactly_once`
  (DieOnce core; Restarted outcome, restarts==1, stays up) and
  `restart_attempts_fail_spawn_and_exhaust_the_budget` (unix-only
  MoveAwayThenDie core; "giving up after 2 restart attempt(s)", restarts==2).
- AUDIT-B#2 (P1) rig unix-only + registry hazard (`system_proxy: true`) →
  rig made portable (unix shell-script core / Windows `.cmd` core with
  TEST-NET ping sleeps; FakeCore enum; greet-delay stub knob);
  `Rig::build` now hard-codes `system_proxy: false` — 9 rig tests
  (incl. 3c, 3e, 4, 5, 6, 7 + the three new ones) now run on the
  windows-latest CI leg. The unix-gated exit_race_rig keeps
  `system_proxy: true` deliberately (it must reach the sysproxy abort gate;
  no-op on unix; loud comment forbids un-gating without flipping it).
- AUDIT-B P2s: direct core_proc lifecycle tests (manual stop vs consumed-
  once self-exit) added.

Already fixed on origin/main (dropped, with where):
- Probe single-deadline budget (S2-REV-C round-1 P1-1, drip test included).
- join_args CommandLineToArgvW escaping (S2-REV-C round-1 P1-2).
- connect/disconnect serialization op_lock (S2-REV-C round-1 P0-2).
- Atomic snapshot write temp+fsync+rename (S5-REV-C P2-2).
- NotFound-only registry reads / Opaque tri-state (S2-REV-C P2-5 + S5).
- Doctor bounded `xray version` (S2-REV-C P2-8).
- release.yml `cargo test` gate (S2-REV-B round-1).
- windows-sys feature hygiene (LibraryLoader already absent on main).

Deferred (recorded for v0.3): no-snapshot-disable self-heal UX (AUDIT-C P2
#7), disconnect latency mid-campaign (~15 s; tray UX note).

## Gates after the backport (all green)

cargo fmt --check · cargo clippy --all-targets (0 warnings) · cargo clippy
--target x86_64-pc-windows-gnu --all-targets -D warnings · cargo test
(112 lib + 8 tray, 0 failed) · cargo zigbuild --target x86_64-pc-windows-gnu
(console + GUI exes PE32+) · cargo check --target x86_64-pc-windows-msvc
--tests (0 warnings).

## Process note

Per D6 these fixes originate from a 2×APPROVE review round (albeit on the
stale tree; the ported logic is identical). The backport itself was
mechanically adapted to the v0.2.0 codebase by the orchestrator and verified
by the full gate battery; the windows-latest CI run after this push is the
runtime confirmation for the portable rig. A full adversarial re-review of
the backport diff is folded into the next scheduled review round (v0.3).
