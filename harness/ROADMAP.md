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

## S6 — v0.3: All-in-One + Material You GUI + native core `[x]`

User directive 2026-11-17: MultiTunnel becomes an all-in-one app with ZERO
user-installed dependencies, combining the functions of v2rayN and net-merge
out of the box, robust Material You GUI, Rust core — built Mixture-of-Experts
style (parallel expert agents, one crate each). Full per-expert contracts in
`harness/CONTRACTS-S6.md`.

- Workspace: root package keeps v0.2 behavior green at every commit; new
  capability lands in `crates/mt-{links,net,engine,m3,platform,service,app}`.
- mt-links: vless/vmess/ss/trojan share links, multi-server profile store,
  subscriptions, legacy-config migration (v2rayN import parity).
- mt-net: native Rust data plane — SOCKS5+HTTP inbound, routing rules,
  net-merge-style link aggregation (failover/round-robin/lowest-latency,
  health probes, quarantine) in-process.
- mt-engine: managed Xray engine, one child per link, config-generated,
  kill-on-drop, self-healing — the REALITY specialist.
- mt-platform: app dirs + engine self-provisioning (locate/verify/download/
  embed) so the app never asks the user to install a core; autostart.
- mt-m3 + mt-app: Material You design system + iced GUI (Status/Servers/
  Routing/Settings/Logs, dynamic color from a seed).
- Root wiring: `multitunnel gui`, `profiles` CLI, RealService over the crates.
- Acceptance: all 5 gates green per crate AND `--workspace`; zigbuild PE32+
  x3 (console, tray, gui); xvfb GUI smoke; 2× adversarial APPROVE (D6).

> S6 IMPLEMENTATION PROGRESS 2026-11-17 (waves 1–3 pushed; review pending):
> wave 1 1b2fe0b — mt-links 64, mt-net 44, mt-m3 12, mt-engine 9,
> mt-platform 25 tests, all gates green per crate (fmt, clippy host +
> windows-gnu via mingw-zig-shims, -D warnings); wave 2 5ea0b27 — mt-app
> iced 0.13 + tiny-skia Material You GUI, 18 tests, zigbuild PE32+ link
> proof; wave 3 a9f7aae — root wiring: RealService, `multitunnel gui`,
> `profiles` CLI, 140 lib + 8 bin root tests, workspace total 320, release
> zigbuild PE32+ x3 (console/tray/gui); smoke: profiles add/list/use +
> gui --smoke exit 0. STAGE CLOSE PENDING: D6 review round (2x APPROVE),
> README/QUICKSTART/NOTICES refresh, v0.3.0 release.
>
> S6 REVIEW ROUND COMPLETE 2026-11-18: consensus 2×APPROVE (REV-C 0P0/5P1/15P2,
> REV-B 0P0/3P1/8P2 — both ran all gates independently, incl. a full 391-package
> license sweep with no copyleft). All 8 P1s fixed by the orchestrator:
> debug eprintln removed + Windows kill-on-close Job Object ported (mt-engine),
> GUI fail-open/self-heal (step-0 stale restore, window-close/Ctrl+C exit
> hooks, pump-based all-links-down supervisor with D5 give-up), LAN bypass
> restored (stock private-CIDR→direct rules + v0.2 ProxyOverride), engine-
> download URL redaction (D9), release.yml workspace gates, MSRV 1.97,
> THIRD_PARTY_NOTICES regenerated (391 crates + OFL fonts + elections).
> Folded P2s: mt-m3 thiserror, CI --smoke leg, stale disconnect flag, GUI
> cancel-while-Connecting. Docs rewritten for the all-in-one (README,
> QUICKSTART); gates re-verified: fmt, clippy host+windows -D warnings,
> 321 tests, release zigbuild PE32+ x3, gui --smoke exit 0.
> Full verdicts: harness/memory/consensus/S6-consensus.md.
>
> S6 CLOSED 2026-11-18: review round 2×APPROVE; all 8 P1s fixed (+4 P2s
> folded); docs refreshed. RELEASE STORY: the first v0.3.0 tag was retracted
> when the frontier's own CI failures (mt-net Windows RST class + mt-engine
> respawn race — see LEDGER 05:30Z) were diagnosed; both fixed with
> production-grade remedies (graceful_close on all error paths, dedicated
> NetError::ProxyUnreachable, forced respawn + synchronous reap); CI green
> on both runners (ubuntu + windows), release workflow green, v0.3.0 zip
> published + verified (PE32+ x2, 5,561,966 bytes) + restored to sandbox
> download/. v0.3.1 backlog: GUI autostart target, same-tag engine file
> races, body-size caps, engine log rotation, profiles.json locking, vmess
> pbk/sid validation, listener max_conns.

