#!/bin/bash
# AdapterOS Service Manager
# Controls individual components independently

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Configuration
BACKEND_PID_FILE="$PROJECT_ROOT/var/backend.pid"
UI_PID_FILE="$PROJECT_ROOT/var/ui.pid"
MENU_BAR_PID_FILE="$PROJECT_ROOT/var/menu-bar.pid"
STATUS_FILE="$PROJECT_ROOT/var/services.json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Create necessary directories
mkdir -p "$PROJECT_ROOT/var"

# Service status tracking
update_status() {
    local service="$1"
    local status="$2"
    local pid="${3:-}"

    # Read existing status or create empty object
    if [ -f "$STATUS_FILE" ]; then
        status_json=$(cat "$STATUS_FILE")
    else
        status_json='{}'
    fi

    # Update status using jq if available, otherwise use sed
    if command -v jq >/dev/null 2>&1; then
        echo "$status_json" | jq --arg service "$service" --arg status "$status" --arg pid "$pid" \
            '. + {($service): {"status": $status, "pid": $pid, "timestamp": now | todate}}' > "$STATUS_FILE.tmp" && \
        mv "$STATUS_FILE.tmp" "$STATUS_FILE"
    else
        # Fallback without jq - basic status tracking
        echo "{\"$service\": {\"status\": \"$status\", \"pid\": \"$pid\", \"timestamp\": \"$(date -Iseconds)\"}}" > "$STATUS_FILE"
    fi
}

# Check if service is running
is_running() {
    local service="$1"
    local pid_file="$2"

    if [ -f "$pid_file" ]; then
        local pid=$(cat "$pid_file")
        if kill -0 "$pid" 2>/dev/null; then
            return 0  # Running
        else
            rm -f "$pid_file"  # Stale PID file
            return 1  # Not running
        fi
    fi
    return 1  # Not running
}

# Get service PID
get_pid() {
    local pid_file="$1"
    [ -f "$pid_file" ] && cat "$pid_file" || echo ""
}

# Backend server management
backend_start() {
    echo -e "${BLUE}Starting AdapterOS Backend Server...${NC}"

    if is_running "backend" "$BACKEND_PID_FILE"; then
        echo -e "${YELLOW}Backend server is already running (PID: $(get_pid "$BACKEND_PID_FILE"))${NC}"
        return 0
    fi

    # Change to project root for relative paths
    cd "$PROJECT_ROOT"

    # Start backend server
    ./target/debug/adapteros-server --skip-pf-check --config configs/cp.toml --single-writer > server.log 2>&1 &
    local pid=$!

    echo $pid > "$BACKEND_PID_FILE"
    update_status "backend" "running" "$pid"

    echo -e "${GREEN}Backend server started (PID: $pid, Port: 3300)${NC}"

    # Wait a bit and verify it's running
    local attempts=0
    local max_attempts=10
    
    while [ $attempts -lt $max_attempts ]; do
        sleep 1
        if ! kill -0 $pid 2>/dev/null; then
            # Process died, check logs for error
            if [ -f "server.log" ]; then
                local last_error=$(tail -5 server.log | grep -iE "(error|panic|fatal)" | tail -1)
                if [ -n "$last_error" ]; then
                    echo -e "${RED}Backend server crashed: $last_error${NC}"
                fi
            fi
            echo -e "${RED}Backend server failed to start. Check server.log${NC}"
            rm -f "$BACKEND_PID_FILE"
            update_status "backend" "failed"
            return 1
        fi
        
        # Check if port is actually listening
        if lsof -i :3300 -a -p $pid >/dev/null 2>&1; then
            echo -e "${GREEN}Backend server is listening on port 3300${NC}"
            return 0
        fi
        
        attempts=$((attempts + 1))
    done
    
    # Process is running but port not bound yet - might be slow startup
    if kill -0 $pid 2>/dev/null; then
        echo -e "${YELLOW}Backend process running but port not yet bound - may still be initializing${NC}"
        return 0
    fi
    
    return 1
}

backend_stop() {
    echo -e "${BLUE}Stopping AdapterOS Backend Server...${NC}"

    if ! is_running "backend" "$BACKEND_PID_FILE"; then
        echo -e "${YELLOW}Backend server is not running${NC}"
        return 0
    fi

    local pid=$(get_pid "$BACKEND_PID_FILE")

    # Graceful shutdown first
    kill -TERM $pid 2>/dev/null || true
    sleep 2

    # Force kill if still running
    if kill -0 $pid 2>/dev/null; then
        echo -e "${YELLOW}Force stopping backend server...${NC}"
        kill -KILL $pid 2>/dev/null || true
        sleep 1
    fi

    rm -f "$BACKEND_PID_FILE"
    update_status "backend" "stopped"

    echo -e "${GREEN}Backend server stopped${NC}"
}

