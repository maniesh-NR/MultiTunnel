# STATE — rewritten at the end of every stage. Last update: S5 OPEN (2026-11-16)

## Current position
- **S0–S4 CLOSED** (v0.1.0 released on GitHub, release run 43391931515
  green, zip asset verified). Post-release recovery session (2026-11-16):
  the sandbox was recycled a 4th time — `download/` deliverables, the
  Go-era `bondtunnel/` prototype (superseded by D1, design lives on in
  DECISIONS/contracts) and the whole toolchain tree were wiped; the repo
  itself was intact but STALE (a pre-recovery S2-era snapshot, superseded
  by origin/main 827e655 — preserved on branch
  `rescue/local-stale-2026-11-16`, then hard-reset to origin/main).
- Toolchain rebuilt per D2 (rust 1.100.0, zig 0.17.1 + cargo-zigbuild
  0.24.1 in `tools/venv`, windows-gnu target; env.sh rewritten for the
  venv layout). All 5 gates re-verified green at 827e655 (78/78 tests).
- v0.1.0 zip re-downloaded from the GitHub Release into sandbox
  `download/` (sha256 d9d67fb03e21cec1bafef1b1d0f773755e6b8aa3de1e4a98ed1348ecc4a3a6b6).
- **S5 (v0.2.0 P2 backlog) OPEN**: contracts fixed in
  `harness/CONTRACTS-S5.md`; orchestrator pre-staged `src/fsutil.rs`
  (atomic writes, +5 tests → 83) and made
  `supervisor::ensure_loopback_listen` pub(crate).

## Next actions
1. Launch implementers S5-A/B/C/D IN PARALLEL (disjoint files per
   CONTRACTS-S5.md); `harness/backup.sh s5-<x>` immediately after each
   agent report lands (D7 hardening).
2. Orchestrator: Cargo.toml → 0.2.0 + all 5 gates.
3. Adversarial review round: S5-REV-C (correctness/races/FFI) +
   S5-REV-B (build/portability/CI/docs) in parallel; consensus per D6.
4. Fix findings, close S5 (ROADMAP [x], consensus file, STATE/LEDGER).
5. Tag `v0.2.0` → release.yml; verify the run + assets; restore the zip
   to `download/`; attempt branch protection (required status checks) via
   API — if the token lacks admin rights, add the one-line user step to
   the README and record it as dismissed-with-reason.
6. Remaining manual user steps (unchanged): server rollout
   (`DOMAIN=… bash server/setup-xray-reality.sh` on the VM) + first REAL
   Windows tray smoke test (QUICKSTART step 6).

## Blockers / open items
- Token in the git remote URL is EXPIRED (API: Bad credentials) — do not
  rely on `git push origin` directly; backup.sh pushes with the still-valid
  token from `upload/Multitunnel github access token.txt` (verified working
  2026-11-16, login maniesh-NR). Consider stripping the stale token from
  the remote URL (cosmetic; fetch works unauthenticated on the public repo).
- Branch protection (required status checks) may need admin rights the
  token does not have — see Next actions 5.

## Environment map (sandbox)
- Repo: /home/z/my-project/MultiTunnel — branch main @ origin/main; rescue
  branch rescue/local-stale-2026-11-16 holds the superseded stale snapshot.
- Toolchains: `. /home/z/my-project/tools/env.sh` FIRST — venv layout:
  RUSTUP_HOME=tools/rustup, CARGO_HOME=tools/cargo, PATH starts with
  tools/venv/bin (zig 0.17.1 wheel + cargo-zigbuild 0.24.1).
  CARGO_TARGET_DIR=/tmp/mt-target.
- Gates: fmt --check; clippy host; clippy --target x86_64-pc-windows-gnu
  -D warnings; cargo test; zigbuild.
- CRITICAL SANDBOX NOTE: tool OUTPUT is filtered — a literal `[main]`
  displays as `ain]`. Verify bracketed literals with `od -An -c`, never
  plain cat/rg/echo (HARNESS.md documents the incident).
