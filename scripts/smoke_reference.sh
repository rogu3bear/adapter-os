#!/usr/bin/env bash
# =============================================================================
# AdapterOS Reference Smoke Test
# =============================================================================
#
# Quick verification script for reference environments.
# Asserts that critical endpoints are working.
#
# Usage:
#   ./scripts/smoke_reference.sh              Run all smoke tests
#   ./scripts/smoke_reference.sh --verbose    Show response bodies
#   ./scripts/smoke_reference.sh --skip-infer Skip inference test (faster)
#   ./scripts/smoke_reference.sh --help       Show this help
#
# Exit codes:
#   0  All tests passed
#   1  Test failure
#   2  Missing prerequisites
#
set -Eeuo pipefail

# =============================================================================
# Configuration
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

: "${AOS_SERVER_PORT:=8080}"
: "${AOS_SERVER_URL:=http://localhost:${AOS_SERVER_PORT}}"
# All endpoints are at root level (no /api prefix)
: "${SERVER_BASE:=${AOS_SERVER_URL%/}}"
: "${API_BASE:=${AOS_SERVER_URL%/}}"

: "${SMOKE_CONNECT_TIMEOUT:=2}"
: "${SMOKE_HTTP_TIMEOUT:=10}"
: "${SMOKE_INFER_TIMEOUT:=60}"
: "${SMOKE_STREAM_TIMEOUT:=30}"
: "${SMOKE_TRAIN_TIMEOUT:=300}"
: "${SMOKE_TRAIN_POLL_INTERVAL:=5}"
: "${SMOKE_PREVIEW_TEXT:=preview}"
: "${SMOKE_USERNAME:=admin}"
: "${SMOKE_PASSWORD:=admin}"

: "${AOS_TOKEN:=${AOS_AUTH_TOKEN:-}}"

# Options
VERBOSE=0
SKIP_INFER=0
SHOW_HELP=0

# Temp files
TMP_DIR=""
RESP_BODY=""
RESP_HEADERS=""

# Counters
TESTS_PASSED=0
TESTS_FAILED=0
TRAINED_ADAPTER_ID=""

# =============================================================================
# Output helpers
# =============================================================================

log()  { printf "[smoke] %s\n" "$*"; }
ok()   { printf "[smoke] PASS: %s\n" "$*"; ((TESTS_PASSED++)) || true; }
fail() { printf "[smoke] FAIL: %s\n" "$*" >&2; ((TESTS_FAILED++)) || true; }
err()  { printf "[smoke] ERROR: %s\n" "$*" >&2; }
die()  { err "$*"; cleanup; exit 2; }

verbose() {
    if [[ $VERBOSE -eq 1 ]]; then
        printf "[smoke] (verbose) %s\n" "$*"
    fi
}

# =============================================================================
# Parse arguments
# =============================================================================

while [[ $# -gt 0 ]]; do
    case "$1" in
        --verbose|-v)
            VERBOSE=1
            ;;
        --skip-infer|--no-infer)
            SKIP_INFER=1
            ;;
        --help|-h)
            SHOW_HELP=1
            ;;
        *)
            err "Unknown option: $1"
            ;;
    esac
    shift
done

if [[ $SHOW_HELP -eq 1 ]]; then
    cat << 'EOF'
AdapterOS Reference Smoke Test

Usage:
  ./scripts/smoke_reference.sh [options]

Options:
  --verbose, -v           Show response bodies
  --skip-infer, --no-infer Skip inference test (faster)
  --help, -h              Show this help

Environment:
  AOS_SERVER_PORT     API port (default: 8080)
  AOS_TOKEN           Auth token (optional; auto-login if missing)
  SMOKE_USERNAME      Login username (default: admin)
  SMOKE_PASSWORD      Login password (default: admin)

