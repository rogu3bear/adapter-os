#!/bin/bash
# Generate test metrics report for adapterOS

set -e

OUTPUT_FILE="${1:-var/test-metrics.md}"
mkdir -p "$(dirname "$OUTPUT_FILE")"

echo "# Test Metrics Report - $(date)" > "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

# Test counts
echo "## Test Counts" >> "$OUTPUT_FILE"
TOTAL_TESTS=$(cargo test --workspace -- --list 2>/dev/null | grep -c "test " || echo "0")
echo "**Total Tests:** $TOTAL_TESTS" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

# Count by test type
UNIT_TESTS=$(cargo test --workspace --lib -- --list 2>/dev/null | grep -c "test " || echo "0")
INTEGRATION_TESTS=$(find tests -name "*.rs" -type f 2>/dev/null | wc -l | tr -d ' ' || echo "0")
E2E_TESTS=$(find tests/e2e -name "*.rs" -type f 2>/dev/null | wc -l | tr -d ' ' || echo "0")

echo "| Test Type | Count |" >> "$OUTPUT_FILE"
echo "|-----------|-------|" >> "$OUTPUT_FILE"
echo "| Unit Tests | $UNIT_TESTS |" >> "$OUTPUT_FILE"
echo "| Integration Tests | $INTEGRATION_TESTS |" >> "$OUTPUT_FILE"
echo "| E2E Tests | $E2E_TESTS |" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

# Coverage summary (if tarpaulin available)
if command -v cargo-tarpaulin &> /dev/null; then
    echo "## Coverage Summary" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
    
    # Try to get coverage data
    if [ -f "coverage.json" ]; then
        echo "| Crate | Coverage |" >> "$OUTPUT_FILE"
        echo "|-------|----------|" >> "$OUTPUT_FILE"
        
        # Parse coverage.json if jq is available
        if command -v jq &> /dev/null; then
            jq -r '.packages[] | "| `\(.name)` | \(.coverage | tostring | .[0:5])% |"' coverage.json >> "$OUTPUT_FILE" 2>/dev/null || true
        else
            echo "| Coverage data available (install jq for detailed view) |" >> "$OUTPUT_FILE"
        fi
    else
        echo "Coverage data not available. Run coverage job to generate." >> "$OUTPUT_FILE"
    fi
    echo "" >> "$OUTPUT_FILE"
else
    echo "## Coverage Summary" >> "$OUTPUT_FILE"
    echo "Tarpaulin not available. Install with: \`cargo install cargo-tarpaulin\`" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
fi

# Test execution summary
echo "## Test Execution Summary" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"
echo "**Last Updated:** $(date)" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"
echo "To run all tests:" >> "$OUTPUT_FILE"
echo "\`\`\`bash" >> "$OUTPUT_FILE"
echo "cargo test --workspace" >> "$OUTPUT_FILE"
echo "\`\`\`" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"

echo "✅ Test metrics generated in $OUTPUT_FILE"

