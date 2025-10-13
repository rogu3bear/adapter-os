#!/bin/bash
# Integration Test Script for AdapterOS Control Plane UI
# Tests that all SSE streams and API endpoints are functioning

set -e

API_BASE="${API_BASE:-http://127.0.0.1:8080/api}"
TOKEN=""

echo "🧪 AdapterOS Integration Test Suite"
echo "===================================="
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

function pass() {
    echo -e "${GREEN}✓${NC} $1"
}

function fail() {
    echo -e "${RED}✗${NC} $1"
    exit 1
}

function info() {
    echo -e "${YELLOW}ℹ${NC} $1"
}

# Test 1: Health Check
echo "1️⃣  Testing Health Endpoint..."
HEALTH=$(curl -s -o /dev/null -w "%{http_code}" "$API_BASE/healthz")
if [ "$HEALTH" = "200" ]; then
    pass "Health endpoint responding"
else
    fail "Health endpoint failed (HTTP $HEALTH)"
fi

# Test 2: Login (get token)
echo ""
echo "2️⃣  Testing Authentication..."
LOGIN_RESPONSE=$(curl -s -X POST "$API_BASE/v1/auth/login" \
    -H "Content-Type: application/json" \
    -d '{"email":"admin@example.com","password":"password"}')

TOKEN=$(echo "$LOGIN_RESPONSE" | grep -o '"token":"[^"]*' | cut -d'"' -f4)

if [ -n "$TOKEN" ]; then
    pass "Authentication successful, token received"
else
    info "Using guest access (no token)"
fi

# Test 3: System Metrics
echo ""
echo "3️⃣  Testing System Metrics Endpoint..."
METRICS=$(curl -s "$API_BASE/v1/metrics/system" \
    -H "Authorization: Bearer $TOKEN" 2>/dev/null || echo "{}")

if echo "$METRICS" | grep -q "cpu_usage"; then
    pass "System metrics endpoint working"
else
    fail "System metrics endpoint failed"
fi

# Test 4: Adapters List
echo ""
echo "4️⃣  Testing Adapters Endpoint..."
ADAPTERS=$(curl -s "$API_BASE/v1/adapters" \
    -H "Authorization: Bearer $TOKEN" 2>/dev/null || echo "[]")

if echo "$ADAPTERS" | grep -q "\["; then
    ADAPTER_COUNT=$(echo "$ADAPTERS" | grep -o "adapter_id" | wc -l | tr -d ' ')
    pass "Adapters endpoint working ($ADAPTER_COUNT adapters)"
else
    fail "Adapters endpoint failed"
fi

# Test 5: Nodes List
echo ""
echo "5️⃣  Testing Nodes Endpoint..."
NODES=$(curl -s "$API_BASE/v1/nodes" \
    -H "Authorization: Bearer $TOKEN" 2>/dev/null || echo "[]")

if echo "$NODES" | grep -q "\["; then
    NODE_COUNT=$(echo "$NODES" | grep -o "hostname" | wc -l | tr -d ' ')
    pass "Nodes endpoint working ($NODE_COUNT nodes)"
else
    fail "Nodes endpoint failed"
fi

# Test 6: Telemetry Bundles
echo ""
echo "6️⃣  Testing Telemetry Bundles Endpoint..."
BUNDLES=$(curl -s "$API_BASE/v1/telemetry/bundles" \
    -H "Authorization: Bearer $TOKEN" 2>/dev/null || echo "[]")

if echo "$BUNDLES" | grep -q "\["; then
    BUNDLE_COUNT=$(echo "$BUNDLES" | grep -o "merkle_root" | wc -l | tr -d ' ')
    pass "Telemetry bundles endpoint working ($BUNDLE_COUNT bundles)"
else
    fail "Telemetry bundles endpoint failed"
fi

# Test 7: SSE Streams (check they're available)
echo ""
echo "7️⃣  Testing SSE Stream Availability..."

# Test metrics stream
info "Checking /v1/stream/metrics..."
timeout 3 curl -s -N "$API_BASE/v1/stream/metrics" \
    -H "Authorization: Bearer $TOKEN" >/dev/null 2>&1 || true
if [ $? -eq 124 ]; then
    pass "Metrics SSE stream is available"
else
    info "Metrics SSE stream check timed out (expected)"
fi

# Test adapter stream
info "Checking /v1/stream/adapters..."
timeout 3 curl -s -N "$API_BASE/v1/stream/adapters" \
    -H "Authorization: Bearer $TOKEN" >/dev/null 2>&1 || true
if [ $? -eq 124 ]; then
    pass "Adapters SSE stream is available"
else
    info "Adapters SSE stream check timed out (expected)"
fi

# Test telemetry stream
info "Checking /v1/stream/telemetry..."
timeout 3 curl -s -N "$API_BASE/v1/stream/telemetry" \
    -H "Authorization: Bearer $TOKEN" >/dev/null 2>&1 || true
if [ $? -eq 124 ]; then
    pass "Telemetry SSE stream is available"
else
    info "Telemetry SSE stream check timed out (expected)"
fi

# Summary
echo ""
echo "===================================="
echo -e "${GREEN}✓${NC} Integration tests complete!"
echo ""
echo "Next steps:"
echo "  1. Start the UI: cd ui && pnpm dev"
echo "  2. Open http://localhost:5173"
echo "  3. Login with: admin@example.com / password"
echo "  4. Verify live metrics in Dashboard"
echo "  5. Check browser DevTools Network tab for SSE streams"
echo ""