Tests performed:
  1. /healthz returns 200 (liveness)
  2. /readyz returns 200 (readiness with DB, worker, models checks)
  3. /system/ready returns 200 (full component health)
  4. /v1/models returns non-empty list (requires auth)
  5. POST /v1/infer/stream returns streaming tokens (requires auth, skip with --skip-infer)
  6. GET /v1/topology?preview_text=... returns suggested adapters
  7. POST /v1/training/jobs completes with adapter_id
  8. POST /v1/adapters/{id}/load succeeds

Exit codes:
  0  All tests passed
  1  Test failure
  2  Missing prerequisites
EOF
    exit 0
fi

# =============================================================================
# Setup and cleanup
# =============================================================================

setup_temp() {
    TMP_DIR="$(mktemp -d)"
    RESP_BODY="$TMP_DIR/body"
    RESP_HEADERS="$TMP_DIR/headers"
}

cleanup() {
    if [[ -n "${TMP_DIR:-}" && -d "$TMP_DIR" ]]; then
        rm -rf "$TMP_DIR" 2>/dev/null || true
    fi
}

trap cleanup EXIT

# =============================================================================
# HTTP helpers
# =============================================================================

# Performs a curl request and stores status code
# Usage: http_get URL | http_post URL BODY
http_get() {
    local url="$1"

    if [[ -n "${AOS_TOKEN:-}" ]]; then
        curl -sS \
            --connect-timeout "$SMOKE_CONNECT_TIMEOUT" \
            --max-time "$SMOKE_HTTP_TIMEOUT" \
            -o "$RESP_BODY" \
            -D "$RESP_HEADERS" \
            -w "%{http_code}" \
            -H "Authorization: Bearer ${AOS_TOKEN}" \
            "$url" 2>/dev/null || echo "000"
    else
        curl -sS \
            --connect-timeout "$SMOKE_CONNECT_TIMEOUT" \
            --max-time "$SMOKE_HTTP_TIMEOUT" \
            -o "$RESP_BODY" \
            -D "$RESP_HEADERS" \
            -w "%{http_code}" \
            "$url" 2>/dev/null || echo "000"
    fi
}

http_post() {
    local url="$1"
    local body="$2"

    if [[ -n "${AOS_TOKEN:-}" ]]; then
        curl -sS \
            --connect-timeout "$SMOKE_CONNECT_TIMEOUT" \
            --max-time "$SMOKE_INFER_TIMEOUT" \
            -o "$RESP_BODY" \
            -D "$RESP_HEADERS" \
            -w "%{http_code}" \
            -X POST \
            -H "Content-Type: application/json" \
            -H "Authorization: Bearer ${AOS_TOKEN}" \
            -d "$body" \
            "$url" 2>/dev/null || echo "000"
    else
        curl -sS \
            --connect-timeout "$SMOKE_CONNECT_TIMEOUT" \
            --max-time "$SMOKE_INFER_TIMEOUT" \
            -o "$RESP_BODY" \
            -D "$RESP_HEADERS" \
            -w "%{http_code}" \
            -X POST \
            -H "Content-Type: application/json" \
            -d "$body" \
            "$url" 2>/dev/null || echo "000"
    fi
}

# Stream POST that expects SSE or NDJSON
http_post_stream() {
    local url="$1"
    local body="$2"

    # Use timeout to limit streaming duration
    if [[ -n "${AOS_TOKEN:-}" ]]; then
        timeout "$SMOKE_STREAM_TIMEOUT" curl -sS \
            --connect-timeout "$SMOKE_CONNECT_TIMEOUT" \
            -o "$RESP_BODY" \
            -D "$RESP_HEADERS" \
            -w "%{http_code}" \
            -X POST \
            -H "Content-Type: application/json" \
            -H "Authorization: Bearer ${AOS_TOKEN}" \
            -d "$body" \
            "$url" 2>/dev/null || echo "000"
    else
        timeout "$SMOKE_STREAM_TIMEOUT" curl -sS \
            --connect-timeout "$SMOKE_CONNECT_TIMEOUT" \
            -o "$RESP_BODY" \
            -D "$RESP_HEADERS" \
            -w "%{http_code}" \
            -X POST \
            -H "Content-Type: application/json" \
            -d "$body" \
            "$url" 2>/dev/null || echo "000"
    fi
}

