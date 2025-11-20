# UPDATE/DELETE Queries and .aos File Fields - Comprehensive Analysis

**Document Status:** Agent 13 PRD-2 Corner Fix
**Date:** 2025-11-19
**Author:** Stability Reinforcement Team
**Scope:** Database consistency for `aos_file_path` and `aos_file_hash` fields

---

## Executive Summary

All UPDATE and DELETE queries in the `adapteros-db` crate have been reviewed for database consistency regarding the `aos_file_path` and `aos_file_hash` fields added in migration 0045.

**Key Finding:** UPDATE queries intentionally use partial updates to preserve aos_file fields. This is the correct pattern.

**Status:** No query modifications needed. All queries are database-consistent.

---

## Background: .aos File Fields

From migration 0045 (`migrations/0045_aos_adapters.sql`):
```sql
-- Add .aos adapter support to existing adapters table
ALTER TABLE adapters ADD COLUMN aos_file_path TEXT;
ALTER TABLE adapters ADD COLUMN aos_file_hash TEXT;
```

These fields track:
- `aos_file_path`: Path to the .aos archive file
- `aos_file_hash`: BLAKE3 hash for integrity verification

---

## UPDATE Query Analysis

### Pattern: Partial Updates Preserve aos_file Fields

All UPDATE queries on the `adapters` table follow a consistent pattern: they update specific columns only, leaving other columns (including aos_file fields) unchanged.

**Rationale:**
- Partial UPDATE queries preserve existing column values
- This is the correct SQL pattern for updating specific fields
- Explicitly listing aos_file fields in UPDATE SET would require passing values, even when not changing them

**Example (from `adapters.rs:729`):**
```rust
sqlx::query(
    "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE adapter_id = ?"
)
.bind(state)
.bind(adapter_id)
```

This UPDATE intentionally excludes `aos_file_path` and `aos_file_hash`. When executed, these columns retain their existing values.

---

## Complete List of UPDATE Queries

### 1. crates/adapteros-db/src/adapters.rs

#### update_adapter_state (line 728-729)
```rust
sqlx::query(
    "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE adapter_id = ?"
)
```
**Fields Updated:** `current_state`, `updated_at`
**aos Fields:** ✓ Preserved
**Reason:** Partial update - doesn't touch aos_file columns

#### update_adapter_memory (line 742-743)
```rust
sqlx::query(
    "UPDATE adapters SET memory_bytes = ?, updated_at = datetime('now') WHERE adapter_id = ?"
)
```
**Fields Updated:** `memory_bytes`, `updated_at`
**aos Fields:** ✓ Preserved
**Reason:** Partial update - doesn't touch aos_file columns

#### update_adapter_state_tx (line 787-789)
```rust
sqlx::query(
    "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE adapter_id = ?"
)
```
**Fields Updated:** `current_state`, `updated_at`
**aos Fields:** ✓ Preserved
**Reason:** Transactional partial update in transaction context

#### update_adapter_memory_tx (line 830-832)
```rust
sqlx::query(
    "UPDATE adapters SET memory_bytes = ?, updated_at = datetime('now') WHERE adapter_id = ?"
)
```
**Fields Updated:** `memory_bytes`, `updated_at`
**aos Fields:** ✓ Preserved
**Reason:** Transactional partial update in transaction context

#### update_adapter_state_and_memory (line 879-882)
```rust
sqlx::query(
    "UPDATE adapters
     SET current_state = ?, memory_bytes = ?, updated_at = datetime('now')
     WHERE adapter_id = ?"
)
```
**Fields Updated:** `current_state`, `memory_bytes`, `updated_at`
**aos Fields:** ✓ Preserved
**Reason:** Atomic partial update - combines state and memory in single transaction

### 2. crates/adapteros-db/src/lib.rs (Heartbeat Recovery)

