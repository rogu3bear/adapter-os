# Dual-Write Integration Test Findings

**Date:** 2025-11-29
**Agent:** Claude Code
**Task:** Create integration tests for adapter dual-write in `crates/adapteros-db/tests/`

## Summary

Created comprehensive integration tests for adapter dual-write functionality in `/Users/mln-dev/Dev/adapter-os/crates/adapteros-db/tests/adapter_dual_write_tests.rs`. However, tests revealed a critical bug in the KV storage layer that prevents dual-write from working correctly.

## Test File Created

Location: `/Users/mln-dev/Dev/adapter-os/crates/adapteros-db/tests/adapter_dual_write_tests.rs`

### Test Coverage

The test file includes 13 comprehensive tests:

1. **test_register_adapter_writes_to_both_sql_and_kv** - Verifies basic dual-write on registration
2. **test_update_adapter_state_writes_to_both** - Tests state updates propagate to both stores
3. **test_update_adapter_state_tx_writes_to_both** - Tests transactional state updates
4. **test_update_adapter_memory_writes_to_both** - Tests memory usage updates
5. **test_update_adapter_state_and_memory_writes_to_both** - Tests combined updates
6. **test_delete_adapter_removes_from_both** - Verifies deletion from both stores
7. **test_delete_adapter_cascade_removes_from_both** - Tests cascade deletion
8. **test_kv_failure_does_not_fail_sql_operation** - Verifies SQL continues on KV failure (PASSING)
9. **test_consistency_after_multiple_updates** - Tests data consistency after multiple operations
10. **test_sql_only_mode_does_not_write_to_kv** - Verifies SqlOnly mode behavior (PASSING)
11. **test_mode_transition_from_sql_to_dual_write** - Tests mode switching
12. **test_adapter_with_extended_fields** - Tests all adapter fields are preserved
13. **test_concurrent_dual_writes_maintain_consistency** - Tests concurrent operation consistency

### Test Status

- **2 Tests Passing:**
  - `test_kv_failure_does_not_fail_sql_operation`
  - `test_sql_only_mode_does_not_write_to_kv`

- **11 Tests Failing:** All KV verification tests fail due to the bug described below

## Critical Bug Discovered

### Location
`crates/adapteros-storage/src/repos/adapter.rs` and `crates/adapteros-storage/src/models/adapter.rs`

### Issue
There's a key mismatch between `create()` and `get()` operations in the AdapterRepository:

**In `create()` (line 49):**
```rust
let key = adapter.primary_key();  // Returns format!("adapter:{}", self.id)
```

**In `get()` (line 78):**
```rust
let key = format!("adapter:{}", adapter_id);  // Uses adapter_id parameter
```

### Problem
- `create()` stores adapters using the UUID (`id` field) as the key: `adapter:{UUID}`
- `get()` retrieves adapters using the external adapter_id: `adapter:{adapter_id}`
- These don't match, so adapters are never found after creation

### Example
```rust
// During create:
adapter.id = "018c-abcd-1234-...";  // UUID v7
adapter.adapter_id = Some("dual-write-test-1");  // External ID
// Stores at: "adapter:018c-abcd-1234-..."

// During get:
get("default-tenant", "dual-write-test-1")
// Looks for: "adapter:dual-write-test-1"
// NOT FOUND!
```

### Impact
- All KV writes succeed silently (no errors logged)
- All KV reads return `None`
- Dual-write appears to work but KV data is inaccessible
- This affects all adapter operations: create, update, delete, query

### Root Cause
The `AdapterKv::primary_key()` method should use `adapter_id` not `id`:

**Current (WRONG):**
```rust
pub fn primary_key(&self) -> String {
    format!("adapter:{}", self.id)
}
```

**Should be:**
```rust
pub fn primary_key(&self) -> String {
    match &self.adapter_id {
        Some(aid) => format!("adapter:{}", aid),
        None => format!("adapter:{}", self.id),  // Fallback for migration
    }
}
```

Alternatively, update `get()` to construct the key consistently with `create()`, but using `adapter_id` is more logical since it's the natural query key.

## Test Helpers Provided

The test file includes useful helpers for KV verification:

```rust
/// Set up test database with KV backend in DualWrite mode
async fn create_dual_write_db() -> (Db, TempDir, TempDir)

/// Get adapter from KV directly (bypassing Db layer)
async fn get_adapter_from_kv(db: &Db, tenant_id: &str, adapter_id: &str) -> Option<Adapter>

/// Check if adapter exists in KV
async fn adapter_exists_in_kv(db: &Db, tenant_id: &str, adapter_id: &str) -> bool
```

## Database Schema Constraints Verified