http_post_empty() {
    local url="$1"

    if [[ -n "${AOS_TOKEN:-}" ]]; then
        curl -sS \
            --connect-timeout "$SMOKE_CONNECT_TIMEOUT" \
            --max-time "$SMOKE_HTTP_TIMEOUT" \
            -o "$RESP_BODY" \
            -D "$RESP_HEADERS" \
            -w "%{http_code}" \
            -X POST \
            -H "Authorization: Bearer ${AOS_TOKEN}" \
            "$url" 2>/dev/null || echo "000"
    else
        curl -sS \
            --connect-timeout "$SMOKE_CONNECT_TIMEOUT" \
            --max-time "$SMOKE_HTTP_TIMEOUT" \
            -o "$RESP_BODY" \
            -D "$RESP_HEADERS" \
            -w "%{http_code}" \
            -X POST \
            "$url" 2>/dev/null || echo "000"
    fi
}

json_extract() {
    local key="$1"
    grep -oE "\"${key}\"[[:space:]]*:[[:space:]]*\"[^\"]+\"" "$RESP_BODY" 2>/dev/null \
        | head -1 \
        | cut -d'"' -f4
}

ensure_auth() {
    if [[ -n "${AOS_TOKEN:-}" ]]; then
        ok "Auth token provided"
        return 0
    fi

    log "Authenticating via /v1/auth/login..."
    local payload
    payload="$(printf '{"username":"%s","password":"%s"}' "$SMOKE_USERNAME" "$SMOKE_PASSWORD")"
    local status
    status=$(http_post "${API_BASE}/v1/auth/login" "$payload")
    show_response

    if [[ "$status" != "200" ]]; then
        fail "Login failed with status $status (set AOS_TOKEN or fix reference credentials)"
        return 1
    fi

    local token
    token="$(json_extract "token")"
    if [[ -z "$token" ]]; then
        fail "Login response missing token"
        return 1
    fi

    export AOS_TOKEN="$token"
    ok "Auth token acquired"
    return 0
}

show_response() {
    if [[ $VERBOSE -eq 1 && -f "$RESP_BODY" ]]; then
        verbose "Response body: $(head -c 500 "$RESP_BODY" 2>/dev/null || echo "(empty)")"
    fi
}

# =============================================================================
# Test functions
# =============================================================================

test_healthz() {
    log "Testing GET /healthz..."

    local status
    # Health endpoints are at root level, not under /api
    status=$(http_get "${SERVER_BASE}/healthz")
    show_response

    if [[ "$status" == "200" ]]; then
        ok "/healthz returned 200"
        return 0
    else
        fail "/healthz returned $status (expected 200)"
        return 1
    fi
}

test_readyz() {
    log "Testing GET /readyz..."

    local status
    # Health endpoints are at root level, not under /api
    status=$(http_get "${SERVER_BASE}/readyz")
    show_response

    if [[ "$status" == "200" ]]; then
        ok "/readyz returned 200"
        return 0
    else
        fail "/readyz returned $status (expected 200)"
        return 1
    fi
}

test_system_ready() {
    log "Testing GET /system/ready..."

    local status
    # System ready endpoint is at root level
    status=$(http_get "${SERVER_BASE}/system/ready")
    show_response

    if [[ "$status" == "200" ]]; then
        # Parse ready field from response
        if grep -qE '"ready"[[:space:]]*:[[:space:]]*true' "$RESP_BODY" 2>/dev/null; then
            ok "/system/ready returned 200 (ready=true)"
        else
            # Still 200 but ready=false - show the reason
            local reason
            reason=$(grep -oE '"reason"[[:space:]]*:[[:space:]]*"[^"]*"' "$RESP_BODY" 2>/dev/null | head -1 || echo "")
            ok "/system/ready returned 200 (ready=false) $reason"
        fi
        return 0
    elif [[ "$status" == "503" ]]; then
        # 503 means system not ready - extract reason for actionable error
        local reason
        reason=$(grep -oE '"reason"[[:space:]]*:[[:space:]]*"[^"]*"' "$RESP_BODY" 2>/dev/null | head -1 || echo "")
        fail "/system/ready returned 503: $reason"
        return 1
    else
        fail "/system/ready returned $status (expected 200 or 503)"
        return 1
    fi
}

