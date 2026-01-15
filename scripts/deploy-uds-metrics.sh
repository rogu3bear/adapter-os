#!/bin/bash
# Deploy UDS metrics exporter with bridge script
# 
# This script sets up the UDS-based metrics export system

set -euo pipefail

# Configuration
TENANT_ID="${AOS_TENANT_ID:-default}"
UDS_SOCKET="/var/run/aos/${TENANT_ID}/metrics.sock"
BRIDGE_SCRIPT="/usr/local/bin/aos-metrics-bridge"
SERVICE_NAME="aos-metrics-bridge"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "🚀 Deploying UDS Metrics Export System"
echo "======================================"

# Check if running as root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}Error: This script must be run as root${NC}"
    exit 1
fi

# Create directory for UDS socket
echo "📁 Creating UDS socket directory..."
sudo mkdir -p "$(dirname "$UDS_SOCKET")"
sudo chown aos:aos "$(dirname "$UDS_SOCKET")"
sudo chmod 755 "$(dirname "$UDS_SOCKET")"

# Install bridge script
echo "📦 Installing metrics bridge script..."
sudo cp scripts/metrics-bridge.sh "$BRIDGE_SCRIPT"
sudo chmod +x "$BRIDGE_SCRIPT"
sudo chown aos:aos "$BRIDGE_SCRIPT"

# Create systemd service
echo "⚙️  Creating systemd service..."
sudo tee "/etc/systemd/system/${SERVICE_NAME}.service" > /dev/null <<EOF
[Unit]
Description=adapterOS Metrics Bridge
After=network.target
Wants=aos-worker.service

[Service]
Type=simple
User=aos
Group=aos
ExecStart=${BRIDGE_SCRIPT}
Restart=always
RestartSec=10
Environment=AOS_METRICS_SOCKET=${UDS_SOCKET}
Environment=PROMETHEUS_PUSH_GATEWAY=http://pushgateway:9091
Environment=PROMETHEUS_JOB=aos
Environment=METRICS_PUSH_INTERVAL=15

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/run/aos

[Install]
WantedBy=multi-user.target
EOF

# Reload systemd and enable service
echo "🔄 Reloading systemd and enabling service..."
sudo systemctl daemon-reload
sudo systemctl enable "$SERVICE_NAME"

# Start the service
echo "▶️  Starting metrics bridge service..."
sudo systemctl start "$SERVICE_NAME"

# Check service status
echo "📊 Checking service status..."
sleep 2
if sudo systemctl is-active --quiet "$SERVICE_NAME"; then
    echo -e "${GREEN}✅ Metrics bridge service is running${NC}"
else
    echo -e "${RED}❌ Metrics bridge service failed to start${NC}"
    sudo systemctl status "$SERVICE_NAME" --no-pager
    exit 1
fi

# Test UDS socket
echo "🔌 Testing UDS socket..."
sleep 5
if [ -S "$UDS_SOCKET" ]; then
    echo -e "${GREEN}✅ UDS socket created: $UDS_SOCKET${NC}"
else
    echo -e "${YELLOW}⚠️  UDS socket not yet available: $UDS_SOCKET${NC}"
    echo "This is normal if the adapterOS worker hasn't started yet."
fi

# Show service logs
echo "📋 Recent service logs:"
sudo journalctl -u "$SERVICE_NAME" --no-pager -n 10

echo ""
echo "🎉 UDS Metrics Export System deployed successfully!"
echo ""
echo "Configuration:"
echo "  UDS Socket: $UDS_SOCKET"
echo "  Bridge Script: $BRIDGE_SCRIPT"
echo "  Service: $SERVICE_NAME"
echo ""
echo "To monitor the service:"
echo "  sudo journalctl -u $SERVICE_NAME -f"
echo ""
echo "To test metrics export:"
echo "  echo 'GET' | socat - UNIX-CONNECT:$UDS_SOCKET"
