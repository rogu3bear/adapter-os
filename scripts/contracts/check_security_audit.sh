#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Security Audit Contract Check
# =============================================================================
# Validates security invariants across the API surface:
#   - Auth enforcement on protected routes
#   - Dev bypass production guard
#   - Rate limiting coverage
#   - Input validation (typed request bodies)
#   - CSRF protection on protected routes
#   - cargo-audit advisory scan (if installed)
# =============================================================================

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

PASS=0
FAIL=0
WARN=0
SUMMARY=()

KNOWN_FINDINGS_FILE="$ROOT_DIR/scripts/contracts/KNOWN_FINDINGS.md"

pass() {
  PASS=$((PASS + 1))
  SUMMARY+=("PASS: $1")
}

fail() {
  FAIL=$((FAIL + 1))
  SUMMARY+=("FAIL: $1")
}

warn() {
  WARN=$((WARN + 1))
  SUMMARY+=("WARN: $1")
}

is_known_finding() {
  local finding_id="$1"
  if [[ -f "$KNOWN_FINDINGS_FILE" ]] && grep -q "$finding_id" "$KNOWN_FINDINGS_FILE"; then
    return 0
  fi
  return 1
}

# Helper: count rg matches safely (pipefail-safe)
count_matches() {
  local pattern="$1"
  shift
  # Run rg in a subshell so pipefail doesn't kill us on zero matches
  local result
  result=$(rg -c "$pattern" "$@" 2>/dev/null | awk -F: '{sum += $2} END {print sum+0}') || true
  echo "${result:-0}"
}

echo "=== Security Audit Contract Check ==="
echo ""

# ---------------------------------------------------------------------------
# CHECK A: Auth enforcement on protected routes
# ---------------------------------------------------------------------------
ROUTES_FILE="$ROOT_DIR/crates/adapteros-server-api/src/routes/mod.rs"
if rg -q "auth_middleware" "$ROUTES_FILE"; then
  pass "Auth middleware referenced in route builder"
else
  fail "auth_middleware not found in routes/mod.rs"
fi

# Verify tenant guard is applied (minimum 2 references: import + usage)
tenant_guard_count=$(rg -c "tenant_route_guard_middleware" "$ROUTES_FILE" || echo "0")
if [[ "$tenant_guard_count" -ge 2 ]]; then
  pass "Tenant guard middleware applied to protected routes ($tenant_guard_count references)"
else
  fail "tenant_route_guard_middleware must be applied in protected route chains (found: $tenant_guard_count)"
fi

# ---------------------------------------------------------------------------
# CHECK B: Dev bypass production guard
# ---------------------------------------------------------------------------
AUTH_FILE="$ROOT_DIR/crates/adapteros-server-api/src/auth.rs"
if rg -q "AOS_DEV_NO_AUTH detected in release build" "$AUTH_FILE"; then
  pass "Dev bypass is blocked in release builds"
else
  fail "Release auth path must reject AOS_DEV_NO_AUTH in release mode"
fi

if rg -q "AOS_DEV_NO_AUTH requested but AOS_PRODUCTION_MODE" "$AUTH_FILE"; then
  pass "Dev bypass is blocked when production mode is active"
else
  fail "Auth path must block dev bypass when AOS_PRODUCTION_MODE is set"
fi

# ---------------------------------------------------------------------------
# CHECK C: Rate limiting coverage
# ---------------------------------------------------------------------------
if rg -q "rate_limiting_middleware" "$ROUTES_FILE"; then
  pass "Rate limiting middleware in global middleware chain"
else
  fail "rate_limiting_middleware not found in routes/mod.rs middleware chain"
fi

# Verify per-tier rate limiting exists
MIDDLEWARE_SECURITY="$ROOT_DIR/crates/adapteros-server-api/src/middleware_security.rs"
if rg -q "RouteTier" "$MIDDLEWARE_SECURITY"; then
  pass "Per-tier rate limiting classification implemented"
else
  fail "RouteTier enum not found in middleware_security.rs"
fi

if rg -q 'event = "rate_limit_exceeded"' "$MIDDLEWARE_SECURITY"; then
  pass "Structured rate limit exceeded audit logging is present"
else
  fail "Structured rate limit exceeded logging (event = \"rate_limit_exceeded\") missing"
fi

MIDDLEWARE_AUTH="$ROOT_DIR/crates/adapteros-server-api/src/middleware/mod.rs"
if rg -q 'event = "auth_failure"' "$MIDDLEWARE_AUTH"; then
  pass "Structured auth failure audit logging is present"
else
  fail "Structured auth failure logging (event = \"auth_failure\") missing"
fi

if rg -q 'event = "input_validation_failure"' "$MIDDLEWARE_SECURITY"; then
  pass "Structured input validation failure logging is present"
else
  warn "Structured input validation failure logging not found"
fi

# ---------------------------------------------------------------------------
# CHECK D: Input validation (typed request bodies)
# ---------------------------------------------------------------------------
HANDLER_DIR="$ROOT_DIR/crates/adapteros-server-api/src/handlers"
untyped_count=0
if [[ -d "$HANDLER_DIR" ]]; then
  untyped_count=$(count_matches "Json<serde_json::Value>|Json<String>" "$HANDLER_DIR" --type rust)
fi

if [[ "$untyped_count" -eq 0 ]]; then
  pass "All POST/PUT/PATCH handlers use typed request bodies"
elif is_known_finding "INPUT-VAL-01"; then
  warn "Untyped request bodies found ($untyped_count occurrences) -- documented in KNOWN_FINDINGS.md"
else
  warn "Untyped request bodies (Json<Value>/Json<String>) found: $untyped_count occurrences"
fi

# ---------------------------------------------------------------------------
# CHECK E: CSRF protection on protected routes
# ---------------------------------------------------------------------------
if rg -q "csrf_middleware" "$ROUTES_FILE"; then
  pass "CSRF middleware applied to protected routes"
else
  fail "csrf_middleware not found in routes/mod.rs for protected routes"
fi

# ---------------------------------------------------------------------------
# CHECK F (WARN): Missing #[validate] annotations
# ---------------------------------------------------------------------------
validate_count=$(count_matches "#\[validate\]" "$HANDLER_DIR" --type rust)
if [[ "$validate_count" -gt 0 ]]; then
  pass "Found $validate_count #[validate] annotations on request structs"
else
  warn "No #[validate] annotations found on request body structs (informational)"
fi

# ---------------------------------------------------------------------------
# CHECK G (WARN): Overly broad rate limit exemptions
# ---------------------------------------------------------------------------
exempt_entries=$(rg "RATE_LIMIT_EXEMPT_PATHS" -A 20 "$MIDDLEWARE_SECURITY" 2>/dev/null | rg -c "\"/" 2>/dev/null || echo "0")
if [[ "$exempt_entries" -le 5 ]]; then
  pass "Rate limit exempt list is narrow ($exempt_entries entries)"
else
  warn "Rate limit exempt list may be too broad ($exempt_entries entries)"
fi

# ---------------------------------------------------------------------------
# CHECK H: var/models permission hardening (if present)
# ---------------------------------------------------------------------------
MODELS_DIR="$ROOT_DIR/var/models"
if [[ -d "$MODELS_DIR" ]]; then
  stat_mode() {
    local target="$1"
    stat -f "%Lp" "$target" 2>/dev/null || stat -c "%a" "$target" 2>/dev/null || echo "unknown"
  }

  root_mode=$(stat_mode "$MODELS_DIR")
  if [[ "$root_mode" == "700" ]]; then
    pass "var/models root directory permissions are 0700"
  else
    fail "var/models root permissions must be 0700 (found: $root_mode)"
  fi

  bad_dir_count=0
  while IFS= read -r -d '' dir; do
    mode=$(stat_mode "$dir")
    if [[ "$mode" != "700" ]]; then
      bad_dir_count=$((bad_dir_count + 1))
    fi
  done < <(find "$MODELS_DIR" -type d -print0)
  if [[ "$bad_dir_count" -eq 0 ]]; then
    pass "All model directories use 0700 permissions"
  else
    fail "Found $bad_dir_count model directories without 0700 permissions"
  fi

  bad_file_count=0
  while IFS= read -r -d '' file; do
    mode=$(stat_mode "$file")
    if [[ "$mode" != "600" ]]; then
      bad_file_count=$((bad_file_count + 1))
    fi
  done < <(find "$MODELS_DIR" -type f -print0)
  if [[ "$bad_file_count" -eq 0 ]]; then
    pass "All model files use 0600 permissions"
  else
    fail "Found $bad_file_count model files without 0600 permissions"
  fi
else
  warn "var/models not present; skipping permission checks"
fi

# ---------------------------------------------------------------------------
# CARGO AUDIT (WARN): Advisory scan
# ---------------------------------------------------------------------------
if command -v cargo-audit &>/dev/null; then
  advisory_output=$(cargo audit 2>&1 || true)
  advisory_count=$(echo "$advisory_output" | rg -c "^warning\[" 2>/dev/null || echo "0")
  if [[ "$advisory_count" -eq 0 ]]; then
    pass "cargo-audit found no advisories"
  else
    warn "cargo-audit found $advisory_count advisories (run 'cargo audit' for details)"
  fi
else
  warn "cargo-audit not installed -- skipping advisory scan (install: cargo install cargo-audit)"
fi

# ---------------------------------------------------------------------------
# SUMMARY
# ---------------------------------------------------------------------------
echo ""
echo "--- Security Audit Summary ---"
for line in "${SUMMARY[@]}"; do
  echo "  $line"
done
echo ""
echo "Results: $PASS passed, $FAIL failed, $WARN warnings"

if [[ "$FAIL" -gt 0 ]]; then
  echo ""
  echo "=== Security Audit Contract Check: FAILED ==="
  exit 1
fi

echo ""
echo "=== Security Audit Contract Check: PASSED ==="
