#!/bin/bash
# AdapterOS API Endpoint Testing Script
# Tests all backend API endpoints and validates responses
#
# Usage:
#   ./scripts/test_api_endpoints.sh [BASE_URL] [TOKEN]
#
# Environment variables:
#   AOS_API_BASE_URL - Base URL for the API (default: http://localhost:8080)
#   AOS_AUTH_TOKEN   - JWT token for authenticated endpoints

set -e

BASE_URL="${1:-${AOS_API_BASE_URL:-http://localhost:8080}}"
TOKEN="${2:-${AOS_AUTH_TOKEN:-}}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Counters
PASSED=0
FAILED=0
SKIPPED=0

# Test result tracking
declare -a FAILED_TESTS=()
declare -a PASSED_TESTS=()
declare -a SKIPPED_TESTS=()

log_pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
    PASSED=$((PASSED + 1))
    PASSED_TESTS+=("$1")
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1 - $2"
    FAILED=$((FAILED + 1))
    FAILED_TESTS+=("$1: $2")
}

log_skip() {
    echo -e "${YELLOW}[SKIP]${NC} $1 - $2"
    SKIPPED=$((SKIPPED + 1))
    SKIPPED_TESTS+=("$1: $2")
}

log_info() {
    echo -e "[INFO] $1"
}

# Test function for public endpoints (no auth required)
test_public_endpoint() {
    local method="$1"
    local path="$2"
    local expected_status="$3"
    local description="$4"

    local url="${BASE_URL}${path}"
    local response
    local status_code

    response=$(curl -s -w "\n%{http_code}" -X "$method" "$url" 2>&1)
    status_code=$(echo "$response" | tail -n1)

    if [ "$status_code" = "$expected_status" ]; then
        log_pass "$method $path - $description"
    else
        log_fail "$method $path" "Expected $expected_status, got $status_code"
    fi
}

# Test function for protected endpoints (auth required)
test_protected_endpoint() {
    local method="$1"
    local path="$2"
    local expected_status="$3"
    local description="$4"
    local body="${5:-}"

    if [ -z "$TOKEN" ]; then
        log_skip "$method $path" "No auth token provided"
        return
    fi

    local url="${BASE_URL}${path}"
    local response
    local status_code

    if [ -n "$body" ]; then
        response=$(curl -s -w "\n%{http_code}" -X "$method" "$url" \
            -H "Authorization: Bearer $TOKEN" \
            -H "Content-Type: application/json" \
            -d "$body" 2>&1)
    else
        response=$(curl -s -w "\n%{http_code}" -X "$method" "$url" \
            -H "Authorization: Bearer $TOKEN" 2>&1)
    fi

    status_code=$(echo "$response" | tail -n1)

    if [ "$status_code" = "$expected_status" ]; then
        log_pass "$method $path - $description"
    else
        log_fail "$method $path" "Expected $expected_status, got $status_code"
    fi
}

# Test unauthorized access to protected endpoints
test_protected_without_auth() {
    local method="$1"
    local path="$2"
    local description="$3"

    local url="${BASE_URL}${path}"
    local response
    local status_code

    response=$(curl -s -w "\n%{http_code}" -X "$method" "$url" 2>&1)
    status_code=$(echo "$response" | tail -n1)

    if [ "$status_code" = "401" ]; then
        log_pass "$method $path - Correctly requires auth"
    else
        log_fail "$method $path - Auth check" "Expected 401, got $status_code"
    fi
}

echo "========================================"
echo "AdapterOS API Endpoint Testing"
echo "========================================"
echo "Base URL: $BASE_URL"
echo "Token provided: $([ -n "$TOKEN" ] && echo 'Yes' || echo 'No')"
echo ""

# ============================================
# PUBLIC ENDPOINTS (No Auth Required)
# ============================================
echo "----------------------------------------"
echo "Testing Public Endpoints"
echo "----------------------------------------"

# Health Check Endpoints
test_public_endpoint "GET" "/healthz" "200" "Basic health check"
test_public_endpoint "GET" "/healthz/all" "200" "All components health check"
test_public_endpoint "GET" "/healthz/db" "200" "Database component health"
test_public_endpoint "GET" "/healthz/router" "200" "Router component health"
test_public_endpoint "GET" "/healthz/loader" "200" "Loader component health"
test_public_endpoint "GET" "/healthz/kernel" "200" "Kernel component health"
test_public_endpoint "GET" "/healthz/telemetry" "200" "Telemetry component health"
test_public_endpoint "GET" "/healthz/system-metrics" "200" "System metrics health"
test_public_endpoint "GET" "/healthz/invalid" "404" "Invalid component returns 404"
test_public_endpoint "GET" "/readyz" "200" "Readiness check"

# Meta/Info Endpoints
test_public_endpoint "GET" "/v1/meta" "200" "API metadata"

# Auth Endpoints (Public access for login/bootstrap)
test_public_endpoint "POST" "/v1/auth/login" "401" "Login endpoint (invalid creds)"
test_public_endpoint "POST" "/v1/auth/bootstrap" "403" "Bootstrap (already configured)"

