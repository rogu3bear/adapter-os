#!/bin/bash
# AdapterOS Service Manager
# Manages starting, stopping, and status of AdapterOS services
#
# Copyright (c) 2025 JKCA / James KC Auchterlonie. All rights reserved.
#
# Usage:
#   ./scripts/service-manager.sh start <service>     Start a service (backend, ui, menu-bar)
#   ./scripts/service-manager.sh stop all [mode]    Stop all services (graceful|fast|immediate)
#   ./scripts/service-manager.sh status             Show status of all services
#
# Called by launch.sh for coordinated service management.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# =============================================================================
# Configuration
# =============================================================================

# PID file locations
PID_DIR="$PROJECT_ROOT/var"
BACKEND_PID_FILE="$PID_DIR/backend.pid"
UI_PID_FILE="$PID_DIR/ui.pid"
MENU_BAR_PID_FILE="$PID_DIR/menu-bar.pid"

# Log file locations
LOG_DIR="$PROJECT_ROOT/var/logs"
BACKEND_LOG="$LOG_DIR/backend.log"
UI_LOG="$LOG_DIR/ui.log"
MENU_BAR_LOG="$LOG_DIR/menu-bar.log"

# Port configuration
BACKEND_PORT="${AOS_SERVER_PORT:-8080}"
UI_PORT="${AOS_UI_PORT:-3200}"

# Timeouts (in seconds)
GRACEFUL_TIMEOUT=120
FAST_TIMEOUT=30
FORCE_TIMEOUT=10
UI_TIMEOUT=15
MENU_BAR_TIMEOUT=5

# =============================================================================
# Colors
# =============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
WHITE='\033[1;37m'
NC='\033[0m' # No Color

# =============================================================================
# Helper Functions
# =============================================================================

status_msg() {
    echo -e "${BLUE}[INFO]${NC} ${1}"
}

success_msg() {
    echo -e "${GREEN}[OK]${NC} ${1}"
}

warning_msg() {
    echo -e "${YELLOW}[WARN]${NC} ${1}"
}

error_msg() {
    echo -e "${RED}[ERROR]${NC} ${1}"
}

# Shared port guard
PORT_GUARD_SCRIPT="$PROJECT_ROOT/scripts/port-guard.sh"
if [ -f "$PORT_GUARD_SCRIPT" ]; then
    # shellcheck disable=SC1090
    source "$PORT_GUARD_SCRIPT"
else
    warning_msg "Port guard script missing at $PORT_GUARD_SCRIPT; port cleanup will be manual."
    ensure_port_free() { return 0; }
fi

# Ensure directories exist
ensure_dirs() {
    mkdir -p "$PID_DIR"
    mkdir -p "$LOG_DIR"
}

# Check if process is running by PID file
is_running() {
    local pid_file="$1"
    if [ -f "$pid_file" ]; then
        local pid=$(cat "$pid_file" 2>/dev/null)
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            return 0
        else
            # Stale PID file, clean up
            rm -f "$pid_file"
            return 1
        fi
    fi
    return 1
}

# Get PID from file
get_pid() {
    local pid_file="$1"
    if [ -f "$pid_file" ]; then
        cat "$pid_file" 2>/dev/null
    fi
}

# Wait for process to stop with timeout
wait_for_stop() {
    local pid="$1"
    local timeout="$2"
    local start_time=$(date +%s)

    while [ $(($(date +%s) - start_time)) -lt "$timeout" ]; do
        if ! kill -0 "$pid" 2>/dev/null; then
            return 0
        fi
        sleep 1
    done

    return 1
}

# Stop a process gracefully with fallback to force kill
stop_process() {
    local pid="$1"
    local service_name="$2"
    local graceful_timeout="$3"
    local mode="${4:-graceful}"

    if [ -z "$pid" ] || ! kill -0 "$pid" 2>/dev/null; then
        return 0  # Already stopped
    fi

    case "$mode" in
        graceful)
            status_msg "Stopping $service_name (PID: $pid) gracefully..."
            if kill -TERM "$pid" 2>/dev/null; then
                if wait_for_stop "$pid" "$graceful_timeout"; then
                    success_msg "$service_name stopped gracefully"
                    return 0
                else
                    warning_msg "$service_name did not stop within ${graceful_timeout}s, forcing..."
                fi
            fi
            ;;
        fast)
            status_msg "Stopping $service_name (PID: $pid) quickly..."
            # Backend supports SIGUSR1 for fast shutdown
            if [ "$service_name" = "Backend" ]; then
                kill -USR1 "$pid" 2>/dev/null || true
                if wait_for_stop "$pid" "$FAST_TIMEOUT"; then
                    success_msg "$service_name stopped quickly"
                    return 0
                fi
            fi
            kill -TERM "$pid" 2>/dev/null || true
            if wait_for_stop "$pid" "$FAST_TIMEOUT"; then
                success_msg "$service_name stopped"
                return 0
            fi
            ;;
        immediate)
            status_msg "Stopping $service_name (PID: $pid) immediately..."
            # Backend supports SIGUSR2 for immediate shutdown
            if [ "$service_name" = "Backend" ]; then
                kill -USR2 "$pid" 2>/dev/null || true
                sleep 2
            fi
            ;;
    esac

    # Force kill if still running
    if kill -0 "$pid" 2>/dev/null; then
        kill -KILL "$pid" 2>/dev/null || true
        if wait_for_stop "$pid" "$FORCE_TIMEOUT"; then
            warning_msg "$service_name force stopped"
            return 0
        else
            error_msg "$service_name failed to stop"
            return 1
        fi
    fi

    return 0
}

