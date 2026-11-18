# MultiTunnel

An **all-in-one Windows proxy app with zero user-installed dependencies**:
a Rust core supervises an Xray VLESS+REALITY tunnel to your own VM, bonds
multiple physical uplinks, imports `vless://` share links, and manages the
Windows system proxy — all behind a **Material You (Material Design 3)
GUI** with dynamic color.

Since v0.3 MultiTunnel combines the functions of a v2rayN-style client and
net-merge-style link bonding **out of the box**: the SOCKS5/HTTP inbounds,
the routing table, the link bonding engine and the profile store all run
in-process, and the Xray engine binary is fetched automatically on first
connect. Nothing to install beside the app itself.

| Binary | What it is |
|---|---|
| `multitunnel.exe` | the all-in-one: `gui` (Material You app), `profiles`, `import`, `connect`, `doctor`, `generate`, `version` |
| `multitunnel-tray.exe` | the legacy v0.2 notification-area (tray) app (Windows subsystem, no console) |

You run everything on hardware you control: the proxy core (Xray) on your
own cloud VM, everything else on your own PC. MultiTunnel sets and clears
the Windows system proxy, keeps the tunnel alive (fail-open), and never
lets the engine outlive the app (Windows kill-on-close job objects).

If you just want it working, read [QUICKSTART.md](QUICKSTART.md) first — the
whole path is about ten minutes.

## Architecture (v0.3 all-in-one)

```
        your Windows PC                                        your VM (cloud, static IP)
┌──────────────────────────────────────────┐                ┌────────────────────────────────────┐
│  Material You GUI (iced + mt-m3)         │                │                                    │
│    │  Arc<dyn MtService>                 │                │                                    │
│    ▼                                     │                │                                    │
│  RealService (root crate)                │                │                                    │
│    ├─ ProfileStore   (mt-links)          │                │                                    │
│    │    vless:// import, subscriptions   │                │                                    │
│    ├─ ProxyServer    (mt-net)            │    VLESS +     │  xray (systemd, User=nobody)       │
│    │    SOCKS :10808 / HTTP :10809       │    REALITY     │    - listens on 0.0.0.0:8443       │
│    │    Router: private CIDRs → direct   │ ═════════════▶ │    - REALITY borrows the TLS cert   │
│    │    merge: failover / lowest-latency │    TCP :8443   │      of the gateway already on     │
│    ├─ ManagedEngine  (mt-engine)         │                │      127.0.0.1:443 (serverNames)   │
│    │    one Xray child per link,         │                │    │    as its camouflage            │
│    │    kill-on-close job, respawn       │                │    ▼ freedom outbound              │
│    ├─ EngineLocator  (mt-platform)       │                │  default routing -> the internet   │
│    │    engine self-download + SHA256    │                │                                    │
│    └─ sysproxy       (WinINET, HKCU)     │                │                                    │
│       fail-open, snapshot + self-heal    │                │                                    │
└──────────────────────────────────────────┘                └────────────────────────────────────┘
```

Workspace layout (Mixture-of-Experts build: one expert crate each, frozen
contracts in `harness/CONTRACTS-S6.md`):

| Crate | Role |
|---|---|
| `mt-links` | share-link parsing (`vless://` strict REALITY-only), profile store, subscriptions, v0.2 migration |
| `mt-net` | the data plane: SOCKS5 + HTTP inbounds, routing table, link bonding (failover / lowest latency), quarantine |
| `mt-engine` | supervised Xray child processes: respawn with backoff, kill-on-drop + kill-on-close job objects |
| `mt-m3` | Material You design tokens with real HCT dynamic color (material-colors) |
| `mt-platform` | engine locator/self-download (SHA256SUMS-verified), autostart, platform dirs |
| `mt-service` | the frozen UI service contract (state machine, events) |
| `mt-app` | the iced 0.13 Material You GUI (tiny-skia renderer) |
| root | `RealService` wiring, the v0.2 CLI (`connect`/`doctor`/`generate`/tray) |

Walking through one request: the application speaks to the loopback proxy
because MultiTunnel has set the WinINET system proxy to `127.0.0.1:10809`.
The router sends private/LAN CIDRs direct and everything else through the
VLESS+REALITY outbound; because mux is off, every proxied connection is its
own TCP stream, so a multi-link setup can spread connections across
uplinks (connection-level bonding, failover or lowest-latency). On the VM,
xray terminates REALITY on port 8443 — an unauthenticated handshake is
proxied to the real gateway, which is what makes the port look like an
ordinary HTTPS endpoint — and exits through a plain `freedom` outbound.

## Why it is built this way

These are deliberate design contracts, not accidents; changing any of them
breaks something subtle.

