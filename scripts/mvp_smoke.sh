#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

: "${AOS_API_URL:=http://localhost:${AOS_SERVER_PORT:-8080}/api}"
: "${MVP_SMOKE_CLIPPY:=0}"
: "${MVP_SMOKE_DEV_BYPASS:=0}"
: "${MVP_SMOKE_TOKEN:=${AOS_AUTH_TOKEN:-}}"
: "${MVP_SMOKE_ENDPOINT_RETRIES:=5}"
: "${MVP_SMOKE_ENDPOINT_DELAY_SECONDS:=1}"
: "${MVP_SMOKE_ENDPOINT_TIMEOUT_SECONDS:=5}"
: "${MVP_REPORT_PATH:=$ROOT_DIR/var/mvp_report.md}"
: "${MVP_REPORT_TEMPLATE:=$ROOT_DIR/scripts/mvp_report.md}"

log() { printf "[mvp_smoke] %s\n" "$*"; }
ok() { printf "[mvp_smoke] OK: %s\n" "$*"; }
warn() { printf "[mvp_smoke] WARN: %s\n" "$*" >&2; }
err() { printf "[mvp_smoke] FAIL: %s\n" "$*" >&2; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || { err "Missing command: $1"; exit 1; }
}

run_step() {
  local name="$1"
  shift
  TOTAL=$((TOTAL + 1))
  log "$name"
  if "$@"; then
    ok "$name"
  else
    err "$name"
    FAILED=$((FAILED + 1))
  fi
}

run_step_in_dir() {
  local name="$1"
  local dir="$2"
  shift 2
  TOTAL=$((TOTAL + 1))
  log "$name"
  if (cd "$dir" && "$@"); then
    ok "$name"
  else
    err "$name"
    FAILED=$((FAILED + 1))
  fi
}

TOTAL=0
FAILED=0

require_cmd cargo
require_cmd pnpm
require_cmd sed

run_step "cargo fmt --check" cargo fmt --check

if [[ "$MVP_SMOKE_CLIPPY" == "1" ]]; then
  run_step "cargo clippy" cargo clippy
else
  log "Skipping cargo clippy (set MVP_SMOKE_CLIPPY=1 to enable)"
fi

run_step "SQLX_OFFLINE=1 cargo check -p adapteros-server-api" env SQLX_OFFLINE=1 cargo check -p adapteros-server-api
run_step "cargo test -p adapteros-server-api -- --test-threads=1" cargo test -p adapteros-server-api -- --test-threads=1
run_step_in_dir "ui pnpm build" "ui" pnpm build

require_cmd curl

API_BASE="${AOS_API_URL%/}"

AUTH_ARGS=()
AUTH_MODE="none"
TMP_DIR=""