# =============================================================================
# Service: Backend
# =============================================================================

start_backend() {
    ensure_dirs

    if is_running "$BACKEND_PID_FILE"; then
        local pid=$(get_pid "$BACKEND_PID_FILE")
        warning_msg "Backend is already running (PID: $pid)"
        return 0
    fi

    status_msg "Starting Backend Server..."

    if ! ensure_port_free "$BACKEND_PORT" "Backend API"; then
        error_msg "Backend port $BACKEND_PORT is busy; unable to start."
        return 1
    fi

    # Check if binary exists
    local server_bin=""
    if [ -f "$PROJECT_ROOT/target/release/adapteros-server" ]; then
        server_bin="$PROJECT_ROOT/target/release/adapteros-server"
    elif [ -f "$PROJECT_ROOT/target/debug/adapteros-server" ]; then
        server_bin="$PROJECT_ROOT/target/debug/adapteros-server"
    else
        status_msg "Backend binary not found. Building..."
        cd "$PROJECT_ROOT"
        if cargo build 2>&1 | tail -10; then
            if [ -f "$PROJECT_ROOT/target/debug/adapteros-server" ]; then
                server_bin="$PROJECT_ROOT/target/debug/adapteros-server"
            else
                error_msg "Build completed but binary not found"
                return 1
            fi
        else
            error_msg "Failed to build backend"
            return 1
        fi
    fi

    # Set up environment
    export DATABASE_URL="${DATABASE_URL:-sqlite://$PROJECT_ROOT/var/aos-cp.sqlite3}"
    export RUST_LOG="${RUST_LOG:-info}"

    # Load model path if available
    if [ -d "$PROJECT_ROOT/models" ]; then
        local model_path=$(find "$PROJECT_ROOT/models" -maxdepth 1 -type d ! -name "models" | head -1)
        if [ -n "$model_path" ]; then
            export AOS_MLX_FFI_MODEL="$model_path"
        fi
    fi

    # Ensure database directory exists
    mkdir -p "$PROJECT_ROOT/var"

    # Start backend server
    nohup "$server_bin" --config "${AOS_CONFIG_PATH:-$PROJECT_ROOT/configs/cp.toml}" \
        > "$BACKEND_LOG" 2>&1 &
    local pid=$!

    echo "$pid" > "$BACKEND_PID_FILE"

    # Give it a moment to start
    sleep 2

    if kill -0 "$pid" 2>/dev/null; then
        success_msg "Backend started (PID: $pid, Port: $BACKEND_PORT)"
        return 0
    else
        error_msg "Backend failed to start. Check logs: $BACKEND_LOG"
        rm -f "$BACKEND_PID_FILE"
        return 1
    fi
}

stop_backend() {
    local mode="${1:-graceful}"

    if ! is_running "$BACKEND_PID_FILE"; then
        status_msg "Backend is not running"
        return 0
    fi

    local pid=$(get_pid "$BACKEND_PID_FILE")
    stop_process "$pid" "Backend" "$GRACEFUL_TIMEOUT" "$mode"
    rm -f "$BACKEND_PID_FILE"
}

# =============================================================================
# Service: UI
# =============================================================================

