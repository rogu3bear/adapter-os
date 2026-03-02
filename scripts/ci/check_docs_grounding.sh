#!/usr/bin/env bash
# CI Gate: Detect documentation patterns that cause AI/agent hallucinations
#
# See docs/DOCS_GROUNDING.md for rationale and forbidden patterns.
#
# Usage:
#   ./scripts/ci/check_docs_grounding.sh   # Exit 1 if any forbidden pattern found

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"
DOCS="$ROOT_DIR/docs"

FAILED=0

# Exclude meta-docs that document the forbidden patterns
EXCLUDE="DOCS_GROUNDING\.md|DEPRECATIONS\.md"

check() {
    local name="$1"
    local pattern="$2"
    local matches
    matches=$(rg -l "$pattern" "$DOCS" 2>/dev/null | grep -v -E "$EXCLUDE" || true)
    if [[ -n "$matches" ]]; then
        echo "FAIL: $name"
        echo "$matches" | sed 's/^/  /'
        FAILED=1
    else
        echo "OK: $name"
    fi
}

echo "=== Documentation Grounding Check ==="
echo ""

check "No adapteros-orchestrator for db migrate/init-tenant/config show" \
    "adapteros-orchestrator.*(db migrate|init-tenant|config show)"

check "No cd ui && pnpm" \
    "cd ui && pnpm"

check "No false React/pnpm claim for main UI" \
    "React-based using pnpm"

check "No download_model.sh (use download-model.sh)" \
    "scripts/download_model\.sh"

check "No http://localhost:18080/api/v1/ (use /v1/)" \
    "http://localhost:18080/api/v1/"

echo ""
if [[ $FAILED -eq 1 ]]; then
    echo "See docs/DOCS_GROUNDING.md for fixes."
    exit 1
fi
echo "All grounding checks passed."
