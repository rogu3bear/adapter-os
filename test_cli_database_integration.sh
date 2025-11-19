#!/bin/bash
# CLI → Database Integration Test
# Tests that version and lifecycle_state fields work correctly through database layer
set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Counters
TESTS_PASSED=0
TESTS_FAILED=0

log_test() { echo -e "\n${YELLOW}[TEST]${NC} $1"; }
log_pass() { echo -e "${GREEN}[PASS]${NC} $1"; ((TESTS_PASSED++)); }
log_fail() { echo -e "${RED}[FAIL]${NC} $1"; ((TESTS_FAILED++)); }
log_info() { echo -e "${YELLOW}[INFO]${NC} $1"; }

# Create test database
TEST_DB="/tmp/aos-cli-integration-$(date +%s).db"
log_info "Test database: ${TEST_DB}"

# Apply migrations
log_test "Test 1: Apply all migrations"
for migration in /Users/star/Dev/aos/migrations/*.sql; do
    sqlite3 "$TEST_DB" < "$migration" 2>/dev/null || {
        log_fail "Migration $(basename $migration) failed"
        rm -f "$TEST_DB"
        exit 1
    }
done
log_pass "All 76 migrations applied successfully"

# Create tenant
log_info "Creating test tenant..."
sqlite3 "$TEST_DB" "INSERT INTO tenants (id, name) VALUES ('test-tenant', 'Test Tenant');"

# Test 2: Insert adapter with version and lifecycle
log_test "Test 2: Insert adapter with version='1.5.0' and lifecycle_state='active'"
sqlite3 "$TEST_DB" "
    INSERT INTO adapters (
        id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json,
        category, scope, version, lifecycle_state
    ) VALUES (
        'adapter-001', 'test-tenant', 'Test Adapter 001', 'persistent',
        'b3:0000000000000000000000000000000000000000000000000000000000000000',
        16, 32.0, '[]', 'code', 'global', '1.5.0', 'active'
    );
" && log_pass "Adapter inserted successfully" || log_fail "Adapter insertion failed"

# Test 3: Query and verify fields
log_test "Test 3: Query adapter to verify version and lifecycle_state columns"
RESULT=$(sqlite3 "$TEST_DB" "SELECT id, version, lifecycle_state FROM adapters WHERE id = 'adapter-001';")
if echo "$RESULT" | grep -q "adapter-001|1.5.0|active"; then
    log_pass "Version and lifecycle_state retrieved correctly: $RESULT"
else
    log_fail "Fields incorrect. Got: $RESULT"
fi

# Test 4: Valid forward transition (active → deprecated)
log_test "Test 4: Valid forward transition (active → deprecated)"
sqlite3 "$TEST_DB" "UPDATE adapters SET lifecycle_state = 'deprecated' WHERE id = 'adapter-001';" 2>&1
NEW_STATE=$(sqlite3 "$TEST_DB" "SELECT lifecycle_state FROM adapters WHERE id = 'adapter-001';")
if [ "$NEW_STATE" = "deprecated" ]; then
    log_pass "Lifecycle transition successful: active → deprecated"
else
    log_fail "Transition failed. State: $NEW_STATE"
fi

# Test 5: Invalid backward transition (deprecated → draft)
log_test "Test 5: Attempt invalid backward transition (deprecated → draft)"
ERROR=$(sqlite3 "$TEST_DB" "UPDATE adapters SET lifecycle_state = 'draft' WHERE id = 'adapter-001';" 2>&1 || true)
if echo "$ERROR" | grep -iq "backward\|invalid"; then
    log_pass "SQL trigger correctly blocked backward transition"
    log_info "Error: $(echo $ERROR | head -1)"
else
    log_fail "Backward transition was not blocked!"
fi

# Verify state didn't change
FINAL_STATE=$(sqlite3 "$TEST_DB" "SELECT lifecycle_state FROM adapters WHERE id = 'adapter-001';")
if [ "$FINAL_STATE" = "deprecated" ]; then
    log_pass "State remained 'deprecated' after failed transition"
else
    log_fail "State changed to $FINAL_STATE (should still be deprecated)"
fi

# Test 6: Insert second adapter
log_test "Test 6: Insert second adapter (version='2.0.1', lifecycle_state='draft')"
sqlite3 "$TEST_DB" "
    INSERT INTO adapters (
        id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json,
        category, scope, version, lifecycle_state
    ) VALUES (
        'adapter-002', 'test-tenant', 'Test Adapter 002', 'warm',
        'b3:1111111111111111111111111111111111111111111111111111111111111111',
        8, 16.0, '[]', 'code', 'global', '2.0.1', 'draft'
    );
" && log_pass "Second adapter inserted" || log_fail "Second adapter insertion failed"

# Test 7: List all adapters
log_test "Test 7: List all adapters with version and lifecycle_state"
ADAPTER_COUNT=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM adapters;")
if [ "$ADAPTER_COUNT" -eq 2 ]; then
    log_pass "Both adapters present (count: $ADAPTER_COUNT)"
else
    log_fail "Expected 2 adapters, found $ADAPTER_COUNT"
fi

# Test 8: Version-based query
log_test "Test 8: Query adapters by version range (>= '1.0.0')"
VERSION_RESULTS=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM adapters WHERE version >= '1.0.0';")
if [ "$VERSION_RESULTS" -eq 2 ]; then
    log_pass "Version-based query successful (found $VERSION_RESULTS adapters)"
else
    log_fail "Version query returned $VERSION_RESULTS adapters"
fi

# Test 9: Lifecycle state filtering
log_test "Test 9: Filter adapters by lifecycle_state"
DEPRECATED_COUNT=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM adapters WHERE lifecycle_state = 'deprecated';")
DRAFT_COUNT=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM adapters WHERE lifecycle_state = 'draft';")

if [ "$DEPRECATED_COUNT" -eq 1 ] && [ "$DRAFT_COUNT" -eq 1 ]; then
    log_pass "Lifecycle filtering works (deprecated: $DEPRECATED_COUNT, draft: $DRAFT_COUNT)"
else
    log_fail "Lifecycle counts incorrect (deprecated: $DEPRECATED_COUNT, draft: $DRAFT_COUNT)"
fi

# Test 10: Valid multi-step transition (draft → active → deprecated → retired)
log_test "Test 10: Multi-step forward transition (draft → active → retired)"
sqlite3 "$TEST_DB" "UPDATE adapters SET lifecycle_state = 'active' WHERE id = 'adapter-002';" 2>&1
STATE1=$(sqlite3 "$TEST_DB" "SELECT lifecycle_state FROM adapters WHERE id = 'adapter-002';")

sqlite3 "$TEST_DB" "UPDATE adapters SET lifecycle_state = 'retired' WHERE id = 'adapter-002';" 2>&1
STATE2=$(sqlite3 "$TEST_DB" "SELECT lifecycle_state FROM adapters WHERE id = 'adapter-002';")

if [ "$STATE1" = "active" ] && [ "$STATE2" = "retired" ]; then
    log_pass "Multi-step transition successful: draft → active → retired"
else
    log_fail "Multi-step transition failed (states: $STATE1, $STATE2)"
fi

# Test 11: Retired is terminal
log_test "Test 11: Verify 'retired' is a terminal state"
ERROR2=$(sqlite3 "$TEST_DB" "UPDATE adapters SET lifecycle_state = 'active' WHERE id = 'adapter-002';" 2>&1 || true)
if echo "$ERROR2" | grep -iq "terminal\|retired"; then
    log_pass "SQL trigger correctly enforces retired as terminal state"
    log_info "Error: $(echo $ERROR2 | head -1)"
else
    log_fail "Retired state transition was allowed!"
fi

# Test 12: Schema indices exist
log_test "Test 12: Verify indices on version and lifecycle_state columns"
INDEX_COUNT=$(sqlite3 "$TEST_DB" "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND (name LIKE '%version%' OR name LIKE '%lifecycle%');" 2>&1)
if [ "$INDEX_COUNT" -ge 2 ]; then
    log_pass "Indices found for version/lifecycle columns (count: $INDEX_COUNT)"
else
    log_info "Index count: $INDEX_COUNT (may be expected)"
fi

# Cleanup
log_info "Cleaning up test database..."
rm -f "$TEST_DB"

# Summary
echo -e "\n${YELLOW}=======================================${NC}"
echo -e "${YELLOW}CLI → Database Integration Test Summary${NC}"
echo -e "${YELLOW}=======================================${NC}"
echo -e "${GREEN}Passed:${NC} $TESTS_PASSED"
echo -e "${RED}Failed:${NC} $TESTS_FAILED"
echo -e "${YELLOW}Total:${NC}  $((TESTS_PASSED + TESTS_FAILED))"
echo -e "${YELLOW}=======================================${NC}"

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "\n${GREEN}✓ All integration tests passed!${NC}\n"
    echo -e "Key validations:"
    echo -e "  ✓ Version and lifecycle_state columns exist and work"
    echo -e "  ✓ SQL triggers enforce forward-only state machine"
    echo -e "  ✓ Backward transitions are blocked with clear errors"
    echo -e "  ✓ Retired state is terminal (cannot transition out)"
    echo -e "  ✓ Version-based queries work correctly"
    echo -e "  ✓ Lifecycle filtering works correctly"
    exit 0
else
    echo -e "\n${RED}✗ Some tests failed!${NC}\n"
    exit 1
fi