start_ui() {
    ensure_dirs

    if is_running "$UI_PID_FILE"; then
        local pid=$(get_pid "$UI_PID_FILE")
        warning_msg "UI is already running (PID: $pid)"
        return 0
    fi

    status_msg "Starting Web UI..."

    if ! ensure_port_free "$UI_PORT" "Web UI"; then
        error_msg "UI port $UI_PORT is busy; unable to start."
        return 1
    fi

    # Check if pnpm is available
    if ! command -v pnpm &> /dev/null; then
        error_msg "pnpm not found. Install with: npm install -g pnpm"
        return 1
    fi

    # Check if UI directory exists
    if [ ! -d "$PROJECT_ROOT/ui" ]; then
        error_msg "UI directory not found: $PROJECT_ROOT/ui"
        return 1
    fi

    # Install dependencies if needed
    if [ ! -d "$PROJECT_ROOT/ui/node_modules" ]; then
        status_msg "Installing UI dependencies..."
        cd "$PROJECT_ROOT/ui"
        pnpm install --silent
        cd "$PROJECT_ROOT"
    fi

    # Start UI dev server
    cd "$PROJECT_ROOT/ui"

    # Set the port for Vite
    export VITE_PORT="$UI_PORT"
    export PORT="$UI_PORT"

    nohup pnpm dev --port "$UI_PORT" > "$UI_LOG" 2>&1 &
    local pid=$!

    cd "$PROJECT_ROOT"

    echo "$pid" > "$UI_PID_FILE"

    # Give it a moment to start
    sleep 3

    if kill -0 "$pid" 2>/dev/null; then
        success_msg "UI started (PID: $pid, Port: $UI_PORT)"
        return 0
    else
        error_msg "UI failed to start. Check logs: $UI_LOG"
        rm -f "$UI_PID_FILE"
        return 1
    fi
}

stop_ui() {
    local mode="${1:-graceful}"

    if ! is_running "$UI_PID_FILE"; then
        status_msg "UI is not running"
        return 0
    fi

    local pid=$(get_pid "$UI_PID_FILE")
    stop_process "$pid" "UI" "$UI_TIMEOUT" "$mode"
    rm -f "$UI_PID_FILE"

    # Also kill any orphaned node processes from the UI
    pkill -f "vite.*$UI_PORT" 2>/dev/null || true
}

# =============================================================================
# Service: Menu Bar App (macOS only)
# =============================================================================

start_menu_bar() {
    ensure_dirs

    if [[ "$OSTYPE" != "darwin"* ]]; then
        status_msg "Menu Bar App is only available on macOS"
        return 0
    fi

    if is_running "$MENU_BAR_PID_FILE"; then
        local pid=$(get_pid "$MENU_BAR_PID_FILE")
        warning_msg "Menu Bar App is already running (PID: $pid)"
        return 0
    fi

    status_msg "Starting Menu Bar App..."

    # Check if the menu bar app exists
    local menu_bar_app="$PROJECT_ROOT/menu-bar-app"

    if [ ! -d "$menu_bar_app" ]; then
        warning_msg "Menu Bar App directory not found: $menu_bar_app"
        return 0  # Not an error, just optional
    fi

    # Check for Swift package
    if [ -f "$menu_bar_app/Package.swift" ]; then
        cd "$menu_bar_app"

        # Build if needed
        if [ ! -d ".build/release" ]; then
            status_msg "Building Menu Bar App..."
            swift build -c release 2>&1 | tail -5 || {
                warning_msg "Menu Bar App build failed (optional component)"
                cd "$PROJECT_ROOT"
                return 0
            }
        fi

        # Find the executable
        local executable=$(find .build/release -maxdepth 1 -type f -perm +111 | head -1)

        if [ -n "$executable" ] && [ -x "$executable" ]; then
            nohup "$executable" > "$MENU_BAR_LOG" 2>&1 &
            local pid=$!
            echo "$pid" > "$MENU_BAR_PID_FILE"

            sleep 1

            if kill -0 "$pid" 2>/dev/null; then
                success_msg "Menu Bar App started (PID: $pid)"
            else
                warning_msg "Menu Bar App failed to start (optional component)"
                rm -f "$MENU_BAR_PID_FILE"
            fi
        else
            warning_msg "Menu Bar App executable not found (optional component)"
        fi

        cd "$PROJECT_ROOT"
    else
        status_msg "Menu Bar App not configured (optional component)"
    fi

    return 0
}

stop_menu_bar() {
    local mode="${1:-graceful}"

    if [[ "$OSTYPE" != "darwin"* ]]; then
        return 0
    fi

    if ! is_running "$MENU_BAR_PID_FILE"; then
        status_msg "Menu Bar App is not running"
        return 0
    fi

    local pid=$(get_pid "$MENU_BAR_PID_FILE")
    stop_process "$pid" "Menu Bar App" "$MENU_BAR_TIMEOUT" "$mode"
    rm -f "$MENU_BAR_PID_FILE"
}

# =============================================================================
# Status Command
# =============================================================================

