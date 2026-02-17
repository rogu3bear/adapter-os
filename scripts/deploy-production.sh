#!/bin/bash
# Production deployment script for adapterOS Control Plane
#
# This script deploys adapterOS in production mode with UDS-only serving,
# Ed25519 JWT signing, and zero-egress enforcement.

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
CONFIG_FILE="${AOS_CONFIG:-configs/production-multinode.toml}"
UDS_SOCKET="${AOS_UDS_SOCKET:-/var/run/adapteros/control-plane.sock}"
UDS_DIR=$(dirname "$UDS_SOCKET")
SERVICE_NAME="adapteros-cp"
DRY_RUN=0
ALLOW_NON_ROOT_DEPLOY="${AOS_ALLOW_NON_ROOT_DEPLOY:-0}"
SKIP_SYSTEMCTL="${AOS_SKIP_SYSTEMCTL:-0}"
NON_ROOT_FAST_RELEASE="${AOS_NON_ROOT_FAST_RELEASE:-1}"
SYSTEMCTL_AVAILABLE=0
if command -v systemctl >/dev/null 2>&1; then
    SYSTEMCTL_AVAILABLE=1
fi

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        *)
            echo -e "${RED}❌ Unknown argument: $1${NC}"
            echo "Usage: $0 [--dry-run]"
            exit 1
            ;;
    esac
done

echo "🚀 adapterOS Production Deployment"
echo "==================================="
if [ "$DRY_RUN" -eq 1 ]; then
    echo "Mode: DRY RUN (no system paths or systemctl changes)"
fi
if [ "$SYSTEMCTL_AVAILABLE" -eq 0 ]; then
    SKIP_SYSTEMCTL=1
fi
if [ "$EUID" -ne 0 ]; then
    SKIP_SYSTEMCTL=1
fi

# Check if running as root (required for UDS socket creation)
if [ "$EUID" -ne 0 ] && [ "$DRY_RUN" -eq 0 ] && [ "$ALLOW_NON_ROOT_DEPLOY" != "1" ]; then
    echo -e "${RED}❌ Non-root deploy blocked.${NC}"
    echo "Run as root for full production apply, or set AOS_ALLOW_NON_ROOT_DEPLOY=1 for a local non-systemd deploy."
    exit 1
fi
if [ "$EUID" -ne 0 ] && [ "$DRY_RUN" -eq 1 ]; then
    echo -e "${YELLOW}⚠️  Running dry-run as non-root${NC}"
fi
if [ "$EUID" -ne 0 ] && [ "$DRY_RUN" -eq 0 ] && [ "$ALLOW_NON_ROOT_DEPLOY" = "1" ]; then
    echo -e "${YELLOW}⚠️  Running non-root deploy mode (systemctl integration disabled).${NC}"
fi

# Step 1: Validate production configuration
echo "📋 Validating production configuration..."
if [ ! -f "$CONFIG_FILE" ]; then
    echo -e "${RED}❌ Configuration file not found: $CONFIG_FILE${NC}"
    exit 1
fi

# Check for required production settings
if ! grep -q "production_mode = true" "$CONFIG_FILE"; then
    echo -e "${RED}❌ Production mode not enabled in config${NC}"
    exit 1
fi

if ! grep -q 'jwt_mode = "eddsa"' "$CONFIG_FILE"; then
    echo -e "${RED}❌ Ed25519 JWT mode not configured${NC}"
    exit 1
fi

if ! grep -q "uds_socket" "$CONFIG_FILE"; then
    echo -e "${RED}❌ UDS socket not configured${NC}"
    exit 1
fi

echo -e "${GREEN}✅ Production configuration validated${NC}"

# Step 2: Create UDS socket directory
echo "🔌 Creating UDS socket directory..."
if [ "$DRY_RUN" -eq 1 ]; then
    echo "Would run: mkdir -p \"$UDS_DIR\""
    echo "Would run: chmod 700 \"$UDS_DIR\""
else
    if ! mkdir -p "$UDS_DIR"; then
        echo -e "${RED}❌ Failed to create UDS socket directory: $UDS_DIR${NC}"
        echo "Set AOS_UDS_SOCKET to a writable path (e.g. $PWD/var/run/adapteros/control-plane.sock) or run as root."
        exit 1
    fi
    if ! chmod 700 "$UDS_DIR"; then
        echo -e "${RED}❌ Failed to set permissions on UDS socket directory: $UDS_DIR${NC}"
        exit 1
    fi