> **v0.3.1 HOTFIX (post-S6, 2026-11-18)**: the first real-Windows user run
> reported "multitunnel.exe does not show a GUI when opened" — the v0.3.0
> no-args default was the `tray` pointer error, so a double-click flashed a
> console and exited. Fixed per D15: bare launch = GUI, Explorer/autostart
> launches free their orphan console, GUI errors/panics surface as a
> MessageBox when no console can carry them, and the empty-argv `gui` slice
> panic found while verifying was fixed (gui_args). 323 tests, all gates
> green, tagged v0.3.1 (see LEDGER 05:33–05:40Z).

## S8 — Adapter bonding menu (CLOSED v0.4.0, 2026-11-18)

User report: "There is no menu to select the network adapters that are being
bonded for the vless connection." The aggregator/policies/probers existed
since S6 but the factory built exactly one unbound engine — bonding had no
adapter surface.

Delivered (see D16 + LEDGER):
- mt-platform `adapters`: Win32 `GetAdaptersAddresses` FFI (loopback
  excluded, IPv4-first ordering, fail-open) + pure-std procfs dev path.
- mt-engine `sendThrough` per engine; one `ManagedEngine` per selected
  adapter with distinct tags (config/log files never collide).
- RealService: `adapters()` / `set_bonded_adapters()` (id validation,
  persistence, live-session restart, set_settings guard); connect resolves
  the selection against the CURRENT enumeration (fail-open skips).
- GUI: new "Adapters" screen (M3 checkboxes, IPv4/up/down chips,
  Apply/Revert/Refresh — Routing-editor draft idiom); MockService seeds
  three synthetic adapters.
- CLI: `multitunnel adapters` (headless diagnostics).
Gates: 335 tests / 0 failed, clippy + fmt clean, zigbuild green,
`gui --smoke` 6/6 views. Tagged v0.4.0.

## S9 — Stream bonding (true per-stream split) + live graph + server picker + advanced settings (v0.5.0)

User report 2026-11-18: (a) exit IP shows Singapore 103.216.221.95 — "are there
any additional server hops? I want each adapter to connect directly to the GCP
server via v2ray reality"; (b) "the system sends and receives as if it is using
one connection; this traffic is split across the two adapters who send it via
v2ray reality to the GCP server where it is merged as a single request again —
same in reverse"; (c) real-time bandwidth graph on the Status tab that scales
with the window (Task Manager style); (d) server selector dropdown on the
Status tab; (e) more advanced settings from v2rayN and net-merge.

Hop analysis (answered with evidence): MultiTunnel adds ZERO server hops —
each per-adapter engine dials the profile's VLESS+REALITY endpoint directly.
103.216.221.95 is Host Universal Pty Ltd (ASN 136557, Singapore — a VPN
provider range), i.e. the GCP VM's own VPN egress (the setup script's
documented ProtonVPN scenario: freedom outbound rides the VM default route).
The in-app Exit Probe (Status tab) makes this verifiable live: exit IP +
geo + ISP vs the profile endpoint.