# OpenAPI/Swagger
test_public_endpoint "GET" "/swagger-ui/" "200" "Swagger UI"
test_public_endpoint "GET" "/api-docs/openapi.json" "200" "OpenAPI JSON spec"

# ============================================
# PROTECTED ENDPOINT AUTH VERIFICATION
# ============================================
echo ""
echo "----------------------------------------"
echo "Testing Auth Requirements"
echo "----------------------------------------"

# These should all return 401 without auth
test_protected_without_auth "GET" "/v1/auth/me" "Auth: Get current user"
test_protected_without_auth "GET" "/v1/tenants" "Auth: List tenants"
test_protected_without_auth "GET" "/v1/adapters" "Auth: List adapters"
test_protected_without_auth "GET" "/v1/nodes" "Auth: List nodes"
test_protected_without_auth "GET" "/v1/workers" "Auth: List workers"
test_protected_without_auth "GET" "/v1/training/jobs" "Auth: List training jobs"
test_protected_without_auth "GET" "/v1/policies" "Auth: List policies"
test_protected_without_auth "GET" "/v1/audit/logs" "Auth: Audit logs"

# ============================================
# PROTECTED ENDPOINTS (With Auth)
# ============================================
echo ""
echo "----------------------------------------"
echo "Testing Protected Endpoints"
echo "----------------------------------------"

# Auth endpoints
test_protected_endpoint "GET" "/v1/auth/me" "200" "Get current user info"
test_protected_endpoint "GET" "/v1/auth/sessions" "200" "List active sessions"
test_protected_endpoint "POST" "/v1/auth/refresh" "200" "Refresh token"

# Tenants
test_protected_endpoint "GET" "/v1/tenants" "200" "List tenants"

# Adapters
test_protected_endpoint "GET" "/v1/adapters" "200" "List adapters"
test_protected_endpoint "GET" "/v1/adapter-stacks" "200" "List adapter stacks"

# Domain Adapters
test_protected_endpoint "GET" "/v1/domain-adapters" "200" "List domain adapters"

# Nodes
test_protected_endpoint "GET" "/v1/nodes" "200" "List nodes"

# Workers
test_protected_endpoint "GET" "/v1/workers" "200" "List workers"

# Models
test_protected_endpoint "GET" "/v1/models/status" "200" "Get model status"

# Plans
test_protected_endpoint "GET" "/v1/plans" "200" "List plans"

# Training
test_protected_endpoint "GET" "/v1/training/jobs" "200" "List training jobs"
test_protected_endpoint "GET" "/v1/training/templates" "200" "List training templates"

# Datasets
test_protected_endpoint "GET" "/v1/datasets" "200" "List datasets"

# Policies
test_protected_endpoint "GET" "/v1/policies" "200" "List policies"

# Jobs
test_protected_endpoint "GET" "/v1/jobs" "200" "List jobs"

# Monitoring
test_protected_endpoint "GET" "/v1/monitoring/rules" "200" "List monitoring rules"
test_protected_endpoint "GET" "/v1/monitoring/alerts" "200" "List alerts"
test_protected_endpoint "GET" "/v1/monitoring/anomalies" "200" "List anomalies"
test_protected_endpoint "GET" "/v1/monitoring/dashboards" "200" "List dashboards"
test_protected_endpoint "GET" "/v1/monitoring/health-metrics" "200" "Health metrics"
test_protected_endpoint "GET" "/v1/monitoring/reports" "200" "Monitoring reports"

# Metrics
test_protected_endpoint "GET" "/v1/metrics/quality" "200" "Quality metrics"
test_protected_endpoint "GET" "/v1/metrics/adapters" "200" "Adapter metrics"
test_protected_endpoint "GET" "/v1/metrics/system" "200" "System metrics"
test_protected_endpoint "GET" "/v1/metrics/snapshot" "200" "Metrics snapshot"
test_protected_endpoint "GET" "/v1/system/memory" "200" "UMA memory info"

# Routing
test_protected_endpoint "GET" "/v1/routing/history" "200" "Routing history"
test_protected_endpoint "GET" "/v1/routing/decisions" "200" "Routing decisions"

# Golden Runs
test_protected_endpoint "GET" "/v1/golden/runs" "200" "List golden runs"

# Telemetry
test_protected_endpoint "GET" "/v1/telemetry/bundles" "200" "List telemetry bundles"
test_protected_endpoint "GET" "/v1/traces/search" "200" "Search traces"
test_protected_endpoint "GET" "/v1/logs/query" "200" "Query logs"

# Git
test_protected_endpoint "GET" "/v1/git/status" "200" "Git status"
test_protected_endpoint "GET" "/v1/git/branches" "200" "List git branches"

# Federation
test_protected_endpoint "GET" "/v1/federation/status" "200" "Federation status"
test_protected_endpoint "GET" "/v1/federation/quarantine" "200" "Quarantine status"

# Code Intelligence
test_protected_endpoint "GET" "/v1/code/repositories" "200" "List repositories"