#### Stale Adapter Recovery (line 254-256)
```rust
sqlx::query(
    "UPDATE adapters SET load_state = 'unloaded', updated_at = datetime('now') WHERE adapter_id = ?"
)
```
**Fields Updated:** `load_state`, `updated_at`
**aos Fields:** ✓ Preserved
**Reason:** Partial update - heartbeat recovery doesn't affect aos_file data

#### Invalid Activation Percentage Reset (line 282-284)
```rust
sqlx::query(
    "UPDATE adapters SET activation_pct = 0.0 WHERE activation_pct > 1.0 OR activation_pct < 0.0"
)
```
**Fields Updated:** `activation_pct`
**aos Fields:** ✓ Preserved
**Reason:** Maintenance query for invalid percentages - doesn't touch aos_file columns

#### Stale Heartbeat Recovery (line 368-375)
```rust
sqlx::query(
    "UPDATE adapters
    SET load_state = 'unloaded',
        last_heartbeat = NULL,
        updated_at = datetime('now')
    WHERE adapter_id = ?"
)
```
**Fields Updated:** `load_state`, `last_heartbeat`, `updated_at`
**aos Fields:** ✓ Preserved
**Reason:** Partial update - resets stale adapters without affecting aos_file data

### 3. crates/adapteros-db/src/lifecycle.rs

#### update_adapter_lifecycle_and_version (line 96-105)
```rust
sqlx::query(
    "UPDATE adapters
     SET lifecycle_state = ?, version = ?, updated_at = datetime('now')
     WHERE adapter_id = ?"
)
```
**Fields Updated:** `lifecycle_state`, `version`, `updated_at`
**aos Fields:** ✓ Preserved
**Reason:** Partial update - lifecycle transitions don't affect aos_file metadata

### 4. crates/adapteros-db/src/validation.rs

#### update_adapter_lifecycle_state (line 78-80)
```rust
sqlx::query(
    "UPDATE adapters SET lifecycle_state = ?, updated_at = datetime('now') WHERE adapter_id = ?"
)
```
**Fields Updated:** `lifecycle_state`, `updated_at`
**aos Fields:** ✓ Preserved
**Reason:** Partial update - validation-driven state changes preserve aos_file data

#### update_adapter_version (line 125-127)
```rust
sqlx::query(
    "UPDATE adapters SET version = ?, updated_at = datetime('now') WHERE adapter_id = ?"
)
```
**Fields Updated:** `version`, `updated_at`
**aos Fields:** ✓ Preserved
**Reason:** Partial update - version bumps don't affect aos_file metadata

#### update_stack_lifecycle_state (line 188-190)
```rust
sqlx::query(
    "UPDATE adapter_stacks SET lifecycle_state = ?, updated_at = datetime('now') WHERE id = ? AND tenant_id = ?"
)
```
**Note:** This is on `adapter_stacks` table, not adapters. Stacks don't have aos_file fields.

#### update_stack_version (line 226-228)
```rust
sqlx::query(
    "UPDATE adapter_stacks SET version = ?, updated_at = datetime('now') WHERE id = ? AND tenant_id = ?"
)
```
**Note:** This is on `adapter_stacks` table, not adapters.

### 5. PostgreSQL Backend (Legacy Code)

#### postgres_adapters.rs::update_adapter_status (line 83-85)
```rust
sqlx::query("UPDATE adapters SET status = $1 WHERE id = $2")
```
**Note:** PostgreSQL backend - uses different schema (status field, not lifecycle_state/aos_file_path)

#### postgres/adapters.rs::update_adapter_status (line 86-91)
```rust
sqlx::query("UPDATE adapters SET status = $1 WHERE id = $2")
```
**Note:** PostgreSQL backend - uses different schema

---

## DELETE Query Analysis

### Pattern: Cascade Deletes via Foreign Keys

DELETE queries rely on foreign key constraints to handle cascade deletion. The `aos_adapter_metadata` table has an FK constraint with ON DELETE CASCADE:

**From migration 0045:**
```sql
FOREIGN KEY (adapter_id) REFERENCES adapters(id) ON DELETE CASCADE
```

