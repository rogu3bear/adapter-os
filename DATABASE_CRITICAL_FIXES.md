# Database Critical Fixes - Implementation Summary

**Date:** 2025-11-27
**Author:** Claude (AI Assistant)
**Status:** Implemented, awaiting testing

## Overview

This document summarizes the implementation of CRITICAL and HIGH database integrity fixes for AdapterOS. All fixes address race conditions, atomicity issues, and foreign key enforcement gaps.

---

## CRITICAL Fixes Implemented

### 1. Atomic Crash Recovery (lib.rs:238-350, 352-439)

**Issue:** `recover_from_crash()` and `recover_stale_adapters()` performed multiple non-atomic operations, risking partial recovery if crash occurred during recovery.

**Fix:** Wrapped entire recovery process in single transaction.

**Changes:**
```rust
// BEFORE: Multiple separate queries to pool
sqlx::query(...).execute(&self.pool).await?;
sqlx::query(...).execute(&self.pool).await?;

// AFTER: Single atomic transaction
let mut tx = self.pool.begin().await?;
sqlx::query(...).execute(&mut *tx).await?;
sqlx::query(...).execute(&mut *tx).await?;
tx.commit().await?;
```

**Files Modified:**
- `crates/adapteros-db/src/lib.rs` (lines 238-439)

**Impact:** Prevents partial recovery states, ensures all-or-nothing recovery.

---

### 2. Row Locking in Lifecycle State Transitions (lifecycle.rs:49-144)

**Issue:** SQLite default READ_COMMITTED isolation allows race conditions between SELECT and UPDATE in lifecycle transitions. Two concurrent transitions could read same version and both update it.

**Fix:** Use IMMEDIATE transaction mode to acquire write lock before SELECT.

**Changes:**
```rust
// BEFORE: Regular transaction (deferred lock acquisition)
let mut tx = self.pool().begin().await?;
let row = sqlx::query("SELECT lifecycle_state, version ...").fetch_optional(&mut *tx).await?;

// AFTER: IMMEDIATE transaction (immediate write lock)
let mut tx = self.pool().begin().await?;
sqlx::query("BEGIN IMMEDIATE").execute(&mut *tx).await?;
let row = sqlx::query("SELECT lifecycle_state, version ...").fetch_optional(&mut *tx).await?;
```

**Files Modified:**
- `crates/adapteros-db/src/lifecycle.rs` (lines 49-144, 148-227)

**Applied To:**
- `transition_adapter_lifecycle()`
- `transition_stack_lifecycle()`

**Impact:** Prevents lost updates and version conflicts in concurrent lifecycle transitions.

---

### 3. Atomic Version Increment (sqlite_backend.rs:146-194)

**Issue:** Race condition in `update_stack()`: read current stack, check if version should increment, then update separately. Two concurrent updates could both read v1, both decide to increment, resulting in lost update.

**Fix:** Single atomic UPDATE with conditional version increment using SQL CASE expression.

**Changes:**
```rust
// BEFORE: Read-then-update pattern
let current = self.get_stack(tenant_id, id).await?;
let should_increment_version = /* check changes */;
if should_increment_version {
    sqlx::query("UPDATE ... version = version + 1 ...").execute(...).await?;
} else {
    sqlx::query("UPDATE ... /* no version change */ ...").execute(...).await?;
}

// AFTER: Single atomic UPDATE with conditional logic
sqlx::query(r#"
    UPDATE adapter_stacks
    SET name = ?, description = ?, adapter_ids_json = ?, workflow_type = ?,
        version = CASE
            WHEN adapter_ids_json != ? OR workflow_type != ?
            THEN version + 1
            ELSE version
        END,
        updated_at = datetime('now')
    WHERE tenant_id = ? AND id = ?
"#).execute(...).await?;
```

**Files Modified:**
- `crates/adapteros-db/src/sqlite_backend.rs` (lines 146-194)

**Impact:** Eliminates SELECT-then-UPDATE race window, ensures version increments are serialized.

---

### 4. PRAGMA foreign_keys = ON Enforcement

**Issue:** Foreign key constraints not enforced by default in SQLite, leading to orphaned records.

**Fix:** Enable foreign_keys pragma on all connections via SqliteConnectOptions.