Delivered (D17–D19):
- `crates/mt-bond` (Expert: stream-bonding): 24-byte framed multiplexing
  protocol (HELLO/OPEN/DATA/CLOSE/ACK/PING/PONG), per-session reordering,
  cumulative-ACK replay on carrier death, round-robin frame scheduling
  across carriers, per-carrier RTT/byte stats. Client `BondOutbound`
  (implements `mt_net::Outbound` — ProxyServer unchanged) + server
  `mt-merge-server` daemon (loopback listener; carriers arrive via Xray
  VLESS → freedom → 127.0.0.1:<merge_port>).
- Bonding mode setting: `per-connection` (S8 default) vs `stream`; stream
  mode falls back to per-connection with a log line when carriers cannot
  be established (no daemon / single link).
- GUI Status: server pick_list (hot-switch reconnects), canvas bandwidth
  graph (window-scaling, up/down series), Exit Probe card.
- GUI Settings → Advanced: SOCKS/HTTP ports, allow-LAN bind, engine log
  level, probe target, bond mode + merge port (v2rayN/net-merge parity).
- Server: `server/setup-merge-daemon.sh` + systemd unit; release.yml gains
  an ubuntu leg building the Linux `mt-merge-server` artifact.

Acceptance: all 5 gates green workspace-wide; mt-bond integration tests
(loopback daemon + 2 fake links: echo integrity, carrier-death failover,
1 MiB blob hash, concurrent sessions); zigbuild PE32+ x3 + Linux daemon
PE/ELF verified; gui --smoke 6/6; docs refreshed; v0.5.0 tagged + released.

> S9 CLOSED 2026-11-18: single-implementation with orchestrator audit
> (S8 precedent). Delivered: crates/mt-bond (frame codec, reorder buffer,
> endpoint with round-robin carriers + cumulative-ACK replay + keepalives,
> BondOutbound, mt-merge-server daemon) — 22 tests incl. full loopback
> integration (1 MiB echo integrity across BOTH carriers, carrier-death
> failover with zero byte loss, concurrent sessions, refused/one-dead-link
> fallback); RealService stream-mode wiring with per-connection fallback;
> exit probe (SOCKS→plain-HTTP ip-echo, D18) + verdict; carrier rows in
> status; engine log level; GUI: server pick_list, window-scaling canvas
> bandwidth graph, exit identity card, Advanced settings card; server
> setup-merge-daemon.sh + systemd unit; release.yml ubuntu daemon leg
> (x86_64 + aarch64). Gates: fmt clean, clippy host+windows-gnu -D
> warnings clean, 356 workspace tests 0 failed, zigbuild PE32+ x4,
> gui --smoke 6 ticks/6 views, linux daemon ELF smoke (listens 127.0.0.1).
> Mid-stage protocol fix found by the integration tests: CLOSE was
> fire-and-forget and could OVERTAKE in-flight DATA on another carrier —
> replaced with ack-flush-then-CLOSE + asymmetric half-close semantics
> (client CLOSE(Eof) = write-close; daemon CLOSE = final) so a lost tail
> is impossible. v0.5.0 tagged + released.

## S7 — Native REALITY core (parked)

Pure-Rust VLESS+REALITY+vision client (hand-rolled TLS 1.3 with Chrome
ClientHello fingerprint + x25519 session_id auth, ported from the
MIT-licensed xray-core reference) to retire the managed engine for good.
Design notes at stage open.

> S1 CLOSED 2026-11-12: consensus 2×APPROVE (S1-REV-C correctness/security,
> S1-REV-B build/portability/license). P1 fixes applied post-approval:
> strict on/off semantics, 0600 credential file, #[cfg(unix)] gating of
> POSIX-dependent process tests. Mid-stage sandbox revert + scope-violation
> incident fully remediated (see LEDGER 14:50–15:12Z). Full verdicts:
> harness/memory/consensus/S1-consensus.md.