# UI management
ui_start() {
    echo -e "${BLUE}Starting Web UI...${NC}"

    if is_running "ui" "$UI_PID_FILE"; then
        echo -e "${YELLOW}UI is already running (PID: $(get_pid "$UI_PID_FILE"))${NC}"
        return 0
    fi

    cd "$PROJECT_ROOT/ui"

    # Check if pnpm is available
    if ! command -v pnpm >/dev/null 2>&1; then
        echo -e "${RED}pnpm not found. Please install pnpm or npm.${NC}"
        return 1
    fi

    # Start UI dev server
    pnpm dev --host 0.0.0.0 --port 3200 > "$PROJECT_ROOT/ui-dev.log" 2>&1 &
    local pid=$!

    echo $pid > "$UI_PID_FILE"
    update_status "ui" "running" "$pid"

    echo -e "${GREEN}UI started (PID: $pid, Port: 3200)${NC}"

    # Wait and verify
    sleep 5
    if ! kill -0 $pid 2>/dev/null; then
        echo -e "${RED}UI failed to start. Check ui-dev.log${NC}"
        rm -f "$UI_PID_FILE"
        update_status "ui" "failed"
        return 1
    fi

    return 0
}

ui_stop() {
    echo -e "${BLUE}Stopping Web UI...${NC}"

    if ! is_running "ui" "$UI_PID_FILE"; then
        echo -e "${YELLOW}UI is not running${NC}"
        return 0
    fi

    local pid=$(get_pid "$UI_PID_FILE")

    kill -TERM $pid 2>/dev/null || true
    sleep 2

    if kill -0 $pid 2>/dev/null; then
        kill -KILL $pid 2>/dev/null || true
    fi

    rm -f "$UI_PID_FILE"
    update_status "ui" "stopped"

    echo -e "${GREEN}UI stopped${NC}"
}

# Menu bar app management (macOS only)
menu_bar_start() {
    if [[ "$OSTYPE" != "darwin"* ]]; then
        echo -e "${RED}Menu bar app is only available on macOS${NC}"
        return 1
    fi

    echo -e "${BLUE}Starting Menu Bar App...${NC}"

    if is_running "menu-bar" "$MENU_BAR_PID_FILE"; then
        echo -e "${YELLOW}Menu bar app is already running (PID: $(get_pid "$MENU_BAR_PID_FILE"))${NC}"
        return 0
    fi

    cd "$PROJECT_ROOT/menu-bar-app"

    # Build if needed
    if [ ! -f ".build/release/AdapterOSMenu" ]; then
        echo "Building menu bar app..."
        swift build -c release
    fi

    # Start menu bar app
    .build/release/AdapterOSMenu > /dev/null 2>&1 &
    local pid=$!

    echo $pid > "$MENU_BAR_PID_FILE"
    update_status "menu-bar" "running" "$pid"

    echo -e "${GREEN}Menu bar app started (PID: $pid)${NC}"
}

menu_bar_stop() {
    echo -e "${BLUE}Stopping Menu Bar App...${NC}"

    if ! is_running "menu-bar" "$MENU_BAR_PID_FILE"; then
        echo -e "${YELLOW}Menu bar app is not running${NC}"
        return 0
    fi

    local pid=$(get_pid "$MENU_BAR_PID_FILE")

    kill -TERM $pid 2>/dev/null || true
    sleep 1

    if kill -0 $pid 2>/dev/null; then
        kill -KILL $pid 2>/dev/null || true
    fi

    rm -f "$MENU_BAR_PID_FILE"
    update_status "menu-bar" "stopped"

    echo -e "${GREEN}Menu bar app stopped${NC}"
}

