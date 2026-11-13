# MultiTunnel

A Windows supervisor that combines an **Xray VLESS+REALITY tunnel to your own
VM** with **net-merge multi-link bonding**: several physical uplinks (e.g. a
cable modem plus a LTE stick) are merged into one virtual link, and the
tunnel's traffic is spread across them, connection by connection.

One crate, two binaries:

| Binary | What it is |
|---|---|
| `multitunnel.exe` | console tool: `doctor`, `generate`, `connect`, `version` |
| `multitunnel-tray.exe` | the GUI: a notification-area (tray) icon app (Windows subsystem, no console) |

You run everything on hardware you control: the proxy core (Xray) on your own
cloud VM, the bonding layer (net-merge) on your own PC. MultiTunnel supervises
both, sets and clears the Windows system proxy, and keeps the tunnel alive.

If you just want it working, read [QUICKSTART.md](QUICKSTART.md) first — the
whole path is about ten minutes.

## Architecture

```
        your Windows PC                                        your VM (cloud, static IP)
┌──────────────────────────────────────────┐                ┌────────────────────────────────────┐
│  applications (browser, ...)             │                │                                    │
│    │                                     │                │                                    │
│    │ WinINET system proxy (HKCU)         │                │                                    │
│    ▼  set/cleared by MultiTunnel         │                │                                    │
│  Xray core (supervised child process)    │                │                                    │
│    - inbound: loopback SOCKS :10808      │    VLESS +     │  xray (systemd, User=nobody)       │
│      and loopback HTTP :10809 ONLY       │    REALITY     │    - listens on 0.0.0.0:8443       │
│    - outbound: VLESS, flow               │ ═════════════▶ │    - REALITY borrows the TLS cert   │
│      xtls-rprx-vision, mux OFF           │    TCP :8443   │      of the gateway already on     │
│    - routing: geoip:private -> direct    │                │      127.0.0.1:443 (serverNames)   │
│    │                                     │                │      as its camouflage             │
│    │ default OS routing                  │                │    │                               │
│    ▼                                     │                │    ▼ freedom outbound              │
│  net-merge wintun adapter                │                │  default routing -> the internet   │
│    (bonding: spreads the tunnel's        │                │  (optionally through a VPN egress  │
│     individual TCP streams across        │                │   chain — invisible to the client) │
│     your WAN links)                      │                │                                    │
└──────────────────────────────────────────┘                └────────────────────────────────────┘
```

Walking through one request: the application speaks to the loopback proxy
because MultiTunnel has set the WinINET system proxy to `127.0.0.1:10809`. The
Xray core forwards it through the VLESS+REALITY outbound; because mux is off,
every proxied connection is its own TCP stream. Those streams leave through
net-merge's wintun adapter, which is the machine's single routing layer and
distributes the streams across the available uplinks. On the VM, xray
terminates REALITY on port 8443 — an unauthenticated handshake is proxied to
the real gateway on `127.0.0.1:443`, which is what makes the port look like an
ordinary HTTPS endpoint — and exits through a plain `freedom` outbound.

## Why it is built this way

These are deliberate design contracts, not accidents; changing any of them
breaks something subtle.

- **Mux must stay OFF.** Bonding works by spreading individual TCP
  connections across uplinks. A muxed tunnel collapses all traffic into a
  single stream, and one stream always rides one link — bonding would be
  defeated. The generated client config pins `"mux": { "enabled": false }`.
- **Loopback-only inbounds.** The Xray core exposes only SOCKS/HTTP on
  `127.0.0.1` and never creates a TUN inbound: net-merge's wintun adapter is
  the single system-wide routing layer. Two adapters fighting over the default
  route is exactly what this avoids.
- **LAN traffic stays local.** The generated routing rule sends
  `geoip:private` to a `direct` outbound, so local network traffic never
  takes the tunnel.
- **Fail-open proxy policy.** The system proxy is a machine-wide setting; a
  dead proxy pointing at a dead tunnel means "no internet". If the core dies,
  MultiTunnel restarts it with backoff; if the restart budget
  (`behavior.restartMax`) is exhausted, it restores (clears) the system proxy
  so normal browsing keeps working, and marks the state `Failed`. True
  fail-close is out of scope for v0.1.
- **op_lock lifecycle.** `connect`/`disconnect` are serialized on a
  whole-operation mutex (an operation can block for tens of seconds), which
  is why the tray runs every supervisor call on a worker thread instead of
  inside the window procedure. Exit is the hard case: the icon is removed
  first, a graceful disconnect gets a bounded window, and if that is not
  enough an op-lock-free `emergency_stop` tears the tunnel down anyway — the
  core can never be orphaned with the system proxy still set.
