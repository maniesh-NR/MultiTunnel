# MultiTunnel QUICKSTART — the 10-minute path

You will end up with: a VLESS+REALITY endpoint on your VM, and the
all-in-one Material You app on your PC that imports your `vless://` link,
supervises the tunnel, bonds your links in-process, and manages the system
proxy. Do the steps in order; nothing before step 3 touches your Windows
machine's network settings.

## Step 0 — get the MultiTunnel binaries

Download the latest `multitunnel-<version>-windows-x64.zip` from the
[Releases](https://github.com/maniesh-NR/MultiTunnel/releases) page (or build
it yourself — see the README) and extract it into a folder you keep, e.g.
`C:\Tools\MultiTunnel`. Extracting yields a folder named `multitunnel-vX.Y.Z`
(with the release version, e.g. `multitunnel-v0.3.0`), so "extract into
`C:\Tools\MultiTunnel`" produces `C:\Tools\MultiTunnel\multitunnel-vX.Y.Z` —
work inside that folder; everything else in this guide happens relative to
it. Its full contents: `multitunnel.exe`, `multitunnel-tray.exe`,
`config.example.json`, `README.md`, `QUICKSTART.md`, `LICENSE`,
`THIRD_PARTY_NOTICES.md`, and `server/setup-xray-reality.sh` (the server
installer script used in step 1).

Nothing else to install: the Xray engine is downloaded and verified by the
app itself on first connect, and link bonding runs in-process.

## Step 1 — run the server script on the VM (about 3 minutes)

SSH to your Ubuntu/Debian VM, copy `server/setup-xray-reality.sh` there (or
fetch the repo), and run it as root with the domain of the gateway that
already runs on the VM:

```sh
sudo DOMAIN=your-gateway.example.org bash setup-xray-reality.sh
```

The script installs the latest Xray-core, generates the credentials, writes
the hardened systemd unit, and starts the endpoint. When it finishes:

1. Do the printed **firewall checklist** — both layers must allow your port
   (default 8443) inbound:
   - the GCP firewall rule (exact `gcloud` command printed by the script; the
     port has been closed externally until now), and
   - on the VM: `iptables -I INPUT -p tcp --dport 8443 -j ACCEPT && netfilter-persistent save`
     (check the existing rules with `iptables -L INPUT -n` first — the exact
     commands are in the script output).
2. **Copy the `vless://` share link** the script prints at the end — you
   will paste it in step 3. (It also prints the legacy `"server": { ... }`
   block; that is only needed for the legacy path in the appendix.)

Sanity check on the VM: `systemctl status xray` should show active (running).

## Step 2 — start the app

Double-click `multitunnel.exe`. Since v0.3.1 the bare launch IS the app: the
Material You window opens, and the throwaway console Windows allocates for a
double-clicked console binary is freed automatically (a console may flash for
a split second first — cosmetic). Prefer a terminal? `multitunnel gui` does
the same and keeps the console for log output; no administrator rights are
needed for the usual per-user system-proxy writes (expect the SmartScreen
"unknown publisher" warning for the unsigned binary either way).

> v0.3.0 note: in the first release the bare launch defaulted to a `tray`
> pointer that printed one line and exited — a double-click looked like
> "nothing happens". v0.3.1 made the GUI the default, and any start failure
> without a usable console now shows a MessageBox instead of dying silently.

**SMOKE TEST — this is the first time the GUI binary runs on real Windows.**
The development sandbox is Linux; the window has been cross-compiled and
unit-tested, never displayed. Do not walk away after launching it — verify
each point below before calling it working:

1. The Material You window opens (Connect dashboard, Servers, Routing,
   Settings screens in the left rail).
2. The theme renders with the baseline purple seed; Settings → theme seed
   and dark mode change it live.
3. Closing the window exits the app cleanly (no lingering
   `multitunnel`/`xray` processes in Task Manager).

## Step 3 — import your server and connect (about 1 minute)

1. Open the **Servers** screen, paste the `vless://` link from step 1 into
   the import field — a preview shows the protocol/endpoint before you
   add it — and add it. (CLI equivalent: `multitunnel profiles add
   "vless://…"` then `multitunnel profiles use <tag>`.)
2. Flip the **connect switch** on the dashboard. First connect downloads
   the engine (progress in the log pane), then brings the tunnel up:
   state flips to `Connected`, the link health line appears, and browsing
   goes through the tunnel (system proxy set to `127.0.0.1:10809`).
3. Browse to any site to confirm; on the **Routing** screen note the stock
   rules — private/LAN CIDRs go direct, so local devices keep working.
4. Flip the switch off — the state returns to `Disconnected` and the
   system proxy is cleared (Settings > Network > Proxy if you want to see
   it).

If anything fails, the dashboard log pane (and `multitunnel profiles list`
from a terminal) is the first stop for clues.

## Step 4 (optional) — multi-server profiles and bonding

