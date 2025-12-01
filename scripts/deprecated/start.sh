#!/usr/bin/env bash
#
# start.sh - Simple startup script for AdapterOS
#
# Starts the backend API and web UI together.
# A simpler alternative to the full run_complete_system.sh script.
#
# Copyright (c) 2025 JKCA / James KC Auchterlonie. All rights reserved.
#
# Usage:
#   ./scripts/start.sh [OPTIONS]
#
# Options:
#   --backend-only    Start only the backend server (no UI)
#   --help            Show this help message
#

set -euo pipefail

# =============================================================================
# Colors
# =============================================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

info() { echo -e "${BLUE}[INFO]${NC} $1"; }
success() { echo -e "${GREEN}[OK]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }

# =============================================================================
# Configuration
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PORT_GUARD_SCRIPT="$PROJECT_ROOT/scripts/port-guard.sh"
if [ -f "$PORT_GUARD_SCRIPT" ]; then
    # shellcheck disable=SC1090
    source "$PORT_GUARD_SCRIPT"
else
    warn "Port guard script missing at $PORT_GUARD_SCRIPT; port cleanup will be manual."
    ensure_port_free() { return 0; }
fi

MODEL_DIR="$PROJECT_ROOT/models"
DB_FILE="$PROJECT_ROOT/var/aos-cp.sqlite3"
UI_DIR="$PROJECT_ROOT/ui"

API_PORT="${AOS_SERVER_PORT:-8080}"
UI_PORT="${AOS_UI_PORT:-3200}"

# Process tracking
SERVER_PID=""
UI_PID=""

# Flags
START_UI=true

# =============================================================================
# Cleanup Handler
# =============================================================================

cleanup() {
    echo ""
    info "Shutting down..."

    if [ -n "$UI_PID" ] && kill -0 "$UI_PID" 2>/dev/null; then
        kill "$UI_PID" 2>/dev/null || true
        wait "$UI_PID" 2>/dev/null || true
    fi

    if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi

    success "AdapterOS stopped"
}

trap cleanup EXIT INT TERM

# =============================================================================
# Help
# =============================================================================

show_help() {
    cat << 'EOF'
start.sh - Simple startup script for AdapterOS

Usage:
  ./scripts/start.sh [OPTIONS]

Options:
  --backend-only    Start only the backend server (no UI)
  --help            Show this help message

This script will:
  1. Check if a model is available
  2. Run database migrations if needed
  3. Start the backend API server
  4. Start the web UI development server

For a more comprehensive startup with system checks, use:
  ./scripts/run_complete_system.sh
EOF
    exit 0
}

# =============================================================================
# Parse Arguments
# =============================================================================

while [[ $# -gt 0 ]]; do
    case $1 in
        --backend-only)
            START_UI=false
            shift
            ;;
        --help|-h)
            show_help
            ;;
        *)
            error "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# =============================================================================
# Main
# =============================================================================

echo ""
echo -e "${BOLD}AdapterOS Startup${NC}"
echo "=================="
echo ""

cd "$PROJECT_ROOT"

# -----------------------------------------------------------------------------
# Step 1: Check for models
# -----------------------------------------------------------------------------

info "Checking for models..."

if [ ! -d "$MODEL_DIR" ] || [ -z "$(ls -A "$MODEL_DIR" 2>/dev/null)" ]; then
    warn "No models found in $MODEL_DIR"
    echo ""
    echo "  To download a model, run:"
    echo ""
    echo "    ./scripts/download_model.sh"
    echo ""
    echo "  This will download the Qwen 2.5 7B model (recommended)."
    echo "  For a smaller model, use:"
    echo ""
    echo "    ./scripts/download_model.sh --size 3b"
    echo ""
    echo "  Continuing without a model (inference will not work)..."
    echo ""
else
    # Find the first model directory
    MODEL_PATH=$(find "$MODEL_DIR" -maxdepth 1 -type d ! -name "models" | head -1)
    if [ -n "$MODEL_PATH" ]; then
        MODEL_NAME=$(basename "$MODEL_PATH")
        success "Found model: $MODEL_NAME"
        export AOS_MLX_FFI_MODEL="$MODEL_PATH"
    fi
fi

# -----------------------------------------------------------------------------
# Step 2: Database migrations
# -----------------------------------------------------------------------------

info "Checking database..."

mkdir -p "$PROJECT_ROOT/var"

