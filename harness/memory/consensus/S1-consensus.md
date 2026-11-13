# S1 Consensus — Foundation (config + xrayconf + netmerge)

Date: 2026-11-12 · Round: 1 (no re-review needed)

## Verdicts

| Reviewer | Lens | Verdict | P0 | P1 | P2 |
|---|---|---|---|---|---|
| S1-REV-C (agent-df58def3) | correctness / security / panics / races / error paths | **APPROVE** | 0 | 2 | 5 |
| S1-REV-B (agent-5b5eb899) | build / portability / licensing / dependency hygiene / test quality | **APPROVE** | 0 | 1 | 6 |

Both reviewers ran the host suite (25 passed at review time) and the
windows-gnu cross-build independently; Reviewer-B verified
`config.example.json` byte-equality with `default_proxy_override()`
computed independently, dependency tree cleanliness (serde/serde_json/regex
only), workflow YAML validity, and `file(1)` subsystem correctness for both
exes. Reviewer-C probed parse_status with adversarial inputs (UTF-8
boundaries, 100k lines, CRLF, `Merging: off`, `enabled: onX`) via a scratch
crate — no panics, no mid-char slicing.

## Findings and dispositions

P1 (fixed before stage close, verified by suite):
- REV-C#1 generate() wrote the credential-bearing xray-client.json 0644 →
  now 0600 on POSIX (`write_private`), Windows default ACLs. **FIXED**
- REV-C#2 netmerge on()/off() inherited lenient "output beats exit code" →
  a failed `net-merge on` could look successful to the future supervisor →
  new `run_strict()` used by on/off; lenient `run()` kept for status only.
  **FIXED**
- REV-B#1 six process tests used POSIX tools without `#[cfg(unix)]` →
  windows CI would go red → gated. **FIXED**

P2 (fixed where cheap):
- REV-C#3 Instant overflow on absurd timeouts → `checked_add` in run_full +
  ensure_running. **FIXED**
- REV-C#6 document the 0-falls-back-to-default convention → schema doc
  comment. **FIXED**
- REV-C#7 rooted-but-relative paths (`\tools\x`) must not be re-rooted →
  `!p.has_root()` condition. **FIXED**
- REV-B#2 release.yml Copy-Item of not-yet-existing docs → Test-Path guards.
  **FIXED**
- REV-B#6 test gaps → golden proxyOverride string test +
  config.example.json == Config::defaults() equality test + fingerprint/
  spiderX asserts. **FIXED**
- REV-B#4 stale ROADMAP note from the crashed S1-B session → removed,
  replaced with closure note. **FIXED**

P2 accepted (recorded, deferred):
- REV-C#4 grandchild pipe semantics (bounded drain / Job Object kill) →
  revisit in S2 supervisor wiring.
- REV-C#5 link-name length cap + dedup in parse_status → cosmetic; revisit.
- REV-B#3 THIRD_PARTY_NOTICES.md does not exist yet → lands in S4 (guarded).
- REV-B#5 config.rs doc cites README.md → README lands in S4.
- REV-B#7 child.kill() does not kill process trees → revisit in S2.

## Resolution

Consensus gate PASSED (both APPROVE). P1s fixed post-approval per protocol;
post-fix suite: **30 passed, 0 failed**; zigbuild green; multitunnel-tray.exe
still PE32+ GUI. Stage S1 CLOSED by orchestrator at 2026-11-12T15:20Z.
