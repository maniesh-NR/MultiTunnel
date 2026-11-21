# MultiTunnel

<p align="center">
  <img src="assets/banner.png" alt="MultiTunnel — one tunnel, every adapter" width="830">
</p>

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
│    │    merge: balanced (default)        │    TCP :8443   │      of the gateway already on     │
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
| `mt-net` | the data plane: SOCKS5 + HTTP inbounds, routing table, link bonding (balanced default / round-robin / priority / failover / lowest-latency), quarantine, live per-link load tracking |
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
uplinks (connection-level bonding, balanced by default — round-robin,
priority, failover and lowest-latency selectable). On the VM,
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

## Stream bonding (v0.5) — one logical connection across adapters

Per-connection spreading (since v0.3) assigns each TCP connection to ONE
link. Stream bonding splits the BYTES of every connection across ALL
bonded adapters and merges them on the server — the net-merge promise at
stream granularity:

- The client runs one carrier per bonded adapter: an ordinary
  VLESS+REALITY connection (per-adapter engine, `sendThrough` pinned)
  whose target is the merge daemon on the server (`127.0.0.1:<merge
  port>`, reached through the tunnel). No new ports are exposed.
- `mt-bond` frames the streams (24-byte headers: HELLO/OPEN/DATA/CLOSE/
  ACK/PING/PONG), spreads DATA frames across carriers under the merge
  policy (balanced = least RELATIVE load, priority = user adapter
  order, both saturation-aware since v0.8 — see below), reorders per
  session, and replays unacked frames on a carrier that dies — a Wi-Fi
  drop mid-download costs zero bytes. Cumulative ACKs bound the replay
  window; TCP-native backpressure provides flow control. The 1 s
  PING/PONG keepalives piggyback each carrier's measured delivery rate
  (v0.8), so both ends share one honest picture of every path's
  capacity; the reorder buffer is capped at 16 MiB per session with a
  backpressure pause when a carrier stalls (its TCP throttles the
  sender while the gap drains).
- `mt-merge-server` (the daemon, installed by
  `server/setup-merge-daemon.sh`) accepts the carriers, re-merges each
  session in order, dials the real target, and splits the reverse
  traffic back across all carriers.
- Bonding mode is a setting: `per-connection` (default, no server
  changes) or `stream`. Stream mode falls back to per-connection with a
  log line when the daemon is unreachable — never a black hole.

**Server requirement (v0.6.2):** the tunnel server's Xray must ALLOW the
merge target. Xray 26.x's freedom outbound blackholes loopback/private
targets dialed from VLESS by default (a server-side SSRF guard) — that
silently killed stream bonding (carriers "connected" but every byte
vanished) while per-connection kept working. `server/setup-xray-reality.sh`
now writes the narrow `finalRules` allow (`127.0.0.1:<merge port>`, tcp)
on fresh installs, and re-running it without FORCE idempotently patches
an EXISTING install in place (credentials untouched, config validated
with `xray run -test`, backup kept). The client side demands a
PING/PONG round trip per carrier before it trusts it, so a dead merge
path now fails loudly into the per-connection fallback instead of
black-holing.

The Status tab verifies the whole path live: a connection frame at the
top (tunnel state, the connect switch and the server dropdown stacked
compactly on the left, the combined bandwidth graph filling the rest of
the frame), then ONE scrollable page below it — wheel scrolling moves
everything (exit identity, carrier rows, every adapter) together, so
arbitrarily long adapter lists stay reachable. One row per carrier (RTT,
wire bytes), a color-coded legend on the combined graph (down =
primary, up = tertiary), per-adapter traffic graphs — press an adapter's
name on the Status tab (or the arrow on any Adapters-tab row) to expand
its own live chart — and the Exit identity card (see below).

## Exit identity (v0.5) — no hidden hops, verifiable in one click

The Status tab's "Check exit IP" rides the live tunnel and reports the
exit IP with geo/ISP plus a verdict against the active profile's server:

