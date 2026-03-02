#!/usr/bin/env bash
# Happy Path Smoke Test
# Validates the complete getting-started workflow in <60 seconds.
#
# Usage:
#   ./scripts/test/smoke_happy_path.sh           # Normal mode
#   ./scripts/test/smoke_happy_path.sh --verbose # Show detailed output
#   ./scripts/test/smoke_happy_path.sh --help    # Show usage
#
# Exit codes:
#   0 - All checks passed
#   1 - Test failure or error

set -euo pipefail

# Configuration
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERBOSE="${VERBOSE:-false}"
SERVER_PORT="${AOS_SERVER_PORT:-18080}"
SERVER_TIMEOUT="${SERVER_TIMEOUT:-30}"
CURL_TIMEOUT="${CURL_TIMEOUT:-5}"

# Paths (use var/ not /tmp per project policy)
VAR_DIR="${ROOT}/var/smoke-happy-path"
DB_PATH="${VAR_DIR}/smoke.sqlite3"
LOG_FILE="${VAR_DIR}/server.log"
PID_FILE="${VAR_DIR}/server.pid"
FIXTURE_MODEL="${ROOT}/tests/fixtures/models/tiny-test"

# Parse arguments
for arg in "$@"; do
    case "$arg" in
        --verbose|-v) VERBOSE=true ;;
        --help|-h)
            echo "Usage: $0 [--verbose] [--help]"
            echo ""
            echo "Happy path smoke test for adapterOS getting-started workflow."
            echo "Validates: build, migrate, seed, serve, health check."
            echo ""
            echo "Options:"
            echo "  --verbose, -v  Show detailed output"
            echo "  --help, -h     Show this help message"
            echo ""
            echo "Environment variables:"
            echo "  AOS_SERVER_PORT   Server port (default: 18080)"
            echo "  SERVER_TIMEOUT    Timeout waiting for server (default: 30s)"
            echo "  VERBOSE           Enable verbose output (default: false)"
            exit 0
            ;;
    esac
done

# Logging
info() { echo "[$(date +%H:%M:%S)] $*"; }
verbose() { [[ "$VERBOSE" == true ]] && echo "[VERBOSE] $*" || true; }
fail() {
    echo ""
    echo "=========================================="
    echo "  SMOKE TEST FAILED"
    echo "=========================================="
    echo ""
    echo "[ERROR] $*" >&2
    echo ""
    if [[ -s "$LOG_FILE" ]]; then
        echo "--- Last 50 lines of server log ---"
        tail -n 50 "$LOG_FILE" 2>/dev/null || true
        echo "--- End of log ---"
    fi
    exit 1
}

