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

From the MultiTunnel folder (an elevated terminal is recommended — the
system-proxy writes want administrator rights; expect the SmartScreen
"unknown publisher" warning for the unsigned binary):

```
multitunnel gui
```

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
  the active profile with one click (`profiles use` / `profiles export` on
  the CLI). Latency tests run per profile from the Servers screen.
- The v0.3 data plane bonds multiple links in-process (failover or
  lowest-latency merge policy in Settings). The legacy external net-merge
  tool is no longer needed; see the appendix if you used it with v0.2.

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
