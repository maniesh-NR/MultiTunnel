# S4 Implementation — Server script + README + QUICKSTART

- **Implementer:** S4 agent (implementation round, stage S4)
- **Date:** 2026-11-13, repo base 06ad6aa (S3 closed), branch main
- **Scope:** three new files only — `server/setup-xray-reality.sh` (new
  `server/` dir, chmod +x), `README.md`, `QUICKSTART.md`. No change to
  `src/`, `.github/`, `Cargo.*`, `config.example.json`, or any harness file
  except this report (constraint honored; `git status` shows exactly the
  three additions).

## 1. `server/setup-xray-reality.sh` (438 lines)

Self-contained bash installer for the VLESS+REALITY endpoint on the user's
Ubuntu/Debian VM. `set -euo pipefail`; `[setup]`-prefixed log/warn/die.

- **Argument validation FIRST (unprivileged, no network):** DOMAIN required
  and DNS-charset-checked (letters/digits/dots/hyphens, no leading/trailing
  dot or hyphen, at least one dot — also blocks JSON/CLI injection through
  the heredocs); PORT numeric 1-65535; DEST `host:port` or `[v6]:port` with
  port range check; INSTALL_ROOT absolute and whitespace-free (it is baked
  into the systemd ExecStart line); FORCE 0/1; stray positional args
  rejected; `-h/--help` prints usage. Then: root check (`id -u`), systemctl
  presence, deps check (curl/unzip/openssl — dies with the exact
  `apt-get update && apt-get install -y ...` command).
- **Latest release, never hardcoded:** GitHub API
  `releases/latest` → `sed` parse of `tag_name` (validated `^v[0-9]...`),
  with a documented fallback to the `releases/latest` redirect
  (`url_effective`) because the unauthenticated API is per-IP rate-limited —
  this mattered in the sandbox itself (the API returned 403 rate-limit while
  the redirect still resolved to v26.11.9).
- **Download → verify → install (no curl|bash):** zip to `mktemp -d` (0700,
  trap-cleanup on EXIT), `unzip -t` integrity check, extract
  xray/geoip.dat/geosite.dat, `xray version` smoke test, stop a running
  xray service first (ETXTBSY), then `install -D` 0755 binary + 0644 geo
  assets into `$INSTALL_ROOT/bin` and `$INSTALL_ROOT/share/xray`.
- **Credentials:** `xray uuid`; `xray x25519` parsed with a case-insensitive
  sed that handles BOTH output generations — v26+ prints
  `PrivateKey:` / `Password (PublicKey):` (verified against the real
  v26.11.9 binary) while older releases printed `Private key:` /
  `Public key:` (verified against a synthetic sample); `openssl rand -hex 8`
  shortId.