**Changes:**
```rust
// BEFORE:
let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path))?
    .create_if_missing(true)
    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
    .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
    .busy_timeout(Duration::from_secs(30))
    .statement_cache_capacity(100);

// AFTER:
let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path))?
    .create_if_missing(true)
    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
    .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
    .busy_timeout(Duration::from_secs(30))
    .statement_cache_capacity(100)
    .foreign_keys(true); // CRITICAL: Enable foreign key constraints
```

**Files Modified:**
- `crates/adapteros-db/src/lib.rs` (line 59)
- `crates/adapteros-db/src/sqlite_backend.rs` (line 27)

**Impact:** All FK constraints now enforced, preventing orphaned records and referential integrity violations.

---

## HIGH Fixes Implemented

### 5. FK Cascade on Stack Deletion (Migration 0105)

**Issue:** `adapter_stacks.tenant_id` had no FK constraint, allowing orphaned stacks when tenant deleted.

**Fix:** Recreate `adapter_stacks` table with proper FK constraint including CASCADE.

**Schema Change:**
```sql
-- NEW constraint added:
CONSTRAINT fk_adapter_stacks_tenant FOREIGN KEY (tenant_id)
    REFERENCES tenants(id) ON DELETE CASCADE
```

**Files Created:**
- `migrations/0105_database_critical_fixes.sql`

**Details:**
- SQLite doesn't support adding FK constraints to existing columns
- Migration recreates table with FK constraint
- All data preserved during migration
- Indexes and triggers recreated

**Impact:** Deleting tenant automatically cascades to delete all its stacks, preventing orphaned data.

---

### 6. Activation Count CHECK Constraint (Migration 0105)

**Issue:** No constraint preventing negative `adapters.activation_count` values.

**Fix:** Add triggers to enforce `activation_count >= 0`.

**Schema Change:**
```sql
-- Enforce on UPDATE
CREATE TRIGGER IF NOT EXISTS enforce_activation_count_non_negative
BEFORE UPDATE ON adapters
FOR EACH ROW
WHEN NEW.activation_count < 0
BEGIN
    SELECT RAISE(ABORT, 'activation_count cannot be negative');
END;

-- Enforce on INSERT
CREATE TRIGGER IF NOT EXISTS enforce_activation_count_non_negative_insert
BEFORE INSERT ON adapters
FOR EACH ROW
WHEN NEW.activation_count < 0
BEGIN
    SELECT RAISE(ABORT, 'activation_count cannot be negative');
END;
```

**Files Modified:**
- `migrations/0105_database_critical_fixes.sql`

**Impact:** Database rejects any attempt to set negative activation_count, preventing data corruption.

---

## Migration Details

### Migration 0105: Database Critical Fixes

**File:** `migrations/0105_database_critical_fixes.sql`

**Operations:**
1. Recreate `adapter_stacks` table with FK constraint
2. Preserve all existing data
3. Recreate all indexes: `idx_adapter_stacks_name`, `idx_adapter_stacks_created_at`, `idx_adapter_stacks_tenant`
4. Recreate `validate_stack_name_format` trigger
5. Add `enforce_activation_count_non_negative` trigger (UPDATE)
6. Add `enforce_activation_count_non_negative_insert` trigger (INSERT)

**Status:** Created, needs signature before deployment

**Verification Queries:**
```sql
-- Verify FK constraint:
SELECT sql FROM sqlite_master WHERE name='adapter_stacks';
-- Should show: FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE

-- Verify triggers:
SELECT name FROM sqlite_master WHERE type='trigger'
  AND (name LIKE '%activation_count%' OR name LIKE '%validate_stack%');
-- Should show 3 triggers
```

---

## Testing Recommendations

### Unit Tests

1. **Test atomic crash recovery:**
   ```rust
   #[tokio::test]
   async fn test_crash_recovery_atomicity() {
       // Insert adapters in loading state
       // Simulate crash during recovery
       // Verify either all recovered or none
   }
   ```

2. **Test lifecycle race conditions:**
   ```rust
   #[tokio::test]
   async fn test_concurrent_lifecycle_transitions() {
       // Start two concurrent transitions on same adapter
       // Verify one succeeds, one fails with lock error
       // Verify version incremented only once
   }
   ```

