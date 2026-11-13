#!/usr/bin/env bash
# server/setup-xray-reality.sh — MultiTunnel server endpoint installer.
#
# Installs a VLESS + REALITY Xray server on YOUR OWN Ubuntu/Debian VM:
#   - resolves and downloads the LATEST Xray-core release from GitHub
#     (the version is never hardcoded in this script),
#   - generates a fresh client UUID, X25519 key pair and REALITY shortId,
#   - writes the server config to $INSTALL_ROOT/etc/xray/config.json,
#   - installs a hardened systemd unit (User=nobody) and starts it,
#   - prints the firewall checklist for this VM (GCP rule + netfilter-persistent)
#     and a READY-TO-PASTE client block matching the repo's config.example.json.
#
# The REALITY handshake borrows the TLS certificate of an existing HTTPS site
# that already runs on this VM (the Pangolin gateway on 127.0.0.1:443): its
# domain is passed as DOMAIN (used as serverNames) and its address as DEST.
#
# Usage (run as root, on the VM):
#   sudo DOMAIN=your-gateway.example.org bash server/setup-xray-reality.sh
#
# Environment knobs:
#   DOMAIN        REQUIRED. The existing gateway domain whose TLS certificate
#                 the REALITY handshake borrows (becomes serverNames, and the
#                 client's sni).
#   PORT          Optional. VLESS listen port (default 8443).
#   DEST          Optional. REALITY dest — where unauthenticated handshakes are
#                 proxied (default 127.0.0.1:443, the gateway on this VM).
#   INSTALL_ROOT  Optional. Installation prefix (default /usr/local). Files:
#                 $INSTALL_ROOT/bin/xray, $INSTALL_ROOT/share/xray/*.dat,
#                 $INSTALL_ROOT/etc/xray/config.json.
#   FORCE         Optional. FORCE=1 overwrites an existing config — and
#                 REGENERATES every credential (the old client block stops
#                 working; paste the newly printed one).
#
# The script never pipes anything into bash (no curl|bash): the release zip is
# downloaded to a temp dir, integrity-checked, extracted, and only then moved
# into place with install(1).

set -euo pipefail

# ---------------------------------------------------------------- helpers ----

log()  { printf '[setup] %s\n' "$*"; }
warn() { printf '[setup] WARN: %s\n' "$*" >&2; }
die()  { printf '[setup] ERROR: %s\n' "$*" >&2; exit 1; }

usage() {
  cat <<'EOF'
Usage: sudo DOMAIN=<your-gateway-domain> [PORT=8443] [DEST=127.0.0.1:443] \
         [INSTALL_ROOT=/usr/local] [FORCE=1] bash server/setup-xray-reality.sh

Environment:
  DOMAIN        REQUIRED  existing gateway domain whose TLS certificate the
                          REALITY handshake borrows (serverNames / client sni)
  PORT          optional  VLESS listen port (default 8443)
  DEST          optional  REALITY dest, default 127.0.0.1:443 (the gateway)
  INSTALL_ROOT  optional  install prefix (default /usr/local, must be absolute)
  FORCE         optional  1 = overwrite an existing xray config (regenerates
                          ALL credentials — the old client block dies)

Example:
  sudo DOMAIN=tunnel.example.org PORT=8443 bash setup-xray-reality.sh
EOF
}

# ------------------------------------------------------- argument checking ----
# Pure validation runs FIRST so misconfigurations are reported without root
# and without touching the network.

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  usage
  exit 0
fi
if [ "$#" -gt 0 ]; then
  usage >&2
  die "unexpected argument(s): $* (all options come from environment variables)"
fi

DOMAIN="${DOMAIN:-}"
PORT="${PORT:-8443}"
DEST="${DEST:-127.0.0.1:443}"
INSTALL_ROOT="${INSTALL_ROOT:-/usr/local}"
FORCE="${FORCE:-0}"

[ -n "$DOMAIN" ] || { usage >&2; die "DOMAIN is required (the gateway domain whose TLS certificate REALITY borrows)"; }