- **Stale-proxy self-heal.** `Ctrl+C` on `multitunnel connect` detaches with
  the proxy still set (deliberate: the tunnel keeps serving the apps already
  configured); the next `connect` restores a clean proxy state at step 0
  before bringing the tunnel up again.

The supervised connect sequence, in order: write the generated Xray config →
start the core and wait for the SOCKS port (15 s) → probe the full chain
through the tunnel (fail fast: stop core, restore proxy, state `Failed`) →
turn net-merge bonding on (non-fatal; a failure is recorded but the tunnel
stays up) → set the system proxy (non-fatal). Disconnect reverses the order.
While connected, a background loop polls every 5 s and probes the tunnel every
`probeIntervalSec`; core restarts back off 1 s, 2 s, 4 s, ... capped at 30 s.

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
firewall checklist for the VM and a ready-to-paste `"server"` block that
matches `config.example.json` field-for-field.

Environment knobs: `DOMAIN` (required — the existing gateway domain whose TLS
certificate the REALITY handshake borrows), `PORT` (default `8443`), `DEST`
(default `127.0.0.1:443` — the gateway on the same VM), `INSTALL_ROOT`
(default `/usr/local`), `FORCE=1` (overwrite an existing config — regenerates
all credentials). The REALITY private key is written only to the server
config; the script never prints it and it never leaves the VM.

## Configuration

MultiTunnel reads `config.json` next to the executable (or the path given with
`-config <path>`). Start from `config.example.json`; the schema is camelCase
and every field has a default. `config.json` is gitignored — credentials never
belong in the repo.

| Field | Default | Meaning |
|---|---|---|
| `server.address` | `35.208.99.240` | the VM's public address |
| `server.port` | `8443` | VLESS+REALITY port on the VM |
| `server.uuid` | — (required) | client UUID, from the server script's output |
| `server.sni` | — (required) | the gateway domain (REALITY serverName) |
| `server.publicKey` | — (required) | X25519 public key (client half) |
| `server.shortId` | — (optional) | REALITY shortId; may stay empty |
| `local.listen` | `127.0.0.1` | loopback bind address for the core's inbounds |
| `local.socksPort` | `10808` | loopback SOCKS inbound |
| `local.httpPort` | `10809` | loopback HTTP inbound (the system proxy target) |
| `paths.xray` | `bin/xray.exe` | Xray core binary (relative paths resolve against the exe dir) |
| `paths.netMerge` | `net-merge` | net-merge executable |
| `paths.xrayConfig` | `xray-client.json` | where the generated Xray config is written |
| `paths.logDir` | `logs` | core log (`xray.log`) lives here |
| `paths.stateDir` | `state` | proxy snapshot, raw status dumps |
| `netMerge.enable` | `true` | turn bonding on/off with the tunnel |
| `behavior.systemProxy` | `true` | set/clear the WinINET system proxy with the tunnel |
| `behavior.proxyOverride` | `localhost;127.*;10.*;192.168.*;172.16.*` … `172.31.*;<local>` | proxy bypass list |
| `behavior.probeTarget` | `example.com:80` | periodic full-chain probe target (host:port) |
| `behavior.probeIntervalSec` | `60` | probe interval in seconds |
| `behavior.restartMax` | `5` | core restart budget after failures (0 = unlimited) |

Conventions: strings left empty and numbers set to `0` fall back to these
defaults on load; purely relative paths are resolved against the executable's
directory (absolute or drive-rooted paths are used as-is). Validation requires
`server.uuid`, `server.sni` and `server.publicKey` (the error names exactly
the missing fields and points at the server script), and the SOCKS/HTTP ports
must differ.

## CLI reference

Both binaries accept `-config <path>` / `--config <path>` to override the
config location.

```
Usage: multitunnel [command] [-config <path>]

  (none)     print a pointer to multitunnel-tray (the tray app is the GUI binary)
  connect    connect headless, run until Ctrl+C
  generate   write the generated Xray client config and exit
  doctor     check environment (binaries, config, ports)
  version    print version
```

- `multitunnel doctor` — environment check; exits 1 if anything fails.
  Checks: elevation (warns when not elevated — net-merge and proxy changes
  need admin rights), the Xray binary exists and `xray version` works,
  net-merge status is reachable (warns instead when bonding is disabled in
  the config), the SOCKS/HTTP ports are free, the log/state dirs are
  writable, and `probeTarget` parses. The config is loaded and validated
  first, so an incomplete `config.json` fails before the checks run.
