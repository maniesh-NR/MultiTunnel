# S3 Consensus — Interface: tray + icon + release binaries

- Stage: S3 (src/tray.rs raw-win32 tray, real src/bin/tray.rs GUI binary,
  assets/icon.ico + scripts/make_icon.py, +21 tests)
- Protocol: D6 — implementer round, then 2 adversarial reviewers in parallel.

## Implementation (2026-11-13T07:08Z, commit 0fb81f0)

Single implementer agent (S3 has one coherent module — parallel split not
applicable). Design per D4 (raw win32, no tray-icon crate): message-only
STATIC window subclassed via GWLP_WNDPROC with a thread-local context,
menu rebuilt from Snapshot at each open, supervisor ops on worker threads
(op_lock can block ~15 s), embedded ico via bounds-checked RT_ICON
extraction + CreateIconFromResourceEx, tooltip timer, exit = NIM_DELETE →
bounded disconnect wait → WM_QUIT. 77/77 tests, host + windows-target
clippy clean, zigbuild green. Report: agent-reports/s3-implementation.md.

## Review round 1 (07:30Z, on 81c7e72)

- **S2/S3-REV-C (with the ROADMAP's unsafe-FFI mandate): APPROVE** —
  0×P0; 32/32 unsafe blocks audited line-by-line with per-block verdicts;
  2×P1: (1) Exit's 2 s budget can be exceeded → tunnel orphaned with the
  proxy still set; (2) no TaskbarCreated handling → explorer restart
  leaves an unreachable elevated zombie. 7×P2.
- **S3-REV-B: APPROVE** — 0×P0; test stability 13/13 green incl. full CPU
  load and single-threaded timing subsets, 0 flakes; PE import table
  proves the tray FFI linked; icon byte-identical to deterministic
  regeneration; Cargo.toml/lock unchanged. 1×P1: SetForegroundWindow on a
  message-only window may be a no-op (stuck-menu, KB Q135788 pattern) —
  recommended a hidden top-level window. 4×P2.
- Full reports: agent-reports/s3-rev-c.md, s3-rev-b.md.

## P1 fix round (07:50Z, commit 799f6cd)

All three P1s applied by the implementer (resumed):
1. Additive `Supervisor::emergency_stop()` (op_lock-free: stop flag →
   core.stop → sysproxy::restore → Failed) + tray Exit escalation
   2 s → emergency_stop → 1 s → WM_QUIT + host test proving promptness
   under a held op_lock.
2. TaskbarCreated registered + handled → icon re-added on shell restart
   (RegisterWindowMessageW confirmed in the PE import table).
3. Window switched to hidden TOP-LEVEL (NULL parent) so the KB135788
   SetForegroundWindow fix is genuine, with the WM_NULL fallback second
   half on activation failure.
Gates after fixes: 78/78 tests, fmt/clippy host/target clean, zigbuild
green.

## Verdict

**S3 CLOSED — consensus 2×APPROVE on round 1; all three P1s fixed and
re-gated.** Residual risk recorded for S4: the tray has never run on REAL
Windows (sandbox is Linux; cross-build proof only) — the QUICKSTART must
make the first Windows run a deliberate smoke test, and the menu-dismiss /
TaskbarCreated behaviors should be checked visually at first launch.
11+4 P2s carried to the S4 backlog (agent-reports have the full lists).