case "$DOMAIN" in
  *[!A-Za-z0-9.-]*)
    die "DOMAIN must be a plain DNS name (letters, digits, dots, hyphens); got: $DOMAIN" ;;
  .*|*.|-*|*-) # leading/trailing dot or hyphen is never valid
    die "DOMAIN must not start or end with a dot or hyphen; got: $DOMAIN" ;;
  *.*) : ;;
  *) die "DOMAIN should be a fully-qualified domain (contains at least one dot); got: $DOMAIN" ;;
esac

case "$PORT" in
  ''|*[!0-9]*) die "PORT must be a number between 1 and 65535; got: $PORT" ;;
esac
if [ "$PORT" -lt 1 ] || [ "$PORT" -gt 65535 ]; then
  die "PORT must be between 1 and 65535; got: $PORT"
fi

# DEST: host:port (hostname or IPv4; bracketed IPv6 also accepted).
if ! printf '%s' "$DEST" | grep -Eq '^([A-Za-z0-9.-]+|\[[0-9A-Fa-f:]+\]):[0-9]{1,5}$'; then
  die "DEST must be host:port (e.g. 127.0.0.1:443); got: $DEST"
fi
DEST_PORT="${DEST##*:}"
if [ "$DEST_PORT" -lt 1 ] || [ "$DEST_PORT" -gt 65535 ]; then
  die "DEST port must be between 1 and 65535; got: $DEST"
fi

case "$INSTALL_ROOT" in
  /*) : ;;
  *) die "INSTALL_ROOT must be an absolute path; got: $INSTALL_ROOT" ;;
esac
case "$INSTALL_ROOT" in
  *[[:space:]]*) die "INSTALL_ROOT must not contain whitespace (it is baked into the systemd ExecStart line); got: $INSTALL_ROOT" ;;
esac

case "$FORCE" in
  0|1) : ;;
  *) die "FORCE must be 0 or 1; got: $FORCE" ;;
esac

# ------------------------------------------------------------ environment ----

[ "$(id -u)" -eq 0 ] || die "must run as root — try: sudo DOMAIN=$DOMAIN bash \"$0\""

command -v systemctl >/dev/null 2>&1 || die "systemctl not found — this script targets Ubuntu/Debian with systemd"

MISSING=""
for dep in curl unzip openssl; do
  command -v "$dep" >/dev/null 2>&1 || MISSING="$MISSING $dep"
done
if [ -n "$MISSING" ]; then
  die "missing required tool(s):$MISSING — install them with:
  apt-get update && apt-get install -y$MISSING"
fi

XRAY_BIN="$INSTALL_ROOT/bin/xray"
ASSET_DIR="$INSTALL_ROOT/share/xray"
CONF_DIR="$INSTALL_ROOT/etc/xray"
CONFIG_PATH="$CONF_DIR/config.json"
UNIT_PATH="/etc/systemd/system/xray.service"
API_LATEST="https://api.github.com/repos/XTLS/Xray-core/releases/latest"
REDIRECT_LATEST="https://github.com/XTLS/Xray-core/releases/latest"
DL_BASE="https://github.com/XTLS/Xray-core/releases/download"

if [ -e "$CONFIG_PATH" ]; then
  if [ "$FORCE" != "1" ]; then
    die "config already exists: $CONFIG_PATH
  To regenerate it (NEW uuid, NEW keys, NEW shortId — the old client block
  stops working), re-run with FORCE=1:
    sudo FORCE=1 DOMAIN=$DOMAIN PORT=$PORT bash \"$0\"
  If you only lost the client block, copy it from the machine where you
  first ran this script instead."
  fi
  warn "FORCE=1 — $CONFIG_PATH will be OVERWRITTEN with fresh credentials"
fi

# -------------------------------------------------------------- work dir ----

WORKDIR=""
cleanup() {
  if [ -n "${WORKDIR:-}" ]; then
    rm -rf "$WORKDIR"
  fi
}
trap cleanup EXIT
WORKDIR="$(mktemp -d)"
chmod 0700 "$WORKDIR"

# --------------------------------------------- resolve the LATEST release ----

log "resolving the latest Xray-core release tag via the GitHub API..."
TAG=""
API_BODY="$(curl -fsSL --retry 3 --max-time 30 "$API_LATEST" 2>/dev/null || true)"
if [ -n "$API_BODY" ]; then
  TAG="$(printf '%s' "$API_BODY" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
fi
if [ -z "$TAG" ]; then
  # GitHub's unauthenticated API is rate-limited per source IP; the
  # releases/latest redirect resolves to the same tag without the API.
  warn "GitHub API returned no tag (rate limit?) — falling back to the releases/latest redirect"
  TAG="$(curl -fsSLI --max-time 30 -o /dev/null -w '%{url_effective}' "$REDIRECT_LATEST" 2>/dev/null | sed 's#.*/tag/##' || true)"
