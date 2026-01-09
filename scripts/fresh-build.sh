#!/bin/bash
# AdapterOS Fresh Build Script
# Ensures clean rebuilds by stopping services and freeing ports
#
# Copyright (c) 2025 JKCA / James KC Auchterlonie. All rights reserved.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
WHITE='\033[1;37m'
NC='\033[0m'

# Configuration
BACKEND_PORT="${AOS_SERVER_PORT:-8080}"
UI_PORT="${AOS_UI_PORT:-3200}"
SERVICE_MANAGER="$PROJECT_ROOT/scripts/service-manager.sh"
GRACEFUL_SHUTDOWN="$PROJECT_ROOT/scripts/graceful-shutdown.sh"
PORT_GUARD_SCRIPT="$PROJECT_ROOT/scripts/port-guard.sh"

if [ -f "$PORT_GUARD_SCRIPT" ]; then
    # shellcheck disable=SC1090
    source "$PORT_GUARD_SCRIPT"
else
    warning_msg "Port guard script missing at $PORT_GUARD_SCRIPT; port cleanup will be manual."
    ensure_port_free() { return 0; }
fi

# Status messages
status_msg() { echo -e "${BLUE}ℹ️  ${1}${NC}"; }
success_msg() { echo -e "${GREEN}✅ ${1}${NC}"; }
warning_msg() { echo -e "${YELLOW}⚠️  ${1}${NC}"; }
error_msg() { echo -e "${RED}❌ ${1}${NC}"; }

# Check if port is occupied
check_port_occupied() {
    local port="$1"
    lsof -i :"$port" >/dev/null 2>&1
}

# Force kill processes on port (last resort)
force_kill_port() {
    local port="$1"
    local service_name="$2"

    warning_msg "Force killing all processes on port $port ($service_name)..."

    # Get PIDs using the port
    local pids=$(lsof -t -i :"$port" 2>/dev/null || true)

    if [ -z "$pids" ]; then
        return 0
    fi

    # Kill each PID
    for pid in $pids; do
        if kill -9 "$pid" 2>/dev/null; then
            status_msg "Force killed PID $pid"
        fi
    done

    # Wait for port to be free
    local attempts=0
    while [ $attempts -lt 5 ]; do
        if ! check_port_occupied "$port"; then
            success_msg "Port $port is now free"
            return 0
        fi
        sleep 1
        ((attempts++))
    done

    error_msg "Failed to free port $port"
    return 1
}

# Stop service gracefully with fallback
stop_service_gracefully() {
    local service="$1"
    local port="$2"
    local service_name="$3"

    status_msg "Checking $service_name status..."

    # Check if service manager exists
    if [ -x "$SERVICE_MANAGER" ]; then
        # Use service manager for graceful shutdown
        if "$SERVICE_MANAGER" status | grep -q "$service.*RUNNING\|$service.*STARTED"; then
            status_msg "Stopping $service_name via service manager..."
            if "$SERVICE_MANAGER" stop "$service" graceful; then
                success_msg "$service_name stopped gracefully"
                return 0
            else
                warning_msg "Service manager failed, trying manual port cleanup..."
            fi
        else
            status_msg "$service_name is not running"
            return 0
        fi
    fi

    # Fallback: check port and kill processes
    if check_port_occupied "$port"; then
        warning_msg "$service_name port $port is occupied, attempting cleanup..."

        # Try shared port guard first (graceful)
        if ! ensure_port_free "$port" "$service_name"; then
            # Try graceful shutdown script if available
            if [ -x "$GRACEFUL_SHUTDOWN" ]; then
                "$GRACEFUL_SHUTDOWN" fast
            fi

            # If still occupied, force kill
            if check_port_occupied "$port"; then
                force_kill_port "$port" "$service_name"
            fi
        fi
    else
        status_msg "Port $port is already free"
    fi
}

# Kill any orphaned processes
kill_orphaned_processes() {
    status_msg "Checking for orphaned AdapterOS processes..."

    # Find processes that look like AdapterOS but aren't managed
    local orphaned_pids=$(ps aux | grep -E "(adapteros)" | grep -v grep | awk '{print $2}' || true)

    if [ -n "$orphaned_pids" ]; then
        warning_msg "Found orphaned processes, cleaning up..."
        for pid in $orphaned_pids; do
            if kill -TERM "$pid" 2>/dev/null; then
                status_msg "Terminated orphaned PID $pid"
            fi
        done
        sleep 2
    else
        status_msg "No orphaned processes found"
    fi
}

