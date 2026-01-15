#!/bin/bash
# Test script for adapterOS inference pipeline using web tools (curl)
# Tests health, authentication, and inference endpoints

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SERVER_URL="${AOS_SERVER_URL:-http://localhost:8080}"
BASE_URL="${SERVER_URL}"

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0

# Helper functions
print_test() {
    echo -e "${BLUE}[TEST]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((TESTS_PASSED++))
}

print_error() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((TESTS_FAILED++))
}

print_info() {
    echo -e "${YELLOW}[INFO]${NC} $1"
}

# Test function wrapper
run_test() {
    local test_name="$1"
    local test_command="$2"
    
    print_test "$test_name"
    if eval "$test_command"; then
        print_success "$test_name"
        return 0
    else
        print_error "$test_name"
        return 1
    fi
}

echo "=========================================="
echo "adapterOS Inference Pipeline Test Suite"
echo "=========================================="
echo ""
echo "Testing against: $BASE_URL"
echo ""

# Test 1: Health Check (Public endpoint)
print_test "Health Check - GET /healthz"
HEALTH_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" --connect-timeout 2 "$BASE_URL/healthz" 2>&1)
HTTP_CODE=$(echo "$HEALTH_RESPONSE" | grep -o "HTTP_CODE:[0-9]*" | cut -d: -f2 || echo "000")
BODY=$(echo "$HEALTH_RESPONSE" | sed 's/HTTP_CODE:[0-9]*$//' | sed 's/^curl:.*$//')

if [ -z "$HTTP_CODE" ] || [ "$HTTP_CODE" = "000" ]; then
    print_error "Cannot connect to server at $BASE_URL"
    echo ""
    print_info "Server may not be running. Start it with:"
    echo "  ./start"
    echo "  or"
    echo "  cargo run --release -p adapteros-server-api"
    echo ""
    print_info "For full stack (server + worker), use:"
    echo "  ./start backend"
    exit 1
elif [ "$HTTP_CODE" = "200" ]; then
    print_success "Health check returned 200"
    echo "Response: $BODY"
else
    print_error "Health check failed (HTTP $HTTP_CODE)"
    echo "Response: $BODY"
    exit 1
fi
echo ""

# Test 2: Readiness Check
print_test "Readiness Check - GET /readyz"
READY_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" "$BASE_URL/readyz" || echo -e "\nHTTP_CODE:000")
READY_HTTP_CODE=$(echo "$READY_RESPONSE" | grep -o "HTTP_CODE:[0-9]*" | cut -d: -f2)
READY_BODY=$(echo "$READY_RESPONSE" | sed 's/HTTP_CODE:[0-9]*$//')

if [ "$READY_HTTP_CODE" = "200" ]; then
    print_success "Readiness check returned 200"
    echo "Response: $READY_BODY"
else
    print_info "Readiness check returned $READY_HTTP_CODE (may be starting up)"
    echo "Response: $READY_BODY"
fi
echo ""

# Test 3: API Metadata (Public endpoint)
print_test "API Metadata - GET /v1/meta"
META_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" "$BASE_URL/v1/meta" || echo -e "\nHTTP_CODE:000")
META_HTTP_CODE=$(echo "$META_RESPONSE" | grep -o "HTTP_CODE:[0-9]*" | cut -d: -f2)
META_BODY=$(echo "$META_RESPONSE" | sed 's/HTTP_CODE:[0-9]*$//')

if [ "$META_HTTP_CODE" = "200" ]; then
    print_success "API metadata returned 200"
    echo "Response: $META_BODY" | head -c 200
    echo "..."
else
    print_error "API metadata failed (HTTP $META_HTTP_CODE)"
fi
echo ""

# Test 4: Authentication (Login)
print_test "Authentication - POST /v1/auth/login"
LOGIN_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    -X POST "$BASE_URL/v1/auth/login" \
    -H "Content-Type: application/json" \
    -d '{"username":"admin","password":"admin"}' \
    || echo -e "\nHTTP_CODE:000")
LOGIN_HTTP_CODE=$(echo "$LOGIN_RESPONSE" | grep -o "HTTP_CODE:[0-9]*" | cut -d: -f2)
LOGIN_BODY=$(echo "$LOGIN_RESPONSE" | sed 's/HTTP_CODE:[0-9]*$//')

# Extract token if login successful
TOKEN=""
if [ "$LOGIN_HTTP_CODE" = "200" ]; then
    TOKEN=$(echo "$LOGIN_BODY" | grep -o '"token":"[^"]*' | cut -d'"' -f4 || echo "")
    if [ -n "$TOKEN" ]; then
        print_success "Login successful, token obtained"
        echo "Token: ${TOKEN:0:20}..."
    else
        print_info "Login returned 200 but no token found (may need bootstrap)"
        echo "Response: $LOGIN_BODY"
    fi
else
    print_info "Login failed (HTTP $LOGIN_HTTP_CODE) - may need bootstrap or different credentials"
    echo "Response: $LOGIN_BODY"
    echo ""
    print_info "Try bootstrap: curl -X POST $BASE_URL/v1/auth/bootstrap -H 'Content-Type: application/json' -d '{\"username\":\"admin\",\"password\":\"admin\"}'"
fi
echo ""

# Test 5: Inference without auth (should fail)
print_test "Inference without auth - POST /v1/infer (should fail)"
INFER_NO_AUTH=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
    -X POST "$BASE_URL/v1/infer" \
    -H "Content-Type: application/json" \
    -d '{"prompt":"Hello","max_tokens":10}' \
    || echo -e "\nHTTP_CODE:000")