fi
printf '%s' "$TAG" | grep -Eq '^v[0-9][A-Za-z0-9.-]*$' \
  || die "could not resolve a valid latest release tag (got: '${TAG:-empty}') — check https://github.com/XTLS/Xray-core/releases"
log "latest Xray-core release: $TAG"

# ------------------------------------- download -> verify -> extract ----
# No curl|bash anywhere: the zip lands in a temp file first.

ZIP_URL="$DL_BASE/$TAG/Xray-linux-64.zip"
log "downloading $ZIP_URL"
curl -fSL --retry 3 -o "$WORKDIR/xray.zip" "$ZIP_URL" \
  || die "download failed: $ZIP_URL"
[ -s "$WORKDIR/xray.zip" ] || die "download produced an empty file: $ZIP_URL"

log "verifying archive integrity..."
unzip -t "$WORKDIR/xray.zip" >/dev/null || die "archive failed the unzip -t integrity check: $WORKDIR/xray.zip"

unzip -o "$WORKDIR/xray.zip" -d "$WORKDIR/extract" xray geoip.dat geosite.dat >/dev/null \
  || die "extraction failed (expected files: xray, geoip.dat, geosite.dat)"
[ -f "$WORKDIR/extract/xray" ]       || die "archive does not contain the xray binary"
[ -f "$WORKDIR/extract/geoip.dat" ]  || die "archive does not contain geoip.dat"
[ -f "$WORKDIR/extract/geosite.dat" ] || die "archive does not contain geosite.dat"
chmod 0755 "$WORKDIR/extract/xray"

log "smoke-testing the extracted binary..."
"$WORKDIR/extract/xray" version >/dev/null \
  || die "extracted xray binary does not run on this machine (wrong architecture?)"

# Replacing a running binary fails with ETXTBSY — stop the old service first.
if systemctl is-active --quiet xray 2>/dev/null; then
  log "stopping the running xray service (it will be restarted on the new binary)..."
  systemctl stop xray
fi

log "installing into $INSTALL_ROOT ..."
install -D -m 0755 "$WORKDIR/extract/xray"        "$XRAY_BIN"
install -D -m 0644 "$WORKDIR/extract/geoip.dat"   "$ASSET_DIR/geoip.dat"
install -D -m 0644 "$WORKDIR/extract/geosite.dat" "$ASSET_DIR/geosite.dat"
XRAY_VER="$("$XRAY_BIN" version | head -n 1 || true)"
log "installed: $XRAY_BIN (${XRAY_VER:-Xray $TAG}), $ASSET_DIR/geoip.dat, $ASSET_DIR/geosite.dat"

# ---------------------------------------------------------- credentials ----

log "generating credentials..."
UUID="$("$XRAY_BIN" uuid)"
X25519_OUT="$("$XRAY_BIN" x25519)"
# Output format differs across Xray versions — handle both spellings:
#   new (v26+):  "PrivateKey: ..."  /  "Password (PublicKey): ..."
#   old:         "Private key: ..." /  "Public key: ..."
# (GNU sed 'I' flag matches either letter case.)
PRIV_KEY="$(printf '%s\n' "$X25519_OUT" | sed -nE 's/^[[:space:]]*private[[:space:]]*key[[:space:]]*:[[:space:]]*([A-Za-z0-9_-]+).*/\1/Ip' | head -n 1)"
PUB_KEY="$(printf '%s\n' "$X25519_OUT" | sed -nE 's/^[[:space:]]*(public[[:space:]]*key|password[[:space:]]*\([[:space:]]*publickey[[:space:]]*\))[[:space:]]*:[[:space:]]*([A-Za-z0-9_-]+).*/\2/Ip' | head -n 1)"
[ -n "$PRIV_KEY" ] || die "could not parse the X25519 private key from 'xray x25519' output"
[ -n "$PUB_KEY" ]  || die "could not parse the X25519 public key from 'xray x25519' output"
SHORT_ID="$(openssl rand -hex 8)"
log "credentials generated (uuid $UUID, shortId $SHORT_ID)"

