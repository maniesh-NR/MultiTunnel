# S5 consensus — CLOSED 2026-11-16

## Verdicts (round 1, on 40ee65c)

- **S5-REV-C (correctness/security/races): APPROVE** — 0 P0, 1 P1, 7 P2.
  All five gates independently green; suite 3x no flakes. S5-B's
  never-reported work (agent crashed post-completion) fully re-derived
  against the contract: legacy snapshot test pins the REAL v0.1.0 on-disk
  format (checked via git show, not self-referential); exit-race gates
  verified at both boundaries; ATYP byte-exact incl. reply drains; pump
  mutex deadlock-free; tray SAFETY 34/34; icon pin cross-checked.
- **S5-REV-B (build/portability/CI/docs): APPROVE** — 0 P0, 0 P1, 8 P2.
  Gates green from a cold target dir; run_bounded cfg boundary correct for
  both MSVC and gnu legs; no new FFI/dependencies; QUICKSTART zip list
  matches release.yml packaging item-by-item; server 0640 root:nogroup
  accurate for Ubuntu.

**Consensus reached per D6 (both APPROVE, round 1).**

## Findings applied at close (orchestrator)

- **P1 (REV-C) — sysproxy Opaque tri-state left OUR values, not the user's:**
  (a) `EnableAction::Opaque → Leave` could leave ProxyEnable=1 after
  disconnect (silent blackhole; regression vs v0.1.0's restore-0) — fixed:
  `Opaque → Write(0)` (v0.1.0 fail-safe restored, enum collapsed to a
  single Write variant, restore destructures). (b) `set()`'s unconditional
  REG_SZ writes clobbered exotic-typed user values before restore could
  protect them — fixed: `set()` REFUSES (clear error naming the values,
  connect continues without proxy per the non-fatal contract) when
  ProxyServer/ProxyOverride is Opaque; AutoConfigURL is exempt (set never
  writes it). +2 tests (refusal matrix incl. exemptions; enable fail-safe).
- **P2-2 (REV-C) — save_snapshot not atomic:** now temp+fsync+rename via
  fsutil (keep-first semantics kept: existing snapshot never overwritten;
  safe under the op_lock single-writer invariant). +1 test (no temp
  residue).
- **P2-1 (REV-C) — residual check-then-act exit-race window:** new
  `undo_if_stopped` post-side-effect companion to the pre-gates — a late
  stop now reverts BOTH effects best-effort (net-merge off + proxy restore
  + core stop, flags cleared) and aborts; wired after the net-merge and
  sysproxy steps. +1 unix-gated test.
- **P2-5 (REV-C) — pump doc overpromised:** softened to per-write
  exclusivity (logical lines spanning two pipe reads can still interleave;
  log cosmetics only).
- **P2-6 (REV-B) — slow-child test orphaned the 30 s sleep:** `exec sleep`
  so the deadline kill reaps the sleeper itself.
- **P2-4 (REV-B) — README/QUICKSTART only best-effort in Package step:**
  now hard-required + plain copy (QUICKSTART step 0 promises them).
- **QUICKSTART example version** v0.3.0 → v0.2.0.

## Findings dismissed with reason

- REV-C P2-3 (`listen: "localhost"` passes validate but connect's
  SocketAddr parse fails): pre-existing, default config uses 127.0.0.1;
  recorded for the v0.3 backlog.
- REV-C P2-4 (join_reader 1 s grace output loss) / REV-B P2-5 (same):
  bounded, documented, doctor-cosmetic only.
- REV-B P2-1/2/3 (snapshot downgrade corner doc sentence, rust-cache tag
  perf, zig cache keying): perf/docs-only; downgrade behavior is
  fail-safe (v0.1.0 errors on the unknown shape and its callers already
  `let _`-drop; deleting state/sysproxy.json while proxy-off resets).
- REV-B P2-7 (notices zmij row URL): pre-existing cosmetic.
- REV-B P2-8 (lockfile v4→v3 by generate-lockfile): harmless, deps
  unchanged.
- REV-C P2 fsutil same-pid temp collision retry: single-writer invariants
  make concurrent same-path writers unreachable.

## Close state

- 114 tests (106 lib + 8 bin) green; fmt + host clippy + windows-target
  clippy + zigbuild clean; both exes PE32+ (console + GUI).
- Cargo 0.2.0. Full reports: harness/memory/agent-reports/s5-rev-{b,c}.md.
- Residual carried to the backlog: localhost inconsistency, join_reader
  grace, restore-error surfacing (all P2, see reports).