3. **Test version increment atomicity:**
   ```rust
   #[tokio::test]
   async fn test_concurrent_stack_updates() {
       // Start two concurrent stack updates
       // Verify version incremented correctly
       // Verify no lost updates
   }
   ```

4. **Test FK enforcement:**
   ```rust
   #[tokio::test]
   async fn test_tenant_cascade_delete() {
       // Create tenant with stacks
       // Delete tenant
       // Verify all stacks deleted
   }
   ```

5. **Test activation_count constraint:**
   ```rust
   #[tokio::test]
   async fn test_negative_activation_count_rejected() {
       // Try to set activation_count = -1
       // Verify operation rejected
   }
   ```

### Integration Tests

1. Run full migration suite on test database
2. Verify all constraints and triggers created
3. Run concurrent workload tests
4. Verify no orphaned records after tenant deletion

---

## Performance Impact

### Positive Impacts:
- **Reduced contention:** Immediate transactions acquire locks faster, reducing lock wait times
- **Fewer queries:** Atomic updates eliminate SELECT-then-UPDATE patterns
- **Better isolation:** Prevents phantom reads and lost updates

### Potential Concerns:
- **Lock duration:** IMMEDIATE transactions hold locks longer (mitigated by fast operations)
- **Migration time:** Table recreation in 0105 takes O(n) time proportional to stack count
  - For 10,000 stacks: ~500ms estimated
  - For 100,000 stacks: ~5s estimated

---

## Rollback Plan

### Code Rollback:
1. Revert commits to `crates/adapteros-db/src/lib.rs`
2. Revert commits to `crates/adapteros-db/src/lifecycle.rs`
3. Revert commits to `crates/adapteros-db/src/sqlite_backend.rs`

### Migration Rollback:
```sql
-- Rollback 0105 (if needed):
-- 1. Drop new table
DROP TABLE IF EXISTS adapter_stacks;

-- 2. Recreate old table (without FK constraint)
CREATE TABLE adapter_stacks (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    name TEXT UNIQUE NOT NULL,
    description TEXT,
    adapter_ids_json TEXT NOT NULL,
    workflow_type TEXT,
    version TEXT NOT NULL DEFAULT '1.0.0',
    lifecycle_state TEXT NOT NULL DEFAULT 'active',
    created_by TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    CONSTRAINT valid_workflow_type CHECK (
        workflow_type IS NULL OR
        workflow_type IN ('Parallel', 'UpstreamDownstream', 'Sequential')
    )
);

-- 3. Restore data from backup

-- 4. Drop triggers
DROP TRIGGER IF EXISTS enforce_activation_count_non_negative;
DROP TRIGGER IF EXISTS enforce_activation_count_non_negative_insert;
```

---

## Deployment Checklist

- [ ] Code review completed
- [ ] Unit tests written and passing
- [ ] Integration tests passing
- [ ] Migration 0105 signed
- [ ] Backup created before migration
- [ ] Migration tested on staging database
- [ ] Performance impact measured on staging
- [ ] Rollback plan validated
- [ ] Documentation updated
- [ ] Deploy to production
- [ ] Verify FK constraints active: `PRAGMA foreign_keys;` returns 1
- [ ] Monitor error logs for FK violations
- [ ] Monitor performance metrics

---

## Summary Statistics

**Files Modified:** 3 Rust source files
**Files Created:** 1 SQL migration file
**Lines Changed:** ~150 lines of Rust code
**Migration Operations:** 10 DDL statements

**Risk Level:** Medium
- CRITICAL fixes address data integrity issues
- Migration involves table recreation (requires careful testing)
- No data loss expected, but backup mandatory

**Estimated Testing Time:** 4-6 hours
**Estimated Deployment Time:** 15 minutes (includes migration + verification)

---

## References

- [SQLite Foreign Key Support](https://www.sqlite.org/foreignkeys.html)
- [SQLite Transaction Modes](https://www.sqlite.org/lang_transaction.html)
- [SQLx Documentation](https://docs.rs/sqlx/)
- AdapterOS CLAUDE.md (Architecture Standards)
- AdapterOS DATABASE_REFERENCE.md

---

**Sign-off:**
- Implementation: Claude AI Assistant
- Review Required: James KC Auchterlonie
- Approval Required: Technical Lead
