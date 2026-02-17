#!/usr/bin/env bash
# validate-docs.sh
#
# Canonical docs validation entrypoint.
# Default: run contract-based claims checks.
# Optional: run legacy heuristic checks with --legacy.
#
# Usage:
#   ./scripts/validate-docs.sh
#   ./scripts/validate-docs.sh --legacy

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RUN_LEGACY=false
for arg in "$@"; do
    case "$arg" in
        --legacy)
            RUN_LEGACY=true
            ;;
        --help|-h)
            cat <<'USAGE'
Usage: ./scripts/validate-docs.sh [--legacy]

Options:
  --legacy   Also run legacy heuristic checks.
USAGE
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

echo "=== Docs Validation (Canonical) ==="
scripts/contracts/check_docs_claims.sh

if [[ "$RUN_LEGACY" != "true" ]]; then
    echo "=== Legacy Checks Skipped (pass --legacy to run) ==="
    exit 0
fi

echo "=== Docs Validation (Legacy Heuristics) ==="
ERRORS=0

report_error() {
    echo "FAIL: $1"
    ERRORS=$((ERRORS + 1))
}

report_success() {
    echo "OK: $1"
}

echo "1. Policy pack count consistency"
# Canonical count lives in PolicyId::all() in registry.rs, not file-count in
# packs/ (which can include helper modules).
CODE_POLICY_COUNT=$(rg -o 'pub fn all\(\) -> .*\[PolicyId; ([0-9]+)\]' \
    crates/adapteros-policy/src/registry.rs -r '$1' | head -1 || true)
DOCS_POLICY_COUNT=$(grep -r "policy packs" docs/ | grep -v archive/ | grep -o "[0-9]\+ policy packs" | head -1 | cut -d' ' -f1 || true)

if [[ -z "$CODE_POLICY_COUNT" ]]; then
    report_error "Could not determine canonical policy pack count from registry.rs"
elif [[ -z "$DOCS_POLICY_COUNT" ]]; then
    report_error "Could not find docs policy pack count"
elif [[ "$CODE_POLICY_COUNT" != "$DOCS_POLICY_COUNT" ]]; then
    report_error "Policy pack count mismatch: code=$CODE_POLICY_COUNT docs=$DOCS_POLICY_COUNT"
else
    report_success "Policy pack count consistent ($CODE_POLICY_COUNT)"
fi

echo "2. Sample cited source files exist"
CITATION_COUNT=0
MISSING_FILES=0
while read -r line; do
    CITATION_COUNT=$((CITATION_COUNT + 1))
    cited_content=$(echo "$line" | sed 's/.*\[source: \([^]]*\)\].*/\1/')
    cited_file=$(echo "$cited_content" | sed 's/ L[0-9-]*$//')
    if [[ -n "$cited_file" && "$cited_file" != "$cited_content" && ! -f "$cited_file" ]]; then
        report_error "Missing cited file: $cited_file"
        MISSING_FILES=$((MISSING_FILES + 1))
    fi
done < <(grep -r "\[source:" docs/ | head -10 || true)

if [[ "$MISSING_FILES" -eq 0 ]]; then
    report_success "Citation check complete ($CITATION_COUNT sampled)"
fi

echo "3. Migration references exist in docs"
DOCS_MIGRATION_REFS=$(grep -r "migration" docs/ | wc -l | tr -d ' ')
if [[ "$DOCS_MIGRATION_REFS" -eq 0 ]]; then
    report_error "No migration references found in docs"
else
    report_success "Migration references present ($DOCS_MIGRATION_REFS)"
fi

echo "4. CLI docs sanity (code-init command mention)"
if grep -q "code-init" crates/adapteros-cli/src/main.rs; then
    report_success "CLI command marker found"
else
    report_error "CLI command marker not found"
fi

echo "5. README version marker sanity"
README_HAS_VERSION=$(grep -c "alpha-v" README.md || true)
if [[ "$README_HAS_VERSION" -gt 0 ]]; then
    report_success "README version marker present"
else
    report_error "README version marker missing"
fi

if [[ "$ERRORS" -eq 0 ]]; then
    echo "=== Legacy Checks Passed ==="
    exit 0
fi

echo "=== Legacy Checks Failed ($ERRORS) ==="
exit 1
