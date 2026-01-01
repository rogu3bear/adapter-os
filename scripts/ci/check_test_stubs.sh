#!/usr/bin/env bash
# CI Guard: Detect empty test stubs that provide no value
# Prevents accumulation of placeholder tests that never get implemented.
#
# Exit codes:
#   0 - No empty stubs found
#   1 - Empty test stubs detected

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

echo "=== Empty Test Stub Detection ==="
echo ""

# Pattern: test files under 10 lines with only a stub function
STUB_FILES=()

while IFS= read -r -d '' file; do
    lines=$(wc -l < "$file")
    if [ "$lines" -lt 15 ]; then
        # Check if it's just a stub (has #[ignore] and empty body)
        if grep -q '#\[ignore\]' "$file" && grep -q 'fn test.*{}' "$file"; then
            STUB_FILES+=("$file")
        fi
    fi
done < <(find tests crates/*/tests -name "*.rs" -type f -print0 2>/dev/null)

if [ ${#STUB_FILES[@]} -gt 0 ]; then
    echo "❌ Found ${#STUB_FILES[@]} empty test stub files:"
    printf '  - %s\n' "${STUB_FILES[@]}"
    echo ""
    echo "These files contain no actual test logic. Either:"
    echo "  1. Implement the tests"
    echo "  2. Delete the stub files"
    echo ""
    exit 1
fi

echo "✓ No empty test stubs found"
echo ""
echo "=== Empty Test Stub Detection: PASSED ==="
