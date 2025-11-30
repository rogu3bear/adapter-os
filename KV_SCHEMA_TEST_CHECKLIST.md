# KV Schema Test Implementation Checklist

**Task:** Add missing KV schema consistency tests
**Priority:** HIGH
**Blocked By:** Path conflict on `crates/adapteros-db/tests` (3 active agents)

---

## Pre-Implementation Checklist

- [x] Review existing schema tests in `schema_consistency_tests.rs`
- [x] Identify KV types: AdapterKv, TenantKv, UserKv
- [x] Analyze field compatibility between SQL and KV types
- [x] Locate existing conversion functions
- [x] Create test code for 7 new tests
- [x] Document findings and recommendations

---

## Implementation Steps

### Step 1: Wait for Path Access

**Check agent status:**
```bash
python3 scripts/dev_context.py status
```

**Wait for these contexts to release:**
- [ ] `ctx-99163` (crates/adapteros-db/tests)
- [ ] `ctx-18637` (migration_tests.rs)
- [ ] `ctx-69786` (kv_primary_reads.rs)

### Step 2: Claim Path

```bash
python3 scripts/dev_context.py claim \
  --intent "Add KV schema consistency tests for AdapterKv, TenantKv, UserKv" \
  --paths "crates/adapteros-db/tests/schema_consistency_tests.rs"
# Save the returned ctx-ID
```

### Step 3: Add Test Code

```bash
# Append the 7 new tests to the file
cat SCHEMA_KV_TESTS_TO_ADD.rs >> crates/adapteros-db/tests/schema_consistency_tests.rs
```

**Or manually copy tests 9-15 from `SCHEMA_KV_TESTS_TO_ADD.rs`**

### Step 4: Update File Header

Edit `/crates/adapteros-db/tests/schema_consistency_tests.rs`:

```rust
/// Schema Consistency Tests
///
/// These tests verify that:
/// 1. Migration application completes successfully
/// 2. Adapter struct fields match database schema columns
/// 3. INSERT statements reference valid columns
/// 4. SELECT queries reference existing columns
/// 5. KV types are field-compatible with SQL types       ← ADD
/// 6. Type conversions are lossless (round-trip)         ← ADD
///
/// Citation: Multi-agent schema audit - Phase 3 schema validation
/// Priority: CRITICAL - Prevents struct-schema drift
```

### Step 5: Run Tests Locally

```bash
# Run just the schema consistency tests
cargo test --test schema_consistency_tests

# Expected: 15 tests pass
# - 8 existing SQL schema tests
# - 7 new KV compatibility tests
```

### Step 6: Fix Any Failures

If tests fail, check:
- [ ] Import statements for KV types correct
- [ ] Conversion functions match current implementations
- [ ] Test data matches current schema constraints

### Step 7: Run Full Test Suite

```bash
# Run all DB tests
cargo test -p adapteros-db

# Ensure no regressions
```

### Step 8: Commit and Release Context

```bash
# Generate diff for your changes only
python3 scripts/dev_context.py diff --id <ctx-ID>

# Review the diff, then commit
git add crates/adapteros-db/tests/schema_consistency_tests.rs
git commit -m "test(db): add KV schema compatibility tests

Add 7 new tests verifying field compatibility and conversion
correctness for AdapterKv, TenantKv, and UserKv types:
- Round-trip conversion (SQL → KV → SQL)
- NULL handling edge cases
- Timestamp format compatibility
- Role enum conversion
- Invalid input handling

Closes schema test gap identified in multi-agent audit.

🤖 Generated with Claude Code
Co-Authored-By: Claude <noreply@anthropic.com>"

# Release context
python3 scripts/dev_context.py release --id <ctx-ID>
```

---

## Test Descriptions

### Test 9: `test_adapter_kv_field_compatibility`
**Lines:** ~100
**Purpose:** Verify all 40+ Adapter fields convert to/from AdapterKv correctly
**Key Checks:**
- Core identity fields
- JSON configuration fields
- Boolean flags (i32 ↔ i32)
- Timestamp fields
- Semantic naming taxonomy
- Round-trip preservation