show_status() {
    echo -e "${CYAN}
================================
   AdapterOS Service Status
================================${NC}"
    echo ""

    # Backend status
    if is_running "$BACKEND_PID_FILE"; then
        local pid=$(get_pid "$BACKEND_PID_FILE")
        echo -e "${GREEN}[RUNNING]${NC} Backend Server (PID: $pid, Port: $BACKEND_PORT)"

        # Check if HTTP endpoint responds
        if curl -s "http://localhost:$BACKEND_PORT/healthz" > /dev/null 2>&1; then
            echo -e "          ${GREEN}Health endpoint responding${NC}"
        else
            echo -e "          ${YELLOW}Health endpoint not responding${NC}"
        fi
    else
        echo -e "${RED}[STOPPED]${NC} Backend Server"
    fi

    # UI status
    if is_running "$UI_PID_FILE"; then
        local pid=$(get_pid "$UI_PID_FILE")
        echo -e "${GREEN}[RUNNING]${NC} Web UI (PID: $pid, Port: $UI_PORT)"

        # Check if UI responds
        if curl -s "http://localhost:$UI_PORT" > /dev/null 2>&1; then
            echo -e "          ${GREEN}Web UI responding${NC}"
        else
            echo -e "          ${YELLOW}Web UI initializing...${NC}"
        fi
    else
        echo -e "${RED}[STOPPED]${NC} Web UI"
    fi

    # Menu Bar status (macOS only)
    if [[ "$OSTYPE" == "darwin"* ]]; then
        if is_running "$MENU_BAR_PID_FILE"; then
            local pid=$(get_pid "$MENU_BAR_PID_FILE")
            echo -e "${GREEN}[RUNNING]${NC} Menu Bar App (PID: $pid)"
        else
            echo -e "${WHITE}[STOPPED]${NC} Menu Bar App (optional)"
        fi
    fi

    echo ""
    echo -e "${CYAN}================================${NC}"
}

# =============================================================================
# Stop All Services
# =============================================================================

stop_all() {
    local mode="${1:-graceful}"

    echo -e "${CYAN}
================================
   Stopping All Services
   Mode: ${mode^^}
================================${NC}"
    echo ""

    # Stop in reverse order of startup

    # 1. UI
    stop_ui "$mode"

    # 2. Menu Bar (macOS only)
    if [[ "$OSTYPE" == "darwin"* ]]; then
        stop_menu_bar "$mode"
    fi

    # 3. Backend (last, as others may depend on it)
    stop_backend "$mode"

    echo ""
    success_msg "All services stopped"
}

# =============================================================================
# Usage
# =============================================================================

usage() {
    echo "AdapterOS Service Manager"
    echo ""
    echo "USAGE:"
    echo "  $0 start <service>        Start a service"
    echo "  $0 stop all [mode]        Stop all services"
    echo "  $0 stop <service> [mode]  Stop a specific service"
    echo "  $0 status                 Show status of all services"
    echo ""
    echo "SERVICES:"
    echo "  backend     Backend API server"
    echo "  ui          Web UI development server"
    echo "  menu-bar    Menu Bar status app (macOS only)"
    echo ""
    echo "STOP MODES:"
    echo "  graceful    Graceful shutdown with full cleanup (default)"
    echo "  fast        Fast shutdown, reduced cleanup"
    echo "  immediate   Immediate shutdown, minimal cleanup"
    echo ""
    echo "EXAMPLES:"
    echo "  $0 start backend          # Start backend server"
    echo "  $0 start ui               # Start web UI"
    echo "  $0 stop all               # Stop all services gracefully"
    echo "  $0 stop all fast          # Fast stop all services"
    echo "  $0 status                 # Show service status"
}

# =============================================================================
# Main Entry Point
# =============================================================================

if [ $# -lt 1 ]; then
    usage
    exit 1
fi

COMMAND="$1"
SERVICE="${2:-}"
MODE="${3:-graceful}"

case "$COMMAND" in
    start)
        case "$SERVICE" in
            backend)
                start_backend
                ;;
            ui)
                start_ui
                ;;
            menu-bar|menubar)
                start_menu_bar
                ;;
            "")
                error_msg "Please specify a service to start"
                usage
                exit 1
                ;;
            *)
                error_msg "Unknown service: $SERVICE"
                usage
                exit 1
                ;;
        esac
        ;;
    stop)
        case "$SERVICE" in
            all)
                stop_all "$MODE"
                ;;
            backend)
                stop_backend "$MODE"
                ;;
            ui)
                stop_ui "$MODE"
                ;;
            menu-bar|menubar)
                stop_menu_bar "$MODE"
                ;;
            "")
                error_msg "Please specify a service to stop (or 'all')"
                usage
                exit 1
                ;;
            *)
                error_msg "Unknown service: $SERVICE"
                usage
                exit 1
                ;;
        esac
        ;;
    status)
        show_status
        ;;
    help|-h|--help)
        usage
        ;;
    *)
        error_msg "Unknown command: $COMMAND"
        usage
        exit 1
        ;;
esac