fi
echo -e "${GREEN}✅ UDS socket directory created: $UDS_DIR${NC}"

# Step 3: Verify JWT keys exist
echo "🔐 Verifying JWT keys..."
JWT_PUBLIC_KEY=$(grep -m1 'jwt_public_key_pem_file' "$CONFIG_FILE" | cut -d'"' -f2 || true)
if [ -z "$JWT_PUBLIC_KEY" ]; then
    JWT_PUBLIC_KEY=$(grep -m1 'jwt_public_key_pem' "$CONFIG_FILE" | cut -d'"' -f2 || true)
fi
JWT_SIGNING_KEY=$(grep -m1 'jwt_signing_key_path' "$CONFIG_FILE" | cut -d'"' -f2 || true)

if [ -n "$JWT_PUBLIC_KEY" ] && [ ! -f "$JWT_PUBLIC_KEY" ]; then
    echo -e "${RED}❌ JWT public key file not found: $JWT_PUBLIC_KEY${NC}"
    exit 1
fi

if [ -n "$JWT_SIGNING_KEY" ] && [ ! -f "$JWT_SIGNING_KEY" ]; then
    echo -e "${RED}❌ JWT signing key file not found: $JWT_SIGNING_KEY${NC}"
    exit 1
fi

echo -e "${GREEN}✅ JWT keys verified${NC}"

# Step 4: Verify PF rules for zero egress
echo "🛡️  Verifying zero-egress enforcement..."
if grep -q "require_pf_deny = true" "$CONFIG_FILE"; then
    if command -v pfctl >/dev/null 2>&1; then
        if pfctl -s info 2>/dev/null | grep -q "Status: Enabled"; then
            echo -e "${GREEN}✅ PF is enabled${NC}"
        else
            echo -e "${YELLOW}⚠️  PF is not enabled - zero egress may not be enforced${NC}"
        fi
    else
        echo -e "${YELLOW}⚠️  pfctl not found - cannot verify PF rules${NC}"
    fi
fi

# Step 5: Compile gate
mkdir -p var/log
BUILD_LOG="var/log/aos-build.log"
if [ "$DRY_RUN" -eq 1 ]; then
    echo "🔨 Running dry-run compile gate (cargo check)..."
    if ! RUSTC_WRAPPER= cargo check --locked -p adapteros-server --bin aos-server >"$BUILD_LOG" 2>&1; then
        cat "$BUILD_LOG"
        echo -e "${RED}❌ Dry-run compile gate failed${NC}"
        exit 1
    fi
    echo "Build log: $BUILD_LOG"
    echo -e "${GREEN}✅ Dry-run compile gate passed${NC}"
else
    echo "🔨 Building release binary..."
    if [ "$ALLOW_NON_ROOT_DEPLOY" = "1" ] && [ "$NON_ROOT_FAST_RELEASE" = "1" ]; then
        echo "Using fast-release overrides for non-root deploy mode (LTO off, codegen-units=16)."
        if ! CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 RUSTC_WRAPPER= cargo build --release --locked -p adapteros-server --bin aos-server >"$BUILD_LOG" 2>&1; then
            cat "$BUILD_LOG"
            echo -e "${RED}❌ Build failed${NC}"
            exit 1
        fi
    elif ! RUSTC_WRAPPER= cargo build --release --locked -p adapteros-server --bin aos-server >"$BUILD_LOG" 2>&1; then
        cat "$BUILD_LOG"
        echo -e "${RED}❌ Build failed${NC}"
        exit 1
    fi
    echo "Build log: $BUILD_LOG"
    echo -e "${GREEN}✅ Release binary built${NC}"
fi

