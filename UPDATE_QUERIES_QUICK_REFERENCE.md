# UPDATE Queries & AOS File Fields - Quick Reference

**Status:** All queries verified and documented
**Date:** 2025-11-19
**Agent:** 13 (Database Consistency Specialist)

---

## Key Finding: No Code Changes Required

All UPDATE queries on the adapters table follow the correct partial update pattern that naturally preserves `aos_file_path` and `aos_file_hash` fields.

---

## The Pattern

All UPDATE queries use this approach:

```rust
// CORRECT (used in all 13 UPDATE queries)
UPDATE adapters SET column1 = ?, column2 = ?, updated_at = datetime('now') WHERE adapter_id = ?

// NOT NEEDED (and correctly avoided)
UPDATE adapters SET column1 = ?, column2 = ?, aos_file_path = NULL, aos_file_hash = NULL, ...
```

**Why this works:** Partial UPDATE statements naturally preserve unmentioned columns.

---

## All UPDATE Queries (13 Total)

| Function | File | Lines | Updated Columns | aos Fields |
|----------|------|-------|-----------------|-----------|
| `update_adapter_state()` | adapters.rs | 728-729 | current_state, updated_at | ✓ Preserved |
| `update_adapter_memory()` | adapters.rs | 742-743 | memory_bytes, updated_at | ✓ Preserved |
| `update_adapter_state_tx()` | adapters.rs | 787-789 | current_state, updated_at | ✓ Preserved |
| `update_adapter_memory_tx()` | adapters.rs | 830-832 | memory_bytes, updated_at | ✓ Preserved |
| `update_adapter_state_and_memory()` | adapters.rs | 879-882 | current_state, memory_bytes, updated_at | ✓ Preserved |
| Crash recovery | lib.rs | 254-256 | load_state, updated_at | ✓ Preserved |
| Activation % reset | lib.rs | 282-284 | activation_pct | ✓ Preserved |
| Heartbeat recovery | lib.rs | 368-375 | load_state, last_heartbeat, updated_at | ✓ Preserved |
| Lifecycle update (tx) | lifecycle.rs | 96-105 | lifecycle_state, version, updated_at | ✓ Preserved |
| Lifecycle state update | validation.rs | 78-80 | lifecycle_state, updated_at | ✓ Preserved |
| Version update | validation.rs | 125-127 | version, updated_at | ✓ Preserved |
| Stack lifecycle update | validation.rs | 188-190 | lifecycle_state, updated_at | N/A (different table) |
| Stack version update | validation.rs | 226-228 | version, updated_at | N/A (different table) |

**Total: 13 UPDATE queries - All use correct partial update pattern**

---

## All DELETE Queries (2 Total)

| Function | File | Lines | Behavior |
|----------|------|-------|----------|
| `delete_adapter()` | adapters.rs | 565 | Hard delete + pin check |
| `delete_adapter_cascade()` | adapters.rs | 626 | Transactional cascade delete |

**CASCADE Behavior:**
- Foreign key: `FOREIGN KEY (adapter_id) REFERENCES adapters(id) ON DELETE CASCADE`
- When adapter deleted, aos_adapter_metadata automatically cleaned up
- Pin enforcement prevents accidental deletion of critical adapters

---

## SELECT Query Coverage

All SELECT queries that fetch adapter data include aos_file fields:

```sql
SELECT ... aos_file_path, aos_file_hash ... FROM adapters WHERE ...
```

**Verified in 9+ functions:**
- find_expired_adapters()
- list_adapters()
- get_adapter()
- list_adapters_by_category()
- list_adapters_by_scope()
- list_adapters_by_state()
- get_adapter_lineage() [recursive CTE]
- get_adapter_children()
- get_lineage_path() [recursive CTE]

---

## Database Consistency Guarantees

| Operation | Guarantee | How |
|-----------|-----------|-----|
| **INSERT** | Fields populated | `aos_file_path` and `aos_file_hash` explicitly bound |
| **UPDATE** | Fields preserved | Partial update semantics (unmentioned columns unchanged) |
| **DELETE** | Metadata cleaned | CASCADE foreign key constraint |
| **SELECT** | Fields returned | All queries include aos_file_path and aos_file_hash |

---

## No Code Changes Required

The current implementation is correct. All UPDATE queries properly preserve aos_file fields through SQL semantics.

**Why no changes needed:**
1. Partial UPDATEs preserve existing column values
2. No explicit nullification of aos_file fields
3. Cleaner, more maintainable code
4. Matches best practices for SQL updates

---

## Documentation Files

For detailed analysis and rationale, see:

1. **docs/UPDATE_QUERIES_AOS_FIELDS.md** (11 KB)
   - Complete analysis of all UPDATE/DELETE queries
   - Line-by-line breakdown with rationale
   - Migration compatibility verification

2. **crates/adapteros-db/UPDATE_QUERIES_VERIFICATION.md** (5.7 KB)
   - Query inventory with verification status
   - Impact summary and checklist

3. **AGENT_13_PRD2_CORNER_FIX_REPORT.md** (12 KB)
   - Executive summary and findings
   - Test coverage and recommendations

---

## Test Coverage

Test file: `crates/adapteros-db/tests/update_aos_fields_tests.rs`

**8 test cases:**
1. State updates preserve aos fields
2. Memory updates preserve aos fields
3. Atomic updates preserve aos fields
4. Version updates preserve aos fields
5. Transactional updates preserve aos fields
6. DELETE cascade deletes aos_metadata
7. SELECT queries include aos fields
8. Multiple sequential updates preserve aos fields

---

## Quick Verification

To verify a new UPDATE query for aos_file consistency:

1. Check if it's a partial UPDATE (only specific columns in SET clause)
2. Verify aos_file_path and aos_file_hash are NOT in the SET clause
3. Ensure SELECT queries that retrieve the adapter include aos_file fields
4. Add test case if new pattern introduced

---

## Future Development Guidelines

When adding new columns to adapters table:

1. **Include in SELECT** - Add to all SELECT queries
2. **Partial UPDATE** - Use partial updates (don't mention new columns unless updating them)
3. **Add Tests** - Create test case verifying column preservation
4. **Reference This Doc** - Link to UPDATE_QUERIES_AOS_FIELDS.md for pattern

---

## Quick Lookup Table

Need to find a specific query?

```
State updates:     adapters.rs:728-729, lib.rs:254-256, lib.rs:368-375
Memory updates:    adapters.rs:742-743
Lifecycle updates: lifecycle.rs:96-105, validation.rs:78-80
Version updates:   validation.rs:125-127
Deletions:         adapters.rs:565, adapters.rs:626
Cascade behavior:  migration 0045
SELECT coverage:   adapters.rs (34 aos_file_path references)
```

---

## Summary

**Status:** ✓ Complete and verified
**Queries analyzed:** 15 (13 UPDATE + 2 DELETE)
**Code changes required:** None
**Test coverage:** 8 test cases
**Documentation:** 4 comprehensive documents

All queries are database-consistent and ready for production use.