# Audit
test_protected_endpoint "GET" "/v1/audit/logs" "200" "Query audit logs"
test_protected_endpoint "GET" "/v1/audit/federation" "200" "Federation audit"
test_protected_endpoint "GET" "/v1/audit/compliance" "200" "Compliance audit"
test_protected_endpoint "GET" "/v1/audits" "200" "Extended audits"

# Contacts
test_protected_endpoint "GET" "/v1/contacts" "200" "List contacts"

# Commits
test_protected_endpoint "GET" "/v1/commits" "200" "List commits"

# Repositories (deprecated)
test_protected_endpoint "GET" "/v1/repositories" "200" "List repositories (deprecated)"

# CP Promotions
test_protected_endpoint "GET" "/v1/cp/promotions" "200" "Promotion history"

# Replay
test_protected_endpoint "GET" "/v1/replay/sessions" "200" "List replay sessions"

# Plugins
test_protected_endpoint "GET" "/v1/plugins" "200" "List plugins"

# Activity
test_protected_endpoint "GET" "/v1/activity/events" "200" "List activity events"
test_protected_endpoint "GET" "/v1/activity/feed" "200" "Activity feed"

# Workspaces
test_protected_endpoint "GET" "/v1/workspaces" "200" "List workspaces"
test_protected_endpoint "GET" "/v1/workspaces/me" "200" "List user workspaces"

# Notifications
test_protected_endpoint "GET" "/v1/notifications" "200" "List notifications"
test_protected_endpoint "GET" "/v1/notifications/summary" "200" "Notification summary"

# Dashboard
test_protected_endpoint "GET" "/v1/dashboard/config" "200" "Dashboard config"

# Tutorials
test_protected_endpoint "GET" "/v1/tutorials" "200" "List tutorials"

# ============================================
# SSE STREAMING ENDPOINTS (verify connection)
# ============================================
echo ""
echo "----------------------------------------"
echo "Testing SSE Streaming Endpoints"
echo "----------------------------------------"

# These return 200 and start streaming - we just verify they accept the request
if [ -n "$TOKEN" ]; then
    for path in "/v1/streams/training" "/v1/streams/discovery" "/v1/streams/contacts" \
                "/v1/streams/file-changes" "/v1/stream/metrics" "/v1/stream/telemetry" \
                "/v1/stream/adapters" "/v1/logs/stream"; do
        response=$(timeout 2 curl -s -w "\n%{http_code}" -X GET "${BASE_URL}${path}" \
            -H "Authorization: Bearer $TOKEN" 2>&1 || true)
        status_code=$(echo "$response" | tail -n1)
        if [ "$status_code" = "200" ] || [ -z "$status_code" ]; then
            log_pass "GET $path - SSE endpoint accessible"
        else
            log_fail "GET $path - SSE endpoint" "Got status $status_code"
        fi
    done
else
    log_skip "SSE endpoints" "No auth token provided"
fi

# ============================================
# OPTIONS REQUESTS (CORS Preflight)
# ============================================
echo ""
echo "----------------------------------------"
echo "Testing CORS Preflight"
echo "----------------------------------------"

for path in "/v1/adapters" "/v1/tenants" "/v1/infer"; do
    response=$(curl -s -w "\n%{http_code}" -X OPTIONS "${BASE_URL}${path}" \
        -H "Origin: http://localhost:3000" \
        -H "Access-Control-Request-Method: GET" 2>&1)
    status_code=$(echo "$response" | tail -n1)
    if [ "$status_code" = "200" ] || [ "$status_code" = "204" ]; then
        log_pass "OPTIONS $path - CORS preflight"
    else
        log_fail "OPTIONS $path" "Expected 200/204, got $status_code"
    fi
done

# ============================================
# METHOD NOT ALLOWED TESTS
# ============================================
echo ""
echo "----------------------------------------"
echo "Testing Method Not Allowed"
echo "----------------------------------------"

# Test wrong methods return 405
test_public_endpoint "POST" "/healthz" "405" "POST to health returns 405"
test_public_endpoint "DELETE" "/healthz" "405" "DELETE to health returns 405"
test_public_endpoint "PUT" "/healthz" "405" "PUT to health returns 405"

# ============================================
# SUMMARY
# ============================================
echo ""
echo "========================================"
echo "Test Summary"
echo "========================================"
echo -e "${GREEN}Passed:${NC}  $PASSED"
echo -e "${RED}Failed:${NC}  $FAILED"
echo -e "${YELLOW}Skipped:${NC} $SKIPPED"
echo "Total:   $((PASSED + FAILED + SKIPPED))"
echo ""

if [ ${#FAILED_TESTS[@]} -gt 0 ]; then
    echo -e "${RED}Failed Tests:${NC}"
    for test in "${FAILED_TESTS[@]}"; do
        echo "  - $test"
    done
    echo ""
fi

if [ ${#SKIPPED_TESTS[@]} -gt 0 ]; then
    echo -e "${YELLOW}Skipped Tests:${NC}"
    for test in "${SKIPPED_TESTS[@]}"; do
        echo "  - $test"
    done
    echo ""
fi

if [ $FAILED -gt 0 ]; then
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
else
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
fi
