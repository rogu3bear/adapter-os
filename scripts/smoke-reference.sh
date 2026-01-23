#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# -----------------------------------------------------------------------------
# Config (override via env)
# -----------------------------------------------------------------------------

: "${AOS_SERVER_PORT:=8080}"
: "${AOS_SERVER_URL:=http://localhost:${AOS_SERVER_PORT}}"

: "${SMOKE_CONNECT_TIMEOUT_SECONDS:=2}"
: "${SMOKE_HTTP_TIMEOUT_SECONDS:=10}"
: "${SMOKE_READY_TIMEOUT_SECONDS:=90}"
: "${SMOKE_INFER_TIMEOUT_SECONDS:=60}"

: "${SMOKE_ALLOW_BUILD:=0}" # If 1, allow service-manager to compile missing binaries

: "${SMOKE_USERNAME:=admin}"
: "${SMOKE_PASSWORD:=admin}"
: "${API_BASE:=${AOS_SERVER_URL%/}/api}"

# -----------------------------------------------------------------------------
# Output helpers
# -----------------------------------------------------------------------------

log() { printf "[smoke-demo] %s\n" "$*"; }
ok() { printf "[smoke-demo] ✅ %s\n" "$*"; }
warn() { printf "[smoke-demo] ⚠️  %s\n" "$*" >&2; }
err() { printf "[smoke-demo] ❌ %s\n" "$*" >&2; }
die() { err "$*"; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "Missing required command: $1"
}

TMP_ROOT="${AOS_VAR_DIR:-$ROOT_DIR/var}/tmp"
if [[ "$TMP_ROOT" == /tmp* || "$TMP_ROOT" == /private/tmp* ]]; then
  die "Refusing temporary directory under /tmp: $TMP_ROOT"
fi
mkdir -p "$TMP_ROOT"

mktemp_dir() {
  mktemp -d "${TMP_ROOT}/aos-smoke.XXXXXX"
}

TMP_DIR="$(mktemp_dir)"
HEALTH_BODY="$TMP_DIR/healthz.json"
READY_BODY="$TMP_DIR/readyz.json"
LOGIN_BODY="$TMP_DIR/login.json"
MODELS_BODY="$TMP_DIR/models.json"
INFER_BODY="$TMP_DIR/infer.json"
CURL_ERR="$TMP_DIR/curl.err"

AUTH_TOKEN="${AOS_TOKEN:-${TOKEN:-}}"

cleanup() {
  rm -rf "$TMP_DIR" >/dev/null 2>&1 || true
}
trap cleanup EXIT

show_diagnostics() {
  echo ""
  err "Diagnostics"
  log "AOS_SERVER_URL=$AOS_SERVER_URL"
  log "API_BASE=$API_BASE"
  log "AOS_SERVER_PORT=$AOS_SERVER_PORT"

  if [[ -f var/backend.pid ]]; then
    local pid
    pid="$(cat var/backend.pid 2>/dev/null || true)"
    if [[ -n "$pid" ]]; then
      log "backend pid: $pid"
      ps -p "$pid" -o pid=,command= 2>/dev/null || true
    fi
  fi

  if [[ -f var/worker.pid ]]; then
    local pid
    pid="$(cat var/worker.pid 2>/dev/null || true)"
    if [[ -n "$pid" ]]; then
      log "worker pid: $pid"
      ps -p "$pid" -o pid=,command= 2>/dev/null || true
    fi
  fi

  for f in "$HEALTH_BODY" "$READY_BODY" "$LOGIN_BODY" "$MODELS_BODY" "$INFER_BODY"; do
    if [[ -s "$f" ]]; then
      echo ""
      log "Last response body: $f"
      sed -n '1,200p' "$f" 2>/dev/null || true
    fi
  done

  if [[ -s "$CURL_ERR" ]]; then
    echo ""
    log "Last curl stderr: $CURL_ERR"
    sed -n '1,200p' "$CURL_ERR" 2>/dev/null || true
  fi

  if [[ -f var/logs/backend.log ]]; then
    echo ""
    log "Last 120 lines: var/logs/backend.log"
    tail -n 120 var/logs/backend.log || true
  fi

  if [[ -f var/logs/worker.log ]]; then
    echo ""
    log "Last 120 lines: var/logs/worker.log"
    tail -n 120 var/logs/worker.log || true
  fi
}

on_error() {
  local exit_code="$?"
  err "FAILED (exit $exit_code) at line ${BASH_LINENO[0]}: ${BASH_COMMAND}"
  show_diagnostics
  exit "$exit_code"
}
trap on_error ERR

# -----------------------------------------------------------------------------
# curl helpers
# -----------------------------------------------------------------------------