test_models() {
    log "Testing GET /v1/models..."

    local status
    status=$(http_get "${API_BASE}/v1/models")
    show_response

    # Handle auth requirements gracefully
    if [[ "$status" == "401" || "$status" == "403" ]]; then
        if [[ -z "${AOS_TOKEN:-}" ]]; then
            log "SKIP: /v1/models requires auth (set AOS_TOKEN)"
            return 0
        fi
        fail "/v1/models returned $status (auth rejected)"
        return 1
    fi

    if [[ "$status" != "200" ]]; then
        fail "/v1/models returned $status (expected 200)"
        return 1
    fi

    # Check for non-empty response
    local has_models=0

    # Check for "total" field with value > 0
    if grep -qE '"total"[[:space:]]*:[[:space:]]*[1-9]' "$RESP_BODY" 2>/dev/null; then
        has_models=1
    fi

    # Or check for non-empty "models" array
    if grep -qE '"models"[[:space:]]*:[[:space:]]*\[[[:space:]]*\{' "$RESP_BODY" 2>/dev/null; then
        has_models=1
    fi

    # Or check for "data" array (OpenAI-compatible format)
    if grep -qE '"data"[[:space:]]*:[[:space:]]*\[[[:space:]]*\{' "$RESP_BODY" 2>/dev/null; then
        has_models=1
    fi

    if [[ $has_models -eq 1 ]]; then
        ok "/v1/models returned non-empty list"
        return 0
    else
        fail "/v1/models returned empty or invalid response"
        return 1
    fi
}

test_infer_stream() {
    log "Testing POST /v1/infer/stream..."

    local payload='{"prompt":"Hello","max_tokens":8,"temperature":0,"stream":true}'

    local status
    status=$(http_post_stream "${API_BASE}/v1/infer/stream" "$payload")
    show_response

    # Accept 200 or streaming response patterns
    if [[ "$status" == "200" ]]; then
        # Check for streaming tokens in response
        if [[ -s "$RESP_BODY" ]]; then
            local has_content=0

            # Check for SSE data events
            if grep -q "^data:" "$RESP_BODY" 2>/dev/null; then
                has_content=1
            fi

            # Or NDJSON with text/token fields
            if grep -qE '"text"|"token"|"content"' "$RESP_BODY" 2>/dev/null; then
                has_content=1
            fi

            if [[ $has_content -eq 1 ]]; then
                ok "/v1/infer/stream returned streaming tokens"
                return 0
            else
                fail "/v1/infer/stream returned 200 but no tokens"
                return 1
            fi
        else
            fail "/v1/infer/stream returned empty response"
            return 1
        fi
    elif [[ "$status" == "401" || "$status" == "403" ]]; then
        # Auth required - this is acceptable if no token provided
        if [[ -z "${AOS_TOKEN:-}" ]]; then
            log "SKIP: /v1/infer/stream requires auth (set AOS_TOKEN)"
            return 0
        else
            fail "/v1/infer/stream returned $status (auth rejected)"
            return 1
        fi
    elif [[ "$status" == "000" ]]; then
        # Check if response body contains auth error (timeout/network issue but got auth error)
        if grep -qE '"TOKEN_MISSING"|"code":"TOKEN_MISSING"' "$RESP_BODY" 2>/dev/null; then
            if [[ -z "${AOS_TOKEN:-}" ]]; then
                log "SKIP: /v1/infer/stream requires auth (set AOS_TOKEN)"
                return 0
            fi
        fi
        fail "/v1/infer/stream connection failed or timeout"
        return 1
    else
        fail "/v1/infer/stream returned $status (expected 200)"
        return 1
    fi
}

