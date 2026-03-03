#!/usr/bin/env bash
# =============================================================================
# AdapterOS Reference Boot Script
# =============================================================================
#
# One-command reference orchestration that wraps the existing ./start script.
# Designed for macOS (Apple Silicon) first. Idempotent - safe to re-run.
#
# Usage:
#   ./scripts/reference.sh              Start reference run with default settings
#   ./scripts/reference.sh --no-seed    Skip adapter seeding
#   ./scripts/reference.sh --no-wait    Start and exit (don't wait for Ctrl+C)
#   ./scripts/reference.sh --verbose    Show detailed output
#   ./scripts/reference.sh --help       Show this help
#
# Environment:
#   AOS_SERVER_PORT     API port (default: 18080)
#   AOS_REFERENCE_TIMEOUT    Max wait for /readyz (default: 90s)
#   AOS_HEALTH_TIMEOUT  Max wait for /healthz (default: 30s)
#
# This script:
#   1. Sources configs/reference.env (if exists)
#   2. Runs ./start (which handles backend + worker + UI)
#   3. Waits for /healthz then /readyz endpoints with timeouts
#   4. Seeds sample adapters if not present (via aosctl)
#   5. Prints URLs, access info, and reference credentials
#   6. Handles clean shutdown on SIGINT/SIGTERM
#
set -Eeuo pipefail

# =============================================================================
# Configuration
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

# Defaults (can be overridden by reference.env or environment)
: "${AOS_SERVER_PORT:=18080}"
: "${AOS_REFERENCE_TIMEOUT:=90}"
: "${AOS_HEALTH_TIMEOUT:=30}"
: "${AOS_VAR_DIR:=var}"

# Script options
SEED_ADAPTERS=1
VERBOSE=0
SHOW_HELP=0
NO_WAIT=0

# =============================================================================
# Output helpers
# =============================================================================

log()  { printf "[reference] %s\n" "$*"; }
ok()   { printf "[reference] OK: %s\n" "$*"; }
warn() { printf "[reference] WARN: %s\n" "$*" >&2; }
err()  { printf "[reference] ERROR: %s\n" "$*" >&2; }
die()  { err "$*"; exit 1; }

# =============================================================================
# Parse arguments
# =============================================================================

while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-seed|--skip-seed)
            SEED_ADAPTERS=0
            ;;
        --no-wait)
            NO_WAIT=1
            ;;
        --verbose|-v)
            VERBOSE=1
            ;;
        --help|-h)
            SHOW_HELP=1
            ;;
        *)
            warn "Unknown option: $1"
            ;;
    esac
    shift
done

if [[ $SHOW_HELP -eq 1 ]]; then
    cat << 'EOF'
AdapterOS Reference Boot Script

Usage:
  ./scripts/reference.sh [options]