- exit == server -> single hop. MultiTunnel never inserts intermediary
  servers: each adapter's engine dials the profile endpoint directly.
- exit != server -> the exit is the SERVER's own egress (e.g. a VPN on
  the VM — the documented ProtonVPN scenario in
  `server/setup-xray-reality.sh`). If you expected the GCP IP and see a
  VPN provider's ASN instead, that is the VM's default route, not an
  added hop.

## Exclusions & routing import (v0.6) — v2rayN-style bypass lists

The Routing tab is a full ordered rule table (first match wins, actions
proxy / direct / reject — enforced in-process by the mt-net router, no
engine round-trip). Since v0.6 it can also **import exclusions from a
file** so selected traffic bypasses the tunnel and connects directly:

- **plain list** — one domain / IP / CIDR per line; `#` comments
  (full-line or inline). `example.com` becomes a `domainSuffix` rule
  (matches the domain and all subdomains), `10.20.0.0/16` an `ipCidr`
  rule, a bare IP becomes a `/32` (or `/128`) host route. Every entry
  imports as `direct`.
- **v2rayN routing JSON** — `{"routing":{"rules":[...]}}` (also a bare
  `{"rules":[...]}` or `[...]`): `domain:`/`full:`/plain domains and
  IP/CIDR arrays map onto our rules, keeping each rule's outbound
  (`direct`/`freedom` -> direct, `proxy` -> proxy, `block`/`blackhole` ->
  reject).
- `geosite:`/`geoip:`/`regexp:` entries are skipped with a per-entry
  reason (no geo databases in a zero-dependency build); `geoip:private`
  is already enforced by the engine.

Imports join the DRAFT (Apply activates them; Revert discards), and the
same import is available headless: `multitunnel routing import
<file>` (plus `multitunnel routing list`). A live session picks new
rules up on the next reconnect.

## Scheduling & bonding modes (v0.7/v0.8) — even spreading and adapter priority

The merge policy picks which bonded adapter carries each NEW connection
(Settings → Network), and — in stream mode — how every connection's BYTES
are striped across the carriers:

- `balanced` (default, v0.7; capacity-aware in v0.8) — each new
  connection goes to the adapter carrying the least traffic RIGHT NOW
  (a 1 s byte-rate estimate plus a small per-connection reserve). A fat
  download stops hogging one NIC and the spread evens out by BYTES, not
  by connection count. With three or more adapters this is the fix for
  "one adapter carries almost everything, the last one rarely sees
  traffic".
- `round-robin` — connections alternate across every ticked adapter;
  even by connection count, not by bytes (one big flow still dominates
  its adapter).
- `priority` (v0.7; capacity-aware in v0.8) — adapters fill in the
  priority order you set on the Adapters tab (↑/↓, or tick to append):
  adapter #1 takes everything until its measured bandwidth saturates,
  then the overflow spills to #2, and so on. When #1's load drops, it
  takes the traffic back automatically.
- `lowest-latency` — each connection takes the adapter with the best
  smoothed (EWMA) RTT.
- `failover` — everything rides the first healthy adapter; the others
  stay idle until it dies.

In stream bonding mode the same policy drives the frame scheduler:
`balanced` sends each frame down the carrier with the least RELATIVE
load, `priority` fills carriers in the Adapters-tab order until one
saturates. Upgrading from v0.6.x? The stock `round-robin` setting
migrates to `balanced` once; an explicit re-choice of `round-robin`
persists.

### Saturation-aware spreading (v0.8) — when one adapter maxes out

v0.7 measured saturation indirectly (byte counters, writer-queue
backpressure). v0.8 measures it DIRECTLY: every second each carrier's
PING carries a report of how many bytes/s that path actually DELIVERED,
and both ends compare it with what they OFFERED (an EWMA on each side).
The score is relative load — offered ÷ delivered:

- `balanced` picks the carrier with the LOWEST score, so a Wi-Fi link
  pinned at its ceiling (offered 1.6 MiB/s, delivered 1.0) immediately
  loses the next frames to the Ethernet link with headroom. Spreading
  is proportional to capacity: a 10:1 bandwidth mismatch yields ~10:1
  byte shares, not a 50/50 split that just overflows the slow path.