# -------------------------------------------------------- server config ----
# JSON has no comments, so the private-key handling is documented here and in
# the final output: the REALITY privateKey is written ONLY to this file — it
# never leaves the VM, and this script never prints it.

log "writing $CONFIG_PATH ..."
install -d -m 0755 "$CONF_DIR"
cat > "$CONFIG_PATH" <<EOF
{
  "log": {
    "loglevel": "warning"
  },
  "inbounds": [
    {
      "tag": "vless-in",
      "listen": "0.0.0.0",
      "port": $PORT,
      "protocol": "vless",
      "settings": {
        "clients": [
          {
            "id": "$UUID",
            "flow": "xtls-rprx-vision"
          }
        ],
        "decryption": "none"
      },
      "streamSettings": {
        "network": "tcp",
        "security": "reality",
        "realitySettings": {
          "show": false,
          "dest": "$DEST",
          "xver": 0,
          "serverNames": [
            "$DOMAIN"
          ],
          "privateKey": "$PRIV_KEY",
          "shortIds": [
            "$SHORT_ID"
          ]
        }
      }
    }
  ],
  "outbounds": [
    {
      "tag": "direct",
      "protocol": "freedom"
    }
  ]
}
EOF
# 0644 root:root — the file holds the REALITY private key, but the service
# runs as User=nobody and must be able to read it; it stays local to the VM.
chmod 0644 "$CONFIG_PATH"
chown root:root "$CONFIG_PATH"

log "validating the config with 'xray run -test'..."
if ! XRAY_LOCATION_ASSET="$ASSET_DIR" "$XRAY_BIN" run -test -c "$CONFIG_PATH" >/dev/null 2>&1; then
  warn "'xray run -test' FAILED — its output follows:"
  XRAY_LOCATION_ASSET="$ASSET_DIR" "$XRAY_BIN" run -test -c "$CONFIG_PATH" \
    || die "the generated config does not pass 'xray run -test' (this is a bug in the script — please report it)"
fi
log "config validated"

# --------------------------------------------------------- systemd unit ----

log "writing $UNIT_PATH ..."
cat > "$UNIT_PATH" <<EOF
[Unit]
Description=Xray (VLESS + REALITY) - MultiTunnel server endpoint
Documentation=https://github.com/maniesh-NR/MultiTunnel
After=network.target nss-lookup.target

[Service]
Type=simple
User=nobody
Environment=XRAY_LOCATION_ASSET=$ASSET_DIR
ExecStart=$XRAY_BIN run -c $CONFIG_PATH
Restart=always
RestartSec=3
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
chmod 0644 "$UNIT_PATH"

log "enabling and (re)starting the xray service..."
systemctl daemon-reload
systemctl enable xray >/dev/null || die "systemctl enable xray failed"
systemctl restart xray
sleep 2
if ! systemctl is-active --quiet xray; then
  warn "xray service is NOT active — last 30 journal lines follow:"
  journalctl -u xray -n 30 --no-pager >&2 || true
  SVC_STATE="$(systemctl is-active xray 2>/dev/null | head -n 1 || true)"
  die "systemctl reports the xray service as '${SVC_STATE:-unknown}' — fix the issue above and re-run, or inspect with: journalctl -u xray -e"
fi
log "xray service is active (listening on 0.0.0.0:$PORT)"

# ----------------------------------------------------------- public IP ----
# Preferred source: the GCP metadata server — it reports the VM's true
# external IP even when the VM's egress is routed through a VPN (e.g.
# ProtonVPN), which would make api.ipify.org return the VPN exit address.

PUBLIC_IP=""
IP_SRC=""
GCP_META_IP="$(curl -fsS --max-time 5 -H 'Metadata-Flavor: Google' \
  'http://metadata.google.internal/computeMetadata/v1/instance/network-interfaces/0/access-configs/0/external-ip' 2>/dev/null || true)"
