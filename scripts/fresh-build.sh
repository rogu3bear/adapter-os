#!/bin/bash
# adapterOS Fresh Build Script
# Ensures clean rebuilds by stopping services and freeing ports
#
# Copyright (c) 2025 JKCA / James KC Auchterlonie. All rights reserved.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
source "$PROJECT_ROOT/scripts/lib/build-targets.sh"

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
BACKEND_PORT="${AOS_SERVER_PORT:-18080}"
UI_PORT="${AOS_UI_PORT:-18081}"
SERVICE_MANAGER="$PROJECT_ROOT/scripts/service-manager.sh"
GRACEFUL_SHUTDOWN="$PROJECT_ROOT/scripts/graceful-shutdown.sh"
PORT_GUARD_SCRIPT="$PROJECT_ROOT/scripts/port-guard.sh"
FULL_CLEAN=0

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
status_msg "Build context: target_root=$(aos_build_target_root) sccache=$(aos_sccache_mode)"

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

# Kill any orphaned processes (PID-file scoped only)
kill_orphaned_processes() {
    status_msg "Checking for orphaned adapterOS processes..."

    local pid_files=(
        "$PROJECT_ROOT/var/backend.pid"
        "$PROJECT_ROOT/var/worker.pid"
        "$PROJECT_ROOT/var/secd.pid"
        "$PROJECT_ROOT/var/node.pid"
        "$PROJECT_ROOT/var/run/foundation-backend.pid"
        "$PROJECT_ROOT/var/run/foundation-smoke-backend.pid"
    )

    local found=0
    for pf in "${pid_files[@]}"; do
        [ -f "$pf" ] || continue
        local pid
        pid=$(cat "$pf" 2>/dev/null) || continue
        [ -n "$pid" ] || continue

        if kill -0 "$pid" 2>/dev/null; then
            warning_msg "Found tracked process still alive: pid=$pid (from $pf), terminating..."
            kill -TERM "$pid" 2>/dev/null || true
            found=1
        else
            # Stale PID file - clean it up
            rm -f "$pf"
        fi
    done

    if [ "$found" -eq 1 ]; then
        sleep 2
    else
        status_msg "No orphaned processes found"
    fi
}

# Clean incremental artifacts only (default surgical mode).
clean_incremental_artifacts() {
    status_msg "Pruning incremental caches..."

    local incremental_dirs=()
    local dir
    local target_root

    add_incremental_dir() {
        local candidate="$1"
        [ -d "$candidate" ] || return 0

        local existing
        for existing in "${incremental_dirs[@]}"; do
            if [ "$existing" = "$candidate" ]; then
                return 0
            fi
        done
        incremental_dirs+=("$candidate")
    }

    # Legacy shared target.
    while IFS= read -r dir; do
        [ -n "$dir" ] && add_incremental_dir "$dir"
    done < <(aos_incremental_dirs_for_target_dir "$PROJECT_ROOT/target")

    # Flow-partitioned targets.
    target_root="$(aos_build_target_root)"
    while IFS= read -r dir; do
        [ -n "$dir" ] && add_incremental_dir "$dir"
    done < <(aos_incremental_dirs_for_target_dir "$target_root")

    local flow
    for flow in ui server worker test; do
        while IFS= read -r dir; do
            [ -n "$dir" ] && add_incremental_dir "$dir"
        done < <(aos_incremental_dirs_for_target_dir "$(aos_target_dir_for_flow "$flow")")
    done

    if [ "${#incremental_dirs[@]}" -eq 0 ]; then
        status_msg "No incremental caches found"
        return 0
    fi

    local removed=0
    for dir in "${incremental_dirs[@]}"; do
        rm -rf "$dir" 2>/dev/null || true
        removed=$((removed + 1))
    done

    success_msg "Pruned $removed incremental cache director$( [ "$removed" -eq 1 ] && echo "y" || echo "ies" )"
}

# Full legacy cleanup behavior (opt-in).
clean_build_artifacts_full() {
    status_msg "Running full clean (explicit --full-clean)..."

    local static_dir="crates/adapteros-server/static"

    cargo clean 2>/dev/null || true
    rm -f metal/*.air metal/*.metallib 2>/dev/null || true
    rm -rf dist 2>/dev/null || true
    if [ -d "$static_dir" ]; then
        find "$static_dir" -mindepth 1 -maxdepth 1 -exec rm -rf {} + 2>/dev/null || true
    fi
    mkdir -p "$static_dir"

    success_msg "Full build artifact cleanup complete"
}

clean_build_artifacts() {
    clean_incremental_artifacts
    if [ "$FULL_CLEAN" -eq 1 ]; then
        clean_build_artifacts_full
    else
        status_msg "Skipping full cargo clean/static wipe (use --full-clean)"
    fi
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

    # Check for any remaining tracked processes
    local stale_pids=0
    for pf in "$PROJECT_ROOT/var/backend.pid" "$PROJECT_ROOT/var/run/foundation-backend.pid"; do
        if [ -f "$pf" ]; then
            local check_pid
            check_pid=$(cat "$pf" 2>/dev/null) || continue
            if [ -n "$check_pid" ] && kill -0 "$check_pid" 2>/dev/null; then
                warning_msg "Tracked backend process still alive: pid=$check_pid (from $pf)"
                stale_pids=1
            fi
        fi
    done
    if [ "$stale_pids" -eq 1 ]; then
        warning_msg "Some tracked adapterOS processes may still be running"
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
    echo "adapterOS Fresh Build Script"
    echo ""
    echo "Ensures clean rebuilds by stopping services and freeing ports"
    echo ""
    echo "USAGE: $0 [options]"
    echo ""
    echo "OPTIONS:"
    echo "  --help, -h       Show this help message"
    echo "  --full-clean     Also run full legacy cleanup (cargo clean + dist wipe + static contents reset)"
    echo ""
    echo "This script will:"
    echo "  1. Stop all running adapterOS services gracefully"
    echo "  2. Kill any orphaned adapterOS processes"
    echo "  3. Verify all ports are free (8080, 3200)"
    echo "  4. Prune incremental caches (default surgical mode)"
    echo "  5. Optional full clean with --full-clean"
    echo "  6. Verify environment is ready for clean build"
    echo ""
    echo "Use with: ./scripts/fresh-build.sh then your build command"
}

# Main
while [[ $# -gt 0 ]]; do
    case "$1" in
        --help|-h)
            usage
            exit 0
            ;;
        --full-clean)
            FULL_CLEAN=1
            shift
            ;;
        *)
            error_msg "Unknown option: $1"
            echo ""
            usage
            exit 1
            ;;
    esac
done

fresh_build
exit $?