test_router_preview() {
    log "Testing GET /v1/topology?preview_text=..."

    local status
    status=$(http_get "${API_BASE}/v1/topology?preview_text=${SMOKE_PREVIEW_TEXT}")
    show_response

    if [[ "$status" == "200" ]]; then
        if grep -q "\"predicted_path\"" "$RESP_BODY" 2>/dev/null && grep -q "\"adapter_id\"" "$RESP_BODY" 2>/dev/null; then
            ok "/v1/topology preview returned suggested adapters"
            return 0
        else
            fail "/v1/topology preview returned 200 but no suggestions"
            return 1
        fi
    elif [[ "$status" == "401" || "$status" == "403" ]]; then
        fail "/v1/topology preview returned $status (auth required)"
        return 1
    else
        fail "/v1/topology preview returned $status (expected 200)"
        return 1
    fi
}

test_training_job() {
    log "Testing POST /v1/training/jobs..."

    # Fetch dataset_id
    local status
    status=$(http_get "${API_BASE}/v1/datasets")
    show_response
    if [[ "$status" != "200" ]]; then
        fail "/v1/datasets returned $status (expected 200)"
        return 1
    fi
    local dataset_id
    dataset_id="$(json_extract "id")"
    if [[ -z "$dataset_id" ]]; then
        fail "No dataset id found for training"
        return 1
    fi

    # Fetch base model id
    status=$(http_get "${API_BASE}/v1/models")
    show_response
    if [[ "$status" != "200" ]]; then
        fail "/v1/models returned $status (expected 200)"
        return 1
    fi
    local model_id
    model_id="$(json_extract "id")"
    if [[ -z "$model_id" ]]; then
        fail "No base model id found for training"
        return 1
    fi

    local payload
    payload="$(printf '{"workspace_id":"","base_model_id":"%s","dataset_id":"%s","params":{"rank":16,"alpha":32,"targets":["q_proj","v_proj"],"training_contract_version":"1.0","pad_token_id":0,"ignore_index":-100,"epochs":1,"learning_rate":0.0001,"batch_size":1}}' "$model_id" "$dataset_id")"

    status=$(http_post "${API_BASE}/v1/training/jobs" "$payload")
    show_response
    if [[ "$status" != "200" && "$status" != "201" ]]; then
        fail "POST /v1/training/jobs returned $status (expected 200/201)"
        return 1
    fi

    local job_id
    job_id="$(json_extract "id")"
    if [[ -z "$job_id" ]]; then
        fail "Training job response missing id"
        return 1
    fi

    local start=$SECONDS
    while true; do
        local elapsed=$((SECONDS - start))
        if [[ $elapsed -ge $SMOKE_TRAIN_TIMEOUT ]]; then
            fail "Training job timed out after ${SMOKE_TRAIN_TIMEOUT}s"
            return 1
        fi

        status=$(http_get "${API_BASE}/v1/training/jobs/${job_id}")
        show_response
        if [[ "$status" != "200" ]]; then
            fail "GET /v1/training/jobs/${job_id} returned $status"
            return 1
        fi

        local job_status
        job_status="$(json_extract "status")"
        if [[ "$job_status" == "completed" ]]; then
            local adapter_id
            adapter_id="$(json_extract "adapter_id")"
            if [[ -z "$adapter_id" ]]; then
                fail "Training completed but adapter_id missing"
                return 1
            fi
            TRAINED_ADAPTER_ID="$adapter_id"
            ok "Training completed with adapter_id=${adapter_id}"
            return 0
        fi

        if [[ "$job_status" == "failed" || "$job_status" == "cancelled" ]]; then
            fail "Training job ${job_id} ended with status=${job_status}"
            return 1
        fi

        sleep "$SMOKE_TRAIN_POLL_INTERVAL"
    done
}

