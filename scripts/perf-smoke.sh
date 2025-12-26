#!/usr/bin/env bash
#
# AdapterOS perf smoke test:
# - time to /readyz 200 from dev-up start
# - time for first inference call
#
# Constraints: bash + time + curl only (no jq/grep/sed/etc).
#
# Usage:
#   bash scripts/perf-smoke.sh
#
# Overrides (env):
#   AOS_SERVER_URL            (default: http://localhost:${AOS_SERVER_PORT:-8080})
#   PERF_SKIP_DEV_UP=1        (skip starting services)
#   PERF_DEV_UP_CMD           (override dev-up command; default uses scripts/service-manager.sh)
#   PERF_READY_TIMEOUT_S      (default: 120)
#   PERF_INFER_TIMEOUT_S      (default: 120)
#   PERF_READY_THRESHOLD_S    (default: 30)
#   PERF_INFER_THRESHOLD_S    (default: 60)
#   PERF_USERNAME/PERF_PASSWORD (default: admin/admin) for optional login
#

set -u

log() { printf "[perf-smoke] %s\n" "$*"; }
warn() { printf "[perf-smoke] WARN: %s\n" "$*" >&2; }
err() { printf "[perf-smoke] ERROR: %s\n" "$*" >&2; }

SCRIPT_PATH="${BASH_SOURCE[0]}"
SCRIPT_DIR="${SCRIPT_PATH%/*}"
if [[ "$SCRIPT_DIR" == "$SCRIPT_PATH" ]]; then
  SCRIPT_DIR="."
fi
ROOT_DIR="${SCRIPT_DIR}/.."
cd "$ROOT_DIR" || exit 1

: "${AOS_SERVER_PORT:=8080}"
: "${AOS_SERVER_URL:=http://localhost:${AOS_SERVER_PORT}}"
API_BASE="${AOS_SERVER_URL%/}/api"

: "${PERF_READY_TIMEOUT_S:=120}"
: "${PERF_INFER_TIMEOUT_S:=120}"

: "${PERF_READY_THRESHOLD_S:=30}"
: "${PERF_INFER_THRESHOLD_S:=60}"

: "${PERF_USERNAME:=admin}"
: "${PERF_PASSWORD:=admin}"

: "${PERF_SKIP_DEV_UP:=0}"

ready_url="${API_BASE}/readyz"
infer_url="${API_BASE}/v1/infer"
login_url="${API_BASE}/v1/auth/login"

seconds_to_ms() {
  local s="${1:-}"
  s="${s//$'\n'/}"
  s="${s//$'\r'/}"

  local int="${s%%.*}"
  local frac=""
  if [[ "$s" == *.* ]]; then
    frac="${s#*.}"
  fi

  if [[ -z "$int" || ! "$int" =~ ^[0-9]+$ ]]; then
    int="0"
  fi

  frac="${frac}000"
  frac="${frac:0:3}"
  if [[ ! "$frac" =~ ^[0-9]{3}$ ]]; then
    frac="000"
  fi

  printf "%s" "$((10#$int * 1000 + 10#$frac))"
}

fmt_ms_s() {
  local ms="${1:-0}"
  printf "%d.%03d" $((ms / 1000)) $((ms % 1000))
}

measure_ms() {
  local out="" status=0
  out="$(
    {
      TIMEFORMAT='%3R'
      time "$@"
    } 2>&1
  )" || status=$?
  if (( status != 0 )); then
    return "$status"
  fi

  out="${out//$'\n'/}"
  out="${out//$'\r'/}"
  seconds_to_ms "$out"
}

dev_up() {
  if [[ "$PERF_SKIP_DEV_UP" == "1" ]]; then
    return 0
  fi

  if [[ -n "${PERF_DEV_UP_CMD:-}" ]]; then
    eval "$PERF_DEV_UP_CMD" >/dev/null 2>&1
    return 0
  fi

  if [[ ! -x "./scripts/service-manager.sh" ]]; then
    err "Missing ./scripts/service-manager.sh (set PERF_DEV_UP_CMD to override)"
    return 1
  fi

  ./scripts/service-manager.sh start backend >/dev/null 2>&1
  ./scripts/service-manager.sh start worker >/dev/null 2>&1
}