# Cleanup handler
cleanup() {
    info "Cleanup..."

    # Kill server if running
    if [[ -f "$PID_FILE" ]]; then
        local pid
        pid="$(cat "$PID_FILE" 2>/dev/null || true)"
        if [[ -n "${pid:-}" ]] && kill -0 "$pid" 2>/dev/null; then
            verbose "Stopping server (pid $pid)"
            kill "$pid" 2>/dev/null || true
            sleep 1
            kill -9 "$pid" 2>/dev/null || true
        fi
        rm -f "$PID_FILE"
    fi

    # Clean up var directory (keep logs on failure for debugging)
    if [[ "${TEST_PASSED:-false}" == true ]]; then
        rm -rf "$VAR_DIR" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

# Verify prerequisites
check_prerequisites() {
    command -v cargo >/dev/null 2>&1 || fail "cargo not found - install Rust first"
    command -v curl >/dev/null 2>&1 || fail "curl not found"

    if [[ ! -f "${ROOT}/Cargo.toml" ]]; then
        fail "Must run from adapter-os repository root"
    fi

    if [[ ! -d "$FIXTURE_MODEL" ]]; then
        fail "Tiny test model fixture not found at: $FIXTURE_MODEL"
    fi
}

# Wait for server to be ready
wait_for_server() {
    local url="http://127.0.0.1:${SERVER_PORT}/healthz"
    local end=$((SECONDS + SERVER_TIMEOUT))

    verbose "Waiting for server at $url (timeout: ${SERVER_TIMEOUT}s)"

    while (( SECONDS < end )); do
        if curl -fsS --max-time "$CURL_TIMEOUT" "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done

    fail "Server failed to become ready within ${SERVER_TIMEOUT}s"
}

# Main test steps
main() {
    local start_time=$SECONDS

    info "Starting happy path smoke test..."
    echo ""

    # Setup
    mkdir -p "$VAR_DIR"
    check_prerequisites

    # Step 1: Build CLI and Server
    echo "[1/6] Building CLI and Server..."
    if [[ "$VERBOSE" == true ]]; then
        cargo build -p adapteros-cli -p adapteros-server --quiet 2>&1 \
            || fail "Build failed"
    else
        cargo build -p adapteros-cli -p adapteros-server --quiet 2>/dev/null \
            || fail "Build failed"
    fi
    verbose "CLI and server built successfully"

    # Step 2: Run migrations
    echo "[2/6] Running migrations..."
    # SQLite requires the file to exist before migrations
    touch "$DB_PATH"
    SQLX_DISABLE_STATEMENT_CHECKS=1 \
    AOS_SKIP_MIGRATION_SIGNATURES=1 \
    DATABASE_URL="sqlite://${DB_PATH}" \
        cargo sqlx migrate run --source "${ROOT}/migrations" >/dev/null 2>&1 \
        || fail "Database migrations failed"
    verbose "Migrations applied to $DB_PATH"

    # Step 3: Seed test model (via direct SQL insert)
    # Note: CLI --model-path has a clap argument conflict; using direct SQL as workaround
    echo "[3/6] Seeding test model..."
    sqlite3 "$DB_PATH" <<'SQL'
INSERT OR IGNORE INTO models (
    id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3,
    model_type, backend, model_path, status, created_at
) VALUES (
    'test-model-001',
    'tiny-test',
    'b3_placeholder_hash_0000000000000000000000000000000000000000000000',
    'b3_config_hash_0000000000000000000000000000000000000000000000000',
    'b3_tokenizer_hash_000000000000000000000000000000000000000000000',
    'b3_tokenizer_cfg_hash_0000000000000000000000000000000000000000',
    'base_model',
    'mlx',
    'tests/fixtures/models/tiny-test',
    'available',
    datetime('now')
);
SQL
    verbose "Test model seeded via SQL"

    # Verify model was seeded
    local model_count
    model_count=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM models;" 2>/dev/null || echo "0")
    if [[ "$model_count" -lt 1 ]]; then
        fail "No models found after seeding"
    fi
    verbose "Model count: $model_count"

    # Step 4: Start server
    echo "[4/6] Starting server..."
    AOS_DEV_NO_AUTH=1 \
    AOS_BACKEND=mock \
    AOS_SERVER_PORT=$SERVER_PORT \
    AOS_SKIP_MIGRATION_SIGNATURES=1 \
    SQLX_DISABLE_STATEMENT_CHECKS=1 \
    DATABASE_URL="sqlite://${DB_PATH}" \
    AOS_DATABASE_URL="sqlite://${DB_PATH}" \
    RUST_LOG="${RUST_LOG:-warn}" \
        "${ROOT}/target/debug/adapteros-server" \
            --config "${ROOT}/configs/cp.toml" \
            >"$LOG_FILE" 2>&1 &
    echo $! >"$PID_FILE"

    wait_for_server
    verbose "Server running on port $SERVER_PORT"

    # Step 5: Test endpoints
    echo "[5/6] Testing endpoints..."

    # Test /healthz
    local healthz_resp
    healthz_resp=$(curl -sS --max-time "$CURL_TIMEOUT" \
        "http://127.0.0.1:${SERVER_PORT}/healthz" 2>&1) \
        || fail "/healthz request failed"
    verbose "/healthz: $healthz_resp"

    # Test /readyz
    local readyz_resp
    readyz_resp=$(curl -sS --max-time "$CURL_TIMEOUT" \
        "http://127.0.0.1:${SERVER_PORT}/readyz" 2>&1) \
        || fail "/readyz request failed"
    verbose "/readyz: $readyz_resp"

    # Test /v1/system/status (if available)
    local status_code
    status_code=$(curl -sS -o /dev/null -w "%{http_code}" \
        --max-time "$CURL_TIMEOUT" \
        "http://127.0.0.1:${SERVER_PORT}/v1/system/status" 2>/dev/null || echo "000")
    verbose "/v1/system/status: HTTP $status_code"

    # Step 6: Cleanup
    echo "[6/6] Cleanup..."
    TEST_PASSED=true

    # Calculate duration
    local duration=$((SECONDS - start_time))

    echo ""
    echo "=========================================="
    echo "  SMOKE TEST PASSED"
    echo "=========================================="
    echo ""
    echo "Duration: ${duration}s"
    echo "Server port: $SERVER_PORT"
    echo "Database: $DB_PATH"
    echo ""
}

main "$@"
