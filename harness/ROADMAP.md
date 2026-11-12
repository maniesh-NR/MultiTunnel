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

## S1 — Foundation: config + xrayconf + netmerge `[ ]`
- `src/netmerge.rs`: bounded `run()` (spawn/poll/kill), `parse_status`
  heuristic parser per module contract, fixtures tests (must not panic on
  garbage), `on/off/status/ensure_running` wired.
- `src/xrayconf.rs` tests: mux disabled, loopback inbounds, REALITY settings
  passthrough, geoip:private -> direct rule.
- `src/config.rs` tests: defaults, fallback defaults, validation messages.
- Acceptance: `cargo test` green; `cargo zigbuild --target
  x86_64-pc-windows-gnu` green; two adversarial reviews APPROVE.

## S2 — Runtime: supervisor + sysproxy + elevate `[ ]`
- `src/core_proc.rs`: Xray process start/stop, log redirection, port wait,
  hand-rolled SOCKS5 end-to-end probe (greeting -> CONNECT -> drain reply).
- `src/supervisor.rs`: connect/disconnect ordering
  (config -> core -> probe -> bonding -> sysproxy), fail-open restart loop
  with backoff, snapshot for UI/CLI.
- `src/sysproxy.rs` + `src/elevate.rs`: WinINET registry set/restore with
  snapshot (winreg), IsElevated/RelaunchElevated; non-windows stubs.
- `doctor` + headless `connect` implemented in main.rs.
- Acceptance: host tests green; windows-gnu build green; adversarial APPROVE.

## S3 — Interface: tray + icon + release binaries `[ ]`
- `src/tray.rs` module (windows): raw win32 via windows-sys —
  Shell_NotifyIconW, popup menu (Connect/Disconnect/links/raw status/logs/
  config/Exit), tooltip state, embedded `assets/icon.ico`.
- `src/bin/tray.rs` becomes a real GUI binary
  (`windows_subsystem = "windows"`), console bin stays for doctor/generate.
- Acceptance: both bins link for x86_64-pc-windows-gnu; tray module unit
  tests (non-UI logic) pass; adversarial APPROVE (one reviewer must
  specifically audit the unsafe FFI).

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

> NOTE (S1-B, 2026-11-12): the sandbox VM rebooted mid-stage and the working
> tree + Rust toolchain were wiped. The crate skeleton below `src/` was
> restored by agent S1-B from session reads: `Cargo.toml`, `src/lib.rs`,
> `src/error.rs`, `harness/ROADMAP.md` are verbatim; `src/config.rs`,
> `src/xrayconf.rs`, `src/main.rs`, `src/bin/tray.rs` are marked
> RESTORED-PLACEHOLDER stubs that their owners (S1-A / S2 / S3) must replace.
> zig + cargo-zigbuild were NOT reinstalled (not needed for `cargo test`).
