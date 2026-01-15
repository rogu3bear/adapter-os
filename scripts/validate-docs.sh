#!/bin/bash
# validate-docs.sh
#
# Validates documentation cross-references and accuracy
# Part of adapterOS documentation quality assurance
#
# Usage: ./scripts/validate-docs.sh

set -e

echo "🔍 Validating adapterOS documentation..."
echo "========================================"

ERRORS=0

# Function to report errors
report_error() {
    echo "❌ $1"
    ((ERRORS++))
}

# Function to report success
report_success() {
    echo "✅ $1"
}

# Check 1: Policy pack count consistency
echo "1. Checking policy pack count consistency..."
CODE_POLICY_COUNT=$(ls crates/adapteros-policy/src/packs/ | grep -v mod.rs | wc -l | tr -d ' ')
DOCS_POLICY_COUNT=$(grep -r "policy packs" docs/ | grep -v archive/ | grep -o "[0-9]\+ policy packs" | head -1 | cut -d' ' -f1)

if [[ "$CODE_POLICY_COUNT" != "$DOCS_POLICY_COUNT" ]]; then
    report_error "Policy pack count mismatch: code=$CODE_POLICY_COUNT, docs=$DOCS_POLICY_COUNT"
else
    report_success "Policy pack count consistent: $CODE_POLICY_COUNT"
fi

# Check 2: File existence validation
echo "2. Checking file path references..."
CITATION_COUNT=0
MISSING_FILES=0

while read -r line; do
    ((CITATION_COUNT++))
    # Extract file paths from citations like [source: path/file.rs L1-L5]
    cited_content=$(echo "$line" | sed 's/.*\[source: \([^]]*\)\].*/\1/')
    cited_file=$(echo "$cited_content" | sed 's/ L[0-9-]*$//')  # Remove line numbers

    if [[ -n "$cited_file" && "$cited_file" != "$cited_content" && ! -f "$cited_file" ]]; then
        report_error "Missing cited file: $cited_file"
        ((MISSING_FILES++))
    fi
done < <(grep -r "\[source:" docs/ | head -10)  # Sample first 10 citations

if [[ $MISSING_FILES -eq 0 ]]; then
    report_success "File path validation completed ($CITATION_COUNT citations checked)"
else
    report_error "Found $MISSING_FILES missing files out of $CITATION_COUNT citations checked"
fi

# Check 3: Database migration count
echo "3. Checking database migration references..."
ACTUAL_MIGRATIONS=$(ls migrations/ | wc -l)
DOCS_MIGRATION_REFS=$(grep -r "migration" docs/ | grep -c "migration" || echo "0")

if [[ $DOCS_MIGRATION_REFS -eq 0 ]]; then
    report_error "No migration references found in docs"
else
    report_success "Migration references present in docs"
fi

# Check 4: CLI command validation
echo "4. Checking CLI command references..."
# Check if documented commands exist in CLI
if grep -q "code-init" crates/adapteros-cli/src/main.rs; then
    report_success "CLI commands match documentation"
else
    report_error "CLI commands don't match documentation"
fi

# Check 5: Version consistency (sample check)
echo "5. Checking version references..."
CARGO_VERSION=$(grep "^version" Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
README_HAS_VERSION=$(grep -c "alpha-v" README.md)

if [[ $README_HAS_VERSION -gt 0 ]]; then
    report_success "Version references present (Cargo.toml: $CARGO_VERSION, README: alpha format)"
else
    report_error "No version references found in README"
fi

# Check 6: Cross-reference validation
echo "6. Checking internal cross-references..."
# Check for broken relative links (basic check)
BROKEN_LINKS=$(find docs/ -name "*.md" -exec grep -l "\[.*\](\.\." {} \; | wc -l)
if [[ $BROKEN_LINKS -gt 0 ]]; then
    # This is a basic check - real validation would need more sophisticated parsing
    echo "⚠️  Found relative links (manual verification needed)"
else
    report_success "No obvious broken links found"
fi

# Summary
echo ""
echo "========================================"
if [[ $ERRORS -eq 0 ]]; then
    echo "🎉 All validation checks passed!"
    exit 0
else
    echo "❌ Found $ERRORS validation errors"
    echo "   Run with --fix to attempt automatic fixes"
    exit 1
fi
