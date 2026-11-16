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

## S4 — Ship: server script, docs, release `[x]`
- `server/setup-xray-reality.sh` (VLESS+REALITY on the VM, REALITY dest
  borrows the Pangolin gateway TLS on 127.0.0.1:443, systemd unit, firewall
  checklist, prints the client config block).
- README.md, QUICKSTART.md, THIRD_PARTY_NOTICES.md.
- Release: local cross-built artifacts zipped into `download/`; `v0.1.0`
  tag pushed so `release.yml` builds + attaches the GitHub Release; when
  `GH_TOKEN` is present the orchestrator also creates the Release directly.
- Acceptance: zips verified (exe present, `file` says PE32+); docs reviewed;
  adversarial APPROVE; final backup.

> S4 CLOSED 2026-11-13: implementer 56340a5 (script validated with `xray run
> -test` against the real binary); review round 2×APPROVE (REV-C: installer
> "injection-proof, fails loudly, never prints the private key"; REV-B:
> 65/78 tests run on the windows-latest leg, no runner risk). Doc P1s fixed
> in d48f32f (restartMax semantics, net-merge resolution, address fallback,
> stale stage jargon, server script added to the release zip). The ci.yml
> "corruption" P1 was DISMISSED with od byte-evidence as a sandbox
> output-filter artifact — lesson recorded in HARNESS.md. Full verdicts:
> harness/memory/consensus/S4-consensus.md.

## S5 — v0.2.0: P2 backlog hardening `[x]`
- Clear the carried P2 backlog from S2–S4 reviews; full per-agent contracts
  in `harness/CONTRACTS-S5.md`. Groups: A config+CLI (loopback validation,
  `--config=`, doctor version timeout, empty proxyOverride semantics),
  B runtime (snapshot tri-state typing, atomic writes, IPv6 probe literals
  incl. ATYP fix, pump interleave lock, Exit re-set micro-race), C tray
  polish (spawn_detached logging, icon pinning, test cleanup, exit
  tooltip), D ship hygiene (release.yml lints, rust-cache, server config
  0640 root:nogroup, QUICKSTART zip list).
- Pre-staged by orchestrator: `src/fsutil.rs` `write_private_atomic`
  (+5 tests, suite 83), `supervisor::ensure_loopback_listen` → pub(crate).
- Acceptance: all 5 gates green (fmt, host+target clippy, tests, zigbuild);
  2 adversarial APPROVE; Cargo 0.2.0; `v0.2.0` tagged + GitHub Release
  verified; artifacts restored to sandbox `download/`.

> S5 CLOSED 2026-11-16: round-1 consensus 2×APPROVE (REV-C 0P0/1P1/7P2,
> REV-B 0P0/0P1/8P2 — both ran all gates independently; REV-C re-derived
> the crashed S5-B implementer's never-reported work against the contract
> in full). P1 (Opaque ProxyEnable fail-safe Write(0) + set() refuses to
> clobber exotic-typed values) and six P2s applied at close; four P2s
> dismissed with reason. 114 tests, all gates clean, both exes PE32+.
> Full verdicts: harness/memory/consensus/S5-consensus.md.

> S1 CLOSED 2026-11-12: consensus 2×APPROVE (S1-REV-C correctness/security,
> S1-REV-B build/portability/license). P1 fixes applied post-approval:
> strict on/off semantics, 0600 credential file, #[cfg(unix)] gating of
> POSIX-dependent process tests. Mid-stage sandbox revert + scope-violation
> incident fully remediated (see LEDGER 14:50–15:12Z). Full verdicts:
> harness/memory/consensus/S1-consensus.md.
