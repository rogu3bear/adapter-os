#!/usr/bin/env bash
# HTTP-only smoke E2E:
# - start control plane
# - login and switch tenant
# - run deterministic inference stub
# - create + fetch trace fixture
# - list evidence
# Target runtime: < 2 minutes
#
# Usage:
#   ./scripts/test/smoke_e2e.sh                # Normal mode
#   ./scripts/test/smoke_e2e.sh --verbose      # Show curl commands and responses
#   ./scripts/test/smoke_e2e.sh --help         # Show usage

set -euo pipefail

# Parse arguments
VERBOSE=false
for arg in "$@"; do
    case "$arg" in
        --verbose|-v)
            VERBOSE=true
            ;;
        --help|-h)
            echo "Usage: $0 [--verbose] [--help]"
            echo ""
            echo "Options:"
            echo "  --verbose, -v  Show curl commands and full responses"
            echo "  --help, -h     Show this help message"
            echo ""
            echo "Environment variables:"
            echo "  API_URL          Server URL (default: http://127.0.0.1:18080)"
            echo "  DB_PATH          SQLite database path"
            echo "  START_TIMEOUT    Timeout waiting for server (default: 60s)"
            echo "  CURL_TIMEOUT     HTTP request timeout (default: 10s)"
            exit 0
            ;;
    esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
API_URL="${API_URL:-http://127.0.0.1:${AOS_SERVER_PORT:-18080}}"
API_BASE="${API_URL%/}/api"
DB_PATH="${DB_PATH:-$ROOT/var/smoke-e2e.sqlite3}"
PID_FILE="${PID_FILE:-$ROOT/var/run/smoke-e2e-api.pid}"
LOG_FILE="${LOG_FILE:-$ROOT/var/log/smoke-e2e-api.log}"
REQ_LOG="${REQ_LOG:-$ROOT/var/log/smoke-e2e.log}"
MANIFEST_PATH="${AOS_MANIFEST_PATH:-$ROOT/manifests/qwen7b.yaml}"
START_TIMEOUT="${START_TIMEOUT:-60}"
CURL_TIMEOUT="${CURL_TIMEOUT:-10}"
TMP_ROOT="${AOS_VAR_DIR:-$ROOT/var}/tmp"

# Track all request IDs for summary
declare -a ALL_REQUEST_IDS=()
declare -a ALL_REQUEST_LABELS=()

if [[ "$TMP_ROOT" == /tmp* || "$TMP_ROOT" == /private/tmp* ]]; then
  echo "[ERROR] refusing temporary directory under /tmp: $TMP_ROOT" >&2
  exit 1
fi

mkdir -p "$TMP_ROOT"
RUN_TMP_DIR="$(mktemp -d "${TMP_ROOT}/smoke-e2e.XXXXXX")"

E2E_EMAIL="test@example.com"
E2E_PASS="password"