Options:
  --no-seed, --skip-seed   Skip adapter seeding step
  --no-wait                Start and exit (don't wait for Ctrl+C)
  --verbose, -v            Show detailed output
  --help, -h               Show this help

Environment:
  AOS_SERVER_PORT     API port (default: 18080)
  AOS_REFERENCE_TIMEOUT    Max wait for /readyz (default: 90s)
  AOS_HEALTH_TIMEOUT  Max wait for /healthz (default: 30s)
  AOS_DEV_NO_AUTH     Set to 1 to disable auth (for dev testing)

Examples:
  ./scripts/reference.sh                    # Start reference run (foreground)
  ./scripts/reference.sh --no-wait          # Start reference run and exit
  AOS_DEV_NO_AUTH=1 ./scripts/reference.sh  # Start without auth
  ./scripts/reference.sh --verbose          # Start with verbose output

Smoke test after starting:
  ./scripts/foundation-smoke.sh --no-start --server-url "http://localhost:${AOS_SERVER_PORT}"  # Verify endpoints
EOF
    exit 0
fi

# =============================================================================
# Source reference environment
# =============================================================================

if [[ -f "$ROOT_DIR/configs/reference.env" ]]; then
    log "Loading configs/reference.env..."
    # Source as key=value pairs (compatible with bash)
    set -a
    # shellcheck source=/dev/null
    source "$ROOT_DIR/configs/reference.env" 2>/dev/null || true
    set +a
    [[ $VERBOSE -eq 1 ]] && log "Loaded: AOS_SERVER_PORT=$AOS_SERVER_PORT"
fi

# Default to reference config if not explicitly set
if [[ -z "${AOS_CONFIG_PATH:-}" && -f "$ROOT_DIR/configs/reference.toml" ]]; then
    export AOS_CONFIG_PATH="$ROOT_DIR/configs/reference.toml"
fi

# Re-apply defaults after sourcing (in case reference.env didn't set them)
: "${AOS_SERVER_PORT:=18080}"
: "${AOS_REFERENCE_TIMEOUT:=90}"

# =============================================================================
# Cleanup handler
# =============================================================================

cleanup() {
    local exit_code=$?

    log "Received shutdown signal, cleaning up..."

    # Stop services via ./start down
    if [[ -x "$ROOT_DIR/start" ]]; then
        "$ROOT_DIR/start" down 2>/dev/null || true
    fi

    log "Cleanup complete."
    exit "$exit_code"
}

trap cleanup SIGINT SIGTERM

# =============================================================================
# Helper: wait for endpoint with timeout
# =============================================================================

wait_for_endpoint() {
    local endpoint="$1"
    local timeout="$2"
    local url="http://localhost:${AOS_SERVER_PORT}${endpoint}"
    local start=$SECONDS

    log "Waiting for ${endpoint} (timeout: ${timeout}s)..."

    while true; do
        local elapsed=$((SECONDS - start))

        if [[ $elapsed -ge $timeout ]]; then
            err "Timeout waiting for ${endpoint} after ${timeout}s"
            return 1
        fi

        local status
        status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 2 "$url" 2>/dev/null || echo "000")

        if [[ "$status" == "200" ]]; then
            ok "${endpoint} returned 200 (${elapsed}s)"
            return 0
        fi

        [[ $VERBOSE -eq 1 ]] && log "Waiting for ${endpoint}... (status=$status, elapsed=${elapsed}s)"
        sleep 2
    done
}

wait_for_health() {
    wait_for_endpoint "/healthz" "$AOS_HEALTH_TIMEOUT"
}

wait_for_ready() {
    wait_for_endpoint "/readyz" "$AOS_REFERENCE_TIMEOUT"
}

# =============================================================================
# Helper: find aosctl binary
# =============================================================================

find_aosctl() {
    if [[ -x "$ROOT_DIR/aosctl" ]]; then
        echo "$ROOT_DIR/aosctl"
    elif [[ -x "$ROOT_DIR/target/release/aosctl" ]]; then
        echo "$ROOT_DIR/target/release/aosctl"
    elif [[ -x "$ROOT_DIR/target/debug/aosctl" ]]; then
        echo "$ROOT_DIR/target/debug/aosctl"
    elif command -v aosctl >/dev/null 2>&1; then
        command -v aosctl
    else
        echo ""
    fi
}

# =============================================================================
# Helper: seed adapters if needed
# =============================================================================

seed_adapters_if_needed() {
    local aosctl
    aosctl="$(find_aosctl)"

    if [[ -z "$aosctl" ]]; then
        warn "aosctl not found, skipping adapter seeding"
        return 0
    fi

    log "Checking adapters..."

    # Check if any adapters exist
    local adapter_count
    adapter_count=$("$aosctl" adapter list --json 2>/dev/null | grep -c '"id"' || echo "0")

    if [[ "$adapter_count" -gt 0 ]]; then
        ok "Found $adapter_count adapter(s), skipping seed"
        return 0
    fi

    # If AOS_MODEL_PATH is set, try to seed models
    if [[ -n "${AOS_MODEL_PATH:-}" ]]; then
        log "Seeding models from $AOS_MODEL_PATH..."
        if "$aosctl" models seed --model-path "$AOS_MODEL_PATH" 2>/dev/null; then
            ok "Models seeded"
        else
            warn "Model seeding failed (non-fatal)"
        fi
    fi

    ok "Adapter setup complete"
}