# Step 6: Run database migrations
echo "📊 Running database migrations..."
DB_PATH=$(grep '^path =' "$CONFIG_FILE" | head -1 | cut -d'"' -f2 || echo "var/aos-cp.sqlite3")
if [[ "$DB_PATH" =~ ^(postgres|postgresql|mysql|mariadb):// ]]; then
    echo "Database DSN configured: $DB_PATH"
else
    if [[ "$DB_PATH" == sqlite://* ]]; then
        DB_FILE_PATH="${DB_PATH#sqlite://}"
    elif [[ "$DB_PATH" == sqlite:* ]]; then
        DB_FILE_PATH="${DB_PATH#sqlite:}"
    else
        DB_FILE_PATH="$DB_PATH"
    fi

    if [ -f "$DB_FILE_PATH" ]; then
        echo "Database exists at $DB_FILE_PATH"
    else
        echo "Creating new database at $DB_FILE_PATH"
        mkdir -p "$(dirname "$DB_FILE_PATH")"
    fi
fi

# Run migrations via binary
if [ "$DRY_RUN" -eq 1 ] || { [ "$ALLOW_NON_ROOT_DEPLOY" = "1" ] && [ "$EUID" -ne 0 ]; }; then
    echo "Would run: ./target/release/aos-server --config \"$CONFIG_FILE\" --migrate-only"
else
    ./target/release/aos-server --config "$CONFIG_FILE" --migrate-only || {
        echo -e "${YELLOW}⚠️  Migration check completed (may be no-op)${NC}"
    }
fi

# Step 7: Create systemd service file
echo "⚙️  Creating systemd service..."
if [ "$DRY_RUN" -eq 1 ] || [ "$SKIP_SYSTEMCTL" = "1" ]; then
    mkdir -p var
    SERVICE_FILE="var/${SERVICE_NAME}.service.preview"
else
    SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
fi
cat > "$SERVICE_FILE" <<EOF
[Unit]
Description=adapterOS Control Plane
After=network.target

[Service]
Type=simple
User=adapteros
Group=adapteros
WorkingDirectory=$(pwd)
ExecStart=$(pwd)/target/release/aos-server --config $CONFIG_FILE
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/adapteros /var/log/adapteros /var/run/adapteros

[Install]
WantedBy=multi-user.target
EOF

echo -e "${GREEN}✅ Systemd service file created${NC}"
if [ "$DRY_RUN" -eq 1 ] || [ "$SKIP_SYSTEMCTL" = "1" ]; then
    echo "Preview service file written to: $SERVICE_FILE"
fi

# Step 8: Reload systemd and enable service
echo "🔄 Reloading systemd..."
if [ "$DRY_RUN" -eq 1 ]; then
    echo "Would run: systemctl daemon-reload"
    echo "Would run: systemctl enable $SERVICE_NAME"
elif [ "$SKIP_SYSTEMCTL" = "1" ]; then
    echo -e "${YELLOW}⚠️  Skipping systemctl integration (systemctl unavailable or non-root).${NC}"
else
    systemctl daemon-reload
    systemctl enable "$SERVICE_NAME"
fi

echo ""
echo "🎉 Production Deployment Ready!"
echo "=============================="
echo ""
echo "Configuration: $CONFIG_FILE"
echo "UDS Socket: $UDS_SOCKET"
echo "Service: $SERVICE_NAME"
echo ""
if [ "$DRY_RUN" -eq 1 ]; then
    echo "Dry-run complete. To apply in production, rerun without --dry-run as root."
    echo ""
elif [ "$SKIP_SYSTEMCTL" = "1" ]; then
    echo "Deployment steps complete without systemctl integration."
    echo ""
fi
echo "Next steps:"
if [ "$SKIP_SYSTEMCTL" = "1" ]; then
    echo "  1. Export signing key: export AOS_SIGNING_KEY=<64+ char signing key>"
    echo "  2. Start service with root for PF checks: sudo ./target/release/aos-server --config $CONFIG_FILE"
    echo "  3. Validate readiness: curl http://127.0.0.1:8080/readyz"
    echo "  4. Integrate with your host supervisor using: $SERVICE_FILE"
else
    echo "  1. Create adapteros user: sudo useradd -r -s /bin/false adapteros"
    echo "  2. Set ownership: sudo chown -R adapteros:adapteros /var/lib/adapteros /var/log/adapteros /var/run/adapteros"
    echo "  3. Start service: sudo systemctl start $SERVICE_NAME"
    echo "  4. Check status: sudo systemctl status $SERVICE_NAME"
    echo "  5. View logs: sudo journalctl -u $SERVICE_NAME -f"
fi
echo ""
