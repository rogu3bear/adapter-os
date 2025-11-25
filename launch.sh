#!/bin/bash
# AdapterOS Launch Panel - Single Command to Start Everything
# This is your pre-service launch panel for the entire system

set -e  # Exit on any error
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors for beautiful output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
WHITE='\033[1;37m'
NC='\033[0m' # No Color

# Shared helpers
PORT_GUARD_SCRIPT="$SCRIPT_DIR/scripts/port-guard.sh"
if [ -f "$PORT_GUARD_SCRIPT" ]; then
    # shellcheck disable=SC1090
    source "$PORT_GUARD_SCRIPT"
else
    echo "[WARN] Port guard script missing at $PORT_GUARD_SCRIPT; port cleanup will be manual."
    ensure_port_free() { return 0; }
fi

# Configuration
PROJECT_NAME="AdapterOS"
LAUNCH_BANNER="${PURPLE}
╔══════════════════════════════════════════════════════════════╗
║                     🚀 ADAPTEROS LAUNCH PANEL                     ║
║                 Single Command System Startup                      ║
╚══════════════════════════════════════════════════════════════╝${NC}
"
SERVICE_MANAGER="$SCRIPT_DIR/scripts/service-manager.sh"
GRACEFUL_SHUTDOWN="$SCRIPT_DIR/scripts/graceful-shutdown.sh"

# Function to print status messages
status_msg() {
    echo -e "${BLUE}ℹ️  ${1}${NC}"
}

success_msg() {
    echo -e "${GREEN}✅ ${1}${NC}"
}

error_msg() {
    echo -e "${RED}❌ ${1}${NC}"
}

warning_msg() {
    echo -e "${YELLOW}⚠️  ${1}${NC}"
}

# Function to wait for service to be ready
wait_for_service() {
    local url=$1
    local service_name=$2
    local max_attempts=30
    local attempt=1

    status_msg "Waiting for $service_name to be ready..."

    while [ $attempt -le $max_attempts ]; do
        if curl -s "$url" >/dev/null 2>&1; then
            success_msg "$service_name is ready!"
            return 0
        fi

        echo -n "."
        sleep 1
        attempt=$((attempt + 1))
    done

    error_msg "$service_name failed to start within ${max_attempts}s"
    return 1
}

# Function to show access information
show_access_info() {
    echo -e "${CYAN}
╔══════════════════════════════════════════════════════════════╗
║                           ACCESS URLS                        ║
╠══════════════════════════════════════════════════════════════╣${NC}"
    echo -e "${WHITE}  Backend API:     ${GREEN}http://localhost:8080${WHITE}"
    echo -e "${WHITE}  Web Dashboard:   ${GREEN}http://localhost:3200${WHITE}"
    echo -e "${WHITE}  Health Check:    ${GREEN}curl http://localhost:8080/healthz${WHITE}"
    echo -e "${WHITE}  API Docs:        ${GREEN}http://localhost:8080/docs${WHITE} (if enabled)"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo -e "${YELLOW}💡 Pro Tips:${NC}"
    echo -e "  • Use ${CYAN}./aos status${NC} to check service status"
    echo -e "  • Use ${CYAN}./aos stop all${NC} to stop everything"
    echo -e "  • Use ${CYAN}./aos logs backend${NC} to see backend logs"
    echo ""
}

