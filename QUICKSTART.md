# MultiTunnel QUICKSTART — the 10-minute path

You will end up with: a VLESS+REALITY endpoint on your VM, and a Windows tray
app that supervises the tunnel, bonds your links, and manages the system
proxy. Do the steps in order; nothing before step 5 touches your Windows
machine's network settings.

## Step 0 — get the MultiTunnel binaries

Download the latest `multitunnel-<version>-windows-x64.zip` from the
[Releases](https://github.com/maniesh-NR/MultiTunnel/releases) page (or build
it yourself — see the README) and extract it into a folder you keep, e.g.
`C:\Tools\MultiTunnel`. The folder now contains `multitunnel.exe`,
`multitunnel-tray.exe`, `config.example.json`, `LICENSE` and
`THIRD_PARTY_NOTICES.md`. Everything else in this guide happens relative to
that folder.

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
2. **Copy the client config block** the script prints at the end (the
   `"server": { ... }` block) — you will paste it in step 4.

Sanity check on the VM: `systemctl status xray` should show active (running).

## Step 2 — install net-merge on Windows (about 2 minutes)

Install net-merge from https://github.com/net-merge/net-merge (it is the link
bonding layer — a separate project that MultiTunnel drives). Make sure
`net-merge` is on your PATH or sits next to `multitunnel.exe` (the default
config looks for it there), plug in your extra uplink(s), and start its tray
app once:

```
net-merge tray
```

Bonding is optional: if you skip net-merge, set `"netMerge": { "enable": false }`
in the config later and MultiTunnel will run the tunnel without bonding.

## Step 3 — place the Xray core (about 2 minutes)

1. Download the latest `Xray-windows-64.zip` from
   https://github.com/XTLS/Xray-core/releases.
2. From the zip, copy into the MultiTunnel folder:
   - `xray.exe` to `bin\xray.exe` (create the `bin` subfolder — this is the
     default `paths.xray` location),
   - `geoip.dat` and `geosite.dat` also into `bin\` (the generated client
     config routes `geoip:private` traffic direct, so the core needs its geo
     files next to the executable).

## Step 4 — create your config (about 1 minute)

In the MultiTunnel folder:

```
copy config.example.json config.json
notepad config.json
```

Paste the server block from step 1 over the existing `"server": { ... }`
block (replace all six values — `address`, `port`, `uuid`, `sni`,
`publicKey`, `shortId`). Leave the rest of the file as is.

## Step 5 — `multitunnel doctor` (all green)

From an elevated terminal (the checks expect administrator rights):

```
multitunnel doctor
```

Every line should read `[ ok ]` — elevation, xray binary, net-merge, socks
port, http port, log dir, state dir, probe target, ending with
`all checks passed`. A `[warn]` on elevation only means the terminal itself
is not elevated. If the net-merge line fails, re-check step 2 (or disable
bonding in the config).

## Step 6 — run the tray app (first run = the real smoke test)

**SMOKE TEST — this is the first time the tray binary runs on real Windows.**
The development sandbox is Linux; the Windows UI has only been cross-compiled
and unit-tested, never run. Do not walk away after launching it — verify each
point below before calling it working:

1. Start `multitunnel-tray.exe` (accept the UAC prompt — net-merge and the
   system proxy need administrator rights; also expect the SmartScreen
   "unknown publisher" warning for the unsigned binary).
2. The MultiTunnel icon appears in the notification area (it may be under the
   `^` overflow chevron; drag it to the taskbar for the test).
3. **Left-click** the icon — the menu opens, first row showing
   `State: Disconnected`, plus Connect / Disconnect / the Links line /
   Show raw status / Open log folder / Edit config / Exit.
4. **Click elsewhere** on the desktop — the menu dismisses immediately (this
   checks the foreground-window fix for the classic stuck-menu bug).
5. **Connect** — the state row (and the hover tooltip) flips to
   `State: Connected` and the Links line shows your bonded links. Browsing
   now goes through the tunnel (system proxy set to `127.0.0.1:10809`).
6. **Explorer restart** — from an elevated command prompt run
   `Taskkill /IM explorer.exe /f`, then restart Explorer (Task Manager:
   File > Run new task > `explorer.exe`, or `start explorer.exe` in the
   still-open console). The MultiTunnel icon must come back with the
   taskbar — that is the shell-restart recovery. If it does not, use Task
   Manager to kill `multitunnel-tray.exe` and report it.
7. Disconnect, then Exit — the icon disappears, the system proxy is cleared
   (check Settings > Network > Proxy if you want to see it).

If anything in the list fails, close the app via Task Manager, and check
`logs\xray.log` and the state files in the MultiTunnel folder for clues.

## Step 7 (optional) — headless connect

Without the tray, from an elevated terminal:

```
multitunnel connect
```

It prints the state, the local proxy ports and the bond summary, then
supervises until you press `Ctrl+C`. Note that `Ctrl+C` detaches with the
system proxy still set; the next `connect` (or tray Exit) cleans it up.

## ProtonVPN on the VM (no action needed)

If the VM's own internet egress is routed through ProtonVPN, nothing changes
for the tunnel: your client connects to the VM's external IP on port 8443,
and xray's `freedom` outbound simply follows the VM's default routing — into
the VPN, if that is where it points. Server-side VPN rotation (or the VPN
being down entirely) is invisible to the client: the tunnel endpoint does not
move, and the client never notices.

## Where to go next

- README.md — architecture, design rationale, full config and CLI reference.
- `multitunnel doctor` is the first stop whenever something misbehaves.
- Server side: `systemctl status xray` / `journalctl -u xray -f` on the VM.