### Test 10: `test_tenant_kv_field_compatibility`
**Lines:** ~50
**Purpose:** Verify all Tenant fields convert to/from TenantKv correctly
**Key Checks:**
- Status field (Option<String> ↔ String with default)
- Quota fields (Option<i32> preservation)
- Timestamp parsing (String ↔ DateTime<Utc>)
- Round-trip preservation

### Test 11: `test_user_kv_field_compatibility`
**Lines:** ~40
**Purpose:** Verify all User fields convert to/from UserKv correctly
**Key Checks:**
- Role field (String ↔ Role enum)
- Timestamp parsing
- Password hash preservation
- Round-trip preservation

### Test 12: `test_tenant_null_status_conversion`
**Lines:** ~20
**Purpose:** Verify NULL status defaults to "active"
**Edge Case:** SQL allows NULL, KV requires value

### Test 13: `test_timestamp_format_compatibility`
**Lines:** ~40
**Purpose:** Verify RFC3339 and SQLite datetime formats both parse
**Formats Tested:**
- "2025-01-15T14:30:00Z" (RFC3339)
- "2025-01-15 14:30:00" (SQLite)

### Test 14: `test_role_enum_conversion`
**Lines:** ~30
**Purpose:** Verify all 5 Role variants convert correctly
**Roles Tested:** admin, operator, sre, compliance, viewer

### Test 15: `test_invalid_role_conversion`
**Lines:** ~20
**Purpose:** Verify invalid role strings return errors (not panics)
**Invalid Input:** "super_admin" (not a valid role)

---

## Verification Checklist

After implementation, verify:

- [ ] All 15 tests pass locally
- [ ] No new clippy warnings
- [ ] No new compilation warnings
- [ ] Test coverage includes all KV types
- [ ] Round-trip conversions tested
- [ ] Edge cases covered (NULL, invalid input)
- [ ] CI pipeline includes new tests
- [ ] Documentation updated (if needed)

---

## Rollback Plan

If tests expose bugs in conversion functions:

1. **DO NOT** disable the tests
2. **DO** fix the conversion functions in:
   - `/crates/adapteros-db/src/adapters_kv.rs`
   - `/crates/adapteros-db/src/tenants_kv.rs`
   - `/crates/adapteros-db/src/users_kv.rs`
3. **DO** re-run tests after fixes
4. **DO** check if dual-write mode has caused data corruption
5. **DO** consider adding data validation migrations

---

## CI Integration

After merging, ensure CI pipeline includes:

```yaml
# .github/workflows/ci.yml
- name: Run schema consistency tests
  run: cargo test --test schema_consistency_tests

# Should fail if:
# - SQL schema changes without updating KV types
# - KV types change without updating conversions
# - Conversion functions break
```

---

## Success Criteria

✅ Tests added successfully when:
- All 15 tests pass
- Code coverage increases for `adapters_kv.rs`, `tenants_kv.rs`, `users_kv.rs`
- CI passes with new tests
- No regressions in existing tests

---

## Related Files

### Analysis Documents
- `SCHEMA_KV_CONSISTENCY_ANALYSIS.md` - Detailed type comparison matrix
- `SCHEMA_TEST_GAP_SUMMARY.md` - Executive summary of findings
- `KV_SCHEMA_TEST_CHECKLIST.md` - This file

### Test Code
- `SCHEMA_KV_TESTS_TO_ADD.rs` - Complete test code ready to merge

### Implementation Files
- `crates/adapteros-db/tests/schema_consistency_tests.rs` - Target file
- `crates/adapteros-db/src/adapters_kv.rs` - Conversion functions
- `crates/adapteros-db/src/tenants_kv.rs` - Conversion functions
- `crates/adapteros-db/src/users_kv.rs` - Conversion functions

### Schema Definitions
- `crates/adapteros-storage/src/models/adapter.rs` - AdapterKv (USED)
- `crates/adapteros-storage/src/entities/adapter.rs` - AdapterKv (NOT USED)
- `crates/adapteros-storage/src/entities/tenant.rs` - TenantKv
- `crates/adapteros-storage/src/entities/user.rs` - UserKv

---

**Status:** Ready to implement when path becomes available
**Estimated Time:** 30 minutes (copy tests + run validation)
**Risk:** LOW (tests only, no production code changes)

---

**Copyright:** 2025 JKCA / James KC Auchterlonie
