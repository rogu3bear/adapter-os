#!/usr/bin/env bash
# check_error_code_coverage.sh — verify that every canonical error code in
# adapteros-core/src/error_codes.rs has a user-facing mapping in the UI
# (user_message_for_code match) and that the CLI ExitCode mapping covers
# all AosError variants.
#
# Exit code 0 = all codes covered.  Exit code 1 = unmapped codes found.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

CORE_CODES="$ROOT_DIR/crates/adapteros-core/src/error_codes.rs"
CORE_ERROR="$ROOT_DIR/crates/adapteros-core/src/error.rs"
UI_ERROR="$ROOT_DIR/crates/adapteros-ui/src/api/error.rs"
CLI_ERROR="$ROOT_DIR/crates/adapteros-cli/src/error_codes.rs"

fail=0

# ---------------------------------------------------------------------------
# 1. Extract all pub const error code strings from core
# ---------------------------------------------------------------------------
CODES=$(rg --no-filename -o 'pub const [A-Z_]+: &str = "([A-Z_]+)"' -r '$1' "$CORE_CODES" | sort -u)

count=$(echo "$CODES" | wc -l | tr -d ' ')
if [[ "$count" -eq 0 ]]; then
  echo "ERROR: no error codes found in $CORE_CODES"
  exit 1
fi

echo "Found $count canonical error codes in error_codes.rs"

# ---------------------------------------------------------------------------
# 2. Check UI coverage (user_message_for_code match arms)
# ---------------------------------------------------------------------------
# The UI's user_message_for_code() match uses quoted string literals.
# Each canonical error code should appear as "CODE" in at least one arm.
# Codes handled by the catch-all (_ =>) are considered unmapped.
ui_content=$(cat "$UI_ERROR")
ui_unmapped=""
ui_count=0

while IFS= read -r code; do
  if ! echo "$ui_content" | rg -q "\"$code\""; then
    ui_unmapped="${ui_unmapped}  - ${code}
"
    ui_count=$((ui_count + 1))
  fi
done <<< "$CODES"

# ---------------------------------------------------------------------------
# 3. Check CLI coverage (AosError variants -> ExitCode)
# ---------------------------------------------------------------------------
# Extract all variant names from the AosError enum definition.
# We look for lines that define enum variants: they start with optional
# whitespace, then a PascalCase identifier, followed by (, {, or comma/newline.
# We exclude derive/attribute lines and documentation lines.
VARIANTS=$(
  rg --no-filename '^\s+[A-Z][a-zA-Z]+(\(|( \{))' "$CORE_ERROR" \
    | rg -v '^\s*(#|//|///|pub |use |impl |fn |let |mod |type |const )' \
    | rg --no-filename -o '^\s*([A-Z][a-zA-Z]+)' -r '$1' \
    | sort -u
)

cli_content=$(cat "$CLI_ERROR")
cli_unmapped=""
cli_count=0

while IFS= read -r variant; do
  [[ -z "$variant" ]] && continue
  if ! echo "$cli_content" | rg -q "AosError::${variant}"; then
    cli_unmapped="${cli_unmapped}  - AosError::${variant}
"
    cli_count=$((cli_count + 1))
  fi
done <<< "$VARIANTS"

# ---------------------------------------------------------------------------
# 4. Report results
# ---------------------------------------------------------------------------
if [[ $ui_count -gt 0 ]]; then
  echo ""
  echo "UNMAPPED in UI (user_message_for_code): $ui_count / $count codes"
  echo "These codes fall through to the catch-all and show raw error text."
  echo "$ui_unmapped"
  fail=1
fi

if [[ $cli_count -gt 0 ]]; then
  echo ""
  echo "UNMAPPED in CLI (ExitCode match): $cli_count variants"
  echo "These AosError variants fall through to ExitCode::Other."
  echo "$cli_unmapped"
  fail=1
fi

if [[ $fail -eq 1 ]]; then
  echo ""
  echo "FAIL: Some error codes lack UI or CLI coverage."
  echo "  UI mapping: crates/adapteros-ui/src/api/error.rs (user_message_for_code)"
  echo "  CLI mapping: crates/adapteros-cli/src/error_codes.rs (From<&AosError> for ExitCode)"
  exit 1
fi

echo "=== Error Code Coverage Check: PASSED ==="
