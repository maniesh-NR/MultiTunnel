# S6 Consensus — v0.3 All-in-One (waves 1–3)

Date: 2026-11-18. Reviewers ran in parallel on frontier 6df8767
(diff base d721cba = s6-plan-skeleton). Both APPROVE under D6 → S6
implementation accepted; P1 list below is the release blocker checklist,
owned by the orchestrator.

## Verdicts

| Reviewer | Lens | Verdict | P0 | P1 | P2 |
|----------|------|---------|----|----|----|
| S6-REV-C | correctness/security | **APPROVE** | 0 | 5 | 15 |
| S6-REV-B | build/portability/license | **APPROVE** | 0 | 3 | 8 |

Both independently reproduced all gates green: fmt, host clippy
(-D warnings), windows-gnu clippy via zig shims (-D warnings), 321
workspace tests, release zigbuild PE32+ x3, feature corners (mt-app
no-default-features, mt-platform embedded + real MT_EMBED_ENGINE zip).

## Accepted P1 findings (release blockers, orchestrator fixes)

1. **REV-C1** `crates/mt-engine/src/engine.rs:336` — leftover debug
   `eprintln!` in the per-connection liveness path (violates the
   no-println-in-libs rule). Fix: delete.
2. **REV-C2** `crates/mt-engine/src/engine.rs:450-461` — v0.2's Windows
   Job Object kill-on-close (v0.2-audit REV-C#2, commit ff0abb4) not
   ported: `xray.exe` orphaned on abnormal GUI death (Task Manager kill,
   Ctrl+C, power). Fix: restore the job-object wrapper around spawned
   children.
3. **REV-C3** `src/real_service.rs` + `mt-app` — GUI path loses the S2
   fail-open/self-heal contract (D5): no sysproxy restore on window
   close, no stale-snapshot heal at startup, no proxy restore on engine
   give-up. Fix: exit-hook restore, connect step-0 heal, give-up restore.
4. **REV-C4** `src/real_service.rs:696-699,587-588` — LAN/private traffic
   no longer bypasses the tunnel under the GUI system proxy (hard-coded
   `"localhost"` override; empty router rules default to Proxy). Regression
   of the stated invariant (LAN bypass + geoip:private→direct). Fix: v0.2
   LAN bypass override + private-CIDR→direct default rules.
5. **REV-C5** `crates/mt-platform/src/engine.rs:129-133` — engine-download
   errors echo the full request URL (token in `$MULTITUNNEL_ENGINE_URL`
   can leak into GUI logs; D9). Fix: `reqwest::Error::without_url()`
   treatment like mt-links.
6. **REV-B1** `.github/workflows/release.yml:20-25` — release gates not
   `--workspace`: only 148/321 tests run on a `v*` tag; the 7 new crates
   ship untested at tag time. Fix: `--all` / `--workspace` on fmt/clippy/test.
7. **REV-B2** `Cargo.toml:8` + `README.md:218` — `rust-version = "1.78"`
   is false (material-colors 0.5.0 requires 1.97; uuid 1.89, tokio 1.85,
   iced 1.80). Fix: bump to the true floor, propagate to members.
8. **REV-B3** `THIRD_PARTY_NOTICES.md` — stale by its own rule: ~375 new
   deps missing, OFL-1.1 fonts embedded in iced_graphics unattributed,
   self_cell/r-efi license elections unrecorded, Xray row framing stale
   for v0.3. Fix: regenerate.

## P2 disposition

- **Folded now (cheap):** mt-m3 unused `thiserror` dep (B-P2-2);
  `--smoke` wired into the CI ubuntu test leg (B-P2-5); stale
  `disconnect_requested` flag cleared at connect start (C-P2-1); GUI can
  cancel a Connecting session (C-P2-2).
- **Parked (documented in reports, not blocking):** same-tag engine file
  races, unbounded subscription/download bodies + no GC of staging dirs,
  unbounded engine logs, blocking I/O in async, drain semantics, Stopped
  event race, GUI autostart target (C-P2-10 — tracked for v0.3.1),
  vmess pbk/sid validation, listener max_conns, profiles.json locking,
  mt-app.exe packaging decision (B-P2-1 — document in README), thiserror
  duplication (forced), doc inaccuracies (folded into the v0.3 docs
  refresh anyway), cross-check debug-only, lifecycle tests unix-only,
  plus the 7 carried v0.2 backlog items (all confirmed still parked).

## Notes

- REV-C verified the C5-adjacent D9/D10 redaction everywhere else clean;
  the mt-links percent-decode policy deviation from v0.2 is the frozen S6
  contract's choice and accepted.
- REV-B's license sweep: all 391 locked packages permissive; self_cell /
  r-efi OR-elections to record in the notices refresh; no copyleft
  anywhere; iced fonts are OFL-1.1 (redistribution notice required).
- Full reports: agent-reports/s6-rev-{c,b}.md.
