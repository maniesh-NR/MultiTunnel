# STATE — rewritten at the end of every stage. Last update: 7th rollback remediated + vless:// importer salvaged onto the frontier (2026-11-17T10:04Z)

## Current position
- **ROADMAP S0–S5 ALL CLOSED under D6 adversarial consensus.** v0.1.0
  shipped 2026-11-13; v0.2.0 closed + released 2026-11-16; v0.2.0 audit
  backport verified + CI green on all legs 2026-11-17 (229fda4).
- 2026-11-17T07:05Z: SIXTH sandbox rollback (same class as the 4th/5th) —
  the session resumed from the stale Nov-12 mid-S2 tree, independently
  re-diagnosed the supervisor-rig hang + hardcoded-restartMax rig bug, went
  green, and committed 3cd1423 on the stale base before the main-push
  rejection exposed the divergence. Remediated (07:10–07:20Z): junk tag
  backup/s2-implementers-fixed deleted from the remote; main hard-reset to
  origin/main ad550fa; stale commit dropped (reflog-only; third independent
  confirmation of the rig-hang root cause); all 5 gates re-verified green
  at ad550fa (112 lib + 8 bin tests, fmt, clippy host + windows-gnu
  -D warnings, release zigbuild PE32+ console+GUI); v0.2.0 zip re-restored
  to download/ (sha256 byte-verified vs ledger).
- 2026-11-17T10:02Z: SEVENTH sandbox rollback (same class) — session resumed
  from the stale Nov-12 mid-S2 tree, violated the RESUME RULE (fetch first),
  re-diagnosed the rig hang + restartMax bug a 4th time, committed 185285c on
  the stale base; tag backup/s2-implementers-import slipped to the remote
  before the push rejection exposed the divergence. Remediated: main reset to
  origin/main c308442; redundant stale fixes dropped; the stale session's
  genuinely-new FEATURE — a vless:// share-link importer — was SALVAGED onto
  the frontier (src/vless.rs, 18 tests + `multitunnel import` command + D10;
  raw-JSON server-block patch via atomic write_private; config.rs Server
  gains PartialEq). All 5 gates re-verified green on the salvage commit
  (fmt, clippy host + windows-gnu -D warnings, 130 lib + 8 bin tests, release
  zigbuild PE32+ x2); smoke-tested fresh import / field preservation / error
  paths. Junk tag deleted local+remote after salvage.

## Next actions (any future session)
1. v0.3 backlog (carried P2s): localhost listen inconsistency,
   join_reader 1 s grace output loss, restore-error surfacing for the
   NON-corrupt `let _ =` callers, snapshot downgrade doc sentence, CI cache
   keying, notices zmij row URL, no-snapshot-disable self-heal UX,
   disconnect latency mid-campaign (~15 s tray UX note). Fold in a review
   of the backport diff (per v02-audit process note).
2. Consider v0.2.1 patch release (tag v0.2.1 → release.yml): the audit
   fixes are user-visible (orphaned xray.exe after Ctrl+C; false-Connected
   + restart churn when a foreign process owns the SOCKS port; corrupt
   snapshot could stick the system proxy). Recommended AFTER the v0.3
   review round covers the backport diff, per D6 discipline.
3. User manual steps (unchanged): server rollout on the VM
   (`DOMAIN=… bash server/setup-xray-reality.sh`), then the first REAL
   Windows tray smoke test (QUICKSTART step 6).

## Blockers / open items
- Embedded remote-URL token EXPIRED — pushes MUST go through
  backup.sh (reads upload/Multitunnel github access token.txt, verified
  valid 2026-11-17, login maniesh-NR). Never print it.
- Branch protection IS set on main (required checks, strict, no
  force-push) since 2026-11-16T15:45Z — the owner token pushes as admin.
  Direct pushes must keep CI green; never force-push.

## Environment map (sandbox)
- Repo: /home/z/my-project/MultiTunnel — branch main @ ad550fa ==
  origin/main (frontier). No rescue branches in this snapshot; stale
  commits are reflog-only (3cd1423 and its backup).
- Toolchains: `. /home/z/my-project/tools/env.sh` FIRST. Tree rebuilt
  2026-11-17T05:35Z per D2 (rust 1.100.0 + zig 0.17.1 tarball +
  cargo-zigbuild 0.24.1 + rustfmt/clippy under tools/).
  CARGO_TARGET_DIR=/tmp/mt-target.
- Gates (all verified green 2026-11-17T05:52Z): fmt --check; clippy host
  --all-targets; clippy --target x86_64-pc-windows-gnu --all-targets
  -D warnings; cargo test (112+8); zigbuild (PE32+ x2).
- CRITICAL SANDBOX NOTES: (1) tool OUTPUT is filtered — a literal `ain]`
  displays as `ain]`; verify bracketed literals with `od -An -c`.
  (2) MultiEdit can PARTIALLY apply before reporting failure — verify the
  file's actual state after any failed MultiEdit. (3) RESUME RULE (5
  incidents now): on ANY session start, `git fetch origin && git log
  --oneline -5 origin/main` FIRST — the local tree may be a rolled-back
  snapshot; origin/main is ALWAYS the frontier. The stale tree's own
  harness/memory files are stale too — do not trust them until synced.