# =============================================================================
# Main
# =============================================================================

main() {
    echo ""
    echo "=============================================="
    echo "           AdapterOS Reference Boot"
    echo "=============================================="
    echo ""

    # Check for ./start script
    if [[ ! -x "$ROOT_DIR/start" ]]; then
        die "Missing ./start script in $ROOT_DIR"
    fi

    # Check for curl
    if ! command -v curl >/dev/null 2>&1; then
        die "curl is required but not found"
    fi

    # Check if already running (idempotent - safe to re-run)
    local healthz_status
    healthz_status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 2 "http://localhost:${AOS_SERVER_PORT}/healthz" 2>/dev/null || echo "000")

    if [[ "$healthz_status" == "200" ]]; then
        log "Server already running on port $AOS_SERVER_PORT (idempotent - continuing)"
        log "To restart fresh: ./start down && ./scripts/reference.sh"

        # Still do readyz check to ensure full readiness
        wait_for_ready || true

    else
        # Start via ./start
        log "Starting AdapterOS via ./start..."

        # Run ./start - it handles backend + worker + UI
        if ! "$ROOT_DIR/start" up; then
            die "Failed to start AdapterOS"
        fi

        # Wait for health first (backend up)
        if ! wait_for_health; then
            err "Backend did not become healthy"
            log "Check logs: $AOS_VAR_DIR/logs/backend.log"
            exit 1
        fi

        # Wait for full readiness (worker ready)
        if ! wait_for_ready; then
            err "Server did not become fully ready"
            log "Check logs: $AOS_VAR_DIR/logs/backend.log"
            log "Check logs: $AOS_VAR_DIR/logs/worker.log"
            exit 1
        fi
    fi

    # Seed adapters if requested
    if [[ $SEED_ADAPTERS -eq 1 ]]; then
        seed_adapters_if_needed
    fi

    # Print access info
    echo ""
    echo "=============================================="
    echo "           AdapterOS is Ready!"
    echo "=============================================="
    echo ""
    echo "  Endpoints:"
    echo "    API:      http://localhost:${AOS_SERVER_PORT}"
    echo "    UI:       http://localhost:${AOS_SERVER_PORT}"
    echo "    Health:   http://localhost:${AOS_SERVER_PORT}/healthz"
    echo "    Ready:    http://localhost:${AOS_SERVER_PORT}/readyz"
    echo "    Swagger:  http://localhost:${AOS_SERVER_PORT}/swagger-ui"
    echo ""

    if [[ "${AOS_DEV_NO_AUTH:-0}" == "1" ]]; then
        echo "  Authentication:"
        echo "    Status:   DISABLED (AOS_DEV_NO_AUTH=1)"
        echo ""
    else
        echo "  Authentication:"
        echo "    Status:   Enabled"
        echo "    Default:  admin / admin (for local dev only)"
        echo "    Bypass:   Set AOS_DEV_NO_AUTH=1 for no-auth mode"
        echo ""
    fi

    echo "  Logs:"
    echo "    Backend:  $AOS_VAR_DIR/logs/backend.log"
    echo "    Worker:   $AOS_VAR_DIR/logs/worker.log"
    echo ""

    echo "Quick commands:"
    echo "  ./aosctl status           # System status"
    echo "  ./aosctl adapter list     # List adapters"
    echo "  ./aosctl chat             # Interactive chat"
    echo "  ./start down              # Stop services"
    echo ""

    echo "Smoke test:"
    echo "  ./scripts/foundation-smoke.sh --no-start --server-url http://localhost:${AOS_SERVER_PORT}   # Verify endpoints"
    echo ""

    ok "Reference boot complete!"
    echo ""

    # Exit early if --no-wait was specified
    if [[ $NO_WAIT -eq 1 ]]; then
        log "Started successfully (--no-wait specified, exiting)"
        exit 0
    fi

    # Keep running (for signal handling)
    log "Press Ctrl+C to stop..."

    # Wait indefinitely (will be interrupted by trap)
    while true; do
        sleep 60
    done
}

main "$@"