cleanup() {
  if [[ -n "$TMP_DIR" ]]; then
    rm -rf "$TMP_DIR" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

setup_auth() {
  if [[ "$MVP_SMOKE_DEV_BYPASS" == "1" ]]; then
    if ! command -v mktemp >/dev/null 2>&1; then
      warn "mktemp not available; skipping dev-bypass auth"
    else
      local tmp_root="${AOS_VAR_DIR:-$ROOT_DIR/var}/tmp"
      if [[ "$tmp_root" == /tmp* || "$tmp_root" == /private/tmp* ]]; then
        warn "Refusing temporary directory under /tmp: $tmp_root"
      else
        mkdir -p "$tmp_root"
        TMP_DIR="$(mktemp -d "${tmp_root}/mvp-smoke.XXXXXX")"
        local cookie_jar="$TMP_DIR/cookies.txt"
        local code
        code="$(curl -sS -o /dev/null -w "%{http_code}" -X POST -c "$cookie_jar" \
          "${API_BASE}/v1/auth/dev-bypass" || true)"
        if [[ "$code" == "200" ]]; then
          AUTH_ARGS=(-b "$cookie_jar")
          AUTH_MODE="dev-bypass"
          log "Auth: dev-bypass"
          return 0
        fi
        warn "Dev-bypass auth failed (status $code); continuing without it"
      fi
    fi
  fi

  if [[ -n "$MVP_SMOKE_TOKEN" ]]; then
    AUTH_ARGS=(-H "Authorization: Bearer $MVP_SMOKE_TOKEN")
    AUTH_MODE="token"
    log "Auth: bearer token"
    return 0
  fi

  AUTH_MODE="none"
  log "Auth: none (accepting 200/401/403 for endpoint checks)"
  return 0
}

http_code() {
  local method="$1"
  local url="$2"
  local body="${3:-}"
  local content_type="${4:-application/json}"

  local args=(curl -sS -o /dev/null -w "%{http_code}" -X "$method" "$url"
    --connect-timeout "$MVP_SMOKE_ENDPOINT_TIMEOUT_SECONDS"
    --max-time "$MVP_SMOKE_ENDPOINT_TIMEOUT_SECONDS"
    -H "Accept: application/json")
  if [[ -n "$body" ]]; then
    args+=(-H "Content-Type: $content_type" -d "$body")
  fi
  if ((${#AUTH_ARGS[@]})); then
    args+=("${AUTH_ARGS[@]}")
  fi

  "${args[@]}"
}

check_endpoint() {
  local method="$1"
  local url="$2"
  local expected_codes="$3"
  local body="${4:-}"
  local content_type="${5:-application/json}"

  local attempt=1
  local code="000"

  while (( attempt <= MVP_SMOKE_ENDPOINT_RETRIES )); do
    if code="$(http_code "$method" "$url" "$body" "$content_type" 2>/dev/null)"; then
      :
    else
      code="000"
    fi

    for expected in $expected_codes; do
      if [[ "$code" == "$expected" ]]; then
        printf "%s" "$code"
        return 0
      fi
    done

    if (( attempt < MVP_SMOKE_ENDPOINT_RETRIES )); then
      sleep "$MVP_SMOKE_ENDPOINT_DELAY_SECONDS"
    fi
    attempt=$((attempt + 1))
  done

  printf "%s" "$code"
  return 1
}

run_endpoint() {
  local label="$1"
  local method="$2"
  local url="$3"
  local expected_codes="$4"
  local body="${5:-}"
  local content_type="${6:-application/json}"
  local result_var="$7"

  TOTAL=$((TOTAL + 1))
  log "$label"
  local code
  if code="$(check_endpoint "$method" "$url" "$expected_codes" "$body" "$content_type")"; then
    ok "$label ($code)"
    printf -v "$result_var" "%s" "ok"
  else
    if [[ "$code" == "000" ]]; then
      err "backend not running - connection refused at $url"
    fi
    err "$label ($code)"
    printf -v "$result_var" "%s" "fail"
    FAILED=$((FAILED + 1))
  fi
}

setup_auth

SYSTEM_STATUS_RESULT="fail"
EVIDENCE_RESULT="fail"
DATASETS_UPLOAD_RESULT="fail"

expected_with_auth="200"
expected_without_auth="200 401 403"
expected_codes="$expected_without_auth"
if [[ "$AUTH_MODE" != "none" ]]; then
  expected_codes="$expected_with_auth"
fi

run_endpoint "system status endpoint" "GET" "${API_BASE}/v1/system/status" "$expected_codes" "" "" SYSTEM_STATUS_RESULT
run_endpoint "evidence endpoint" "GET" "${API_BASE}/v1/evidence?limit=1" "$expected_codes" "" "" EVIDENCE_RESULT

DATASET_BODY='{"file_name":"mvp-smoke.txt","total_size":1,"content_type":"text/plain"}'
run_endpoint "datasets upload endpoint" "POST" "${API_BASE}/v1/datasets/chunked-upload/initiate" \
  "$expected_codes" "$DATASET_BODY" "application/json" DATASETS_UPLOAD_RESULT

render_report() {
  local system_status="$1"
  local evidence="$2"
  local datasets="$3"

  if [[ ! -f "$MVP_REPORT_TEMPLATE" ]]; then
    err "Missing report template: $MVP_REPORT_TEMPLATE"
    return 1
  fi

  if [[ "$MVP_REPORT_PATH" == "$MVP_REPORT_TEMPLATE" ]]; then
    err "MVP_REPORT_PATH points to the template; refusing to overwrite."
    return 1
  fi

  mkdir -p "$(dirname "$MVP_REPORT_PATH")"

  sed \
    -e "s/^system status endpoint .*/system status endpoint ${system_status}/" \
    -e "s/^evidence endpoint .*/evidence endpoint ${evidence}/" \
    -e "s/^datasets upload endpoint .*/datasets upload endpoint ${datasets}/" \
    "$MVP_REPORT_TEMPLATE" > "$MVP_REPORT_PATH"
}

if render_report "$SYSTEM_STATUS_RESULT" "$EVIDENCE_RESULT" "$DATASETS_UPLOAD_RESULT"; then
  ok "report: $MVP_REPORT_PATH"
else
  err "report: failed to write"
  FAILED=$((FAILED + 1))
fi

log "Summary: total=${TOTAL} failed=${FAILED}"
if [[ "$FAILED" -gt 0 ]]; then
  exit 1
fi
exit 0
