# STATE — rewritten at the end of every stage. Last update: S5 CLOSED (2026-11-16)

## Current position
- **ROADMAP S0–S5 ALL CLOSED under D6 adversarial consensus.** v0.1.0
  shipped 2026-11-13; S5 (v0.2.0, the P2 backlog) closed 2026-11-16:
  4 parallel implementers (A config+CLI, B runtime, C tray, D ship
  hygiene; B's agent crashed post-completion — work verified complete by
  REV-C), round-1 consensus 2×APPROVE, P1 + six P2s applied at close.
  114 tests, all 5 gates clean, Cargo 0.2.0.
- v0.2.0 tag push → release.yml builds the GitHub Release on
  windows-latest (now with fmt+clippy gates + rust-cache).

## Next actions (any future session)
1. Verify the v0.2.0 release run + asset (Releases page / API with the
   upload token); if failed, read the Actions log and fix release.yml.
2. Restore/verify the v0.2.0 zip in sandbox `download/` (v0.1.0 zip is
   already restored, sha256 d9d67fb0…3a6b6).
3. v0.3 backlog (carried P2s): localhost listen inconsistency,
   join_reader 1 s grace output loss, restore-error surfacing
   (`let _ =` callers), snapshot downgrade doc sentence, CI cache
   keying, notices zmij row URL.
4. User manual steps (unchanged): server rollout on the VM
   (`DOMAIN=… bash server/setup-xray-reality.sh`), then the first REAL
   Windows tray smoke test (QUICKSTART step 6).

## Blockers / open items
- Embedded remote-URL token EXPIRED — pushes MUST go through
  backup.sh (reads upload/Multitunnel github access token.txt, verified
  valid 2026-11-16, login maniesh-NR). Never print it.
- Branch protection / required status checks: needs admin rights the
  token may lack — attempt via API, else document as a user step.

## Environment map (sandbox)
- Repo: /home/z/my-project/MultiTunnel — branch main; rescue branch
  rescue/local-stale-2026-11-16 holds a superseded snapshot (deletable).
- Toolchains: `. /home/z/my-project/tools/env.sh` FIRST — venv layout
  (tools/venv: ziglang 0.17.1 + cargo-zigbuild 0.24.1 + pyyaml);
  RUSTUP_HOME=tools/rustup, CARGO_HOME=tools/cargo (rust 1.100.0);
  CARGO_TARGET_DIR=/tmp/mt-target.
- Gates: fmt --check; clippy host; clippy --target x86_64-pc-windows-gnu
  -D warnings; cargo test; zigbuild. YAML check: python3 -c "import
  yaml; …" (pyyaml now in the venv).
- CRITICAL SANDBOX NOTE: tool OUTPUT is filtered — a literal `[main]`
  displays as `ain]`. Verify bracketed literals with `od -An -c`, never
  plain cat/rg/echo. ALSO: the Edit tool's "atomic" MultiEdit can
  PARTIALLY apply before reporting failure (observed 2026-11-16) — after
  any failed MultiEdit, verify the file's actual state before retrying.