- **Server config** `$INSTALL_ROOT/etc/xray/config.json`: VLESS inbound
  0.0.0.0:$PORT, client flow `xtls-rprx-vision`, streamSettings
  security=reality with dest/serverNames/privateKey/shortIds, freedom
  outbound; written 0644 root:root (User=nobody must read it); private key
  documented to stay VM-local (JSON has no comments — documented in the
  echo output). Existing config + FORCE!=1 → refusal with re-run
  instructions and the regeneration warning. **The generated config was
  validated for real:** the script's own heredoc, extracted verbatim and fed
  real xray-generated credentials, passes `xray run -test` ("Configuration
  OK.") against the real v26.11.9 linux-64 binary — and that `-test` gate
  is built into the script (hard fail with diagnostics).
- **systemd unit** `/etc/systemd/system/xray.service`: User=nobody,
  Environment=XRAY_LOCATION_ASSET, Restart=always, RestartSec=3,
  NoNewPrivileges=true, ProtectSystem=strict, ProtectHome=true,
  PrivateTmp=true; daemon-reload + enable + restart; `sleep 2` then
  `systemctl is-active --quiet`; on failure dumps `journalctl -u xray -n 30`
  and dies naming the service state.
- **Firewall checklist (this VM's real state):** (a) GCP rule for tcp/$PORT
  inbound with the exact `gcloud compute firewall-rules create` command,
  noting the port was previously closed externally and suggesting narrower
  source-ranges; (b) netfilter-persistent/rules.v4 with the exact
  `iptables -I INPUT -p tcp --dport $PORT -j ACCEPT && netfilter-persistent
  save` one-liner, preceded by an explicit "check existing rules first"
  step.
- **Client block:** ready-to-paste `"server": { address/port/uuid/sni/
  publicKey/shortId }` matching `config.example.json` field-for-field
  (verified by diffing the rendered keys/order against the example file).
  Public IP: GCP metadata server preferred (unaffected by VPN egress),
  `api.ipify.org` fallback (warned — behind a VPN egress it reports the VPN
  exit IP), placeholder last; plus the ProtonVPN note (server-side rotation
  is orthogonal; xray's freedom outbound uses default routing) and the
  35.208.99.240 cross-check hint (D8).

## 2. `README.md` (17 KB)

What it is (both binaries), ASCII layering diagram (apps → WinINET proxy →
loopback xray inbounds → VLESS+REALITY → VM systemd xray → freedom; net-merge
wintun as the single routing layer), the key design rationale (mux OFF,
loopback-only inbounds, fail-open proxy policy, op_lock lifecycle +
emergency-stop exit path, stale-proxy self-heal, connect order, backoff
numbers), server-setup pointer, config reference table (all 23 fields with
defaults matching `config.rs`, plus the empty/0-fallback and exe-dir path
conventions), CLI reference for doctor/generate/connect/version/help and the
tray binary (menu items verified against `menu_plan`, raw-status dump path
verified, first-run template behavior verified), build instructions (stable
Rust, native MSVC, cargo-zigbuild cross), CI/release story (ci.yml matrix +
windows-target clippy; release.yml test-then-build on tags, zip contents,
fail_on_unmatched_files), security notes (0600 writes, no secrets logged,
gitignored config, unsigned-binary SmartScreen, UAC/admin, fail-open), MIT +
THIRD_PARTY_NOTICES, harness pointer. Every CLI claim was verified by
building and running the console binary (`version`, no-arg pointer error,
`doctor` on a filled config, `generate` writing 0600 xray-client.json with
mux off). No emoji anywhere.

## 3. `QUICKSTART.md` (7 KB)

Ten-minute path, ordered: (0) get the release zip, (1) run the server script
+ firewall checklist + copy the client block, (2) install net-merge +
`net-merge tray`, (3) Xray-core windows-64 zip → `bin\xray.exe` **plus
geoip.dat/geosite.dat into `bin\`** (needed: the generated client config
routes `geoip:private` direct and xray finds geo assets next to its exe),
(4) `copy config.example.json config.json` + paste block, (5) elevated
`multitunnel doctor` all green (elevation-warn nuance documented), (6)
`multitunnel-tray.exe` with the **bold SMOKE TEST warning** — first run on
real Windows, with the seven checkpoints: icon appears, left-click opens
menu, menu dismisses on outside click (KB135788 fix), Connect →
`State: Connected`, explorer restart via `Taskkill /IM explorer.exe /f` +
restart → icon comes back (TaskbarCreated recovery), Disconnect/Exit clean,
(7) optional headless `multitunnel connect` with the Ctrl+C/proxy note.
ProtonVPN chain note included (rotation orthogonal, client never notices).

## Verification

Script: `bash -n` clean; validation matrix run for real (no DOMAIN, bad
PORT, bad DEST, DEST port 0, bad DOMAIN charset/no-dot/leading-dot, relative
and whitespace INSTALL_ROOT, bad FORCE, stray args, `--help`) — all clean
`[setup] ERROR:` + exit 1; valid boundaries (mid-hyphen domain, PORT 1 and
65535, `[::1]:443` DEST, uppercase DOMAIN, FORCE=0) reach the root check,
which correctly fails in the unprivileged sandbox; `xray x25519`/`uuid`
parse, tag parse (compact + spaced JSON samples + live redirect), zip
download/integrity/extract, `xray run -test` on the generated config, and
the rendered client block were all exercised against the REAL v26.11.9
release artifacts (then cleaned from /tmp).

Gates (docs-only change, but rules are rules): `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings` (host),
`cargo clippy --all-targets --target x86_64-pc-windows-gnu -- -D warnings`,
`cargo test` **78/78**, `cargo zigbuild --target x86_64-pc-windows-gnu`
(both PEs re-confirmed PE32+ console + GUI). No source file touched.

## Known residuals (for reviewers)

- `multitunnel` (console) with no command still prints the S3-era message
  "tray lands in stage S3 — build multitunnel-tray"; the README describes
  the actual behavior (a pointer to the GUI binary) without quoting the
  stale wording. Fixing the string would touch frozen `src/main.rs` — left
  for the orchestrator to arbitrate.
- Public-IP detection prefers the GCP metadata server over api.ipify.org
  (spec listed ipify as the source): on this user's VM the egress may be
  ProtonVPN-routed, in which case ipify reports the VPN exit IP and the
  client block would be wrong. ipify remains the network fallback, with a
  warning when it is the source.
- The gcloud "check first" step prints a plain
  `gcloud compute firewall-rules list` rather than an invented filter
  expression (unverifiable here, no gcloud in sandbox).
