#!/usr/bin/env bash
# MultiTunnel stream-bonding merge daemon installer (S9, D17).
#
# Installs `mt-merge-server` on the TUNNEL SERVER (the same VM that runs
# the Xray VLESS+REALITY endpoint). Stream bonding mode requires it:
# each bonded adapter's VLESS connection targets 127.0.0.1:$PORT and this
# daemon re-merges the split streams and dials the real targets.
#
# Usage (on the VM, as root):
#   ./setup-merge-daemon.sh                 # default port 7443
#   ./setup-merge-daemon.sh --port 7443
#   ./setup-merge-daemon.sh --bin /path/to/mt-merge-server   # local binary
#   ./setup-merge-daemon.sh --release v0.5.0                # download from GitHub
#
# Security model (D17 §6): loopback-only by default — authentication is
# REALITY+UUID at the Xray edge; anything that can reach this port is
# already inside the server. Do NOT expose the port externally.
#
# Idempotent: safe to re-run (upgrades in place, restarts the service).

set -euo pipefail

PORT=7443
BIN_URL=""
LOCAL_BIN=""
RELEASE_TAG="${RELEASE_TAG:-}"

log()  { printf '%s\n' "[setup] $*"; }
warn() { printf '%s\n' "[setup] WARNING: $*" >&2; }
die()  { printf '%s\n' "[setup] ERROR: $*" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || die "run as root (sudo)"

while [ $# -gt 0 ]; do
  case "$1" in
    --port)    PORT="${2:?}"; shift 2 ;;
    --bin)     LOCAL_BIN="${2:?}"; shift 2 ;;
    --release) RELEASE_TAG="${2:?}"; shift 2 ;;
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
    *) die "unknown argument $1 (try --help)" ;;
  esac
done

printf '%s\n' "$PORT" | grep -Eq '^[1-9][0-9]{0,4}$' || die "port must be 1-65535"
[ "$PORT" -lt 65536 ] || die "port must be 1-65535"

INSTALL_DIR=/usr/local/bin
UNIT_PATH=/etc/systemd/system/mt-merge-server.service
BIN="$INSTALL_DIR/mt-merge-server"

# ---------------------------------------------------------- binary --------
if [ -n "$LOCAL_BIN" ]; then
  [ -f "$LOCAL_BIN" ] || die "local binary not found: $LOCAL_BIN"
  install -m 0755 "$LOCAL_BIN" "$BIN"
  log "installed $LOCAL_BIN -> $BIN"
else
  # Resolve the download URL: explicit --release tag, else latest GitHub
  # release asset for the host arch.
  ARCH="$(uname -m)"
  case "$ARCH" in
    x86_64)  ASSET_ARCH="x86_64" ;;
    aarch64) ASSET_ARCH="aarch64" ;;
    *) die "unsupported architecture: $ARCH" ;;
  esac
  if [ -z "$RELEASE_TAG" ]; then
    log "resolving latest release from GitHub…"
    RELEASE_TAG="$(curl -fsSL --max-time 15 \
      https://api.github.com/repos/maniesh-NR/MultiTunnel/releases/latest \
      | sed -nE 's/.*"tag_name": *"([^"]+)".*/\1/p' | head -n 1 || true)"
    [ -n "$RELEASE_TAG" ] || die "could not resolve the latest release tag (pass --release vX.Y.Z)"
  fi
  BIN_URL="https://github.com/maniesh-NR/MultiTunnel/releases/download/${RELEASE_TAG}/mt-merge-server-${ASSET_ARCH}-unknown-linux-gnu"
  log "downloading $BIN_URL"
  TMP_BIN="$(mktemp /tmp/mt-merge-server.XXXXXX)"
  trap 'rm -f "$TMP_BIN"' EXIT
  curl -fSL --retry 3 --max-time 120 -o "$TMP_BIN" "$BIN_URL"
  chmod 0755 "$TMP_BIN"
  # Basic sanity: ELF binary that answers --help.
  file "$TMP_BIN" | grep -q ELF || die "downloaded file is not an ELF binary"
  "$TMP_BIN" --help >/dev/null 2>&1 || die "downloaded binary does not run (wrong arch?)"
  install -m 0755 "$TMP_BIN" "$BIN"
  log "installed mt-merge-server ($RELEASE_TAG) -> $BIN"
fi

# ----------------------------------------------------------- systemd ------
cat > "$UNIT_PATH" <<EOF
[Unit]
Description=MultiTunnel stream-bonding merge daemon
After=network-online.target xray.service
Wants=network-online.target

[Service]
Type=simple
ExecStart=$BIN --listen 127.0.0.1:$PORT
Restart=always
RestartSec=2
# Hardening: the daemon only ever dials targets from the tunnel and
# listens on loopback.
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
CapabilityBoundingSet=
AmbientCapabilities=

[Install]
WantedBy=multi-user.target
EOF
chmod 0644 "$UNIT_PATH"
systemctl daemon-reload
systemctl enable mt-merge-server >/dev/null
systemctl restart mt-merge-server
sleep 1
systemctl is-active --quiet mt-merge-server \
  || die "service not active — check: journalctl -u mt-merge-server -e"

# -------------------------------------------------------- verify ----------
if ss -ltn 2>/dev/null | grep -q "127.0.0.1:$PORT"; then
  log "listening on 127.0.0.1:$PORT (verified)"
else
  warn "could not verify the listener with ss — check: ss -ltn | grep $PORT"
fi

cat <<EOF

[setup] ============================== DONE ==============================
[setup] mt-merge-server is running on 127.0.0.1:$PORT.

[setup] On the Windows machine (MultiTunnel v0.5.0+):
[setup]   Settings -> Advanced -> bonding mode: "stream"
[setup]   Settings -> Advanced -> server merge port: $PORT
[setup]   Adapters: tick the adapters to bond, Apply, connect.
[setup] The Status tab then shows one row per bonded adapter (carrier)
[setup] with live RTT/bytes; every connection's bytes are split across
[setup] them and merged here.

[setup] Service commands:
[setup]   systemctl status mt-merge-server
[setup]   journalctl -u mt-merge-server -f
[setup] ====================================================================
EOF

# vim: set et sw=2:
