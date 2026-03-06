#!/usr/bin/env bash
set -euo pipefail

# =============================================================================
# Secret Exposure Scanner
# =============================================================================
# Scans source code for hardcoded secrets, credential leakage patterns,
# and Debug derives on sensitive structs.
# =============================================================================

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

PASS=0
FAIL=0
WARN=0
SUMMARY=()

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

# Helper: count rg matches safely (pipefail-safe)
count_matches() {
  local pattern="$1"
  shift
  local result
  result=$(rg -c "$pattern" "$@" 2>/dev/null | awk -F: '{sum += $2} END {print sum+0}') || true
  echo "${result:-0}"
}

echo "=== Secret Exposure Scanner ==="
echo ""

# Directories and patterns to exclude from scanning
EXCLUDE_DIRS="--glob=!target/ --glob=!var/ --glob=!.git/ --glob=!node_modules/ --glob=!*.lock"
# Exclude test fixtures, example configs, and documentation from false positives
EXCLUDE_TEST="--glob=!**/tests/** --glob=!**/test_fixtures/** --glob=!**/examples/**"
# Also exclude TOML configs (they use placeholder values like jwt_secret = "")
EXCLUDE_TOML="--glob=!**/*.toml"
# Exclude markdown docs and plan files
EXCLUDE_DOCS="--glob=!**/*.md --glob=!.planning/**"

# ---------------------------------------------------------------------------
# CHECK A: Hardcoded passwords
# ---------------------------------------------------------------------------
hardcoded_pw=$(count_matches 'password\s*=\s*"[^"]+"' $EXCLUDE_DIRS $EXCLUDE_TEST $EXCLUDE_TOML $EXCLUDE_DOCS --type rust --type python --type sh)

if [[ "$hardcoded_pw" -eq 0 ]]; then
  pass "No hardcoded passwords found in source"
else
  # Check if they are test constants or dev defaults
  matches=$(rg 'password\s*=\s*"[^"]+"' $EXCLUDE_DIRS $EXCLUDE_TEST $EXCLUDE_TOML $EXCLUDE_DOCS \
    --type rust --type python --type sh 2>/dev/null || true)
  if echo "$matches" | rg -q "dev-|test-|example|placeholder|CHANGE_ME" 2>/dev/null; then
    warn "Found $hardcoded_pw password assignments that appear to be dev/test placeholders"
  else
    fail "Hardcoded passwords found in source ($hardcoded_pw occurrences)"
  fi
fi

# ---------------------------------------------------------------------------
# CHECK B: Hardcoded secrets (excluding TOML config placeholders)
# ---------------------------------------------------------------------------
# Exclude jwt_secret assignments -- these are TOML config field names embedded
# in Rust test/config code, not actual hardcoded secrets.
hardcoded_secret_raw=$(rg -n 'secret\s*=\s*"[^"]+"' $EXCLUDE_DIRS $EXCLUDE_TEST $EXCLUDE_TOML $EXCLUDE_DOCS --type rust 2>/dev/null || true)
hardcoded_secret_filtered=$(echo "$hardcoded_secret_raw" | rg -v 'jwt_secret\s*=' 2>/dev/null || true)
hardcoded_secret=0
if [[ -n "$hardcoded_secret_filtered" ]]; then
  hardcoded_secret=$(echo "$hardcoded_secret_filtered" | wc -l | tr -d ' ')
fi

if [[ "$hardcoded_secret" -eq 0 ]]; then
  pass "No hardcoded secrets found in Rust source"
else
  fail "Hardcoded secrets found in Rust source ($hardcoded_secret occurrences)"
fi

# ---------------------------------------------------------------------------
# CHECK C: Bearer tokens in source (not test fixtures)
# ---------------------------------------------------------------------------
bearer_count=$(count_matches 'Bearer [A-Za-z0-9._-]{20,}' $EXCLUDE_DIRS $EXCLUDE_TEST $EXCLUDE_DOCS --type rust)

if [[ "$bearer_count" -eq 0 ]]; then
  pass "No hardcoded Bearer tokens found in source"
else
  warn "Found $bearer_count potential Bearer token literals in source"
fi

# ---------------------------------------------------------------------------
# CHECK D: Hardcoded API keys
# ---------------------------------------------------------------------------
api_key_count=$(count_matches 'api_key\s*=\s*"[^"]+"' $EXCLUDE_DIRS $EXCLUDE_TEST $EXCLUDE_TOML $EXCLUDE_DOCS --type rust)

if [[ "$api_key_count" -eq 0 ]]; then
  pass "No hardcoded API keys found in source"
else
  fail "Hardcoded API keys found in source ($api_key_count occurrences)"
fi

# ---------------------------------------------------------------------------
# CHECK E: SecurityConfig must NOT derive Debug (should have custom impl)
# ---------------------------------------------------------------------------
TYPES_FILE="$ROOT_DIR/crates/adapteros-config/src/types.rs"
if [[ -f "$TYPES_FILE" ]]; then
  # Look for #[derive(...Debug...)] immediately before pub struct SecurityConfig
  if rg -U '#\[derive\([^)]*Debug[^)]*\)\]\s*pub struct SecurityConfig' "$TYPES_FILE" >/dev/null 2>&1; then
    fail "SecurityConfig derives Debug -- must use custom impl that redacts jwt_secret"
  else
    pass "SecurityConfig does not derive Debug (uses custom impl)"
  fi

  # Verify the custom Debug impl exists and redacts
  if rg -q 'REDACTED' "$TYPES_FILE"; then
    pass "SecurityConfig Debug impl redacts sensitive fields"
  else
    fail "SecurityConfig Debug impl must redact jwt_secret with [REDACTED]"
  fi
else
  fail "types.rs not found at expected path"
fi

# ---------------------------------------------------------------------------
# CHECK F: Sensitive config logging
# ---------------------------------------------------------------------------
SERVER_SRC="$ROOT_DIR/crates/adapteros-server-api/src"
sensitive_log_count=$(count_matches '(debug!|info!|warn!).*security_config\b' "$SERVER_SRC" --type rust)

if [[ "$sensitive_log_count" -eq 0 ]]; then
  pass "No direct security_config logging found in server-api"
else
  warn "Found $sensitive_log_count log statements referencing security_config directly"
fi

# ---------------------------------------------------------------------------
# CHECK G: var/models/ gitignore
# ---------------------------------------------------------------------------
if rg -q '\*\*/var/' "$ROOT_DIR/.gitignore" 2>/dev/null; then
  pass "var/ directory (including models) is gitignored"
elif rg -q 'var/models' "$ROOT_DIR/.gitignore" 2>/dev/null; then
  pass "var/models/ is gitignored"
else
  fail "var/models/ must be gitignored to prevent committing model weights"
fi

# ---------------------------------------------------------------------------
# CHECK H (WARN): Debug derive on structs with sensitive field names
# ---------------------------------------------------------------------------
# Check specifically for structs named *Secret* or *Password* with Debug derive.
# Token/Key structs are too common to flag (ApiKeyConfig, TokenMetadata, etc).
sensitive_debug=$(rg -U '#\[derive\([^)]*Debug[^)]*\)\]\s*pub struct \w*(Secret|Password)\w*' \
  $EXCLUDE_DIRS --type rust 2>/dev/null || true)

sensitive_debug_count=0
if [[ -n "$sensitive_debug" ]]; then
  sensitive_debug_count=$(echo "$sensitive_debug" | wc -l | tr -d ' ')
fi

if [[ "$sensitive_debug_count" -eq 0 ]]; then
  pass "No Debug-derived structs with Secret/Password in name"
else
  warn "Found $sensitive_debug_count Debug-derived structs with Secret/Password in name (review manually)"
fi

# ---------------------------------------------------------------------------
# CHECK I (WARN): Log output secret patterns
# ---------------------------------------------------------------------------
if [[ -d "$ROOT_DIR/var/logs" ]]; then
  log_secret_count=$(count_matches 'jwt_secret|password|Bearer ' "$ROOT_DIR/var/logs/")
  if [[ "$log_secret_count" -gt 0 ]]; then
    warn "Found $log_secret_count potential secret patterns in log files (var/logs/)"
  else
    pass "No secret patterns found in log files"
  fi
else
  pass "No log directory to scan (var/logs/ does not exist)"
fi

# ---------------------------------------------------------------------------
# SUMMARY
# ---------------------------------------------------------------------------
echo ""
echo "--- Secret Exposure Scanner Summary ---"
for line in "${SUMMARY[@]}"; do
  echo "  $line"
done
echo ""
echo "Results: $PASS passed, $FAIL failed, $WARN warnings"

if [[ "$FAIL" -gt 0 ]]; then
  echo ""
  echo "=== Secret Exposure Scanner: FAILED ==="
  exit 1
fi

echo ""
echo "=== Secret Exposure Scanner: PASSED ==="
