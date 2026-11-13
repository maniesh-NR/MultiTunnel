# S4 Consensus — Ship: server script, docs, release

- Stage: S4 (server/setup-xray-reality.sh + README.md + QUICKSTART.md +
  release packaging)
- Protocol: D6 — implementer round, then 2 adversarial reviewers in parallel.

## Implementation (2026-11-13T08:00Z, commit 56340a5)

Single implementer agent. The script was validated beyond the norm: bash -n,
14 argument-validation paths, x25519 parse against BOTH current and legacy
xray output formats (with the real v26.11.9 binary), the exact generated
config passed `xray run -test` ("Configuration OK"), tag_name parse proven
on compact+spaced API JSON, client block verified field-for-field against
config.example.json. Deviations (both sensible): public-IP detection
prefers GCP metadata over ipify (ipify would report the ProtonVPN exit IP);
QUICKSTART adds geoip.dat/geosite.dat to the bin\ requirements (the client
routing rule needs them). Report: agent-reports/s4-implementation.md.

## Review round 1 (08:05Z, on 56340a5)

- **S4-REV-C: APPROVE** — 0×P0. "The installer is injection-proof, verifies
  everything it downloads and generates, fails loudly rather than
  half-working, never prints the private key, and cannot replace a working
  endpoint with a broken config (the -test gate)." 4×P1: three false doc
  claims (restartMax=0 semantics; net-merge PATH resolution; server.address
  fallback) + one ci.yml trigger "corruption".
- **S4-REV-B: APPROVE** — 0×P0. Release-readiness audited end-to-end;
  WINDOWS_LEG_TESTS: 65 of 78 run on the windows-latest leg (pure/IO/loopback
  only, no timing-sensitive, S2 hardening in place — no runner risk). 3×P1:
  restartMax doc inversion (+ stale stage jargon in the console binary's
  tray message), release zip omits server/setup-xray-reality.sh (QUICKSTART
  step 1's prerequisite), plus the same doc issues.
- Full reports: agent-reports/s4-rev-c.md, s4-rev-b.md.

## P1 disposition (08:15Z, commit d48f32f)

Applied: README restartMax/conventions/server.address rows corrected;
QUICKSTART net-merge resolution requirement corrected (exe-dir resolution,
PATH-only installs are not found); main.rs tray pointer message de-jargonized
(one-string UX fix to a frozen module, additive-safe); release.yml now packs
server/setup-xray-reality.sh into the zip.

**Dismissed with byte-level evidence — S4-REV-C's ci.yml "corruption":**
the trigger reads `branches: [main]` at HEAD and at every commit since the
file was created; the `branches: ain]` appearance is a SANDBOX OUTPUT-FILTER
ARTIFACT (`echo "literal: [main]"` reproduces it; `od -An -c` shows the true
bytes). The filter also produced a self-inflicted sed corruption while
"fixing" the phantom (matched the phantom substring `[ain]` inside the real
`[main]`), immediately restored byte-identical. Lesson recorded in
HARNESS.md ("sandbox output-filter artifact" section): bracketed literals
must be verified with `od`, never plain cat/rg/echo. No CI change was ever
needed.

## Verdict

**S4 CLOSED — consensus 2×APPROVE (round 1), P0 count 0; all actionable
P1s applied (d48f32f); one P1 dismissed as a proven environment artifact
and documented.** Windows CI leg carries 65 tests; release.yml runs the
suite before packaging on every v* tag; the zip now contains the exes,
config.example.json, LICENSE, THIRD_PARTY_NOTICES.md, README, QUICKSTART
and the server setup script.

P2 backlog carried (see the two round-1 reports): config::validate loopback,
sysproxy snapshot .ok() typing, temp+rename writes, IPv6 probe literals,
doctor xray-version timeout, pump interleave, --config= form, tray Exit
~1 s re-set micro-race, fmt/clippy on tag builds (optional), CI caching,
required status checks.