if [ ! -f "$DB_FILE" ]; then
    info "Database not found, running migrations..."

    # Try aosctl first, then orchestrator, then create empty db
    if command -v aosctl &> /dev/null; then
        aosctl db migrate 2>/dev/null || true
    elif [ -f "$PROJECT_ROOT/target/release/aosctl" ]; then
        "$PROJECT_ROOT/target/release/aosctl" db migrate 2>/dev/null || true
    elif [ -f "$PROJECT_ROOT/target/release/adapteros-orchestrator" ]; then
        "$PROJECT_ROOT/target/release/adapteros-orchestrator" db migrate 2>/dev/null || true
    else
        # Build and run migrations
        info "Building migration tool..."
        cargo build --release -p adapteros-cli 2>/dev/null || {
            warn "Could not build migration tool"
        }
        if [ -f "$PROJECT_ROOT/target/release/aosctl" ]; then
            "$PROJECT_ROOT/target/release/aosctl" db migrate 2>/dev/null || true
        else
            touch "$DB_FILE"
            warn "Created empty database (run 'aosctl db migrate' to initialize schema)"
        fi
    fi

    if [ -f "$DB_FILE" ]; then
        success "Database ready"
    fi
else
    success "Database exists"
fi

export DATABASE_URL="sqlite://$DB_FILE"

# -----------------------------------------------------------------------------
# Step 3: Build backend if needed
# -----------------------------------------------------------------------------

info "Checking backend..."

SERVER_BIN="$PROJECT_ROOT/target/release/adapteros-server"
if [ ! -f "$SERVER_BIN" ]; then
    info "Building backend (this may take a few minutes)..."
    cargo build --release -p adapteros-server 2>&1 | tail -5 || {
        error "Backend build failed"
        echo "Try building manually with: cargo build --release -p adapteros-server"
        exit 1
    }
fi

if [ -f "$SERVER_BIN" ]; then
    success "Backend ready"
else
    error "Backend binary not found: $SERVER_BIN"
    exit 1
fi

# -----------------------------------------------------------------------------
# Step 4: Check UI dependencies
# -----------------------------------------------------------------------------

if [ "$START_UI" = true ]; then
    info "Checking UI dependencies..."

    if ! command -v pnpm &> /dev/null; then
        error "pnpm not found"
        echo "  Install with: npm install -g pnpm"
        exit 1
    fi

    if [ ! -d "$UI_DIR/node_modules" ]; then
        info "Installing UI dependencies..."
        cd "$UI_DIR"
        pnpm install --silent
        cd "$PROJECT_ROOT"
    fi

    success "UI dependencies ready"
fi

# -----------------------------------------------------------------------------
# Step 5: Start services
# -----------------------------------------------------------------------------

echo ""
info "Starting services..."
echo ""

# Ensure ports are free
if ! ensure_port_free "$API_PORT" "Backend API"; then
    error "Backend port $API_PORT is occupied; aborting."
    exit 1
fi
if [ "$START_UI" = true ]; then
    if ! ensure_port_free "$UI_PORT" "Web UI"; then
        error "UI port $UI_PORT is occupied; aborting."
        exit 1
    fi
fi

# Start backend
export RUST_LOG="${RUST_LOG:-info}"
"$SERVER_BIN" --config configs/cp.toml > /tmp/aos-server.log 2>&1 &
SERVER_PID=$!
info "Backend starting (PID: $SERVER_PID)..."

# Wait for backend
MAX_WAIT=20
WAITED=0
while [ $WAITED -lt $MAX_WAIT ]; do
    if curl -s "http://localhost:$API_PORT/healthz" > /dev/null 2>&1; then
        break
    fi
    sleep 1
    ((WAITED++))
done

if [ $WAITED -ge $MAX_WAIT ]; then
    error "Backend failed to start"
    echo "  Check logs: /tmp/aos-server.log"
    exit 1
fi
success "Backend running at http://localhost:$API_PORT"

# Start UI
if [ "$START_UI" = true ]; then
    cd "$UI_DIR"
    AOS_UI_PORT="$UI_PORT" pnpm dev -- --port "$UI_PORT" > /tmp/aos-ui.log 2>&1 &
    UI_PID=$!
    cd "$PROJECT_ROOT"

    sleep 3
    if kill -0 "$UI_PID" 2>/dev/null; then
        success "UI running at http://localhost:$UI_PORT"
    else
        warn "UI may have failed to start. Check: /tmp/aos-ui.log"
    fi
fi

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------

echo ""
echo -e "${BOLD}${GREEN}AdapterOS is running${NC}"
echo ""
echo "  Backend:   http://localhost:$API_PORT"
echo "  Health:    http://localhost:$API_PORT/healthz"
echo "  Swagger:   http://localhost:$API_PORT/swagger-ui"
if [ "$START_UI" = true ]; then
    echo "  UI:        http://localhost:$UI_PORT"
fi
echo ""
echo "  Logs:"
echo "    Backend: /tmp/aos-server.log"
if [ "$START_UI" = true ]; then
    echo "    UI:      /tmp/aos-ui.log"
fi
echo ""
echo -e "${YELLOW}Press Ctrl+C to stop${NC}"
echo ""

# Wait for processes
wait
