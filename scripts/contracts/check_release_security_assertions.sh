#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

AUTH_FILE="$ROOT_DIR/crates/adapteros-server-api/src/auth.rs"
ROUTES_FILE="$ROOT_DIR/crates/adapteros-server-api/src/routes/mod.rs"
SECURITY_DOC="$ROOT_DIR/docs/SECURITY.md"

fail() {
  echo "FAIL: $1"
  exit 1
}

require_match() {
  local pattern="$1"
  local file="$2"
  local msg="$3"
  if ! rg -q -- "$pattern" "$file"; then
    fail "$msg (pattern: $pattern in $file)"
  fi
}

is_truthy() {
  local v="${1:-}"
  local v_lc
  v_lc="$(printf '%s' "$v" | tr '[:upper:]' '[:lower:]')"
  case "$v_lc" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

require_match "AOS_DEV_NO_AUTH=1" "$SECURITY_DOC" "SECURITY doc must declare dev bypass control"
require_match "tenant_route_guard_middleware" "$SECURITY_DOC" "SECURITY doc must declare tenant guard control"

require_match "AOS_DEV_NO_AUTH detected in release build - this flag is ignored in production" "$AUTH_FILE" "Release auth path must ignore dev bypass"
require_match "AOS_DEV_NO_AUTH requested but AOS_PRODUCTION_MODE=1 is set" "$AUTH_FILE" "Auth path must block bypass in production mode"

tenant_guard_count="$(rg -c "tenant_route_guard_middleware" "$ROUTES_FILE")"
if [[ "$tenant_guard_count" -lt 2 ]]; then
  fail "tenant_route_guard_middleware must remain in protected route middleware chains"
fi

for flag in AOS_DEV_NO_AUTH AOS_DEV_SKIP_METALLIB_CHECK AOS_DEV_SKIP_DRIFT_CHECK; do
  value="${!flag:-}"
  if is_truthy "$value"; then
    fail "Release assertions reject active dev bypass flag: $flag=$value"
  fi
done

bash scripts/ci/dev_flag_release_guard.sh --check-only

echo "=== Release Security Assertions: PASSED ==="
