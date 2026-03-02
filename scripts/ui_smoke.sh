#!/usr/bin/env bash
set -Eeuo pipefail

# adapterOS UI smoke checks (static + API).
# - Verifies UI root serves built assets (or dev server index.html)
# - Verifies API readiness + meta endpoint
# - Optionally performs dev-bypass and checks authenticated MVP endpoints

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

: "${AOS_UI_URL:=http://localhost:${AOS_SERVER_PORT:-18080}}"
: "${AOS_API_URL:=http://localhost:${AOS_SERVER_PORT:-18080}/api}"
: "${SMOKE_CONNECT_TIMEOUT_SECONDS:=2}"
: "${SMOKE_HTTP_TIMEOUT_SECONDS:=10}"
: "${SMOKE_DEV_BYPASS:=0}" # If 1, require /v1/auth/dev-bypass and run authed checks

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log() { printf "[ui_smoke] %s\n" "$*"; }
ok() { printf "[ui_smoke] ${GREEN}✓${NC} %s\n" "$*"; }
warn() { printf "[ui_smoke] ${YELLOW}!${NC} %s\n" "$*" >&2; }
err() { printf "[ui_smoke] ${RED}✗${NC} %s\n" "$*" >&2; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || { err "Missing required command: $1"; exit 1; }
}

require_cmd curl
require_cmd grep
require_cmd sed

TMP_ROOT="${AOS_VAR_DIR:-$ROOT_DIR/var}/tmp"
if [[ "$TMP_ROOT" == /tmp* || "$TMP_ROOT" == /private/tmp* ]]; then
  err "Refusing temporary directory under /tmp: $TMP_ROOT"
  exit 1
fi
mkdir -p "$TMP_ROOT"

mktemp_dir() {
  mktemp -d "${TMP_ROOT}/aos-ui-smoke.XXXXXX"
}

TMP_DIR="$(mktemp_dir)"
COOKIE_JAR="$TMP_DIR/cookies.txt"
INDEX_HTML="$TMP_DIR/index.html"
DEV_BYPASS_BODY="$TMP_DIR/dev_bypass.json"

cleanup() {
  rm -rf "$TMP_DIR" >/dev/null 2>&1 || true
}
trap cleanup EXIT

TOTAL=0
FAILED=0

http_code() {
  curl -sS -o /dev/null \
    --connect-timeout "$SMOKE_CONNECT_TIMEOUT_SECONDS" \
    --max-time "$SMOKE_HTTP_TIMEOUT_SECONDS" \
    -w "%{http_code}" \
    "$@"
}

expect_code() {
  local name="$1"
  local expected="$2"
  shift 2
  TOTAL=$((TOTAL + 1))
  local code="000"
  if code="$(http_code "$@")"; then
    :
  fi
  if [[ "$code" == "$expected" ]]; then
    ok "${name} (${code})"
  else
    err "${name} (expected ${expected}, got ${code})"
    FAILED=$((FAILED + 1))
  fi
}

expect_code_any() {
  local name="$1"
  shift 1
  local expected_raw="$1"
  shift 1
  local expected_list=()
  # shellcheck disable=SC2206
  expected_list=($expected_raw)
  TOTAL=$((TOTAL + 1))
  local code="000"
  if code="$(http_code "$@")"; then
    :
  fi
  for expected in "${expected_list[@]}"; do
    if [[ "$code" == "$expected" ]]; then
      ok "${name} (${code})"
      return 0
    fi
  done
  err "${name} (expected one of: ${expected_list[*]}, got ${code})"
  FAILED=$((FAILED + 1))
  return 1
}

log "UI URL:  ${AOS_UI_URL}"
log "API URL: ${AOS_API_URL}"

# -----------------------------------------------------------------------------
# UI: index + assets
# -----------------------------------------------------------------------------

log "UI: fetching index.html"
TOTAL=$((TOTAL + 1))
if curl -fsS \
  --connect-timeout "$SMOKE_CONNECT_TIMEOUT_SECONDS" \
  --max-time "$SMOKE_HTTP_TIMEOUT_SECONDS" \
  -o "$INDEX_HTML" \
  "${AOS_UI_URL%/}/"; then
  ok "UI root served index.html"
else
  err "UI root did not serve index.html"
  FAILED=$((FAILED + 1))
fi

JS_ASSET="$(grep -oE '/assets/[^\" ]+\\.js' "$INDEX_HTML" | head -n 1 || true)"
CSS_ASSET="$(grep -oE '/assets/[^\" ]+\\.css' "$INDEX_HTML" | head -n 1 || true)"

if [[ -n "$JS_ASSET" ]]; then
  expect_code_any "UI JS asset" "200 304" -I "${AOS_UI_URL%/}${JS_ASSET}"
else
  warn "UI JS asset not found in index.html (dev server or non-standard build?)"
fi

if [[ -n "$CSS_ASSET" ]]; then
  expect_code_any "UI CSS asset" "200 304" -I "${AOS_UI_URL%/}${CSS_ASSET}"
else
  warn "UI CSS asset not found in index.html (dev server or non-standard build?)"
fi

# -----------------------------------------------------------------------------
# API: readiness + meta
# -----------------------------------------------------------------------------

expect_code "API /readyz" "200" "${AOS_API_URL%/}/readyz"
expect_code "API /v1/meta" "200" "${AOS_API_URL%/}/v1/meta"

# -----------------------------------------------------------------------------
# Optional: dev bypass auth + MVP endpoints
# -----------------------------------------------------------------------------

if [[ "$SMOKE_DEV_BYPASS" == "1" ]]; then
  log "auth: dev-bypass"
  TOTAL=$((TOTAL + 1))
  DEV_BYPASS_CODE="$(curl -sS \
    --connect-timeout "$SMOKE_CONNECT_TIMEOUT_SECONDS" \
    --max-time "$SMOKE_HTTP_TIMEOUT_SECONDS" \
    -X POST \
    -c "$COOKIE_JAR" \
    -o "$DEV_BYPASS_BODY" \
    -w "%{http_code}" \
    "${AOS_API_URL%/}/v1/auth/dev-bypass" || true)"

  if [[ "$DEV_BYPASS_CODE" == "200" ]]; then
    ok "dev-bypass login (${DEV_BYPASS_CODE})"
  else
    err "dev-bypass login failed (expected 200, got ${DEV_BYPASS_CODE})"
    FAILED=$((FAILED + 1))
  fi

  expect_code "auth /v1/auth/me" "200" -b "$COOKIE_JAR" "${AOS_API_URL%/}/v1/auth/me"

  # MVP surfaces
  expect_code "adapters list /v1/adapters" "200" -b "$COOKIE_JAR" "${AOS_API_URL%/}/v1/adapters"
  expect_code "datasets list /v1/datasets" "200" -b "$COOKIE_JAR" "${AOS_API_URL%/}/v1/datasets"

  if grep -q $'\tcsrf_token\t' "$COOKIE_JAR" 2>/dev/null; then
    ok "csrf_token cookie present"
  else
    warn "csrf_token cookie missing (mutations may fail behind some proxies)"
  fi
fi

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------

log "Summary: total=${TOTAL} failed=${FAILED}"
if [[ "$FAILED" -gt 0 ]]; then
  exit 1
fi
exit 0