- `priority` keeps filling adapter #1 until its score crosses 1.15
  (offered ≥ 15% over delivered) or its writer queue backs up — then it
  spills to #2, and takes the traffic back the moment #1's score drops
  again.
- A carrier with no report yet (fresh, or an older peer) is probed at
  32 KiB/s instead of looking free, so unknown paths earn trust before
  they attract load — and a dead path that never reports never wins a
  frame.

This is the fix for "when one adapter reaches its maximum bandwidth, the
rest of the traffic does not split over to the remaining adapters":
saturation is now measured per-path, per-second, at both ends.

To use **every adapter for a single connection at the same time**, set
bonding mode to `stream` (Advanced): the bytes of every connection are
split across all ticked adapters and re-merged by `mt-merge-server` on
the tunnel server — see "Stream bonding (v0.5)" above.

## App identity & tray (v0.9–v0.12) — logo, blended title bar, close-to-tray

The window itself became a first-class part of the app (all of it in the
GUI, no new dependencies):

<img src="assets/logo_transparent.png" alt="MultiTunnel logo" width="110" align="right">

- **Your logo everywhere**: the window icon, the taskbar entry, the
  title bar and the tray icon all use the attached logo image
  (re-generated at build time by `scripts/make_logo_assets.py` —
  stdlib-only PNG decode, emblem crops for the small sizes, full lockup
  for 256 px). Since v0.10 the title-bar emblem rides on a TRANSPARENT
  background (flood-fill key-out with area-averaged resampling), so the
  mark blends into the window surface instead of sitting on a colored
  chip — same for the README banner above.
  Since v0.12 ALL the runtime icons (tray + taskbar/window icon) are
  transparent-background with a hue-preserving contrast boost and a
  tighter margin — the mark is larger and legible on light AND dark
  taskbars (no more opaque tile).
- **Chromium-style title bar**: the native window chrome is replaced by
  an in-app strip on the SAME surface as the page — logo + app name on
  the left, minimize/maximize/close on the right, and every other pixel
  of the strip drags the window (a fast second press maximizes, like a
  double-click on a classic title bar).
- **Title-bar right-click menu (v0.10)**: right-click anywhere on the
  strip for the window menu — **Always on top** (checkable, persisted
  across restarts, cross-platform via the window-manager's
  always-above state), Minimize, Maximize/Restore and Close. The menu
  opens at the cursor, Chromium-style.
- **Close-to-tray toggle** (Settings → Window & tray): when ON, the ✕
  button hides the window into the tray instead of exiting — the tunnel
  keeps running. OFF (default) closing really closes.
- **A persistent tray icon** while the app is open or hidden
  (Windows; raw Win32, same approach as the legacy `multitunnel-tray`
  companion): left-click reopens the window, and it survives an
  explorer.exe restart.
- **State-aware tray icon (v0.10)**: while connected the tray shows the
  colored logo; in every not-connected state it switches to a
  monochrome, transparent-background version of the same mark — the
  connection state is readable from across the room.
- **Live speeds on hover (v0.10, reworked v0.12)**: hovering the tray
  icon shows a three-line tooltip — the app name with the release
  version on line 1, the session's total transferred volume on line 2
  and the current download/upload speed on line 3 (e.g.
  `MultiTunnel v0.12.0` / `Total ↓3.0 GB ↑42.9 MB` /
  `↓9.4 MB/s ↑1.2 MB/s`).
- **Version + live speeds in the title bar (v0.11)**: the release
  version rides right next to the app name (it moves in lock-step with
  the release tag — one workspace version line), and the strip's right
  side shows the live global download/upload rates (the same 1 s
  throughput sample the tray tooltip reports), so the current speeds
  are readable without opening the app's Status tab.