- **Mux must stay OFF.** Bonding works by spreading individual TCP
  connections across uplinks. A muxed tunnel collapses all traffic into a
  single stream, and one stream always rides one link — bonding would be
  defeated. The generated client config pins `"mux": { "enabled": false }`.
- **Loopback-only inbounds.** The proxy plane exposes only SOCKS/HTTP on
  `127.0.0.1` and never creates a TUN inbound.
- **LAN traffic stays local.** The stock routing rules send every private
  CIDR (`10/8`, `172.16/12`, `192.168/16`, loopback, link-local, ULA) to a
  `direct` outbound, and the system proxy override lists the same ranges —
  local network traffic never takes the tunnel.
- **Fail-open proxy policy.** The system proxy is a machine-wide setting; a
  dead proxy pointing at a dead tunnel means "no internet". If the engine
  dies, MultiTunnel restarts it with backoff; if the restart budget is
  exhausted, it restores (clears) the system proxy so normal browsing keeps
  working, and marks the state `Failed`. True fail-close is out of scope.
- **The engine can never be orphaned.** On Windows every Xray child runs
  inside a kill-on-close job object: even if MultiTunnel itself is killed
  (Task Manager, Ctrl+C, power loss), the kernel tears the engine down
  with it.
- **Stale-proxy self-heal.** A run that dies without disconnecting can
  leave the registry pointing at a dead listener; the next connect (CLI or
  GUI) restores a clean proxy state at step 0 before bringing the tunnel up
  again.
- **Strict share-link parsing (D10).** `vless://` imports accept only
  `security=reality`, `type=tcp`, flow `xtls-rprx-vision` or unset, a
  canonical UUID, and well-formed `pbk`/`sid`; unknown parameters are
  ignored, and no error message ever echoes credential values.

The supervised connect sequence, in order: self-heal any stale system
proxy → write the generated Xray config → start the engine and wait for
the SOCKS port → probe the full chain through the tunnel (fail fast: stop
engine, restore proxy, state `Failed`) → set the system proxy (non-fatal).
While connected, the event pump doubles as a supervisor: link health is
checked on a 5 s cadence, and all-links-down for three consecutive checks
fires the fail-open give-up (proxy restored, state `Failed`). Engine
restarts back off 1 s, 2 s, 4 s, ... capped at 30 s.

## Server setup

`server/setup-xray-reality.sh` installs the VLESS+REALITY endpoint on your own
Ubuntu/Debian VM in one run:

```sh
sudo DOMAIN=your-gateway.example.org bash server/setup-xray-reality.sh
```

It resolves the **latest** Xray-core release via the GitHub API (never a
hardcoded version), downloads and verifies the archive, installs
`xray` to `/usr/local/bin/xray` with geo assets to `/usr/local/share/xray/`,
generates a fresh UUID / X25519 key pair / shortId, writes
`/usr/local/etc/xray/config.json` (validated with `xray run -test`), installs a
hardened systemd unit (`User=nobody`, `ProtectSystem=strict`,
`NoNewPrivileges`, auto-restart), and starts it. At the end it prints the
firewall checklist for the VM and either a ready-to-paste `"server"` block
(legacy config.json path) or a full `vless://` share link (v0.3 GUI/`profiles`
path).

Environment knobs: `DOMAIN` (required — the existing gateway domain whose TLS
certificate the REALITY handshake borrows), `PORT` (default `8443`), `DEST`
(default `127.0.0.1:443` — the gateway on the same VM), `INSTALL_ROOT`
(default `/usr/local`), `FORCE=1` (overwrite an existing config — regenerates
all credentials). The REALITY private key is written only to the server
config; the script never prints it and it never leaves the VM.

## The v0.3 way: GUI + profiles (recommended)

1. `multitunnel gui` opens the Material You window (theme seed is
   configurable; dark mode included).
2. Paste a `vless://` share link on the Servers screen — a preview shows
   protocol/endpoint before you add it. Or use the CLI:
   `multitunnel profiles add "vless://…"` then `multitunnel profiles use <tag>`.
3. Flip the connect switch. On first connect the engine is downloaded and
   verified automatically (see the mt-platform locator; `MULTITUNNEL_ENGINE_URL`
   can point at a mirror). The Routing screen edits the rule table
   (private CIDRs go direct by default); Settings covers the system-proxy
   flag, merge policy, autostart and theme.

Closing the window (or Ctrl+C in the console) disconnects cleanly — the
system proxy is always restored. `multitunnel gui --smoke` runs the app
headless for CI.

Settings and profiles live in the platform config dir
(`%APPDATA%\MultiTunnel` unless overridden): `profiles.json` (the multi-
server store) and `settings.json` (ports, rules, theme, flags). Neither
file contains secrets beyond the profile credentials themselves; profile
files are written with restrictive permissions.