# Clean build artifacts
clean_build_artifacts() {
    status_msg "Cleaning build artifacts..."

    # Cargo clean
    cargo clean 2>/dev/null || true

    # Remove Metal artifacts
    rm -f metal/*.air metal/*.metallib 2>/dev/null || true

    # Remove dist artifacts
    rm -rf dist 2>/dev/null || true
    rm -rf crates/adapteros-server/static 2>/dev/null || true

    success_msg "Build artifacts cleaned"
}

# Main fresh build function
fresh_build() {
    echo -e "${PURPLE}
╔══════════════════════════════════════════════════════════════╗
║                   🧹 FRESH BUILD CLEANUP                      ║
║             Preparing clean build environment               ║
╚══════════════════════════════════════════════════════════════╝${NC}"
    echo ""

    local start_time=$(date +%s)
    local errors=0

    # Phase 1: Stop running services
    echo -e "${CYAN}Phase 1: Stopping running services...${NC}"

    # Stop UI service (port 3200)
    if ! stop_service_gracefully "ui" "$UI_PORT" "Web UI"; then
        ((errors++))
    fi

    # Stop backend service (port 8080)
    if ! stop_service_gracefully "backend" "$BACKEND_PORT" "Backend Server"; then
        ((errors++))
    fi

    # Kill orphaned processes
    kill_orphaned_processes

    echo ""

    # Phase 2: Verify ports are free
    echo -e "${CYAN}Phase 2: Verifying ports are free...${NC}"

    local ports_free=true

    if check_port_occupied "$BACKEND_PORT"; then
        error_msg "Backend port $BACKEND_PORT is still occupied"
        ports_free=false
    fi

    if check_port_occupied "$UI_PORT"; then
        error_msg "UI port $UI_PORT is still occupied"
        ports_free=false
    fi

    if $ports_free; then
        success_msg "All ports are free"
    else
        error_msg "Some ports are still occupied - build may fail"
        ((errors++))
    fi

    echo ""

    # Phase 3: Clean build artifacts
    echo -e "${CYAN}Phase 3: Cleaning build artifacts...${NC}"
    clean_build_artifacts

    echo ""

    # Phase 4: Final verification
    echo -e "${CYAN}Phase 4: Final verification...${NC}"

    # Check for any remaining issues
    if pgrep -f "adapteros-server" >/dev/null 2>&1; then
        warning_msg "Some AdapterOS processes may still be running"
        ((errors++))
    fi

    local end_time=$(date +%s)
    local duration=$((end_time - start_time))

    echo -e "${CYAN}════════════════════════════════════════════════${NC}"

    if [ $errors -eq 0 ]; then
        success_msg "Fresh build preparation complete in ${duration}s"
        success_msg "Ready for clean build!"
        echo -e "${WHITE}Next: Run your build command (cargo build --release --locked --offline, etc.)${NC}"
        return 0
    else
        warning_msg "Fresh build preparation completed with $errors issue(s) in ${duration}s"
        warning_msg "Build may encounter issues - consider manual cleanup"
        return 1
    fi
}

# Show usage
usage() {
    echo "AdapterOS Fresh Build Script"
    echo ""
    echo "Ensures clean rebuilds by stopping services and freeing ports"
    echo ""
    echo "USAGE: $0 [options]"
    echo ""
    echo "OPTIONS:"
    echo "  --help, -h    Show this help message"
    echo ""
    echo "This script will:"
    echo "  1. Stop all running AdapterOS services gracefully"
    echo "  2. Kill any orphaned AdapterOS processes"
    echo "  3. Verify all ports are free (8080, 3200)"
    echo "  4. Clean build artifacts"
    echo "  5. Verify environment is ready for clean build"
    echo ""
    echo "Use with: ./scripts/fresh-build.sh then your build command"
}

# Main
case "${1:-}" in
    --help|-h)
        usage
        exit 0
        ;;
    "")
        fresh_build
        exit $?
        ;;
    *)
        error_msg "Unknown option: $1"
        echo ""
        usage
        exit 1
        ;;
esac
