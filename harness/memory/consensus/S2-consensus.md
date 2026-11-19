# S2 Consensus — Runtime: supervisor + sysproxy + elevate

- Stage: S2 (core_proc + supervisor + sysproxy + elevate + doctor/connect CLI)
- Opened: 2026-11-12T15:31Z (implementers S2-A/S2-B); interrupted by INCIDENT-2
  and RECOVERY-3; resumed and verified 2026-11-13T05:12Z–06:40Z
- Protocol: D6 (two adversarial reviewers, both must APPROVE; BLOCK → fix →
  re-review, max 3 rounds)

## Round 1 (2026-11-13T06:05Z) — 59d7bba

- **S2-REV-C (correctness/security): BLOCK** — 2×P0, 3×P1, 9×P2.
  - P0-1 core_proc::start() joined the live child's waiter before the slot
    check → indefinite hang then silent double-start (empirically demo'd).
  - P0-2 connect()/disconnect() not serialized as whole operations → a
    disconnect during an in-flight connect returned Ok in 44 ms while the
    sequence finished into Connected with the system proxy re-set (demo'd).
  - P1-1 socks_probe had no total deadline (byte-dripping proxy unbounded).
  - P1-2 elevate::join_args did not escape quotes/backslash-before-quote.
  - P1-3 local.listen never validated loopback (noauth SOCKS on 0.0.0.0).
  - Full report: agent-reports/s2-rev-c.md
- **S2-REV-B (build/portability/license/tests): APPROVE** — 0×P0, 3×P1,
  7×P2. Independently confirmed the serialization gap (same as P0-2) and
  flagged: v* tags shipped untested; THIRD_PARTY_NOTICES.md referenced by
  LICENSE but missing; windows-sys feature list verified complete; both PE
  subsystems verified; license graph clean (no copyleft).
  - Full report: agent-reports/s2-rev-b.md

## Fix round (06:10Z–06:40Z) — eb1235a

All P0/P1 findings fixed; both residual reviewer P1s applied at close:
- start(): slot check first; leftover waiter joined only when is_finished().
- Supervisor op_lock serializes whole connect()/disconnect(); spawn_loop
  joins the exiting loop instead of racing the loop_running window.
- socks_read_exact rewritten deadline-aware (io read timeouts are
  per-underlying-syscall; re-arming per read_exact is NOT sufficient —
  first fix attempt was wrong, caught by the new drip test).
- elevate: full CommandLineToArgvW quoting (quote_windows_arg).
- connect_sequence: ensure_loopback_listen guard (contract #2).
- release.yml runs cargo test before build; THIRD_PARTY_NOTICES.md authored
  (and corrected per round-2: serde_core row, full MIT text); tray stub
  panic-safe; netmerge spawns CREATE_NO_WINDOW; core_proc stub
  nonblocking-accept with 10 s cap + forced-blocking accepted socket;
  ci.yml lint job + windows-target clippy in cross-check; Cargo.toml
  repository + rust-version = "1.78"; fail_on_unmatched_files.
- 5 new tests (52 → 57), incl. serialization, drip-deadline, start-twice,
  loopback, argv quoting.
- Incident note: s2-fix-round-1 (9fafc6c) was pushed prematurely with 2
  failing tests (orchestrator gate-script `rg||echo` pitfall) — superseded
  by eb1235a within minutes; process rule recorded (gates must gate the
  push, not just print).

## Round 2 (06:40Z) — eb1235a

- **S2-REV-C: APPROVE** — 0×P0, 1 residual P1 (concurrent external
  start() in the ~50 ms post-death window; unreachable via Supervisor;
  one-line hardening suggested), 11×P2. All round-1 findings confirmed
  fixed (P0-1, P0-2, P1-1, P1-2, P1-3, P2-1).
  - Full report: agent-reports/s2-rev-c-round2.md
- **S2-REV-B: APPROVE** — 0×P0, 1 residual P1 (Winsock does not document
  nonblocking inheritance for accept()ed sockets → stub could
  WouldBlock-race on the windows-latest CI leg; test-only), 5×P2
  (notices completeness, generate/doctor bypass the loopback guard,
  disconnect may block ~15 s behind an in-flight connect — documented for
  S3, lint only covers Linux surface → fixed, process note on the gate
  incident). All round-1 findings confirmed fixed.
  - Full report: agent-reports/s2-rev-b-round2.md
- Both residual P1s were applied immediately after the verdicts (start()
  joins only finished waiters; stub forces the accepted socket blocking),
  plus the notices + cross-target-lint P2s. Final gate: 57/57 tests ×2,
  host clippy clean, **windows-target clippy clean**, zigbuild green,
  both binaries PE32+.

## Verdict

**S2 CLOSED — consensus 2×APPROVE (round 2), P0 count 0, both reviewers
confirmed every round-1 finding fixed.** Residual P2 backlog carried into
S3/S4 (see STATE.md).

P2 backlog (carried forward, from both round-2 reports):
- config::validate should also enforce loopback listen (S1-module
  follow-up; generate/doctor currently bypass the supervisor guard).
- S3 tray MUST call connect()/disconnect() from a worker thread — ops can
  block up to ~15 s (op_lock behind an in-flight connect).
- sysproxy snapshot read uses .ok() (type-mismatch deletes a user value);
  xray-client.json writes should be temp+rename; probe target lacks IPv6
  literal support; doctor `xray version` lacks a timeout; pump threads can
  interleave log lines; `--config=path` equals-form unsupported.
- CI: consider caching; consider required status checks on main.
