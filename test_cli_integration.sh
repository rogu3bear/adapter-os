#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0

# Helper functions
log_test() {
    echo -e "\n${YELLOW}[TEST]${NC} $1"
}

log_pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((TESTS_PASSED++))
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((TESTS_FAILED++))
}

log_info() {
    echo -e "${YELLOW}[INFO]${NC} $1"
}

# Setup
log_info "Setting up test environment..."

# Create temporary database
TEST_DB=$(mktemp /tmp/aos-test-XXXXXX.db)
export DATABASE_URL="sqlite://${TEST_DB}"

log_info "Test database: ${TEST_DB}"

# Build CLI (if not already built)
if [ ! -f ./target/release/aosctl ]; then
    log_info "Building aosctl..."
    cargo build --release -p aosctl
fi

# Initialize database
log_test "Test 0: Database migration"
if ./target/release/aosctl db migrate; then
    log_pass "Database migration successful"
else
    log_fail "Database migration failed"
    exit 1
fi

# Initialize default tenant
log_test "Test 0a: Initialize default tenant"
if ./target/release/aosctl init-tenant --id default --uid 1000 --gid 1000; then
    log_pass "Default tenant initialized"
else
    log_fail "Default tenant initialization failed"
fi

# Test 1: Register adapter with version
log_test "Test 1: Register adapter with version and lifecycle"
if ./target/release/aosctl adapter register \
    --id test-adapter-001 \
    --hash b3:0000000000000000000000000000000000000000000000000000000000000000 \
    --tier tier_1 \
    --rank 16 \
    --acl default \
    --version 1.5.0 \
    --lifecycle active; then
    log_pass "Adapter registered with version 1.5.0 and lifecycle active"
else
    log_fail "Adapter registration failed"
fi

# Test 2: List adapters and verify version column
log_test "Test 2: List adapters (verify version column)"
OUTPUT=$(./target/release/aosctl adapter list 2>&1)
if echo "$OUTPUT" | grep -q "test-adapter-001"; then
    log_pass "Adapter appears in list"
    if echo "$OUTPUT" | grep -q "1.5.0"; then
        log_pass "Version 1.5.0 displayed in list"
    else
        log_fail "Version not displayed in list output"
        echo "$OUTPUT"
    fi
else
    log_fail "Adapter not found in list"
    echo "$OUTPUT"
fi

# Test 3: List with metadata flag
log_test "Test 3: List with --include-meta flag"
META_OUTPUT=$(./target/release/aosctl adapter list --include-meta 2>&1)
if echo "$META_OUTPUT" | grep -q "schema_version"; then
    log_pass "Metadata includes schema_version field"

    # Try to parse as JSON if possible
    if command -v jq &> /dev/null; then
        SCHEMA_VERSION=$(echo "$META_OUTPUT" | jq -r '.schema_version' 2>/dev/null || echo "")
        if [ -n "$SCHEMA_VERSION" ] && [ "$SCHEMA_VERSION" != "null" ]; then
            log_pass "Schema version parsed: $SCHEMA_VERSION"
        else
            log_info "Schema version field present but not in JSON format"
        fi
    else
        log_info "jq not installed, skipping JSON validation"
    fi
else
    log_fail "Metadata does not include schema_version"
    echo "$META_OUTPUT"
fi

# Test 4: Update lifecycle state (valid transition)
log_test "Test 4: Update lifecycle state (active -> deprecated)"
if ./target/release/aosctl adapter update-lifecycle test-adapter-001 deprecated; then
    log_pass "Lifecycle state updated to deprecated"
else
    log_fail "Lifecycle state update failed"
fi

# Test 5: Verify lifecycle update
log_test "Test 5: Verify lifecycle state was updated"
VERIFY_OUTPUT=$(./target/release/aosctl adapter list 2>&1)
if echo "$VERIFY_OUTPUT" | grep -q "deprecated"; then
    log_pass "Lifecycle state shows as deprecated"
else
    log_fail "Lifecycle state not updated correctly"
    echo "$VERIFY_OUTPUT"
fi

