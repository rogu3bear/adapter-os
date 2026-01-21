#!/bin/bash
# adapterOS Graceful Shutdown Script
# Provides robust, phased shutdown with proper cleanup and timeouts

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Configuration
BACKEND_PID_FILE="$PROJECT_ROOT/var/backend.pid"
UI_PID_FILE="$PROJECT_ROOT/var/ui.pid"

# Timeouts (in seconds)
GRACEFUL_TIMEOUT=120  # 2 minutes for graceful shutdown
FORCE_TIMEOUT=10      # 10 seconds before force kill
UI_TIMEOUT=15         # UI should stop quickly

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Shutdown mode
SHUTDOWN_MODE="${1:-graceful}"  # graceful, fast, immediate

# Helper functions
status_msg() {
    echo -e "${BLUE}ℹ️  ${1}${NC}"
}

success_msg() {
    echo -e "${GREEN}✅ ${1}${NC}"
}

warning_msg() {
    echo -e "${YELLOW}⚠️  ${1}${NC}"
}

error_msg() {
    echo -e "${RED}❌ ${1}${NC}"
}

# Check if process is running
is_running() {
    local pid_file="$1"
    if [ -f "$pid_file" ]; then
        local pid=$(cat "$pid_file")
        if kill -0 "$pid" 2>/dev/null; then
            return 0
        else
            rm -f "$pid_file"
            return 1
        fi
    fi
    return 1
}

# Get PID from file
get_pid() {
    local pid_file="$1"
    [ -f "$pid_file" ] && cat "$pid_file" || echo ""
}

# Wait for process to stop with timeout
wait_for_stop() {
    local pid="$1"
    local service_name="$2"
    local timeout="$3"
    local start_time=$(date +%s)
    
    while [ $(($(date +%s) - start_time)) -lt "$timeout" ]; do
        if ! kill -0 "$pid" 2>/dev/null; then
            return 0
        fi
        sleep 1
        echo -n "."
    done
    
    return 1
}

# Graceful stop with timeout and fallback
graceful_stop() {
    local pid="$1"
    local service_name="$2"
    local graceful_timeout="$3"
    local force_timeout="$4"
    
    if [ -z "$pid" ] || ! kill -0 "$pid" 2>/dev/null; then
        return 0  # Already stopped
    fi
    
    status_msg "Stopping $service_name (PID: $pid) gracefully..."
    
    # Send SIGTERM for graceful shutdown
    if kill -TERM "$pid" 2>/dev/null; then
        # Wait for graceful shutdown
        if wait_for_stop "$pid" "$service_name" "$graceful_timeout"; then
            success_msg "$service_name stopped gracefully"
            return 0
        else
            warning_msg "$service_name did not stop within ${graceful_timeout}s, forcing shutdown..."
        fi
    else
        warning_msg "Failed to send SIGTERM to $service_name, process may have already stopped"
        return 0
    fi
    
    # Force kill if still running
    if kill -0 "$pid" 2>/dev/null; then
        if kill -KILL "$pid" 2>/dev/null; then
            if wait_for_stop "$pid" "$service_name" "$force_timeout"; then
                warning_msg "$service_name force stopped"
                return 0
            else
                error_msg "$service_name failed to stop even after force kill"
                return 1
            fi
        else
            error_msg "Failed to force kill $service_name"
            return 1
        fi
    fi
    
    return 0
}

# Fast stop (SIGUSR1 for backend, SIGTERM for others)
fast_stop() {
    local pid="$1"
    local service_name="$2"
    local timeout="$3"
    
    if [ -z "$pid" ] || ! kill -0 "$pid" 2>/dev/null; then
        return 0
    fi
    
    status_msg "Stopping $service_name (PID: $pid) quickly..."
    
    # Backend supports SIGUSR1 for fast shutdown
    if [ "$service_name" = "Backend Server" ]; then
        if kill -USR1 "$pid" 2>/dev/null; then
            if wait_for_stop "$pid" "$service_name" "$timeout"; then
                success_msg "$service_name stopped quickly"
                return 0
            fi
        fi
    fi
    
    # Fallback to SIGTERM
    if kill -TERM "$pid" 2>/dev/null; then
        if wait_for_stop "$pid" "$service_name" "$timeout"; then
            success_msg "$service_name stopped"
            return 0
        fi
    fi
    
    # Force kill
    kill -KILL "$pid" 2>/dev/null || true
    wait_for_stop "$pid" "$service_name" "$FORCE_TIMEOUT"
    return 0
}