# Main launch function
launch_system() {
    echo -e "$LAUNCH_BANNER"

    echo -e "${WHITE}Starting ${PROJECT_NAME} System...${NC}"
    echo ""

    # Pre-flight checks
    status_msg "Running pre-flight checks..."

    # Check if we're in the right directory
    if [ ! -f "configs/cp.toml" ]; then
        error_msg "Not in AdapterOS project directory. Please run from the project root."
        exit 1
    fi

    # Check if build exists
    if [ ! -f "target/debug/adapteros-server" ]; then
        warning_msg "Backend binary not found. Building..."
        status_msg "This may take a few minutes on first build..."
        if ! cargo build 2>&1 | grep -E "(Compiling|Finished|error|warning)" | tail -20; then
            error_msg "Failed to build backend. Check build output above."
            exit 1
        fi
        if [ ! -f "target/debug/adapteros-server" ]; then
            error_msg "Build completed but binary not found. Run 'cargo build' manually."
            exit 1
        fi
        success_msg "Backend built successfully"
    fi

    # Check database exists or can be initialized
    if [ ! -f "var/aos-cp.sqlite3" ]; then
        warning_msg "Database not found. Will be created on first run."
        mkdir -p var
    fi

    # Check required dependencies
    if ! command -v pnpm >/dev/null 2>&1 && ! command -v npm >/dev/null 2>&1; then
        warning_msg "pnpm/npm not found. UI will not be able to start."
    fi

    # Check and free ports
    if ! ensure_port_free 8080 "Backend API"; then
        error_msg "Cannot free port 8080. Please stop conflicting processes manually."
        exit 1
    fi

    if ! ensure_port_free 3200 "Web UI"; then
        warning_msg "Cannot free port 3200. UI may not start properly."
    fi

    success_msg "Pre-flight checks complete"
    echo ""

    # Start services in order
    status_msg "Starting services..."

    # 1. Start Backend (most critical)
    status_msg "Starting Backend Server on port 8080..."
    if "$SERVICE_MANAGER" start backend; then
        success_msg "Backend Server process started"

        # Wait for backend to be ready - verify HTTP response
        if wait_for_service "http://localhost:8080/v1/meta" "Backend API"; then
            success_msg "Backend is responding on port 8080"
        elif wait_for_service "http://localhost:8080/healthz" "Backend Health"; then
            success_msg "Backend health endpoint responding"
        else
            # Process check as last resort
            if pgrep -f "adapteros-server" >/dev/null; then
                warning_msg "Backend process is running but not responding to HTTP requests"
                warning_msg "Server may still be initializing. Check server.log for errors:"
                warning_msg "  tail -20 server.log"
                # Don't exit - let user decide, but warn them
            else
                error_msg "Backend process died. Check server.log:"
                error_msg "  tail -30 server.log"
                exit 1
            fi
        fi
    else
        error_msg "Failed to start Backend Server process"
        exit 1
    fi

    echo ""

    # 2. Start Web UI
    status_msg "Starting Web Dashboard on port 3200..."
    if "$SERVICE_MANAGER" start ui; then
        success_msg "Web Dashboard started"

        # Wait a bit for UI to initialize
        sleep 3

        if curl -s "http://localhost:3200" | grep -q "AdapterOS"; then
            success_msg "Web Dashboard is responding"
        else
            warning_msg "Web Dashboard started but may still be initializing..."
        fi
    else
        warning_msg "Failed to start Web Dashboard. Backend will still work."
    fi

    echo ""

    # 3. Start Menu Bar App (macOS only)
    if [[ "$OSTYPE" == "darwin"* ]]; then
        status_msg "Starting Menu Bar Status App..."
        if "$SERVICE_MANAGER" start menu-bar; then
            success_msg "Menu Bar App started"
        else
            warning_msg "Menu Bar App failed to start (optional)"
        fi
        echo ""
    fi

    # Final status check
    echo ""
    status_msg "System launch complete!"
    "$SERVICE_MANAGER" status

    echo ""
    show_access_info

    echo -e "${GREEN}🎉 ${PROJECT_NAME} is now running!${NC}"
    echo -e "${CYAN}Press Ctrl+C to stop all services${NC}"

    # Wait for user interrupt to stop everything gracefully
    cleanup_and_exit() {
        echo -e "\n${YELLOW}Shutting down all services gracefully...${NC}"
        if [ -f "$GRACEFUL_SHUTDOWN" ]; then
            "$GRACEFUL_SHUTDOWN" graceful
        else
            "$SERVICE_MANAGER" stop all graceful
        fi
        echo -e "${GREEN}All services stopped. Goodbye! 👋${NC}"
        exit 0
    }
    
    trap cleanup_and_exit INT TERM

    # Keep running and show periodic status
    while true; do
        sleep 30
        echo -e "\n${BLUE}════════════════════════════════════════════════${NC}"
        echo -e "${BLUE}System Status Check (Ctrl+C to stop all):${NC}"
        "$SERVICE_MANAGER" status | grep -E "(✅|❌)" || true
        echo -e "${BLUE}════════════════════════════════════════════════${NC}"
    done
}

# Handle command line arguments
case "${1:-}" in
    "")
        # No arguments - launch the full system
        launch_system
        ;;
    "status")
        # Show status
        "$SERVICE_MANAGER" status
        ;;
    "stop")
        # Stop all services
        local mode="${2:-graceful}"
        if [ -f "$GRACEFUL_SHUTDOWN" ]; then
            "$GRACEFUL_SHUTDOWN" "$mode"
        else
            "$SERVICE_MANAGER" stop all "$mode"
        fi
        echo -e "${GREEN}All services stopped${NC}"
        ;;
    "backend")
        # Launch only backend
        echo -e "${BLUE}Launching Backend Only...${NC}"
        
        # Check for MLX backend option
        if [ "${2:-}" = "mlx" ]; then
            if [ -z "${3:-}" ]; then
                error_msg "MLX backend requires model path"
                echo "Usage: ./launch.sh backend mlx <model-path>"
                exit 1
            fi
            
            MODEL_PATH="$3"
            if [ ! -d "$MODEL_PATH" ]; then
                error_msg "MLX model path does not exist: $MODEL_PATH"
                exit 1
            fi
            
            status_msg "Setting AOS_MLX_FFI_MODEL=$MODEL_PATH"
            export AOS_MLX_FFI_MODEL="$MODEL_PATH"
        fi
        
        "$SERVICE_MANAGER" start backend
        wait_for_service "http://localhost:8080/healthz" "Backend API"
        echo -e "${GREEN}Backend ready at http://localhost:8080${NC}"
        ;;
    "ui")
        # Launch only UI
        echo -e "${BLUE}Launching UI Only...${NC}"
        "$SERVICE_MANAGER" start backend
        wait_for_service "http://localhost:8080/healthz" "Backend API"
        "$SERVICE_MANAGER" start ui
        echo -e "${GREEN}UI ready at http://localhost:3200${NC}"
        ;;
    "help"|"-h"|"--help")
        echo "AdapterOS Launch Panel"
        echo ""
        echo "Single command to launch the entire AdapterOS system"
        echo ""
        echo "USAGE:"
        echo "  ./launch.sh                    # Launch full system (backend + UI + menu bar)"
        echo "  ./launch.sh backend            # Launch backend only (Metal backend)"
        echo "  ./launch.sh backend mlx <path>  # Launch backend with MLX backend (requires --features mlx-ffi-backend)"
        echo "  ./launch.sh ui                 # Launch backend + UI only"
        echo "  ./launch.sh status             # Show service status"
        echo "  ./launch.sh stop [mode]        # Stop all services (graceful|fast|immediate)"
        echo "  ./launch.sh help               # Show this help"
        echo ""
        echo "The launch panel will:"
        echo "  • Run pre-flight checks"
        echo "  • Start services in dependency order"
        echo "  • Wait for services to be ready"
        echo "  • Show access URLs"
        echo "  • Monitor system health"
        echo "  • Handle graceful shutdown (Ctrl+C)"
        ;;
    *)
        echo -e "${RED}Unknown command: $1${NC}"
        echo "Run './launch.sh help' for usage information"
        exit 1
        ;;
esac
