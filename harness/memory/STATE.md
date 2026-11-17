# STATE — rewritten at the end of every stage. Last update: S6 waves 1–3 implemented and pushed (2026-11-17T17:20Z)

## Current position
- **ROADMAP S0–S5 ALL CLOSED under D6 adversarial consensus.** v0.1.0
  shipped 2026-11-13; v0.2.0 closed + released 2026-11-16; v0.2.0 audit
  backport verified + CI green 2026-11-17 (229fda4).
- **S6 (v0.3 All-in-One) IMPLEMENTED THROUGH ALL THREE WAVES — review +
  release pending.** Frontier = a9f7aae (s6-wave3-root-wiring):
  - Wave 1 (1b2fe0b): mt-links 64 tests; mt-net 44 tests (real fixes:
    bare-IPv6 authority rejected, IPv4-literal CONNECT authority, serde
    camelCase rename_all, headless-request parse, quarantine recovery);
    mt-m3 12 tests (material-colors 0.5.0, real HCT, feature `iced`);
    mt-engine 9 tests (fake_engine.py, backoff respawn, kill-on-drop);
    mt-platform 25 tests (dirs, locator order, zip install, autostart,
    HttpFetch fixtures).
  - Wave 2 (5ea0b27): mt-app — iced 0.13 + tiny-skia Material You GUI,
    18 tests, zigbuild PE32+ link proof (renderer pick documented in its
    Cargo.toml; MockService under default feature `mock`).
  - Wave 3 (a9f7aae): root wiring — src/real_service.rs (RealService over
    ProfileStore + ProxyServer + engines + sysproxy + settings.json),
    src/profiles.rs, `multitunnel gui` (mt-app with RealService) +
    `profiles list/add/use/remove/export`; v0.2 commands unchanged.
    Root 140 lib + 8 bin tests; workspace total 320, all green.
  - Smoke-tested: `profiles add/list/use` on the synthetic REALITY vector;
    `gui --smoke` exits 0 headless ("6 ticks, 0 events, 6 views").
  - Windows release zigbuild: multitunnel.exe (console), multitunnel-tray.exe
    (GUI), mt-app.exe — all PE32+.
- 2026-11-17T~14:40Z: NINTH sandbox rollback (same class as 4th–8th) —
  session resumed from the stale Nov-12 tree, rebuilt the toolchain,
  re-diagnosed the supervisor-rig hang a 5th time (stoppable-stub fix +
  restartMax rig param), committed 644af70 + junk tag on the stale base.
  Remediated: tag deleted local+remote; main reset to d721cba; stale commit
  reflog-only; gates re-verified on the frontier. LESSON (recorded again):
  RESUME RULE — `git fetch origin && git log --oneline -5 origin/main`
  FIRST on every session start, before ANY work.

## Next actions (S6 stage close, in order)
1. D6 adversarial review round on a9f7aae: REV-C (correctness/security —
   audit RealService connect flow, credential redaction in new error paths,
   engine lifecycle, zip-slip/SHA256SUMS, MT_EMBED_ENGINE) + REV-B
   (build/portability/license — 3 exe zigbuild, CI --workspace legs,
   third-party notices for iced/material-colors/reqwest/zip, feature-gate
   matrix). Reports → harness/memory/agent-reports/s6-rev-{c,b}.md;
   consensus → consensus/S6-consensus.md.
2. Fix findings; fold the v0.3 backlog P2s carried from v0.2 (localhost
   listen inconsistency, join_reader grace loss, restore-error surfacing,
   snapshot downgrade doc, CI cache keying, no-snapshot-disable UX,
   disconnect-latency tray note) if free.
3. Docs: README + QUICKSTART for the all-in-one (gui command, profiles
   CLI, engine self-provisioning, autostart, theme seeds); THIRD_PARTY
   NOTICES refresh (iced, tiny-skia, material-colors, reqwest/rustls, zip,
   sha2, base64/uuid/url/percent-encoding additions).
4. Release v0.3.0: bump version, tag, let release.yml build; verify asset;
   restore zip to sandbox download/.
5. User manual steps: real-Windows run of `multitunnel gui` (first REAL
   display smoke of the Material You window) + tray coexistence check;
   server rollout unchanged (server/setup-xray-reality.sh).

## Blockers / open items
- None blocking. GitHub pushes MUST go through harness/backup.sh (token in
  upload/, never printed; branch protection active since 2026-11-16 — no
  force-push, CI must be green).
- Agent context deadline (~50 min wall) killed 4 of 6 expert agents mid-run;
  every one left complete-or-near-complete work that was verified and
  reconciled by the orchestrator/finisher. For future waves: budget ~40 min
  of agent work per task and split bigger scopes.

## Environment map (sandbox)
- Repo: /home/z/my-project/MultiTunnel — main @ a9f7aae == origin/main.
- Toolchain: `. /home/z/my-project/tools/env.sh` FIRST. Persisted under
  /home/z/my-project/tools/ (rustup 1.100.0 minimal + rustfmt + clippy +
  x86_64-pc-windows-gnu target; cargo-zigbuild 0.24.1; zig via pip wheel).
  **zig MUST be ziglang==0.14.1** — the 0.17.1 wheel breaks cargo-zigbuild
  ("Failed to parse `zig cc -v` output"); env.sh pins PYTHONPATH to
  tools/pypkgs. rustfmt/clippy components re-add after rebuilds:
  `rustup component add rustfmt clippy`.
- DISK: / has ~10G. Iced debug builds + incremental are ~5G per target dir —
  `rm -rf /tmp/target-*` between phases; prefer release zigbuild for proofs.
- Gates (all green at a9f7aae): fmt --all --check; clippy --all --all-targets
  (host + windows-gnu via scripts/mingw-zig-shims.sh, -D warnings);
  cargo test (320 total); zigbuild --release --bins (+ -p mt-app) PE32+ ×3.
- CRITICAL SANDBOX NOTES (unchanged): tool output filtered (verify bracketed
  literals with od); MultiEdit can PARTIALLY apply before reporting failure;
  RESUME RULE — fetch origin/main FIRST on session start.
