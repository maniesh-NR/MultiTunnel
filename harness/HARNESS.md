# MultiTunnel Harness

This directory is the **persistent brain** of the MultiTunnel project. The
development sandbox may be recycled and roll back at any moment; everything
needed to resume — protocol, roadmap, decisions, event ledger, agent reports
— lives here and is pushed to GitHub after every stage (see `backup.sh`).

## Layout

```
harness/
├── HARNESS.md            this file — the protocol
├── ROADMAP.md            staged roadmap with acceptance criteria + status
├── backup.sh             backup workflow (run at the END OF EVERY STAGE)
└── memory/
    ├── STATE.md          current stage, next actions, blockers  (overwrite-style)
    ├── LEDGER.md         append-only event log (one line per event)
    ├── DECISIONS.md      decision records (D1, D2, ... immutable once written)
    ├── consensus/        per-stage consensus verdicts (S1-consensus.md, ...)
    └── agent-reports/    saved implementer/reviewer reports (stage-agent.md)
```

## Agent protocol (parallel adversarial development)

Every roadmap stage after S0 follows the same loop:

1. **Implement** — 2+ implementer agents run IN PARALLEL on disjoint modules
   (interfaces fixed by the orchestrator in advance to avoid conflicts). Each
   agent must compile and test its code (`cargo test`) before reporting.
2. **Adversarial review** — 2 reviewer agents run IN PARALLEL with distinct
   lenses:
   - Reviewer-C: correctness, security, panics, unsafe FFI, races, error paths
   - Reviewer-B: build/portability (windows-gnu vs msvc), dependency hygiene,
     licensing, test quality
3. **Consensus gate** — each reviewer returns `APPROVE` or `BLOCK` with
   file:line findings. The stage advances ONLY when both return APPROVE.
   BLOCK findings are fixed and re-reviewed. After 3 failed rounds the
   orchestrator may arbitrate; the rationale is recorded in
   `memory/consensus/Sx-consensus.md`.
4. **Backup** — `harness/backup.sh <stage>` commits + tags + pushes.
5. **Memory update** — STATE.md rewritten, LEDGER.md appended, reports saved.

## Backup / resilience contract

- `harness/backup.sh <label>` runs at the end of EVERY plan/stage.
- It always creates a local commit + tag `backup/<label>`.
- Push happens when credentials exist: `GH_TOKEN` in the environment (or in
  `upload/Multitunnel github access token.txt`, read into the env one-shot
  and never printed). The push URL embeds the token ONLY for the single push
  invocation; push logs are scrubbed of `x-access-token:<secret>`.
- Releases: tags `v*` trigger `.github/workflows/release.yml` (builds on a
  real Windows runner, attaches the zip to a GitHub Release).

## Toolchain bootstrap (sandbox note)

Toolchains live OUTSIDE the repo at `/home/z/my-project/tools/`:

```sh
. /home/z/my-project/tools/env.sh
cargo zigbuild --target x86_64-pc-windows-gnu   # windows exe cross-check
cargo test                                      # host tests
```

Rust stable (rustup) + zig 0.17.x + cargo-zigbuild, no root required
(DECISIONS.md D2). If the sandbox reverts the tools tree, reinstall per D2 —
the repo itself must never depend on the sandbox.

## Sandbox output-filter artifact (IMPORTANT for all agents)

The sandbox post-processes tool OUTPUT and rewrites certain bracketed
patterns: a literal `[main]` in command output is DISPLAYED as `ain]` (the
`[m` is eaten). This is an artifact of the display pipeline — it has already
produced one phantom reviewer P1 ("corrupted ci.yml trigger", S4 round 1)
and one self-inflicted sed corruption while "fixing" the phantom. Rules:

- NEVER "fix" bracketed literals (`[main]`, `[S2]`, …) based on plain
  `cat`/`rg`/`echo` output — verify the BYTES first: `sed -n '4p' FILE | od -An -c`.
- `od`/`hexdump` output passes through unmodified and shows the truth.
- Files on disk and git objects are NOT corrupted by the filter — it only
  rewrites what tools print.