# Status display
status() {
    echo -e "${BLUE}AdapterOS Services Status${NC}"
    echo "=========================="

    # Backend status
    if is_running "backend" "$BACKEND_PID_FILE"; then
        echo -e "${GREEN}✅ Backend Server${NC} (PID: $(get_pid "$BACKEND_PID_FILE"), Port: 3300)"
    else
        echo -e "${RED}❌ Backend Server${NC} (stopped)"
    fi

    # UI status
    if is_running "ui" "$UI_PID_FILE"; then
        echo -e "${GREEN}✅ Web UI${NC} (PID: $(get_pid "$UI_PID_FILE"), Port: 3200)"
    else
        echo -e "${RED}❌ Web UI${NC} (stopped)"
    fi

    # Menu bar status (macOS only)
    if [[ "$OSTYPE" == "darwin"* ]]; then
        if is_running "menu-bar" "$MENU_BAR_PID_FILE"; then
            echo -e "${GREEN}✅ Menu Bar App${NC} (PID: $(get_pid "$MENU_BAR_PID_FILE"))"
        else
            echo -e "${RED}❌ Menu Bar App${NC} (stopped)"
        fi
    fi

    # Database status
    if [ -f "$PROJECT_ROOT/var/aos-cp.sqlite3" ]; then
        echo -e "${GREEN}✅ Database${NC} ($PROJECT_ROOT/var/aos-cp.sqlite3)"
    else
        echo -e "${RED}❌ Database${NC} (not initialized)"
    fi

    echo ""
    echo "Access URLs:"
    echo "  Backend API: http://localhost:3300"
    echo "  Web UI:      http://localhost:3200"
    echo "  Health Check: curl http://localhost:3300/healthz"
}

# Start all services
start_all() {
    echo -e "${BLUE}Starting all AdapterOS services...${NC}"

    backend_start
    sleep 2  # Let backend initialize
    ui_start

    if [[ "$OSTYPE" == "darwin"* ]]; then
        sleep 1
        menu_bar_start
    fi

    echo -e "${GREEN}All services started. Run '$0 status' to check status.${NC}"
}

# Stop all services
stop_all() {
    echo -e "${BLUE}Stopping all AdapterOS services...${NC}"

    ui_stop
    menu_bar_stop
    backend_stop

    echo -e "${GREEN}All services stopped.${NC}"
}

# Help
usage() {
    echo "AdapterOS Service Manager"
    echo ""
    echo "USAGE: $0 <command> [service]"
    echo ""
    echo "COMMANDS:"
    echo "  start [all|backend|ui|menu-bar]    Start services"
    echo "  stop [all|backend|ui|menu-bar]     Stop services"
    echo "  restart [all|backend|ui|menu-bar]  Restart services"
    echo "  status                               Show service status"
    echo "  logs [backend|ui]                   Show service logs"
    echo ""
    echo "SERVICES:"
    echo "  backend    AdapterOS API server (Port 3300)"
    echo "  ui         Web interface (Port 3200)"
    echo "  menu-bar   macOS menu bar app (macOS only)"
    echo ""
    echo "EXAMPLES:"
    echo "  $0 start all           # Start everything"
    echo "  $0 start backend       # Start only backend"
    echo "  $0 stop ui             # Stop only UI"
    echo "  $0 restart backend     # Restart backend"
    echo "  $0 status              # Show status"
    echo "  $0 logs backend        # Show backend logs"
}

# Logs
logs() {
    local service="$1"

    case "$service" in
        backend)
            if [ -f "$PROJECT_ROOT/server.log" ]; then
                tail -50 "$PROJECT_ROOT/server.log"
            else
                echo "No backend logs found"
            fi
            ;;
        ui)
            if [ -f "$PROJECT_ROOT/ui-dev.log" ]; then
                tail -50 "$PROJECT_ROOT/ui-dev.log"
            else
                echo "No UI logs found"
            fi
            ;;
        *)
            echo "Usage: $0 logs [backend|ui]"
            ;;
    esac
}

# Restart service
restart() {
    local service="$1"

    case "$service" in
        backend)
            backend_stop
            sleep 2
            backend_start
            ;;
        ui)
            ui_stop
            sleep 2
            ui_start
            ;;
        menu-bar)
            menu_bar_stop
            sleep 2
            menu_bar_start
            ;;
        all)
            stop_all
            sleep 3
            start_all
            ;;
        *)
            echo "Unknown service: $service"
            echo "Available: backend, ui, menu-bar, all"
            exit 1
            ;;
    esac
}

# Main command processing
case "${1:-}" in
    start)
        case "${2:-all}" in
            all) start_all ;;
            backend) backend_start ;;
            ui) ui_start ;;
            menu-bar) menu_bar_start ;;
            *) echo "Unknown service: $2"; usage; exit 1 ;;
        esac
        ;;
    stop)
        case "${2:-all}" in
            all) stop_all ;;
            backend) backend_stop ;;
            ui) ui_stop ;;
            menu-bar) menu_bar_stop ;;
            *) echo "Unknown service: $2"; usage; exit 1 ;;
        esac
        ;;
    restart)
        restart "${2:-all}"
        ;;
    status)
        status
        ;;
    logs)
        logs "${2:-}"
        ;;
    *)
        usage
        exit 1
        ;;
esac
