# STATE — rewritten at the end of every stage. Last update: v0.2.0 audit backport (2026-11-17)

## Current position
- **ROADMAP S0–S5 ALL CLOSED under D6 adversarial consensus.** v0.1.0
  shipped 2026-11-13; v0.2.0 (S5, the P2 backlog) closed 2026-11-16;
  v0.2.0 released (asset verified).
- 2026-11-17: FOURTH sandbox rollback incident — a stale Nov-12 tree was
  mistaken for the frontier and re-reviewed; caught at push time (branch
  protection + non-fast-forward). The round was salvaged as the **v0.2.0
  audit**: 5 still-live findings + rig portability backported onto
  s5-final and verified by all gates (consensus/v02-audit-consensus.md).
  Post-backport suite: 112 lib + 8 bin tests, fmt/clippy(host+gnu)/
  zigbuild/msvc-check clean. Stale commits: rescue/local-stale-2026-11-17
  (+ remote side tag backup/s2-runtime — both deletable once cold).

## Next actions (any future session)
1. Confirm the post-backport ci.yml run on main went green — especially the
   **windows-latest leg**, which now RUNS 9 previously-unix-only rig tests
   (portable .cmd fake cores; first real runtime execution on Windows).
   If the .cmd timing assumptions fail on the runner, fix write_fake_xray.
2. v0.3 backlog (carried P2s): localhost listen inconsistency,
   join_reader 1 s grace output loss, restore-error surfacing for the
   NON-corrupt `let _ =` callers (the corrupt-snapshot case is now
   fail-open — v02-audit REV-C#3), snapshot downgrade doc sentence, CI
   cache keying, notices zmij row URL, no-snapshot-disable self-heal UX
   (v02-audit REV-C P2-7), disconnect latency mid-campaign (~15 s tray UX
   note). Fold in a review of the backport diff (per v02-audit process
   note).
3. Consider v0.2.1 patch release once CI confirms (tag v0.2.1 →
   release.yml; the audit fixes are user-visible: orphaned xray after
   Ctrl+C, false-Connected on port conflicts).
4. User manual steps (unchanged): server rollout on the VM
   (`DOMAIN=… bash server/setup-xray-reality.sh`), then the first REAL
   Windows tray smoke test (QUICKSTART step 6).

## Blockers / open items
- Embedded remote-URL token EXPIRED — pushes MUST go through
  backup.sh (reads upload/Multitunnel github access token.txt, verified
  valid 2026-11-16, login maniesh-NR). Never print it.
- Branch protection IS set on main (required checks, strict, no
  force-push) since 2026-11-16T15:45Z — the owner token pushes as admin.
  Direct pushes must keep CI green; never force-push.

## Environment map (sandbox)
- Repo: /home/z/my-project/MultiTunnel — branch main; rescue branches
  rescue/local-stale-2026-11-16 and rescue/local-stale-2026-11-17 hold
  superseded snapshots (deletable).
- Toolchains: `. /home/z/my-project/tools/env.sh` FIRST. NOTE: the Nov-17
  rollback wiped tools/venv; the tree was rebuilt MINIMAL (rust 1.100.0 +
  zig 0.17.1 tarball + cargo-zigbuild 0.24.1 under tools/, rustfmt+clippy
  components added 2026-11-17). pyyaml for YAML checks: use system
  python3 if available, else reinstall the wheel.
  CARGO_TARGET_DIR=/tmp/mt-target. msvc std target installed (compile
  checks only).
- Gates: fmt --check; clippy host --all-targets; clippy --target
  x86_64-pc-windows-gnu --all-targets -D warnings; cargo test; zigbuild;
  (bonus) cargo check --target x86_64-pc-windows-msvc --tests.
- CRITICAL SANDBOX NOTE: tool OUTPUT is filtered — a literal `[main]`
  displays as `ain]`. Verify bracketed literals with `od -An -c`, never
  plain cat/rg/echo. ALSO: the Edit tool's "atomic" MultiEdit can
  PARTIALLY apply before reporting failure (observed 2026-11-16 AND
  2026-11-17) — after any failed MultiEdit, verify the file's actual
  state before retrying.
- RESUME RULE (learned the hard way, 4th incident): on any session start,
  `git fetch origin && git log --oneline -5 origin/main` FIRST — the
  local tree may be a rolled-back snapshot; origin/main is the frontier.