### Complete List of DELETE Queries

#### 1. delete_adapter (line 565)
```rust
sqlx::query("DELETE FROM adapters WHERE id = ?")
```
**Behavior:** Hard delete - removes adapter and all associated records via CASCADE
**aos Fields:** Deleted via CASCADE from aos_adapter_metadata table
**Protection:** Pin enforcement prevents deletion of pinned adapters

#### 2. delete_adapter_cascade (line 626)
```rust
sqlx::query("DELETE FROM adapters WHERE id = ?")
```
**Behavior:** Transactional cascade delete
**aos Fields:** Deleted via CASCADE
**Protection:** Same as delete_adapter, with explicit transaction management

---

## SELECT Query Coverage

### SELECT Queries Already Updated

All SELECT queries in the following functions include aos_file_path and aos_file_hash:

1. **find_expired_adapters** - Checks for expired adapters
2. **list_adapters** - General adapter listing
3. **get_adapter** - Fetch single adapter by external ID
4. **list_adapters_by_category** - Filter by category
5. **list_adapters_by_scope** - Filter by scope
6. **list_adapters_by_state** - Filter by lifecycle state
7. **get_adapter_lineage** - Full lineage tree (with ancestors/descendants)
8. **get_adapter_children** - Direct children
9. **get_lineage_path** - Path from root to adapter

**Verification:** All SELECT statements contain:
```sql
... aos_file_path, aos_file_hash ...
```

---

## Database Consistency Guarantees

### INSERT Operations
- **register_adapter_extended**: aos_file fields bound explicitly
- **register_adapter_with_aos**: aos_file fields validated and bound, aos_adapter_metadata record created

### UPDATE Operations
- **Partial updates preserve aos_file fields** (intended behavior)
- **No explicit nullification** of aos_file fields in any UPDATE
- **No unintended overwrites** possible

### DELETE Operations
- **Cascade constraints** on aos_adapter_metadata ensure cleanup
- **Pin enforcement** prevents accidental deletion of critical adapters
- **Transaction protection** in delete_adapter_cascade ensures atomicity

---

## Migration Compatibility

### Migration 0045 - .aos File Support
- Adds aos_file_path and aos_file_hash columns to adapters table
- Creates aos_adapter_metadata table
- Establishes FK constraint with ON DELETE CASCADE
- Creates indices for efficient lookups

### Schema Validation Checklist
- [x] aos_file_path and aos_file_hash added to adapters table
- [x] aos_file_path and aos_file_hash included in all SELECT queries
- [x] No UPDATE queries overwrite aos_file fields unintentionally
- [x] DELETE operations properly cascade aos_adapter_metadata
- [x] INSERT operations explicitly bind aos_file fields (when provided)

---

## Test Coverage

Test suite: `crates/adapteros-db/tests/update_aos_fields_tests.rs`

Tests verify:
1. UPDATE adapter state preserves aos_file fields
2. UPDATE adapter memory preserves aos_file fields
3. Atomic UPDATE (state + memory) preserves aos_file fields
4. UPDATE version preserves aos_file fields
5. Transactional UPDATE preserves aos_file fields
6. DELETE adapter cascade deletes aos_adapter_metadata
7. SELECT queries include aos_file fields in results
8. Multiple sequential UPDATEs preserve aos_file fields

---

## Recommendations

### No Changes Required
The current implementation is correct. Partial UPDATE queries are the appropriate pattern for updating specific fields.

### Documentation
This analysis should be referenced in:
- PR reviews for adapter-related changes
- Database schema documentation
- API change guidelines

### Future Extensions
If new columns are added to the adapters table:
1. Include them in all SELECT queries that fetch adapters
2. Preserve the partial UPDATE pattern
3. Add corresponding tests

---

## Signature

```
Analysis completed by: Agent 13 (PRD-2 Corner Fix Specialist)
Verification status: All UPDATE/DELETE queries are database-consistent
No modifications required to existing queries
Recommend archiving this analysis for future reference
```