# Test 6: Attempt invalid backward transition (should fail)
log_test "Test 6: Attempt invalid backward transition (deprecated -> draft)"
if ./target/release/aosctl adapter update-lifecycle test-adapter-001 draft 2>&1 | grep -iq "error\|backward\|invalid"; then
    log_pass "Invalid transition correctly rejected"
else
    log_fail "Invalid transition was not rejected"
fi

# Test 7: Verify error message quality
log_test "Test 7: Verify error message mentions constraint/trigger"
ERROR_OUTPUT=$(./target/release/aosctl adapter update-lifecycle test-adapter-001 draft 2>&1 || true)
if echo "$ERROR_OUTPUT" | grep -iq "backward\|invalid\|constraint\|trigger"; then
    log_pass "Error message provides helpful context"
    log_info "Error message: $ERROR_OUTPUT"
else
    log_fail "Error message does not mention constraint or trigger"
    echo "$ERROR_OUTPUT"
fi

# Test 8: Register second adapter for listing tests
log_test "Test 8: Register second adapter with different version"
if ./target/release/aosctl adapter register \
    --id test-adapter-002 \
    --hash b3:1111111111111111111111111111111111111111111111111111111111111111 \
    --tier tier_2 \
    --rank 8 \
    --acl default \
    --version 2.0.1 \
    --lifecycle experimental; then
    log_pass "Second adapter registered"
else
    log_fail "Second adapter registration failed"
fi

# Test 9: Verify multiple adapters are listed
log_test "Test 9: Verify multiple adapters in listing"
LIST_OUTPUT=$(./target/release/aosctl adapter list 2>&1)
ADAPTER_COUNT=$(echo "$LIST_OUTPUT" | grep -c "test-adapter-" || echo "0")
if [ "$ADAPTER_COUNT" -eq 2 ]; then
    log_pass "Both adapters appear in list (count: $ADAPTER_COUNT)"
else
    log_fail "Expected 2 adapters, found $ADAPTER_COUNT"
    echo "$LIST_OUTPUT"
fi

# Test 10: Test version filtering (if supported)
log_test "Test 10: Verify version display for multiple adapters"
if echo "$LIST_OUTPUT" | grep -q "1.5.0" && echo "$LIST_OUTPUT" | grep -q "2.0.1"; then
    log_pass "Both versions displayed correctly"
else
    log_fail "Not all versions displayed"
    echo "$LIST_OUTPUT"
fi

# Test 11: Valid forward transition
log_test "Test 11: Valid forward transition (experimental -> active)"
if ./target/release/aosctl adapter update-lifecycle test-adapter-002 active; then
    log_pass "Valid forward transition succeeded"
else
    log_fail "Valid forward transition failed"
fi

# Test 12: Test version update (if command exists)
log_test "Test 12: Check if version update is possible"
UPDATE_HELP=$(./target/release/aosctl adapter --help 2>&1 || true)
if echo "$UPDATE_HELP" | grep -q "update.*version"; then
    log_info "Version update command appears to be available"
    # Try to update version
    if ./target/release/aosctl adapter update-version test-adapter-001 1.5.1 2>/dev/null; then
        log_pass "Version update command works"
    else
        log_info "Version update command exists but may require different syntax"
    fi
else
    log_info "Version update command not available (expected for v1)"
fi

# Cleanup
log_info "Cleaning up test database..."
rm -f "${TEST_DB}"

# Summary
echo -e "\n${YELLOW}========================================${NC}"
echo -e "${YELLOW}Test Summary${NC}"
echo -e "${YELLOW}========================================${NC}"
echo -e "${GREEN}Passed:${NC} $TESTS_PASSED"
echo -e "${RED}Failed:${NC} $TESTS_FAILED"
echo -e "${YELLOW}Total:${NC}  $((TESTS_PASSED + TESTS_FAILED))"
echo -e "${YELLOW}========================================${NC}"

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "\n${GREEN}All tests passed!${NC}\n"
    exit 0
else
    echo -e "\n${RED}Some tests failed!${NC}\n"
    exit 1
fi