curl_try() {
  local method="$1"
  local url="$2"
  local out_file="$3"
  local max_time="${4:-$SMOKE_HTTP_TIMEOUT_SECONDS}"
  local data="${5:-}"
  shift 5 || true
  local headers=("$@")

  local curl_args=(
    -sS
    --connect-timeout "$SMOKE_CONNECT_TIMEOUT_SECONDS"
    --max-time "$max_time"
    -o "$out_file"
    -w "%{http_code}"
    -X "$method"
  )

  if [[ "$method" != "GET" && "$method" != "HEAD" ]]; then
    curl_args+=(-H "Content-Type: application/json" --data "$data")
  fi

  if [[ ${#headers[@]} -gt 0 ]]; then
    curl_args+=("${headers[@]}")
  fi

  local code="000"
  : >"$CURL_ERR" 2>/dev/null || true
  if code="$(curl "${curl_args[@]}" "$url" 2>"$CURL_ERR")"; then
    printf "%s" "$code"
    return 0
  fi
  printf "%s" "000"
}

wait_for_200() {
  local url="$1"
  local out_file="$2"
  local timeout_seconds="$3"
  local start
  start="$(date +%s)"

  while true; do
    local code
    code="$(curl_try "GET" "$url" "$out_file" "$SMOKE_HTTP_TIMEOUT_SECONDS" "" )"
    if [[ "$code" == "200" ]]; then
      return 0
    fi
    if (( $(date +%s) - start >= timeout_seconds )); then
      err "Timed out waiting for 200 from $url (last status $code)"
      return 1
    fi
    sleep 1
  done
}

auth_header_args() {
  if [[ -n "${AUTH_TOKEN:-}" ]]; then
    printf "%s\n" "-H" "Authorization: Bearer ${AUTH_TOKEN}"
  fi
}

# -----------------------------------------------------------------------------
# Steps
# -----------------------------------------------------------------------------

verify_migrations() {
  log "migrations: verify"

  if [[ -x ./scripts/check-migrations.sh ]]; then
    ./scripts/check-migrations.sh
    ok "migrations: OK (scripts/check-migrations.sh)"
    return 0
  fi

  # Fallback: verify signatures via aosctl if available (no cargo build here)
  local aosctl_bin=""
  if [[ -x ./target/release/aosctl ]]; then
    aosctl_bin="./target/release/aosctl"
  elif [[ -x ./target/debug/aosctl ]]; then
    aosctl_bin="./target/debug/aosctl"
  elif command -v aosctl >/dev/null 2>&1; then
    aosctl_bin="aosctl"
  fi

  if [[ -n "$aosctl_bin" ]]; then
    "$aosctl_bin" db migrate --verify-only
    ok "migrations: OK (${aosctl_bin} db migrate --verify-only)"
    return 0
  fi

  die "No migration verifier found (expected ./scripts/check-migrations.sh or built aosctl)"
}

ensure_backend_running() {
  log "services: ensure backend is up"

  local health_url="${API_BASE}/healthz"
  local code
  code="$(curl_try "GET" "$health_url" "$HEALTH_BODY" "$SMOKE_HTTP_TIMEOUT_SECONDS" "" )"
  if [[ "$code" == "200" ]]; then
    ok "backend: already running (${health_url})"
    return 0
  fi

  log "backend: not responding (healthz=$code); attempting start"
  [[ -f scripts/service-manager.sh ]] || die "Missing scripts/service-manager.sh"

  if [[ "$SMOKE_ALLOW_BUILD" != "1" ]]; then
    if [[ ! -x target/release/adapteros-server && ! -x target/debug/adapteros-server ]]; then
      die "Backend binary not found at target/{debug,release}/adapteros-server. Build it or re-run with SMOKE_ALLOW_BUILD=1."
    fi
  fi

  bash scripts/service-manager.sh start backend

  # Best-effort worker start (some deployments don't require it, but it helps inference).
  if ! bash scripts/service-manager.sh start worker; then
    warn "worker: start failed (continuing; inference may fail if worker is required)"
  fi

  wait_for_200 "$health_url" "$HEALTH_BODY" "$SMOKE_READY_TIMEOUT_SECONDS"
  ok "backend: health OK"
}

verify_readiness() {
  log "http: verify /healthz and /readyz"
  wait_for_200 "${API_BASE}/healthz" "$HEALTH_BODY" "$SMOKE_READY_TIMEOUT_SECONDS"
  wait_for_200 "${API_BASE}/readyz" "$READY_BODY" "$SMOKE_READY_TIMEOUT_SECONDS"
  ok "http: /healthz 200"
  ok "http: /readyz 200"
}

try_login_if_needed() {
  if [[ -n "${AUTH_TOKEN:-}" ]]; then
    ok "auth: using token from env"
    return 0
  fi

  log "auth: attempting login (${SMOKE_USERNAME})"
  local login_url="${API_BASE}/v1/auth/login"
  local payload
  payload="$(printf '{"username":"%s","password":"%s"}' "$SMOKE_USERNAME" "$SMOKE_PASSWORD")"

  local code
  code="$(curl_try "POST" "$login_url" "$LOGIN_BODY" "$SMOKE_HTTP_TIMEOUT_SECONDS" "$payload")"
  if [[ "$code" != "200" ]]; then
    warn "auth: login failed (status $code); continuing without token"
    return 1
  fi

  local token=""
  token="$(grep -o '"token":"[^"]*' "$LOGIN_BODY" | head -n1 | cut -d'"' -f4 || true)"
  if [[ -z "$token" ]]; then
    warn "auth: login 200 but no token in response; continuing without token"
    return 1
  fi

  AUTH_TOKEN="$token"
  ok "auth: login OK (token acquired)"
}

list_models_assert_non_empty() {
  log "api: GET /v1/models (assert non-empty)"
  local url="${API_BASE}/v1/models"

  local headers=()
  if [[ -n "${AUTH_TOKEN:-}" ]]; then
    headers=(-H "Authorization: Bearer ${AUTH_TOKEN}")
  fi

  local code
  code="$(curl_try "GET" "$url" "$MODELS_BODY" "$SMOKE_HTTP_TIMEOUT_SECONDS" "" "${headers[@]}")"

  if [[ ( "$code" == "401" || "$code" == "403" ) && -z "${AUTH_TOKEN:-}" ]]; then
    try_login_if_needed || true
    if [[ -n "${AUTH_TOKEN:-}" ]]; then
      headers=(-H "Authorization: Bearer ${AUTH_TOKEN}")
      code="$(curl_try "GET" "$url" "$MODELS_BODY" "$SMOKE_HTTP_TIMEOUT_SECONDS" "" "${headers[@]}")"
    fi
  fi

  [[ "$code" == "200" ]] || die "GET /v1/models failed (status $code)"

  local total=""
  total="$(grep -Eo '"total"[[:space:]]*:[[:space:]]*[0-9]+' "$MODELS_BODY" | head -n1 | grep -Eo '[0-9]+' || true)"
  if [[ -n "$total" ]] && (( total > 0 )); then
    ok "api: /v1/models returned total=${total}"
    return 0
  fi

  if grep -Eq '"models"[[:space:]]*:[[:space:]]*\\[[[:space:]]*\\{' "$MODELS_BODY"; then
    ok "api: /v1/models returned non-empty models[]"
    return 0
  fi

  die "api: /v1/models returned empty list (expected at least 1 model)"
}

run_minimal_inference() {
  log "api: POST /v1/infer (minimal)"
  local url="${API_BASE}/v1/infer"

  local headers=()
  if [[ -n "${AUTH_TOKEN:-}" ]]; then
    headers=(-H "Authorization: Bearer ${AUTH_TOKEN}")
  fi

  local payload='{"prompt":"Hello","max_tokens":8,"temperature":0,"adapters":[]}'
  local code
  code="$(curl_try "POST" "$url" "$INFER_BODY" "$SMOKE_INFER_TIMEOUT_SECONDS" "$payload" "${headers[@]}")"

  if [[ ( "$code" == "401" || "$code" == "403" ) && -z "${AUTH_TOKEN:-}" ]]; then
    try_login_if_needed || true
    if [[ -n "${AUTH_TOKEN:-}" ]]; then
      headers=(-H "Authorization: Bearer ${AUTH_TOKEN}")
      code="$(curl_try "POST" "$url" "$INFER_BODY" "$SMOKE_INFER_TIMEOUT_SECONDS" "$payload" "${headers[@]}")"
    fi
  fi

  [[ "$code" == "200" ]] || die "POST /v1/infer failed (status $code)"

  local tokens_generated=""
  tokens_generated="$(grep -Eo '"tokens_generated"[[:space:]]*:[[:space:]]*[0-9]+' "$INFER_BODY" | head -n1 | grep -Eo '[0-9]+' || true)"
  if [[ -n "$tokens_generated" ]] && (( tokens_generated > 0 )); then
    ok "api: inference OK (tokens_generated=${tokens_generated})"
    return 0
  fi

  if grep -Eq '"text"[[:space:]]*:[[:space:]]*\"[^\"]' "$INFER_BODY"; then
    ok "api: inference OK (non-empty text)"
    return 0
  fi

  die "api: inference returned empty response (no tokens_generated/text)"
}

main() {
  require_cmd bash
  require_cmd curl

  local started="$SECONDS"

  verify_migrations
  ensure_backend_running
  verify_readiness
  list_models_assert_non_empty
  run_minimal_inference

  local elapsed=$((SECONDS - started))
  ok "PASS (elapsed ${elapsed}s)"
}

main "$@"
