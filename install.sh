#!/usr/bin/env bash
set -euo pipefail

REPO="Heartcoolman/wordforge"
INSTALL_DIR="/opt/wordforge"
SERVICE="wordforge"
SVC_USER="wordforge"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

# --- checks ---
[ "$(id -u)" -eq 0 ] || error "Run as root: sudo bash install.sh"
for cmd in curl tar openssl systemctl; do
  command -v "$cmd" >/dev/null 2>&1 || error "Required command not found: $cmd"
done

case $(uname -m) in
  x86_64)        ARCH="x86_64"  ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) error "Unsupported architecture: $(uname -m)" ;;
esac

# --- fetch release ---
info "Fetching latest release for $ARCH..."
API_RESP=$(curl -sf "https://api.github.com/repos/${REPO}/releases/latest") \
  || error "Cannot reach GitHub API. Check network or verify that a release exists."

DOWNLOAD_URL=$(echo "$API_RESP" \
  | grep '"browser_download_url"' \
  | grep "linux-${ARCH}" \
  | head -1 \
  | cut -d '"' -f 4)

[ -n "$DOWNLOAD_URL" ] || error "No release artifact found for linux-${ARCH}"

VERSION=$(echo "$API_RESP" | grep '"tag_name"' | head -1 | cut -d '"' -f 4)
info "Version: $VERSION"

# --- download ---
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

info "Downloading..."
curl -sfL "$DOWNLOAD_URL" -o "$TMP/wordforge.tar.gz"
tar xzf "$TMP/wordforge.tar.gz" -C "$TMP" --strip-components=1

# --- system user ---
if ! id "$SVC_USER" &>/dev/null; then
  useradd --system --no-create-home --shell /bin/false "$SVC_USER"
fi

# --- install files ---
mkdir -p "$INSTALL_DIR"
cp "$TMP/wordforge" "$INSTALL_DIR/wordforge"
chmod +x "$INSTALL_DIR/wordforge"
cp -r "$TMP/static" "$INSTALL_DIR/"

# --- .env (preserved on update) ---
if [ ! -f "$INSTALL_DIR/.env" ]; then
  info "Generating .env with random secrets..."
  cp "$TMP/.env.example" "$INSTALL_DIR/.env"

  JWT=$(openssl rand -hex 32)
  ADMIN_JWT=$(openssl rand -hex 32)
  REFRESH_JWT=$(openssl rand -hex 32)

  sed -i \
    -e "s|MUST_CHANGE_USE_openssl_rand_hex_32_different|${ADMIN_JWT}|g" \
    -e "s|MUST_CHANGE_USE_openssl_rand_hex_32_refresh|${REFRESH_JWT}|g" \
    -e "s|MUST_CHANGE_USE_openssl_rand_hex_32|${JWT}|g" \
    -e "s|HOST=127.0.0.1|HOST=0.0.0.0|" \
    "$INSTALL_DIR/.env"

  chmod 600 "$INSTALL_DIR/.env"
  warn "Review $INSTALL_DIR/.env before exposing to the internet"
else
  info ".env already exists — preserved"
fi

chown -R "$SVC_USER:$SVC_USER" "$INSTALL_DIR"

# --- systemd ---
cat > "/etc/systemd/system/${SERVICE}.service" << EOF
[Unit]
Description=WordForge Learning Backend
After=network.target

[Service]
Type=simple
WorkingDirectory=${INSTALL_DIR}
ExecStart=${INSTALL_DIR}/wordforge
EnvironmentFile=${INSTALL_DIR}/.env
Environment=ENABLE_SELF_WATCHDOG=true
Environment=SELF_WATCHDOG_INTERVAL_SECS=15
Environment=SELF_WATCHDOG_FAILURE_THRESHOLD=3
Restart=on-failure
RestartSec=5
TimeoutStopSec=15
User=${SVC_USER}
Group=${SVC_USER}
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=${INSTALL_DIR}

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable "$SERVICE"
systemctl restart "$SERVICE"

info "WordForge $VERSION installed and running"
info "Config : $INSTALL_DIR/.env"
info "Logs   : journalctl -u $SERVICE -f"
info "Status : systemctl status $SERVICE"