info() { echo "[INFO] $*"; }
verbose() {
  if [[ "$VERBOSE" == true ]]; then
    echo "[VERBOSE] $*"
  fi
}
fail() {
  echo ""
  echo "╔══════════════════════════════════════════════════════════════╗"
  echo "║                    SMOKE E2E FAILED                          ║"
  echo "╚══════════════════════════════════════════════════════════════╝"
  echo ""
  echo "[ERROR] $*" >&2
  echo ""

  # Print request ID summary for debugging
  if [[ ${#ALL_REQUEST_IDS[@]} -gt 0 ]]; then
    echo "Request IDs collected before failure:"
    for i in "${!ALL_REQUEST_IDS[@]}"; do
      echo "  ${ALL_REQUEST_LABELS[$i]}: ${ALL_REQUEST_IDS[$i]}"
    done
    echo ""
  fi

  if [[ -s "$LOG_FILE" ]]; then
    echo "--- Last 100 lines of backend log ($LOG_FILE) ---" >&2
    tail -n 100 "$LOG_FILE" >&2 || true
    echo "--- end of backend log ---" >&2
  fi

  if [[ -s "$REQ_LOG" ]]; then
    echo ""
    echo "--- Request log ($REQ_LOG) ---" >&2
    cat "$REQ_LOG" >&2 || true
    echo "--- end of request log ---" >&2
  fi

  cleanup
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "Missing required command: $1"
}

cleanup() {
  if [[ -f "$PID_FILE" ]]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [[ -n "${pid:-}" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      sleep 1
      kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$PID_FILE"
  fi
  rm -rf "${RUN_TMP_DIR:-}" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

wait_for_health() {
  local end=$((SECONDS + START_TIMEOUT))
  while (( SECONDS < end )); do
    if curl -fsS --max-time "$CURL_TIMEOUT" "$API_BASE/readyz" >/dev/null 2>&1; then
      info "API ready at $API_BASE"
      return 0
    fi
    sleep 2
  done
  fail "API failed to become ready within ${START_TIMEOUT}s"
}

start_stack() {
  if [[ -f "$PID_FILE" ]] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
    fail "Existing smoke E2E server running (pid $(cat "$PID_FILE")). Stop it first."
  fi

  mkdir -p "$(dirname "$PID_FILE")" "$(dirname "$LOG_FILE")" "$(dirname "$DB_PATH")"

  info "Running migrations (sqlite://${DB_PATH})"
  SQLX_DISABLE_STATEMENT_CHECKS="${SQLX_DISABLE_STATEMENT_CHECKS:-1}" \
    AOS_SKIP_MIGRATION_SIGNATURES="${AOS_SKIP_MIGRATION_SIGNATURES:-1}" \
    DATABASE_URL="sqlite://${DB_PATH}" \
    cargo sqlx migrate run >/dev/null

  info "Starting control plane (E2E_MODE=1)"
  E2E_MODE=1 \
  VITE_ENABLE_DEV_BYPASS=1 \
  AOS_DEV_NO_AUTH="${AOS_DEV_NO_AUTH:-0}" \
  AOS_SKIP_MIGRATION_SIGNATURES="${AOS_SKIP_MIGRATION_SIGNATURES:-1}" \
  SQLX_DISABLE_STATEMENT_CHECKS="${SQLX_DISABLE_STATEMENT_CHECKS:-1}" \
  DATABASE_URL="sqlite://${DB_PATH}" \
  AOS_DATABASE_URL="sqlite://${DB_PATH}" \
  AOS_MANIFEST_PATH="$MANIFEST_PATH" \
  RUST_LOG="${RUST_LOG:-info}" \
    cargo run -q -p adapteros-server -- --config configs/cp.toml >"$LOG_FILE" 2>&1 &
  echo $! >"$PID_FILE"

  wait_for_health
}

call_api() {
  local method="$1"
  local url="$2"
  local body="${3:-}"
  local token="${4:-}"
  local label="${5:-$method $url}"

  RESP_HEADERS="$(mktemp "${RUN_TMP_DIR}/headers.XXXXXX")"
  RESP_BODY="$(mktemp "${RUN_TMP_DIR}/body.XXXXXX")"

  local args=(curl -sS -D "$RESP_HEADERS" -o "$RESP_BODY" -w "%{http_code}" -X "$method" "$url" \
    --connect-timeout "$CURL_TIMEOUT" --max-time "$CURL_TIMEOUT" \
    -H "Accept: application/json")
  if [[ -n "$body" ]]; then
    args+=(-H "Content-Type: application/json" -d "$body")
  fi
  if [[ -n "$token" ]]; then
    args+=(-H "Authorization: Bearer $token")
  fi

  # Verbose mode: show the curl command
  if [[ "$VERBOSE" == true ]]; then
    echo "[VERBOSE] curl -X $method $url"
    if [[ -n "$body" ]]; then
      echo "[VERBOSE]   Body: $body"
    fi
    if [[ -n "$token" ]]; then
      echo "[VERBOSE]   Auth: Bearer ${token:0:20}..."
    fi
  fi

  STATUS=$("${args[@]}" || true)
  local req_id
  req_id="$(get_request_id)"

  echo "[$(date -Is)] $method $url -> $STATUS (x-request-id: ${req_id:-none})" >>"$REQ_LOG"

  # Store request ID for summary
  if [[ -n "${req_id:-}" ]]; then
    ALL_REQUEST_IDS+=("$req_id")
    ALL_REQUEST_LABELS+=("$label")
  fi

  # Verbose mode: show response
  if [[ "$VERBOSE" == true ]]; then
    echo "[VERBOSE]   Status: $STATUS"
    echo "[VERBOSE]   Request-ID: ${req_id:-none}"
    if [[ -s "$RESP_BODY" ]]; then
      echo "[VERBOSE]   Response: $(cat "$RESP_BODY" | head -c 500)"
    fi
    echo ""
  fi
}

expect_status() {
  local expected="$1"
  local label="$2"
  if [[ "$STATUS" != "$expected" ]]; then
    echo "--- response body ---" >&2
    cat "$RESP_BODY" >&2 || true
    fail "$label failed (status $STATUS, expected $expected)"
  fi
}

get_request_id() {
  grep -i '^x-request-id:' "$RESP_HEADERS" | awk '{print $2}' | tr -d '\r' | tail -n 1
}

require_cmd curl
require_cmd jq
require_cmd cargo

start_stack

info "Resetting database via testkit"
call_api POST "$API_BASE/testkit/reset" "{}"
expect_status "200" "testkit reset"

info "Seeding deterministic fixtures"
call_api POST "$API_BASE/testkit/seed_minimal" "{}"
expect_status "200" "seed_minimal"
PRIMARY_TENANT="$(jq -r '.tenant_id' "$RESP_BODY")"
SECONDARY_TENANT="$(jq -r '.secondary_tenant_id' "$RESP_BODY")"
info "Tenants: primary=${PRIMARY_TENANT}, secondary=${SECONDARY_TENANT}"

info "Logging in"
LOGIN_PAYLOAD=$(jq -n --arg email "$E2E_EMAIL" --arg pass "$E2E_PASS" '{email:$email, username:$email, password:$pass}')
call_api POST "$API_BASE/v1/auth/login" "$LOGIN_PAYLOAD"
expect_status "200" "login"
TOKEN_PRIMARY="$(jq -r '.token' "$RESP_BODY")"
LOGIN_TENANT="$(jq -r '.tenant_id' "$RESP_BODY")"
LOGIN_REQ_ID="$(get_request_id)"
info "Login tenant=${LOGIN_TENANT} request_id=${LOGIN_REQ_ID}"

info "Switching tenant -> ${SECONDARY_TENANT}"
SWITCH_PAYLOAD=$(jq -n --arg tenant "$SECONDARY_TENANT" '{tenant_id:$tenant}')
call_api POST "$API_BASE/v1/auth/tenants/switch" "$SWITCH_PAYLOAD" "$TOKEN_PRIMARY"
expect_status "200" "tenant switch"
TOKEN_SECONDARY="$(jq -r '.token' "$RESP_BODY")"
SWITCH_TENANT="$(jq -r '.tenant_id' "$RESP_BODY")"
SWITCH_REQ_ID="$(get_request_id)"
info "Switched tenant=${SWITCH_TENANT} request_id=${SWITCH_REQ_ID}"

info "Running inference stub"
call_api POST "$API_BASE/testkit/inference_stub" "$(jq -n --arg prompt "smoke e2e" '{prompt:$prompt}')" "$TOKEN_SECONDARY"
expect_status "200" "inference stub"
INFER_TRACE_ID="$(jq -r '.run_receipt.trace_id' "$RESP_BODY")"
INFER_REQ_ID="$(get_request_id)"
info "Inference trace_id=${INFER_TRACE_ID} request_id=${INFER_REQ_ID}"

info "Creating trace fixture"
TRACE_PAYLOAD=$(jq -n --arg tenant "$SWITCH_TENANT" '{tenant_id:$tenant, token_count:3}')
call_api POST "$API_BASE/testkit/create_trace_fixture" "$TRACE_PAYLOAD"
expect_status "200" "create_trace_fixture"
TRACE_FIX_ID="$(jq -r '.trace_id' "$RESP_BODY")"
info "Trace fixture created trace_id=${TRACE_FIX_ID}"

info "Fetching trace ${TRACE_FIX_ID}"
call_api GET "$API_BASE/v1/traces/${TRACE_FIX_ID}" "" "$TOKEN_SECONDARY"
expect_status "200" "get_trace"
TRACE_REQ_ID="$(get_request_id)"
info "Trace fetch request_id=${TRACE_REQ_ID}"

info "Creating evidence fixture"
EVIDENCE_PAYLOAD=$(jq -n --arg tenant "$SWITCH_TENANT" --arg inference "$TRACE_FIX_ID" '{tenant_id:$tenant, inference_id:$inference}')
call_api POST "$API_BASE/testkit/create_evidence_fixture" "$EVIDENCE_PAYLOAD"
expect_status "200" "create_evidence_fixture"

info "Listing evidence (tenant ${SWITCH_TENANT})"
call_api GET "$API_BASE/v1/evidence?limit=5" "" "$TOKEN_SECONDARY"
expect_status "200" "list_evidence"
EVIDENCE_COUNT="$(jq 'length' "$RESP_BODY")"
EVIDENCE_REQ_ID="$(get_request_id)"
info "Evidence entries=${EVIDENCE_COUNT} request_id=${EVIDENCE_REQ_ID}"

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                  SMOKE E2E PASSED ✓                          ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "Summary:"
echo "────────────────────────────────────────────────────────────────"
printf "  %-25s %s\n" "Step" "Request ID / Details"
echo "────────────────────────────────────────────────────────────────"
printf "  %-25s %s\n" "Login" "${LOGIN_REQ_ID}"
printf "  %-25s %s\n" "Tenant switch" "${SWITCH_REQ_ID}"
printf "  %-25s %s (trace: %s)\n" "Inference stub" "${INFER_REQ_ID}" "${INFER_TRACE_ID}"
printf "  %-25s %s (trace: %s)\n" "Trace fixture" "${TRACE_REQ_ID}" "${TRACE_FIX_ID}"
printf "  %-25s %s (count: %s)\n" "Evidence list" "${EVIDENCE_REQ_ID}" "${EVIDENCE_COUNT}"
echo "────────────────────────────────────────────────────────────────"
echo ""
echo "All request IDs (for server log correlation):"
for i in "${!ALL_REQUEST_IDS[@]}"; do
  printf "  %s: %s\n" "${ALL_REQUEST_LABELS[$i]}" "${ALL_REQUEST_IDS[$i]}"
done
echo ""
echo "Request log: $REQ_LOG"
echo "Server log:  $LOG_FILE"
echo ""