- **Logs screen navigation (v0.11)**: the Logs tab scrolls only the log
  lines (toolbar stays put), with floating jump-to-top/jump-to-bottom
  buttons over the viewport's corner and an **Auto-scroll** toggle in
  the toolbar — ON by default, it keeps the newest line in view as
  events land; turn it off to read back through history without the
  view jumping to the tail.
- **Accent color wheel + appearance mode (v0.12)**: Settings → Appearance
  now has a full COLOR WHEEL — click or drag to pick ANY accent color
  (hue = angle, saturation = distance from the center; the theme
  re-derives live while you drag and persists on release). The 8 preset
  swatches and the numeric seed field stay for exact values. Next to it,
  the appearance mode picks **System / Light / Dark** — System follows
  the OS (the Windows Personalize setting / the freedesktop
  color-scheme), re-checked every few seconds while the app runs. A
  survey of the multipath-bonding literature (BLEST, ECF, MP-QUIC…) and
  how it maps onto the scheduler shipped alongside in `docs/research/`.
- **Server editing, offline probes, kill switch, diagnostics (v0.13)**:
  every server card gained an **Edit** button — name, remark, address,
  port, UUID and the REALITY fields (SNI, fingerprint, public key, short
  ID, spiderX) are editable after import; the id, the active selection and
  the list position are preserved, and an edit that would break a profile
  that used to connect is rejected with the reason. The Status tab's
  **Check exit IP** and the new **Speed test** work even while
  DISCONNECTED: both spawn a temporary probe tunnel through the active
  server (the same engine path a latency test uses) and tear it down
  after — the card shows whether the answer came through the live tunnel
  or a temporary one. The speed test downloads a payload through the
  server (URL configurable in Advanced settings) and reports Mbps, volume
  and duration; tunnel setup is excluded from the rate. Settings →
  Network gained a **kill switch**: when the tunnel dies on its own while
  MultiTunnel owns the system proxy, the proxy is re-pointed at a dead
  loopback port so traffic FAILS instead of silently leaking direct
  (fail-closed); connecting again — or toggling the switch off, or a
  manual disconnect — restores normal operation. Settings → Diagnostics
  writes one **paste-ready support log** (`diagnostics-<timestamp>.log`
  in the log folder): connection lifecycle events, engine output (log
  level forced to `debug` for the session), link health and a redacted
  settings/profile summary — UUIDs, passwords and public keys never
  appear in it. Turn it on, reproduce a connection issue, then paste the
  file into a support chat.
- **Quick Settings on right-click**: the tray menu is built fresh from
  live state — Connect/Disconnect (state-reflecting), the server list
  (radio-checked on the active profile), Open MultiTunnel and Exit.
  Selections ride the same paths as the GUI (switching a server from the
  tray reconnects exactly like the Status dropdown does).

On platforms without a tray the toggle degrades honestly: the window
minimizes instead of hiding, so it stays reachable.

## Rendering fixes (v0.6, v0.6.1)

v0.5.1 shipped three rendering defects, all root-caused and fixed via
surgical patches to the terminal iced 0.13 releases (vendored under
`vendor/`, see `vendor/*/README-MULTITUNNEL.md`): the Status/Adapters
graphs did not paint on incremental frames (canvas damage was computed
in the wrong coordinate space), canvas labels could render displaced
("out of frame"), and "Check exit IP" could abort the process with
`user-provided comparison function does not correctly implement a total
order` (NaN damage rectangles + a non-total sort comparator in
`iced_graphics::damage::group`).

