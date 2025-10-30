#!/bin/bash

# AdapterOS UI Smoke Tests
# Tests key endpoints and basic functionality

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
BASE_URL="${ADAPTEROS_BASE_URL:-http://localhost:8080}"
API_BASE="${BASE_URL}/api"

echo "🔍 AdapterOS UI Smoke Tests"
echo "================================"
echo "Base URL: ${BASE_URL}"
echo "API Base: ${API_BASE}"
echo ""

# Test counter
TOTAL_TESTS=0
PASSED_TESTS=0

# Helper function to run a test
run_test() {
    local name="$1"
    local command="$2"
    local expected_status="${3:-200}"

    echo -n "Testing ${name}... "
    TOTAL_TESTS=$((TOTAL_TESTS + 1))

    if eval "$command" 2>/dev/null | grep -q "HTTP.*${expected_status}"; then
        echo -e "${GREEN}✓ PASS${NC}"
        PASSED_TESTS=$((PASSED_TESTS + 1))
    else
        echo -e "${RED}✗ FAIL${NC}"
        echo "  Command: $command"
    fi
}

# Test UI endpoint (should serve index.html or redirect)
run_test "UI root endpoint" "curl -s -I ${BASE_URL}/" 200

# Test API endpoints
echo ""
echo "API Endpoint Tests:"
echo "==================="

# Health check
run_test "Health endpoint" "curl -s -I ${API_BASE}/healthz" 200

# Meta endpoint
run_test "Meta endpoint" "curl -s -I ${API_BASE}/v1/meta" 200

# Auth endpoints (expect 401 without auth)
run_test "Auth login (unauthenticated)" "curl -s -I ${API_BASE}/v1/auth/login" 401

# Metrics endpoints
run_test "System metrics" "curl -s -I ${API_BASE}/v1/metrics/system" 401

# Tenants endpoint
run_test "Tenants list" "curl -s -I ${API_BASE}/v1/tenants" 401

# Adapters endpoint
run_test "Adapters list" "curl -s -I ${API_BASE}/v1/adapters" 401

# Policies endpoint
run_test "Policies list" "curl -s -I ${API_BASE}/v1/policies" 401

# Telemetry logs (audit)
run_test "Telemetry logs (audit)" "curl -s -I \"${API_BASE}/v1/telemetry/logs?category=audit\"" 401

# Test static assets (if UI is embedded)
echo ""
echo "Static Asset Tests:"
echo "==================="

# Test common static files
run_test "Main CSS" "curl -s -I ${BASE_URL}/assets/index-*.css" 200

run_test "Main JS" "curl -s -I ${BASE_URL}/assets/index-*.js" 200

# Summary
echo ""
echo "Summary:"
echo "========"
echo "Total tests: ${TOTAL_TESTS}"
echo "Passed: ${PASSED_TESTS}"
echo "Failed: $((TOTAL_TESTS - PASSED_TESTS))"

if [ "$PASSED_TESTS" -eq "$TOTAL_TESTS" ]; then
    echo -e "${GREEN}🎉 All smoke tests passed!${NC}"
    exit 0
else
    echo -e "${RED}❌ Some smoke tests failed. Check the server logs.${NC}"
    exit 1
fi