- `multitunnel generate` — renders the Xray client config to
  `paths.xrayConfig` (written with mode 0600 on POSIX; on Windows the default
  user-profile ACL applies — it embeds the VLESS credential) and exits. This
  is exactly what `connect` does internally as step 1.
- `multitunnel connect` — headless connect: elevates via UAC on Windows when
  needed, brings the tunnel up, prints the state/ports/bonding summary, and
  supervises until `Ctrl+C`. `Ctrl+C` detaches with the system proxy still
  set; the next `connect` self-heals it.
- `multitunnel version`, `multitunnel help` — what they say.
- `multitunnel-tray.exe` — the GUI. Elevates via UAC when needed, finds
  `config.json` beside the executable (or the working directory), and puts an
  icon in the notification area. On first run without a config it writes a
  template plus a `config-needed.txt` note and exits. Left- or right-click
  the icon for the menu (rebuilt fresh from the live state at every open):

  - `State: Disconnected / Connecting / Connected / Failed`
  - `Connect` (enabled while down or failed), `Disconnect` (enabled while a
    cycle is live)
  - `Links: N (merging)` — the bond status line
  - `Show raw status` — dumps the full status to `state\net-merge-status.txt`
    and opens it in Notepad
  - `Open log folder` — opens `paths.logDir` in Explorer
  - `Edit config` — opens the loaded `config.json` in Notepad
  - `Exit` — removes the icon, disconnects cleanly (with a bounded fallback
    to emergency teardown), and quits

  The tooltip refreshes once a second with the live state. If Explorer dies
  and restarts, the app re-registers its icon on the `TaskbarCreated`
  broadcast, so it never becomes an invisible background process.

## Building

Stable Rust (MSRV 1.78). From a checkout:

```sh
cargo build --release        # both binaries: multitunnel, multitunnel-tray
cargo test                   # host test suite
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
  ```

- The Windows-only dependencies (`windows-sys`, `winreg`) are feature-gated
  behind `cfg(windows)`; a Linux build compiles the library and console
  binary with stubs, which is how the test suite runs on Linux CI.

## Release and CI

- `.github/workflows/ci.yml` runs on every push to `main` and on pull
  requests: `cargo test` on a matrix of `ubuntu-latest` and `windows-latest`;
  a lint job enforcing `cargo fmt --check` and
  `cargo clippy --all-targets -- -D warnings`; and a cross-check job running
  `cargo zigbuild --target x86_64-pc-windows-gnu` plus
  `cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings`
  so the Windows-only code surface is linted too, not just the Linux one.
- `.github/workflows/release.yml` runs on tags matching `v*`: on a
  `windows-latest` runner it runs `cargo test` **first** (tag pushes bypass
  the CI workflow, so a release tag must never ship an untested build), then
  builds `--release`, packages both executables together with
  `config.example.json`, `LICENSE`, `THIRD_PARTY_NOTICES.md`, `README.md` and
  `QUICKSTART.md` into a zip, and publishes the GitHub Release
  (`fail_on_unmatched_files` is set so a packaging typo fails loudly instead
  of shipping an empty release).

## Security notes

- **Credentials.** `config.json` holds the VLESS UUID and REALITY public
  parameters and is gitignored; only the empty `config.example.json` is
  committed. The generated `xray-client.json` embeds the UUID and is written
  with mode 0600 on POSIX (default user-profile ACL on Windows). On the
  server, the REALITY private key lives only in the VM's
  `/usr/local/etc/xray/config.json` (root-owned) and is never printed by the
  setup script.
- **No secrets in logs.** Logs contain the Xray core's own output (warning
  level, `logs/xray.log`), net-merge status text and state summaries — never
  credentials.
- **Unsigned binaries.** Release artifacts are not code-signed: expect the
  Windows SmartScreen / "unknown publisher" warning and the UAC prompt.
  Administrator rights are required (the HKCU proxy writes and net-merge's
  wintun bonding need elevation). Only run binaries you built yourself or
  downloaded from this repository's Releases.
- **Fail-open.** See the design section: on unrecoverable failure the system
  proxy is restored rather than left pointing at a dead tunnel. After an
  abrupt termination (e.g. power loss) the proxy can remain set until the
  next `connect` or tray `Exit` run cleans it up.

## Licensing

MultiTunnel is MIT-licensed — see [LICENSE](LICENSE). Third-party components
(compile-time crates, and the user-installed Xray-core / net-merge binaries,
which are not redistributed) are listed in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Development

The project is developed under a staged harness with parallel adversarial
review — see [harness/HARNESS.md](harness/HARNESS.md) for the protocol,
[harness/ROADMAP.md](harness/ROADMAP.md) for the roadmap and stage history,
and `harness/memory/` for decisions, consensus verdicts and agent reports.
