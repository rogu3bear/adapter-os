#!/bin/bash
# Browser QA Test Script for adapterOS
# Tests API endpoints and provides checklist for manual browser testing

set -e

BASE_URL="${AOS_BASE_URL:-http://localhost:18080}"
VERBOSE="${VERBOSE:-0}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

# Test function
test_endpoint() {
    local name="$1"
    local endpoint="$2"
    local expected_status="${3:-200}"
    
    if [ "$VERBOSE" = "1" ]; then
        echo -e "${CYAN}Testing: $name${NC}"
        echo "  GET $BASE_URL$endpoint"
    fi
    
    local status_code
    status_code=$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL$endpoint")
    
    if [ "$status_code" = "$expected_status" ]; then
        echo -e "${GREEN}✓${NC} $name (HTTP $status_code)"
        ((TESTS_PASSED++))
        return 0
    else
        echo -e "${RED}✗${NC} $name (HTTP $status_code, expected $expected_status)"
        ((TESTS_FAILED++))
        return 1
    fi
}

# JSON validation test
test_json_endpoint() {
    local name="$1"
    local endpoint="$2"
    
    if [ "$VERBOSE" = "1" ]; then
        echo -e "${CYAN}Testing JSON: $name${NC}"
    fi
    
    if curl -s "$BASE_URL$endpoint" | jq . >/dev/null 2>&1; then
        echo -e "${GREEN}✓${NC} $name (valid JSON)"
        ((TESTS_PASSED++))
        return 0
    else
        echo -e "${RED}✗${NC} $name (invalid JSON)"
        ((TESTS_FAILED++))
        return 1
    fi
}

echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  adapterOS Browser QA Test Suite${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "Base URL: $BASE_URL"
echo ""

# Health & Readiness Checks
echo -e "${YELLOW}Health & Readiness Checks${NC}"
echo "────────────────────────────────────────────────────────────────────────────────"
test_endpoint "Health Check" "/healthz"
test_endpoint "Readiness Check" "/readyz"
test_json_endpoint "System Status" "/v1/system/status"
test_json_endpoint "System Overview" "/v1/system/overview"
echo ""

# Core API Endpoints
echo -e "${YELLOW}Core API Endpoints${NC}"
echo "────────────────────────────────────────────────────────────────────────────────"
test_json_endpoint "Adapters List" "/v1/adapters"
test_json_endpoint "Models List" "/v1/models"
test_json_endpoint "System Integrity" "/v1/system/integrity"
test_json_endpoint "Pilot Status" "/v1/system/pilot-status"
echo ""

# UI Static Assets
echo -e "${YELLOW}UI Static Assets${NC}"
echo "────────────────────────────────────────────────────────────────────────────────"
test_endpoint "Main HTML" "/" 200
test_endpoint "Base CSS" "/base-dff6fb076c809b10.css" 200
test_endpoint "Components CSS" "/components-651a84f7bfac21c8.css" 200
test_endpoint "Glass CSS" "/glass-da6fb41f9d5581be.css" 200
echo ""

# Summary
echo ""
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${CYAN}  Test Summary${NC}"
echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
TOTAL=$((TESTS_PASSED + TESTS_FAILED))
if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All $TESTS_PASSED tests passed!${NC}"
    echo ""
    echo "Next steps for manual browser testing:"
    echo "  1. Open $BASE_URL in your browser"
    echo "  2. Open DevTools (F12) and check Console for errors"
    echo "  3. Navigate through all pages listed in qa-browser-tests.md"
    echo "  4. Test responsive design, accessibility, and forms"
    exit 0
else
    echo -e "${GREEN}Passed: $TESTS_PASSED${NC}"
    echo -e "${RED}Failed: $TESTS_FAILED${NC}"
    echo ""
    echo "Some tests failed. Check the output above for details."
    exit 1
fi