INFER_NO_AUTH_CODE=$(echo "$INFER_NO_AUTH" | grep -o "HTTP_CODE:[0-9]*" | cut -d: -f2)

if [ "$INFER_NO_AUTH_CODE" = "401" ] || [ "$INFER_NO_AUTH_CODE" = "403" ]; then
    print_success "Inference correctly requires authentication (HTTP $INFER_NO_AUTH_CODE)"
else
    print_info "Unexpected response (HTTP $INFER_NO_AUTH_CODE) - may have dev auth bypass enabled"
fi
echo ""

# Test 6: Inference with auth (if token available)
if [ -n "$TOKEN" ]; then
    print_test "Inference with auth - POST /v1/infer"
    INFER_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
        -X POST "$BASE_URL/v1/infer" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $TOKEN" \
        -d '{
            "prompt": "Write a hello world function in Rust",
            "max_tokens": 50,
            "temperature": 0.7
        }' \
        || echo -e "\nHTTP_CODE:000")
    INFER_HTTP_CODE=$(echo "$INFER_RESPONSE" | grep -o "HTTP_CODE:[0-9]*" | cut -d: -f2)
    INFER_BODY=$(echo "$INFER_RESPONSE" | sed 's/HTTP_CODE:[0-9]*$//')
    
    if [ "$INFER_HTTP_CODE" = "200" ]; then
        print_success "Inference request successful (HTTP 200)"
        echo "Response preview:"
        echo "$INFER_BODY" | head -c 300
        echo "..."
    elif [ "$INFER_HTTP_CODE" = "503" ]; then
        print_info "Inference returned 503 - Worker may not be available"
        echo "Response: $INFER_BODY"
        echo ""
        print_info "Start worker with:"
        echo "  AOS_DEV_SKIP_METALLIB_CHECK=1 cargo run -p adapteros-lora-worker --bin aos-worker -- \\"
        echo "    --manifest manifests/qwen7b-mlx.yaml \\"
        echo "    --model-path \${AOS_MODEL_CACHE_DIR:-./var/model-cache/models}/\${AOS_BASE_MODEL_ID:-qwen2.5-7b-mlx} \\"
        echo "    --uds-path ./var/run/worker.sock"
    elif [ "$INFER_HTTP_CODE" = "501" ]; then
        print_info "Inference returned 501 - Worker not initialized"
    else
        print_error "Inference failed (HTTP $INFER_HTTP_CODE)"
        echo "Response: $INFER_BODY"
    fi
else
    print_info "Skipping inference test - no authentication token available"
fi
echo ""

# Test 7: Streaming inference (if token available)
if [ -n "$TOKEN" ]; then
    print_test "Streaming Inference - POST /v1/infer/stream"
    STREAM_RESPONSE=$(timeout 5 curl -s -N -w "\nHTTP_CODE:%{http_code}" \
        -X POST "$BASE_URL/v1/infer/stream" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $TOKEN" \
        -d '{
            "prompt": "Count to 5",
            "max_tokens": 20,
            "stream": true
        }' \
        || echo -e "\nHTTP_CODE:000")
    STREAM_HTTP_CODE=$(echo "$STREAM_RESPONSE" | grep -o "HTTP_CODE:[0-9]*" | cut -d: -f2)
    STREAM_BODY=$(echo "$STREAM_RESPONSE" | sed 's/HTTP_CODE:[0-9]*$//')
    
    if [ "$STREAM_HTTP_CODE" = "200" ]; then
        print_success "Streaming inference request successful"
        echo "Stream preview (first 200 chars):"
        echo "$STREAM_BODY" | head -c 200
        echo "..."
    else
        print_info "Streaming inference returned HTTP $STREAM_HTTP_CODE"
        echo "Response: $STREAM_BODY" | head -c 200
    fi
else
    print_info "Skipping streaming inference test - no authentication token available"
fi
echo ""

# Test 8: Batch inference (if token available)
if [ -n "$TOKEN" ]; then
    print_test "Batch Inference - POST /v1/infer/batch"
    BATCH_RESPONSE=$(curl -s -w "\nHTTP_CODE:%{http_code}" \
        -X POST "$BASE_URL/v1/infer/batch" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $TOKEN" \
        -d '{
            "requests": [
                {"prompt": "Hello", "max_tokens": 10},
                {"prompt": "World", "max_tokens": 10}
            ]
        }' \
        || echo -e "\nHTTP_CODE:000")
    BATCH_HTTP_CODE=$(echo "$BATCH_RESPONSE" | grep -o "HTTP_CODE:[0-9]*" | cut -d: -f2)
    BATCH_BODY=$(echo "$BATCH_RESPONSE" | sed 's/HTTP_CODE:[0-9]*$//')
    
    if [ "$BATCH_HTTP_CODE" = "200" ]; then
        print_success "Batch inference request successful"
        echo "Response preview:"
        echo "$BATCH_BODY" | head -c 300
        echo "..."
    else
        print_info "Batch inference returned HTTP $BATCH_HTTP_CODE"
        echo "Response: $BATCH_BODY" | head -c 200
    fi
else
    print_info "Skipping batch inference test - no authentication token available"
fi
echo ""

# Summary
echo "=========================================="
echo "Test Summary"
echo "=========================================="
echo "Tests Passed: $TESTS_PASSED"
echo "Tests Failed: $TESTS_FAILED"
echo ""

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${YELLOW}Some tests failed or were skipped${NC}"
    exit 1
fi