v0.6.1 fixes the one that survived HiDPI screens: at a display scale
factor other than 100% (Windows' default 125%/150% on laptops) the
render backend composed each canvas' translation and the viewport scale
in the wrong order, displacing the whole graph — grid, series, labels —
`offset × (scale − 1)` px up and left of its card ("the axes of the
graph jump to the upper left, out of bounds", and the shifted canvas
could collide with the exit-IP card above it). Since v0.7.1 the Status
tab is one connection frame + one page scrollable: the graph lives
INSIDE the frame (fixed-height panel filling the space right of the
connect switch and server dropdown, so that space is used instead of a
wide dead gap) and everything below the frame — exit identity, carrier
rows, every adapter row — scrolls together; the old inner-scrollable
adapter list capped the visible rows at whatever space three stacked
cards left, which made long adapter lists effectively unusable. Press
an adapter's name to expand its own live traffic chart.

## The v0.3 way: GUI + profiles (recommended)

1. Double-click `multitunnel.exe` — since v0.3.1 the bare launch IS the
   GUI (the orphan console a double-click allocates is freed automatically;
   `multitunnel gui` from a terminal does the same and keeps the console).
   The Material You window opens (theme seed is configurable; dark mode
   included).
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

  (none)     launch the all-in-one Material You GUI (default — v0.3.1)
  gui        launch the all-in-one Material You GUI (--smoke: headless CI run)
  profiles   manage the multi-server profile store (list/add/use/remove/export)
  routing    routing rules: list | import <file> (v2rayN-style exclusions, v0.6)
  adapters   list network adapters and the bonded selection
  import     fill config.json's server block from a vless:// link
  connect    connect headless (legacy v0.2 path), run until Ctrl+C
  generate   write the generated Xray client config and exit
  doctor     check environment (binaries, config, ports)
  tray       pointer only: the tray app is the separate multitunnel-tray.exe
  version    print version
```

- Double-click / `multitunnel gui` — the all-in-one app (see above). Closing
  the window or Ctrl+C disconnects and restores the system proxy. A GUI
  launch from Explorer detaches its throwaway console, and a start failure
  without a usable console is reported in a MessageBox — the app can no
  longer die invisibly.
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

## Attribution

MultiTunnel stands on the shoulders of several projects, and the UI's
feature set follows two well-known tools:

- **[Xray-core](https://github.com/XTLS/Xray-core)** (Apache-2.0) — the
  VLESS + REALITY tunnel engine, fetched and SHA256-verified at runtime
  (never redistributed inside the release archives).
- **[iced](https://github.com/iced-rs/iced)** (MIT) — the GUI toolkit
  behind the Material You app. Two renderer crates are vendored in
  `vendor/` with tiny, documented damage-tracking fixes (see
  `vendor/*/README-MULTITUNNEL.md`); every other dependency comes
  straight from crates.io. The complete machine-generated dependency
  manifest with licenses lives in
  [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
- **Material You / Material Design 3** — Google's design language; the
  dynamic-color scheme is derived with the `material-colors` HCT color
  science crate, not copied assets.
- **[v2rayN](https://github.com/2dust/v2rayN)** — the Windows proxy
  client whose workflow (paste a `vless://` link, pick a server, done)
  MultiTunnel reimplements; the routing-import filter also accepts
  v2rayN routing JSON.
- **net-merge** — the link-bonding concept MultiTunnel generalizes into
  the in-process `mt-bond` stream merger.
- **BondTunnel** — this project's own predecessor (same author);
  MultiTunnel is its Rust rewrite.
- **The MultiTunnel logo and banner** are the project's own artwork
  (contributed by the author); the build-time variants (transparent,
  monochrome, ICO sizes) are derived deterministically by
  `scripts/make_logo_assets.py`.

Tooling that makes the zero-dependency cross-build possible:
**[cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild)** and
**[Zig](https://ziglang.org/)** (the C cross-toolchain for the Windows
builds), and the **Rust** language itself.

## Licensing

MultiTunnel is MIT-licensed — see [LICENSE](LICENSE). Third-party components
(compile-time crates including the fonts embedded in the GUI, and the
runtime-fetched Xray-core engine, which is not redistributed) are listed in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) — see also
[Attribution](#attribution) for the projects this app is built on.

## Development

The project is developed under a staged harness with parallel adversarial
review — see [harness/HARNESS.md](harness/HARNESS.md) for the protocol,
[harness/ROADMAP.md](harness/ROADMAP.md) for the roadmap and stage history,
and `harness/memory/` for decisions, consensus verdicts and agent reports.