## The legacy v0.2 way (still supported)

MultiTunnel reads `config.json` next to the executable (or the path given with
`-config <path>`) for the `connect`/`doctor`/`generate` commands and the tray
app. Start from `config.example.json`; the schema is camelCase and every
field has a default. `config.json` is gitignored — credentials never
belong in the repo. `multitunnel import "vless://…"` fills its server block
from a share link (only the six `server.*` fields are patched — your local
ports and paths survive).

| Field | Default | Meaning |
|---|---|---|
| `server.address` | `35.208.99.240` (template prefill) | the VM's public address — required, no fallback when cleared |
| `server.port` | `8443` | VLESS+REALITY port on the VM |
| `server.uuid` | — (required) | client UUID, from the server script's output |
| `server.sni` | — (required) | the gateway domain (REALITY serverName) |
| `server.publicKey` | — (required) | X25519 public key (client half) |
| `server.shortId` | — (optional) | REALITY shortId; may stay empty |
| `local.listen` | `127.0.0.1` | loopback bind address for the core's inbounds |
| `local.socksPort` | `10808` | loopback SOCKS inbound |
| `local.httpPort` | `10809` | loopback HTTP inbound (the system proxy target) |
| `paths.xray` | `bin/xray.exe` | Xray core binary (relative paths resolve against the exe dir) |
| `paths.netMerge` | `net-merge` | net-merge executable (legacy bonding) |
| `paths.xrayConfig` | `xray-client.json` | where the generated Xray config is written |
| `paths.logDir` | `logs` | core log (`xray.log`) lives here |
| `paths.stateDir` | `state` | proxy snapshot, raw status dumps |
| `netMerge.enable` | `true` | turn legacy net-merge bonding on/off with the tunnel |
| `behavior.systemProxy` | `true` | set/clear the WinINET system proxy with the tunnel |
| `behavior.proxyOverride` | `localhost;127.*;10.*;192.168.*;172.16.*` … `172.31.*;<local>` | proxy bypass list |
| `behavior.probeTarget` | `example.com:80` | periodic full-chain probe target (host:port) |
| `behavior.probeIntervalSec` | `60` | probe interval in seconds |
| `behavior.restartMax` | `5` | core restart budget after failures (0 = never restart: give up at once and restore the proxy) |

Conventions: fields left empty fall back to these defaults on load — except
`server.address` (required; an empty value is rejected by validation) and
`behavior.restartMax` (0 means never restart, not the default); purely
relative paths are resolved against the executable's
directory (absolute or drive-rooted paths are used as-is). Validation requires
`server.uuid`, `server.sni` and `server.publicKey` (the error names exactly
the missing fields and points at the server script), and the SOCKS/HTTP ports
must differ.

## CLI reference

Both binaries accept `-config <path>` / `--config <path>` to override the
config location (legacy commands only; the GUI path uses `settings.json`).

```
Usage: multitunnel [command] [-config <path>]

  (none)     print this usage (the tray app is the separate multitunnel-tray.exe)
  gui        launch the all-in-one Material You GUI (--smoke: headless CI run)
  profiles   manage the multi-server profile store (list/add/use/remove/export)
  import     fill config.json's server block from a vless:// link
  connect    connect headless (legacy v0.2 path), run until Ctrl+C
  generate   write the generated Xray client config and exit
  doctor     check environment (binaries, config, ports)
  version    print version
```

- `multitunnel gui` — the all-in-one app (see above). Closing the window or
  Ctrl+C disconnects and restores the system proxy.
- `multitunnel profiles list` — protocol/address/port/tag per profile,
  active marked; never prints credentials. `profiles add <share-link>`
  (previews, then upserts), `profiles use <tag-or-id>`, `profiles remove
  <tag-or-id>`, `profiles export [tag]` (prints a share link — that is its
  purpose; vmess profiles cannot be re-exported).
- `multitunnel import "vless://…"` — patches only the six `server.*` fields
  of `config.json`; everything else (ports, paths, behavior) survives.
  Writes the file atomically with restrictive permissions.
- `multitunnel doctor` — environment check; exits 1 if anything fails.
  Checks: elevation, the Xray binary, net-merge reachability (warns when
  bonding is disabled), free SOCKS/HTTP ports, writable log/state dirs, and
  a parseable `probeTarget`. The config is loaded and validated first.
- `multitunnel connect` — legacy headless connect: elevates via UAC on
  Windows when needed, brings the tunnel up, and supervises until `Ctrl+C`.
  `Ctrl+C` detaches with the system proxy still set; the next `connect`
  self-heals it.