wait_ready_200() {
  local retries="${PERF_READY_TIMEOUT_S%%.*}"
  if (( retries < 1 )); then
    retries=1
  fi

  curl -s -o /dev/null \
    --connect-timeout 1 \
    --max-time 2 \
    --retry "$retries" \
    --retry-delay 1 \
    --retry-connrefused \
    --retry-all-errors \
    --retry-max-time "${PERF_READY_TIMEOUT_S%%.*}" \
    --fail \
    "$ready_url"
}

start_and_wait_ready() {
  dev_up
  wait_ready_200
}

maybe_login() {
  if [[ -n "${AOS_TOKEN:-}" ]]; then
    return 0
  fi

  local payload
  payload='{"username":"'"$PERF_USERNAME"'","password":"'"$PERF_PASSWORD"'"}'

  local out="" body="" code="" rest="" token=""
  out="$(curl -s \
    --connect-timeout 2 \
    --max-time 10 \
    -H "Content-Type: application/json" \
    -d "$payload" \
    -w $'\n%{http_code}' \
    "$login_url" 2>/dev/null || true)"

  code="${out##*$'\n'}"
  body="${out%$'\n'*}"

  if [[ "$code" != "200" ]]; then
    return 1
  fi

  rest="${body#*\"token\"}"
  if [[ "$rest" == "$body" ]]; then
    return 1
  fi
  rest="${rest#*\"}"
  token="${rest%%\"*}"
  if [[ -z "$token" ]]; then
    return 1
  fi

  AOS_TOKEN="$token"
  return 0
}

infer_once() {
  local payload='{"prompt":"Hello","max_tokens":8,"temperature":0,"adapters":[]}'

  local auth_args=()
  if [[ -n "${AOS_TOKEN:-}" ]]; then
    auth_args=(-H "Authorization: Bearer ${AOS_TOKEN}")
  fi

  curl -s -o /dev/null \
    --connect-timeout 2 \
    --max-time "$PERF_INFER_TIMEOUT_S" \
    -H "Content-Type: application/json" \
    "${auth_args[@]}" \
    -d "$payload" \
    --fail \
    "$infer_url"
}

main() {
  if ! command -v curl >/dev/null 2>&1; then
    err "Missing required command: curl"
    exit 1
  fi

  local ready_threshold_ms infer_threshold_ms
  ready_threshold_ms="$(seconds_to_ms "$PERF_READY_THRESHOLD_S")"
  infer_threshold_ms="$(seconds_to_ms "$PERF_INFER_THRESHOLD_S")"

  log "server: $AOS_SERVER_URL (api: $API_BASE)"
  log "thresholds: ready<=$(fmt_ms_s "$ready_threshold_ms")s infer<=$(fmt_ms_s "$infer_threshold_ms")s"

  local ready_ms=0 infer_ms=0

  if ! ready_ms="$(measure_ms start_and_wait_ready)"; then
    err "Failed waiting for /readyz 200 (${ready_url})"
    exit 1
  fi

  log "time_to_ready: $(fmt_ms_s "$ready_ms")s"

  if (( ready_ms > ready_threshold_ms )); then
    warn "time_to_ready exceeded threshold ($(fmt_ms_s "$ready_ms")s > $(fmt_ms_s "$ready_threshold_ms")s)"
  fi

  maybe_login >/dev/null 2>&1 || true

  if ! infer_ms="$(measure_ms infer_once)"; then
    err "First inference failed (${infer_url}). If auth is enabled, set AOS_TOKEN or PERF_USERNAME/PERF_PASSWORD."
    exit 1
  fi

  log "first_infer: $(fmt_ms_s "$infer_ms")s"

  if (( infer_ms > infer_threshold_ms )); then
    warn "first_infer exceeded threshold ($(fmt_ms_s "$infer_ms")s > $(fmt_ms_s "$infer_threshold_ms")s)"
  fi
}

main "$@"