During testing, discovered important constraints:

### Valid Scopes
Only these scopes are permitted (enforced by trigger in migration 0012):
- `global`
- `tenant`
- `repo`
- `commit`

Invalid scopes like `"session"` will cause `ABORT` error.

### Adapter Naming Convention
If `adapter_name` is provided, it must match the format (enforced by trigger in migration 0061):
```
{tenant}/{domain}/{purpose}/r{NNN}
```

Example: `"testns/code/analysis/r001"`

Components:
- `tenant`: lowercase alphanumeric, no consecutive hyphens
- `domain`: lowercase alphanumeric
- `purpose`: lowercase alphanumeric
- `revision`: `r` followed by digits (e.g., `r001`, `r042`)

## Dual-Write Implementation Verified

The dual-write logic in `crates/adapteros-db/src/adapters.rs` is correctly implemented:

### Register Adapter (lines 448-530)
```rust
pub async fn register_adapter(&self, params: AdapterRegistrationParams) -> Result<String> {
    // 1. Write to SQL (primary)
    sqlx::query("INSERT INTO adapters...").execute(&*self.pool()).await?;

    // 2. Write to KV (dual-write mode)
    if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
        if let Err(e) = repo.register_adapter_kv(params.clone()).await {
            warn!("Failed to write adapter to KV backend (dual-write)");
            // Logged but doesn't fail the operation
        }
    }
    Ok(id)
}
```

### Update State (lines 954-981)
Similar pattern: SQL write first, then KV write with error logging.

### Delete Adapter (lines 666-723)
Similar pattern: SQL delete first, then KV delete with error logging.

### Error Handling
- KV write failures are **logged but don't fail** the SQL operation
- This is correct behavior per the "current behavior" requirement
- Test `test_kv_failure_does_not_fail_sql_operation` verifies this

## Coordination Note

Agent `ctx-99163` is working on `crates/adapteros-db/tests/kv_integration.rs` with intent:
"Enable disabled KV integration tests for adapter registration and CRUD operations"

Their test file (`kv_integration.rs`) contains similar test stubs but they are all marked with TODOs for KV verification. The bug discovered here will affect their tests as well.

## Recommendations

### Immediate Actions

1. **Fix the primary_key bug** in `crates/adapteros-storage/src/models/adapter.rs`:
   ```rust
   pub fn primary_key(&self) -> String {
       self.adapter_id
           .as_ref()
           .map(|aid| format!("adapter:{}", aid))
           .unwrap_or_else(|| format!("adapter:{}", self.id))
   }
   ```

2. **Add index on adapter_id** to KV backend initialization to support reverse lookups if needed

3. **Coordinate with agent ctx-99163** to share findings about the key mismatch bug

### After Bug Fix

1. Run the full test suite:
   ```bash
   cargo test --package adapteros-db --test adapter_dual_write_tests
   ```

2. All 13 tests should pass once the key mismatch is fixed

3. Verify dual-write with the debug test:
   ```bash
   cargo test --package adapteros-db --test adapter_dual_write_tests \
     test_register_adapter_writes_to_both_sql_and_kv -- --nocapture
   ```

### Additional Test Coverage Needed

After fixing the bug, consider adding:

1. **Lineage tests** - Test that parent/child relationships work in KV
2. **Migration tests** - Test SqlOnly → DualWrite → KvPrimary transitions
3. **Bulk operation tests** - Test batch registrations/updates
4. **Index query tests** - Test queries by category, scope, tier, hash
5. **Pagination tests** - Test large result set handling

## Files Modified

1. **Created:** `/Users/mln-dev/Dev/adapter-os/crates/adapteros-db/tests/adapter_dual_write_tests.rs` (638 lines)
2. **Created:** This findings document

## Test Execution

```bash
# Compile test
cargo test --package adapteros-db --test adapter_dual_write_tests --no-run

# Run all tests
cargo test --package adapteros-db --test adapter_dual_write_tests

# Run specific test with output
cargo test --package adapteros-db --test adapter_dual_write_tests \
  test_register_adapter_writes_to_both_sql_and_kv -- --nocapture

# Run with debug logging
RUST_LOG=debug cargo test --package adapteros-db --test adapter_dual_write_tests -- --nocapture
```

## Validation Evidence

Test output shows:
```
Storage mode: DualWrite
Has KV backend: true
Write to KV: true
Checking KV for adapter...
KV result: false
```

This confirms:
- ✅ Database is in DualWrite mode
- ✅ KV backend is attached
- ✅ write_to_kv() returns true
- ❌ But KV data is not retrievable (due to key mismatch bug)

---

**Copyright JKCA | 2025 James KC Auchterlonie**
