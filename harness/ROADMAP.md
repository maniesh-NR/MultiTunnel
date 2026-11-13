# MultiTunnel Roadmap

Status markers: `[ ]` pending, `[~]` in progress, `[x]` done (consensus
reached + backup completed).

## S0 — Bootstrap (orchestrator) `[x]`
- Public repo `maniesh-NR/MultiTunnel` cloned (was empty), git identity set.
- Toolchain: Rust stable + zig + cargo-zigbuild (rootless cross-linking).
- Harness scaffolding written (this directory).
- Skeleton crate compiles; first backup commit.
- Acceptance: `cargo test` green on host; `cargo zigbuild
  --target x86_64-pc-windows-gnu` green for the skeleton.

## S1 — Foundation: config + xrayconf + netmerge `[x]`
- `src/netmerge.rs`: bounded `run()` (spawn/poll/kill), `parse_status`
  heuristic parser per module contract, fixtures tests (must not panic on
  garbage), `on/off/status/ensure_running` wired.
- `src/xrayconf.rs` tests: mux disabled, loopback inbounds, REALITY settings
  passthrough, geoip:private -> direct rule.
- `src/config.rs` tests: defaults, fallback defaults, validation messages.
- Acceptance: `cargo test` green; `cargo zigbuild --target
  x86_64-pc-windows-gnu` green; two adversarial reviews APPROVE.

## S2 — Runtime: supervisor + sysproxy + elevate `[x]`
- `src/core_proc.rs`: Xray process start/stop, log redirection, port wait,
  hand-rolled SOCKS5 end-to-end probe (greeting -> CONNECT -> drain reply).
- `src/supervisor.rs`: connect/disconnect ordering
  (config -> core -> probe -> bonding -> sysproxy), fail-open restart loop
  with backoff, snapshot for UI/CLI.
- `src/sysproxy.rs` + `src/elevate.rs`: WinINET registry set/restore with
  snapshot (winreg), IsElevated/RelaunchElevated; non-windows stubs.
- `doctor` + headless `connect` implemented in main.rs.
- Acceptance: host tests green; windows-gnu build green; adversarial APPROVE.

> S2 CLOSED 2026-11-13: round 1 BLOCK (S2-REV-C: 2×P0 — start() waiter-join
> hang + double-start; connect/disconnect not serialized, both demo'd) / APPROVE
> (S2-REV-B); all findings fixed; round 2 2×APPROVE with both residual P1s
> applied at close. 57 tests, host + windows-target clippy clean, zigbuild
> green. Two mid-stage infra incidents + one premature-push incident fully
> remediated (see LEDGER 05:12Z–06:40Z). Full verdicts:
> harness/memory/consensus/S2-consensus.md.

## S3 — Interface: tray + icon + release binaries `[x]`
- `src/tray.rs` module (windows): raw win32 via windows-sys —
  Shell_NotifyIconW, popup menu (Connect/Disconnect/links/raw status/logs/
  config/Exit), tooltip state, embedded `assets/icon.ico`.
- `src/bin/tray.rs` becomes a real GUI binary
  (`windows_subsystem = "windows"`), console bin stays for doctor/generate.
- Acceptance: both bins link for x86_64-pc-windows-gnu; tray module unit
  tests (non-UI logic) pass; adversarial APPROVE (one reviewer must
  specifically audit the unsafe FFI).

> S3 CLOSED 2026-11-13: implementer round 0fb81f0 (77 tests, 32 SAFETY-
> commented unsafe blocks); review round 2×APPROVE with a line-by-line
> 32/32 unsafe audit (REV-C) and 13/13 stability runs incl. loaded 2-core
> host (REV-B); all 3 P1s fixed in 799f6cd (op_lock-free emergency_stop +
> Exit escalation; TaskbarCreated re-add; hidden top-level window for the
> KB135788 foreground fix). 78 tests, fmt/clippy host+target clean,
> zigbuild green. Residual: first REAL-Windows run is the smoke test.
> Full verdicts: harness/memory/consensus/S3-consensus.md.

## S4 — Ship: server script, docs, release `[ ]`
- `server/setup-xray-reality.sh` (VLESS+REALITY on the VM, REALITY dest
  borrows the Pangolin gateway TLS on 127.0.0.1:443, systemd unit, firewall
  checklist, prints the client config block).
- README.md, QUICKSTART.md, THIRD_PARTY_NOTICES.md.
- Release: local cross-built artifacts zipped into `download/`; `v0.1.0`
  tag pushed so `release.yml` builds + attaches the GitHub Release; when
  `GH_TOKEN` is present the orchestrator also creates the Release directly.
- Acceptance: zips verified (exe present, `file` says PE32+); docs reviewed;
  adversarial APPROVE; final backup.

> S1 CLOSED 2026-11-12: consensus 2×APPROVE (S1-REV-C correctness/security,
> S1-REV-B build/portability/license). P1 fixes applied post-approval:
> strict on/off semantics, 0600 credential file, #[cfg(unix)] gating of
> POSIX-dependent process tests. Mid-stage sandbox revert + scope-violation
> incident fully remediated (see LEDGER 14:50–15:12Z). Full verdicts:
> harness/memory/consensus/S1-consensus.md.