test_adapter_load() {
    log "Testing POST /v1/adapters/{id}/load..."

    local adapter_id="${TRAINED_ADAPTER_ID}"
    if [[ -z "$adapter_id" ]]; then
        local status
        status=$(http_get "${API_BASE}/v1/adapters")
        show_response
        if [[ "$status" != "200" ]]; then
            fail "/v1/adapters returned $status (expected 200)"
            return 1
        fi
        adapter_id="$(json_extract "id")"
    fi

    if [[ -z "$adapter_id" ]]; then
        fail "No adapter id available to load"
        return 1
    fi

    local status
    status=$(http_post_empty "${API_BASE}/v1/adapters/${adapter_id}/load")
    show_response

    if [[ "$status" == "200" ]]; then
        ok "/v1/adapters/${adapter_id}/load returned 200"
        return 0
    elif [[ "$status" == "401" || "$status" == "403" ]]; then
        fail "/v1/adapters/${adapter_id}/load returned $status (auth required)"
        return 1
    else
        fail "/v1/adapters/${adapter_id}/load returned $status (expected 200)"
        return 1
    fi
}

test_topology() {
    log "Testing GET /v1/topology..."

    local status
    status=$(http_get "${API_BASE}/v1/topology")
    show_response

    if [[ "$status" == "200" ]]; then
        ok "/v1/topology returned 200"
        return 0
    elif [[ "$status" == "404" ]]; then
        log "SKIP: /v1/topology not available (404)"
        return 0
    elif [[ "$status" == "401" || "$status" == "403" ]]; then
        if [[ -z "${AOS_TOKEN:-}" ]]; then
            log "SKIP: /v1/topology requires auth"
            return 0
        fi
        fail "/v1/topology returned $status"
        return 1
    else
        fail "/v1/topology returned $status"
        return 1
    fi
}

test_training_jobs() {
    log "Testing GET /v1/training/jobs..."

    local status
    status=$(http_get "${API_BASE}/v1/training/jobs")
    show_response

    if [[ "$status" == "200" ]]; then
        ok "/v1/training/jobs returned 200"
        return 0
    elif [[ "$status" == "404" ]]; then
        log "SKIP: /v1/training/jobs not available (404)"
        return 0
    elif [[ "$status" == "401" || "$status" == "403" ]]; then
        if [[ -z "${AOS_TOKEN:-}" ]]; then
            log "SKIP: /v1/training/jobs requires auth"
            return 0
        fi
        fail "/v1/training/jobs returned $status"
        return 1
    elif [[ "$status" == "500" || "$status" == "503" ]]; then
        # Server error - may be expected if training not configured
        log "SKIP: /v1/training/jobs returned $status (training may not be configured)"
        return 0
    else
        fail "/v1/training/jobs returned $status"
        return 1
    fi
}

# =============================================================================
# Main
# =============================================================================

main() {
    echo ""
    echo "=============================================="
    echo "      AdapterOS Reference Smoke Test"
    echo "=============================================="
    echo ""

    # Prerequisites
    if ! command -v curl >/dev/null 2>&1; then
        die "curl is required but not found"
    fi

    setup_temp

    log "Server: $SERVER_BASE"
    log "API:    $API_BASE"
    echo ""

    # Run tests
    test_healthz || true
    test_readyz || true
    test_system_ready || true
    if ensure_auth; then
        test_models || true

        if [[ $SKIP_INFER -eq 0 ]]; then
            test_infer_stream || true
        else
            log "SKIP: inference test (--skip-infer)"
        fi

        test_router_preview || true
        test_training_job || true
        test_adapter_load || true
    else
        log "SKIP: auth-required tests (models/stream/preview/train/load)"
    fi

    # Summary
    echo ""
    echo "=============================================="

    local total=$((TESTS_PASSED + TESTS_FAILED))

    if [[ $TESTS_FAILED -eq 0 ]]; then
        echo "  RESULT: ALL TESTS PASSED ($TESTS_PASSED/$total)"
        echo "=============================================="
        echo ""
        exit 0
    else
        echo "  RESULT: $TESTS_FAILED TEST(S) FAILED ($TESTS_PASSED/$total passed)"
        echo "=============================================="
        echo ""
        exit 1
    fi
}

main "$@"