# Immediate stop (SIGUSR2 for backend, SIGKILL for others)
immediate_stop() {
    local pid="$1"
    local service_name="$2"
    
    if [ -z "$pid" ] || ! kill -0 "$pid" 2>/dev/null; then
        return 0
    fi
    
    status_msg "Stopping $service_name (PID: $pid) immediately..."
    
    # Backend supports SIGUSR2 for immediate shutdown
    if [ "$service_name" = "Backend Server" ]; then
        kill -USR2 "$pid" 2>/dev/null || true
        sleep 2
    fi
    
    # Force kill everything
    kill -KILL "$pid" 2>/dev/null || true
    sleep 1
    
    if ! kill -0 "$pid" 2>/dev/null; then
        success_msg "$service_name stopped immediately"
        return 0
    else
        error_msg "$service_name failed to stop"
        return 1
    fi
}

# Stop UI service
stop_ui() {
    if ! is_running "$UI_PID_FILE"; then
        return 0
    fi
    
    local pid=$(get_pid "$UI_PID_FILE")
    
    case "$SHUTDOWN_MODE" in
        graceful)
            graceful_stop "$pid" "Web UI" "$UI_TIMEOUT" "$FORCE_TIMEOUT"
            ;;
        fast)
            fast_stop "$pid" "Web UI" "$UI_TIMEOUT"
            ;;
        immediate)
            immediate_stop "$pid" "Web UI"
            ;;
    esac
    
    rm -f "$UI_PID_FILE"
}

# Stop backend server
stop_backend() {
    if ! is_running "$BACKEND_PID_FILE"; then
        return 0
    fi
    
    local pid=$(get_pid "$BACKEND_PID_FILE")
    
    case "$SHUTDOWN_MODE" in
        graceful)
            status_msg "Initiating graceful shutdown of Backend Server..."
            graceful_stop "$pid" "Backend Server" "$GRACEFUL_TIMEOUT" "$FORCE_TIMEOUT"
            ;;
        fast)
            status_msg "Initiating fast shutdown of Backend Server..."
            fast_stop "$pid" "Backend Server" "$GRACEFUL_TIMEOUT"
            ;;
        immediate)
            status_msg "Initiating immediate shutdown of Backend Server..."
            immediate_stop "$pid" "Backend Server"
            ;;
    esac
    
    rm -f "$BACKEND_PID_FILE"
}

# Main shutdown function
shutdown_all() {
    echo -e "${CYAN}
╔══════════════════════════════════════════════════════════════╗
║                  🛑 GRACEFUL SHUTDOWN                        ║
║              Mode: ${SHUTDOWN_MODE^^}                                    ║
╚══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    
    local shutdown_start=$(date +%s)
    local errors=0
    
    # Stop services in reverse order of startup
    # 1. UI (depends on backend)
    status_msg "Phase 1: Stopping Web UI..."
    if ! stop_ui; then
        ((errors++))
    fi
    echo ""
    
    # 2. Backend server (core service, stop last)
    status_msg "Phase 2: Stopping Backend Server..."
    if ! stop_backend; then
        ((errors++))
    fi
    echo ""
    
    # Final status
    local shutdown_duration=$(($(date +%s) - shutdown_start))
    
    echo -e "${CYAN}════════════════════════════════════════════════${NC}"
    if [ $errors -eq 0 ]; then
        success_msg "All services stopped successfully in ${shutdown_duration}s"
    else
        warning_msg "Shutdown completed with $errors error(s) in ${shutdown_duration}s"
    fi
    echo -e "${CYAN}════════════════════════════════════════════════${NC}"
    
    return $errors
}

# Usage
usage() {
    echo "adapterOS Graceful Shutdown"
    echo ""
    echo "USAGE: $0 [mode]"
    echo ""
    echo "MODES:"
    echo "  graceful   Graceful shutdown with full cleanup (default)"
    echo "  fast       Fast shutdown, skips drain phase"
    echo "  immediate  Immediate shutdown, minimal cleanup"
    echo ""
    echo "EXAMPLES:"
    echo "  $0              # Graceful shutdown (default)"
    echo "  $0 graceful     # Explicit graceful shutdown"
    echo "  $0 fast         # Fast shutdown"
    echo "  $0 immediate    # Immediate shutdown"
    echo ""
    echo "The script will:"
    echo "  1. Stop Web UI (if running)"
    echo "  2. Stop Backend Server (with phased cleanup)"
    echo ""
    echo "Timeouts:"
    echo "  Backend graceful: ${GRACEFUL_TIMEOUT}s"
    echo "  UI: ${UI_TIMEOUT}s"
}

# Main
case "${SHUTDOWN_MODE}" in
    graceful|fast|immediate)
        shutdown_all
        exit $?
        ;;
    help|-h|--help)
        usage
        exit 0
        ;;
    *)
        error_msg "Unknown shutdown mode: $SHUTDOWN_MODE"
        echo ""
        usage
        exit 1
        ;;
esac


