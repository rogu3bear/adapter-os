#!/bin/bash
# Collect and format stress test results

set -e

INPUT_FILE="${1:-var/stress-test-output.txt}"
OUTPUT_FILE="${2:-var/stress-test-results.json}"

mkdir -p "$(dirname "$OUTPUT_FILE")"

# Initialize JSON structure
cat > "$OUTPUT_FILE" <<EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "status": "completed",
  "metrics": {},
  "errors": [],
  "summary": ""
}
EOF

# Parse test output if available
if [ -f "$INPUT_FILE" ]; then
    # Extract key metrics from test output
    # Look for patterns like "latency: 123ms", "throughput: 456 ops/s", etc.
    
    LATENCY=$(grep -i "latency\|time\|duration" "$INPUT_FILE" | head -1 | grep -oE '[0-9]+\.[0-9]+[a-z]*' | head -1 || echo "N/A")
    THROUGHPUT=$(grep -i "throughput\|ops/s\|req/s" "$INPUT_FILE" | head -1 | grep -oE '[0-9]+\.[0-9]+' | head -1 || echo "N/A")
    ERROR_COUNT=$(grep -i "error\|fail\|panic" "$INPUT_FILE" | wc -l | tr -d ' ' || echo "0")
    TEST_COUNT=$(grep -E "test.*\.\.\." "$INPUT_FILE" | wc -l | tr -d ' ' || echo "0")
    PASSED=$(grep -E "test.*\.\.\. ok" "$INPUT_FILE" | wc -l | tr -d ' ' || echo "0")
    FAILED=$(grep -E "test.*\.\.\. FAILED\|test.*\.\.\. FAIL" "$INPUT_FILE" | wc -l | tr -d ' ' || echo "0")
    
    # Update JSON with extracted metrics
    python3 <<PYTHON_SCRIPT
import json
import sys

with open("$OUTPUT_FILE", 'r') as f:
    data = json.load(f)

data["metrics"] = {
    "latency": "$LATENCY",
    "throughput": "$THROUGHPUT",
    "error_count": int("$ERROR_COUNT"),
    "test_count": int("$TEST_COUNT"),
    "passed": int("$PASSED"),
    "failed": int("$FAILED")
}

if int("$FAILED") > 0:
    data["status"] = "failed"
    data["summary"] = f"Stress tests completed with {int('$FAILED')} failures out of {int('$TEST_COUNT')} tests"
else:
    data["status"] = "passed"
    data["summary"] = f"All {int('$PASSED')} stress tests passed"

with open("$OUTPUT_FILE", 'w') as f:
    json.dump(data, f, indent=2)
PYTHON_SCRIPT
else
    # No input file, create minimal result
    python3 <<PYTHON_SCRIPT
import json

data = {
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "status": "no_data",
    "metrics": {},
    "errors": ["No stress test output file found"],
    "summary": "Stress test results could not be collected"
}

with open("$OUTPUT_FILE", 'w') as f:
    json.dump(data, f, indent=2)
PYTHON_SCRIPT
fi

echo "✅ Stress test results collected in $OUTPUT_FILE"

