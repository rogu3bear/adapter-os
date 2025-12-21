#!/usr/bin/env bash
# Install AdapterOS worker as a launchd service on macOS
#
# This script installs the aos-worker as a system-level launchd service
# with automatic restart on failure.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

error() {
    echo -e "${RED}ERROR: $1${NC}" >&2
}

success() {
    echo -e "${GREEN}$1${NC}"
}

warning() {
    echo -e "${YELLOW}WARNING: $1${NC}"
}

info() {
    echo -e "$1"
}

# Check if running on macOS
if [[ "$(uname)" != "Darwin" ]]; then
    error "This script only works on macOS"
    exit 1
fi

# Check if binary exists
WORKER_BIN="$PROJECT_ROOT/target/release/aos-worker"
if [[ ! -f "$WORKER_BIN" ]]; then
    error "Worker binary not found at $WORKER_BIN"
    error "Please build the project first: cargo build --release"
    exit 1
fi

# Configuration
PLIST_NAME="com.adapteros.worker.plist"
PLIST_SOURCE="$SCRIPT_DIR/launchd/$PLIST_NAME"
PLIST_DEST="$HOME/Library/LaunchAgents/$PLIST_NAME"

# Verify plist exists
if [[ ! -f "$PLIST_SOURCE" ]]; then
    error "Plist file not found at $PLIST_SOURCE"
    exit 1
fi

info "Installing AdapterOS Worker as launchd service..."
info ""

# Create necessary directories
info "Creating directories..."
mkdir -p /var/log/aos
mkdir -p /var/run/aos/default
mkdir -p /var/lib/aos/manifests
mkdir -p /var/lib/aos/models
mkdir -p "$HOME/Library/LaunchAgents"

# Install binary to /usr/local/bin
info "Installing worker binary to /usr/local/bin..."
sudo cp "$WORKER_BIN" /usr/local/bin/aos-worker
sudo chmod +x /usr/local/bin/aos-worker

# Copy plist to LaunchAgents
info "Installing launchd plist..."
cp "$PLIST_SOURCE" "$PLIST_DEST"

# Unload existing service if running
if launchctl list | grep -q "com.adapteros.worker"; then
    warning "Service already loaded, unloading..."
    launchctl unload "$PLIST_DEST" 2>/dev/null || true
fi

# Load service
info "Loading service..."
launchctl load "$PLIST_DEST"

# Wait a moment for service to start
sleep 2

# Check status
if launchctl list | grep -q "com.adapteros.worker"; then
    success "Service installed and loaded successfully!"
    info ""
    info "Service management commands:"
    info "  Start:   launchctl start com.adapteros.worker"
    info "  Stop:    launchctl stop com.adapteros.worker"
    info "  Restart: launchctl kickstart -k gui/\$(id -u)/com.adapteros.worker"
    info "  Status:  launchctl list | grep adapteros"
    info "  Logs:    tail -f /var/log/aos/worker-*.log"
    info ""
    info "To uninstall:"
    info "  launchctl unload $PLIST_DEST"
    info "  rm $PLIST_DEST"
else
    error "Service failed to load. Check system logs:"
    error "  log show --predicate 'subsystem == \"com.apple.launchd\"' --last 5m"
    exit 1
fi