if printf '%s' "$GCP_META_IP" | grep -Eq '^[0-9]{1,3}(\.[0-9]{1,3}){3}$'; then
  PUBLIC_IP="$GCP_META_IP"
  IP_SRC="GCP metadata server"
fi
if [ -z "$PUBLIC_IP" ]; then
  IPIFY_IP="$(curl -fsS --max-time 10 https://api.ipify.org 2>/dev/null || true)"
  if printf '%s' "$IPIFY_IP" | grep -Eq '^[0-9]{1,3}(\.[0-9]{1,3}){3}$'; then
    PUBLIC_IP="$IPIFY_IP"
    IP_SRC="api.ipify.org"
    warn "public IP taken from api.ipify.org: if this VM's egress is routed through a VPN, that is the VPN EXIT address, not the VM's real external IP — verify with: gcloud compute instances list"
  fi
fi
if [ -z "$PUBLIC_IP" ]; then
  PUBLIC_IP="<VM_PUBLIC_IP>"
  IP_SRC="placeholder (could not detect)"
  warn "could not detect the public IP — fill in <VM_PUBLIC_IP> in the client config yourself (gcloud compute instances list)"
fi
log "client address: $PUBLIC_IP (source: $IP_SRC)"

# --------------------------------------------------------------- summary ----

cat <<EOF

[setup] ================================ DONE ================================
[setup] xray $TAG is installed and the VLESS+REALITY endpoint is running.

[setup] Files on this VM (private — the REALITY privateKey lives ONLY in the
[setup] server config and never leaves this machine):
[setup]   $XRAY_BIN
[setup]   $ASSET_DIR/geoip.dat, $ASSET_DIR/geosite.dat
[setup]   $CONFIG_PATH   (0644 root:root)
[setup]   $UNIT_PATH

[setup] Service commands:
[setup]   systemctl status xray
[setup]   journalctl -u xray -f

[setup] -------------------------- FIREWALL CHECKLIST -------------------------
[setup] Two layers must allow tcp/$PORT INBOUND before the client can connect.

[setup] (a) GCP firewall rule (the port has been CLOSED externally until now).
[setup]     From any machine with gcloud access to the project, list the
[setup]     current rules first and look for one already allowing tcp/$PORT:
[setup]       gcloud compute firewall-rules list
[setup]     then create the rule (adjust --network / add --target-tags to
[setup]     match this VM; narrow --source-ranges to your client IP if it is
[setup]     static — REALITY tolerates a public port, but narrower is safer):
[setup]       gcloud compute firewall-rules create allow-multitunnel-xray-$PORT \\
[setup]           --network=default --direction=INGRESS --action=ALLOW \\
[setup]           --rules=tcp:$PORT --source-ranges=0.0.0.0/0

[setup] (b) On the VM itself: iptables is persisted with netfilter-persistent
[setup]     (/etc/iptables/rules.v4). CHECK the existing rules first:
[setup]       iptables -L INPUT -n --line-numbers
[setup]     then, if nothing already accepts tcp/$PORT:
[setup]       iptables -I INPUT -p tcp --dport $PORT -j ACCEPT && netfilter-persistent save

[setup] ------------------------ CLIENT CONFIG BLOCK --------------------------
[setup] Paste this over the "server" block of config.json on the Windows
[setup] machine (start from config.example.json in the MultiTunnel release):

  "server": {
    "address": "$PUBLIC_IP",
    "port": $PORT,
    "uuid": "$UUID",
    "sni": "$DOMAIN",
    "publicKey": "$PUB_KEY",
    "shortId": "$SHORT_ID"
  },

[setup] Notes:
[setup] - publicKey/shortId/uuid go to the client; privateKey stays on the VM.
[setup] - If this VM's egress is routed through ProtonVPN, nothing changes
[setup]   here: xray's freedom outbound simply uses the VM's default routing,
[setup]   and the endpoint stays on the VM's external IP above — server-side
[setup]   VPN rotation is invisible to the client.
[setup] - The MultiTunnel repo's config.example.json prefills the address
[setup]   35.208.99.240 — if this is that VM, the address above must be it.
[setup] =======================================================================
EOF

# vim: set et sw=2:
