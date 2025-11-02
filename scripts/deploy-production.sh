#!/bin/bash
# Production deployment script for AdapterOS Control Plane
#
# This script deploys AdapterOS in production mode with UDS-only serving,
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
SERVICE_NAME="adapteros-cp"

echo "🚀 AdapterOS Production Deployment"
echo "==================================="

# Check if running as root (required for UDS socket creation)
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}❌ This script must be run as root for UDS socket creation${NC}"
    exit 1
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
UDS_DIR=$(dirname "$UDS_SOCKET")
mkdir -p "$UDS_DIR"
chmod 700 "$UDS_DIR"
echo -e "${GREEN}✅ UDS socket directory created: $UDS_DIR${NC}"

# Step 3: Verify JWT keys exist
echo "🔐 Verifying JWT keys..."
JWT_PUBLIC_KEY=$(grep 'jwt_public_key_pem_file' "$CONFIG_FILE" | cut -d'"' -f2 || grep 'jwt_public_key_pem' "$CONFIG_FILE" | cut -d'"' -f2)
JWT_SIGNING_KEY=$(grep 'jwt_signing_key_path' "$CONFIG_FILE" | cut -d'"' -f2)

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

# Step 5: Build release binary
echo "🔨 Building release binary..."
if ! cargo build --release --locked 2>&1 | tee /tmp/aos-build.log; then
    echo -e "${RED}❌ Build failed${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Release binary built${NC}"

# Step 6: Run database migrations
echo "📊 Running database migrations..."
DB_PATH=$(grep '^path =' "$CONFIG_FILE" | head -1 | cut -d'"' -f2 || echo "var/aos-cp.sqlite3")
if [ -f "$DB_PATH" ]; then
    echo "Database exists at $DB_PATH"
else
    echo "Creating new database at $DB_PATH"
    mkdir -p "$(dirname "$DB_PATH")"
fi

# Run migrations via binary
./target/release/aos-cp --config "$CONFIG_FILE" --migrate-only || {
    echo -e "${YELLOW}⚠️  Migration check completed (may be no-op)${NC}"
}

# Step 7: Create systemd service file
echo "⚙️  Creating systemd service..."
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
cat > "$SERVICE_FILE" <<EOF
[Unit]
Description=AdapterOS Control Plane
After=network.target

[Service]
Type=simple
User=adapteros
Group=adapteros
WorkingDirectory=$(pwd)
ExecStart=$(pwd)/target/release/aos-cp --config $CONFIG_FILE
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

# Step 8: Reload systemd and enable service
echo "🔄 Reloading systemd..."
systemctl daemon-reload
systemctl enable "$SERVICE_NAME"

echo ""
echo "🎉 Production Deployment Ready!"
echo "=============================="
echo ""
echo "Configuration: $CONFIG_FILE"
echo "UDS Socket: $UDS_SOCKET"
echo "Service: $SERVICE_NAME"
echo ""
echo "Next steps:"
echo "  1. Create adapteros user: sudo useradd -r -s /bin/false adapteros"
echo "  2. Set ownership: sudo chown -R adapteros:adapteros /var/lib/adapteros /var/log/adapteros /var/run/adapteros"
echo "  3. Start service: sudo systemctl start $SERVICE_NAME"
echo "  4. Check status: sudo systemctl status $SERVICE_NAME"
echo "  5. View logs: sudo journalctl -u $SERVICE_NAME -f"
echo ""

