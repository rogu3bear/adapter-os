#!/usr/bin/env bash
# Install adapterOS stack guardian as a launchd service on macOS
#
# The guardian is a per-user LaunchAgent that periodically ensures backend
# and worker are running via scripts/service-manager.sh.

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

# Configuration
GUARDIAN_LABEL="com.adapteros.stack.guardian"
GUARDIAN_PLIST_NAME="${GUARDIAN_LABEL}.plist"
GUARDIAN_PLIST_SOURCE="$SCRIPT_DIR/launchd/${GUARDIAN_PLIST_NAME}.template"
GUARDIAN_PLIST_DEST="$HOME/Library/LaunchAgents/$GUARDIAN_PLIST_NAME"
GUARDIAN_SCRIPT="$SCRIPT_DIR/launchd/aos-launchd-ensure.sh"
GUARDIAN_LOG="$PROJECT_ROOT/var/logs/launchd-guardian.log"

BACKEND_LABEL="com.adapteros.backend"
BACKEND_PLIST_NAME="${BACKEND_LABEL}.plist"
BACKEND_PLIST_SOURCE="$SCRIPT_DIR/launchd/${BACKEND_PLIST_NAME}.template"
BACKEND_PLIST_DEST="$HOME/Library/LaunchAgents/$BACKEND_PLIST_NAME"
BACKEND_SCRIPT="$SCRIPT_DIR/launchd/aos-launchd-run-backend.sh"
BACKEND_LOG="$PROJECT_ROOT/var/logs/launchd-backend.log"

UID_NUM="$(id -u)"
TARGET_DOMAIN="gui/${UID_NUM}"

# Verify required files exist
if [[ ! -f "$GUARDIAN_PLIST_SOURCE" ]]; then
    error "Guardian plist template not found at $GUARDIAN_PLIST_SOURCE"
    exit 1
fi
if [[ ! -f "$BACKEND_PLIST_SOURCE" ]]; then
    error "Backend plist template not found at $BACKEND_PLIST_SOURCE"
    exit 1
fi
if [[ ! -x "$GUARDIAN_SCRIPT" ]]; then
    error "Guardian script not executable at $GUARDIAN_SCRIPT"
    exit 1
fi
if [[ ! -x "$BACKEND_SCRIPT" ]]; then
    error "Backend launchd script not executable at $BACKEND_SCRIPT"
    exit 1
fi

info "Installing adapterOS stack guardian as launchd service..."
info ""

# Create necessary directories
info "Creating directories..."
mkdir -p "$HOME/Library/LaunchAgents"
mkdir -p "$PROJECT_ROOT/var/logs"
mkdir -p "$PROJECT_ROOT/var/run"

# Render plists to LaunchAgents with absolute paths
info "Rendering launchd plists..."
sed \
    -e "s#__PROJECT_ROOT__#${PROJECT_ROOT}#g" \
    -e "s#__LAUNCHD_LOG__#${GUARDIAN_LOG}#g" \
    "$GUARDIAN_PLIST_SOURCE" > "$GUARDIAN_PLIST_DEST"
chmod 644 "$GUARDIAN_PLIST_DEST"

sed \
    -e "s#__PROJECT_ROOT__#${PROJECT_ROOT}#g" \
    -e "s#__LAUNCHD_BACKEND_LOG__#${BACKEND_LOG}#g" \
    "$BACKEND_PLIST_SOURCE" > "$BACKEND_PLIST_DEST"
chmod 644 "$BACKEND_PLIST_DEST"

# Reload backend and guardian
for label in "$BACKEND_LABEL" "$GUARDIAN_LABEL"; do
    if launchctl print "${TARGET_DOMAIN}/${label}" >/dev/null 2>&1; then
        warning "${label} already loaded, reloading..."
        launchctl bootout "${TARGET_DOMAIN}/${label}" >/dev/null 2>&1 || true
    fi
done

info "Bootstrapping backend service..."
if [[ -x "$SCRIPT_DIR/service-manager.sh" ]]; then
    info "Handing backend ownership to launchd (stopping existing backend first)..."
    "$SCRIPT_DIR/service-manager.sh" stop backend graceful >/dev/null 2>&1 || true
fi
launchctl bootstrap "${TARGET_DOMAIN}" "$BACKEND_PLIST_DEST"
launchctl enable "${TARGET_DOMAIN}/${BACKEND_LABEL}" >/dev/null 2>&1 || true
launchctl kickstart -k "${TARGET_DOMAIN}/${BACKEND_LABEL}" >/dev/null 2>&1 || true

info "Bootstrapping guardian service..."
launchctl bootstrap "${TARGET_DOMAIN}" "$GUARDIAN_PLIST_DEST"
launchctl enable "${TARGET_DOMAIN}/${GUARDIAN_LABEL}" >/dev/null 2>&1 || true
launchctl kickstart -k "${TARGET_DOMAIN}/${GUARDIAN_LABEL}" >/dev/null 2>&1 || true

# Wait a moment for service to start
sleep 2

# Check status
if launchctl print "${TARGET_DOMAIN}/${GUARDIAN_LABEL}" >/dev/null 2>&1 &&
    launchctl print "${TARGET_DOMAIN}/${BACKEND_LABEL}" >/dev/null 2>&1; then
    success "Services installed and loaded successfully."
    info ""
    info "Service management commands:"
    info "  Backend start:   launchctl kickstart -k ${TARGET_DOMAIN}/${BACKEND_LABEL}"
    info "  Backend stop:    launchctl bootout ${TARGET_DOMAIN}/${BACKEND_LABEL}"
    info "  Backend status:  launchctl print ${TARGET_DOMAIN}/${BACKEND_LABEL}"
    info "  Backend logs:    tail -f ${BACKEND_LOG}"
    info ""
    info "  Guardian start:  launchctl kickstart -k ${TARGET_DOMAIN}/${GUARDIAN_LABEL}"
    info "  Guardian stop:   launchctl bootout ${TARGET_DOMAIN}/${GUARDIAN_LABEL}"
    info "  Guardian status: launchctl print ${TARGET_DOMAIN}/${GUARDIAN_LABEL}"
    info "  Guardian logs:   tail -f ${GUARDIAN_LOG}"
    info ""
    info "To uninstall:"
    info "  launchctl bootout ${TARGET_DOMAIN}/${GUARDIAN_LABEL}"
    info "  launchctl bootout ${TARGET_DOMAIN}/${BACKEND_LABEL}"
    info "  rm $GUARDIAN_PLIST_DEST"
    info "  rm $BACKEND_PLIST_DEST"
else
    error "One or more services failed to load. Check system logs:"
    error "  log show --predicate 'subsystem == \"com.apple.launchd\"' --last 5m"
    exit 1
fi