- Add as many `vless://` links as you like on the Servers screen and switch
  the active profile with one click — or with the server dropdown on the
  Status tab (switching while connected reconnects automatically).
  Latency tests run per profile from the Servers screen.
- The data plane bonds multiple links in-process (merge policy in
  Settings: **round-robin by default** — new connections alternate across
  every ticked adapter, so both NICs carry traffic; `failover` keeps
  everything on the first healthy adapter; `lowest-latency` picks the
  lowest-RTT adapter per connection). The legacy external net-merge tool
  is no longer needed; see the appendix if you used it with v0.2.
- Every adapter row on the Adapters tab has an arrow (▸) that expands a
  live per-interface traffic graph — the same two color-coded lines as the
  Status graph (down = primary, up = tertiary), so you can SEE which NIC
  carries what. Upgrading from v0.5.0? The stock merge policy migrates to
  round-robin automatically (Settings shows it; pick `failover` there if
  you really want the old behavior).

## Step 5 (optional) — stream bonding: one connection, split across adapters

Per-connection spreading (Step 4) sends each TCP connection down ONE
adapter. Stream bonding (v0.5) goes further — exactly what the net-merge
promise means at stream granularity: the system sends and receives as if
it used one connection, the BYTES of every connection are split across
the bonded adapters, and the server merges them into single requests
again. Reverse traffic is split the same way back.

1. On the VM, install the merge daemon (one command, downloads the
   release binary and registers a systemd service):
   `sudo ./server/setup-merge-daemon.sh`
   (it listens on 127.0.0.1:7443 — loopback only, never exposed).
2. In MultiTunnel: Settings -> Advanced -> bonding mode = "stream"
   (leave the merge port at 7443 unless you changed it on the VM).
3. Adapters tab: tick the adapters to bond, Apply, connect.

The Status tab then shows one row per bonded adapter (carrier) with live
RTT and byte counters, and the log names each carrier's verdict at connect
(`INFO  carrier <name>: live` / `WARN … down (<reason>)`) — a dead adapter
is never silent. If the daemon is not reachable, MultiTunnel logs
`stream bonding unavailable` and falls back to per-connection spreading —
the tunnel never goes down because of a daemon problem. A carrier that
dies mid-stream (Wi-Fi drop) is replayed on the surviving carriers: no
bytes are lost.

## Step 6 — verify your exit (the Status tab's Exit identity card)

"Check exit IP" answers the classic "which server am I actually exiting
through?" question in one click: it rides the live tunnel and shows the
exit IP, its country/city/ISP, and a verdict.

- `single hop — the exit IS your server` — the exit matches your profile's
  endpoint. There are no hidden hops: MultiTunnel adds none, ever.
- `extra egress hop` — the exit differs from your server. This is NOT a
  MultiTunnel hop: it means the server's OWN outbound egress goes through
  something else (typically a VPN on the VM, e.g. ProtonVPN — see the
  appendix below). If the exit ISP is not what you expect, check the VM.

The bandwidth graph above it plots the last two minutes of combined
inbound/outbound throughput, one sample per second, with a color-coded
legend (down = primary, up = tertiary) and current rates at the top
right; it scales with the window (Task Manager style).

## Appendix — the legacy v0.2 path (config.json + tray)

The v0.2 commands still work and are documented in the README. The short
version:

1. `copy config.example.json config.json`, paste the `"server"` block from
   the server script over the existing one (all six values), save.
2. Put `xray.exe` (from the official Xray-core Windows zip) at
   `bin\xray.exe` with `geoip.dat`/`geosite.dat`, or run `multitunnel
   import "vless://…"` to fill the server fields from the share link.
3. `multitunnel doctor` from an elevated terminal — every line should read
   `[ ok ]` (a `[warn]` on elevation only means the terminal is not
   elevated; a net-merge failure only matters if you use external
   net-merge bonding — install it from
   https://github.com/net-merge/net-merge next to `multitunnel.exe` and
   start `net-merge tray` once, or set `"netMerge": { "enable": false }`).
4. `multitunnel-tray.exe` is the notification-area app; `multitunnel
   connect` is the headless equivalent (Ctrl+C detaches with the system
   proxy still set — the next connect self-heals it).

## ProtonVPN on the VM (no action needed)

If the VM's own internet egress is routed through ProtonVPN, nothing changes
for the tunnel: your client connects to the VM's external IP on port 8443,
and xray's `freedom` outbound simply follows the VM's default routing — into
the VPN, if that is where it points. Server-side VPN rotation (or the VPN
being down entirely) is invisible to the client: the tunnel endpoint does
not move, and the client never notices.

## Where to go next

- README.md — architecture, design rationale, full config and CLI reference.
- The dashboard log pane and `multitunnel doctor` are the first stops
  whenever something misbehaves.
- Server side: `systemctl status xray` / `journalctl -u xray -f` on the VM.