- `multitunnel generate` — renders the Xray client config to
  `paths.xrayConfig` (0600 on POSIX; default user-profile ACL on Windows —
  it embeds the VLESS credential) and exits.
- `multitunnel-tray.exe` — the legacy tray app. Elevates via UAC when
  needed, reads `config.json` beside the executable, and puts an icon in
  the notification area with Connect/Disconnect, raw status, log folder,
  config editing and clean Exit.

## Building

Stable Rust (MSRV 1.97 — material-colors 0.5.0 requires it). From a
checkout:

```sh
cargo build --release        # multitunnel + multitunnel-tray
cargo build -p mt-app        # the standalone GUI dev binary (mock backend)
cargo test --workspace       # full suite (321 tests)
```

- Native Windows with the MSVC toolchain is the supported way to produce
  release binaries (this is what CI uses).
- Cross-checking from Linux without a mingw toolchain works via
  [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild), which uses
  zig as the linker driver:

  ```sh
  rustup target add x86_64-pc-windows-gnu
  cargo install cargo-zigbuild
  cargo zigbuild --target x86_64-pc-windows-gnu --release
  cargo zigbuild --target x86_64-pc-windows-gnu --release -p mt-app
  ```

- The Windows-only dependencies (`windows-sys`, `winreg`) are feature-gated
  behind `cfg(windows)`; a Linux build compiles the library and console
  binary with stubs, which is how the test suite runs on Linux CI.
- `mt-app.exe` is a development binary (mock backend); the shipped
  all-in-one GUI is `multitunnel gui` inside `multitunnel.exe`, which links
  the real service.

## Release and CI

- `.github/workflows/ci.yml` runs on every push to `main` and on pull
  requests: `cargo test --workspace` on a matrix of `ubuntu-latest` and
  `windows-latest` (plus the headless GUI smoke run on ubuntu); a lint job
  enforcing `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings` and the same again for the
  `x86_64-pc-windows-gnu` target (zig cc shims); and a cross-check job
  running `cargo zigbuild --workspace --target x86_64-pc-windows-gnu`.
- `.github/workflows/release.yml` runs on tags matching `v*`: on a
  `windows-latest` runner it runs the full workspace gates (`fmt --all`,
  `clippy --workspace`, `test --workspace`) **first** (tag pushes bypass
  the CI workflow, so a release tag must never ship an untested build),
  then builds `--release`, packages both executables together with
  `config.example.json`, `LICENSE`, `THIRD_PARTY_NOTICES.md`, `README.md`,
  `QUICKSTART.md` and `server/setup-xray-reality.sh` into a zip, and
  publishes the GitHub Release (`fail_on_unmatched_files` is set so a
  packaging typo fails loudly instead of shipping an empty release).

## Security notes

- **Credentials.** `config.json`, `profiles.json` and the generated
  `xray-client.json`/engine configs hold the VLESS UUID and REALITY public
  parameters; they are gitignored (only `config.example.json` is committed)
  and written with mode 0600 on POSIX / default user-profile ACL on
  Windows. On the server, the REALITY private key lives only in the VM's
  `/usr/local/etc/xray/config.json` (root-owned) and is never printed by
  the setup script.
- **No secrets in logs or errors.** Parse errors name lengths and field
  positions, never credential values; download errors never echo URLs that
  may carry embedded tokens; logs contain engine output and state
  summaries only.
- **Engine integrity.** The self-downloaded engine is verified against the
  published SHA256SUMS when present (advisory by default;
  `MULTITUNNEL_ENGINE_STRICT=1` refuses on mismatch — see DECISIONS D14),
  installed atomically from a private staging dir with zip-slip
  protection.
- **Unsigned binaries.** Release artifacts are not code-signed: expect the
  Windows SmartScreen / "unknown publisher" warning and the UAC prompt.
  Administrator rights are required for the HKCU proxy writes (the v0.3 GUI
  path needs no net-merge/wintun driver). Only run binaries you built
  yourself or downloaded from this repository's Releases.
- **Fail-open.** See the design section: on unrecoverable failure the system
  proxy is restored rather than left pointing at a dead tunnel. After an
  abrupt termination the proxy can remain set until the next connect run
  cleans it up.

## Licensing

MultiTunnel is MIT-licensed — see [LICENSE](LICENSE). Third-party components
(compile-time crates including the fonts embedded in the GUI, and the
runtime-fetched Xray-core engine, which is not redistributed) are listed in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Development

The project is developed under a staged harness with parallel adversarial
review — see [harness/HARNESS.md](harness/HARNESS.md) for the protocol,
[harness/ROADMAP.md](harness/ROADMAP.md) for the roadmap and stage history,
and `harness/memory/` for decisions, consensus verdicts and agent reports.
