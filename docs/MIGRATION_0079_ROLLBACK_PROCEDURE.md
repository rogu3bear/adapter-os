# Migration 0079 Rollback Procedure

**Migration:** `0079_readd_aos_file_columns.sql`
**Date Applied:** 2025-11-19
**Author:** Agent 14 - Migration Safeguards
**Purpose:** Document rollback procedures for `.aos` file column restoration

## Overview

Migration 0079 re-adds the `aos_file_path` and `aos_file_hash` columns to the `adapters` table after they were removed in migration 0059. This document provides complete rollback procedures in case rollback becomes necessary.

### Columns Added
- `adapters.aos_file_path` (TEXT, nullable)
- `adapters.aos_file_hash` (TEXT, nullable)
- Index: `idx_adapters_aos_file_hash` on `aos_file_hash`

### Related Tables
- `aos_adapter_metadata` (created in migration 0045, already exists)
- Foreign key: `aos_adapter_metadata.adapter_id` → `adapters.id`

## Pre-Rollback Validation

### Critical Checks

Before rollback, verify:

1. **No Active .aos Uploads in Progress**
   ```sql
   -- Should return 0 rows
   SELECT COUNT(*) as active_uploads FROM (
     SELECT adapter_id FROM aos_adapter_metadata
     WHERE adapter_id IN (
       SELECT id FROM adapters WHERE load_state = 'loading'
     )
   ) t;
   ```

2. **No Dependent Training Jobs**
   ```sql
   -- Should return 0 rows
   SELECT COUNT(*) as active_jobs FROM training_jobs
   WHERE status IN ('running', 'pending')
   AND dataset_id IN (
     SELECT id FROM training_datasets
     WHERE id IN (
       SELECT adapter_id FROM aos_adapter_metadata
     )
   );
   ```

3. **Adapter State Machine Consistency**
   ```sql
   -- Verify no adapters are in transition states
   SELECT COUNT(*) as invalid_states FROM adapters
   WHERE load_state NOT IN ('unloaded', 'cold', 'warm', 'hot', 'resident')
   OR current_state NOT IN ('unloaded', 'cold', 'warm', 'hot', 'resident');
   ```

4. **Database Integrity**
   ```sql
   -- Verify foreign key constraints are satisfied
   PRAGMA foreign_key_check;
   ```

## Rollback Impact Analysis

### Data Retention

- **Adapter Records:** Full history preserved
- **aos_adapter_metadata:** Retained (not deleted during rollback)
- **aos_file_path/aos_file_hash:** Columns dropped, **values discarded**
- **Actual .aos Files:** Remain on disk (must be cleaned separately)

### Impact on Active Operations

| Component | Impact | Mitigation |
|-----------|--------|-----------|
| .aos Uploads | Blocked after rollback | Prevent uploads before rollback |
| Active Adapters | Lose file path info | Track files separately if needed |
| Training Pipeline | Continues unaffected | No changes required |
| Lifecycle Manager | Continues unaffected | No changes required |

### Data Loss Scenarios

**If rolled back with active .aos files:**
- Column values are lost permanently
- Actual files remain on disk but become orphaned
- Adapter records continue functioning
- `aos_adapter_metadata` table becomes stale

**Mitigation:**
1. Backup `aos_adapter_metadata` before rollback
2. Identify and delete orphaned files
3. Validate adapter integrity post-rollback

## Rollback SQL Procedure

### Step 1: Disable Foreign Keys (if needed)

```sql
-- Only if rollback encounters foreign key constraint violations
PRAGMA foreign_keys=OFF;
```

### Step 2: Drop Index

```sql
-- Drop the index created in migration 0079
DROP INDEX IF EXISTS idx_adapters_aos_file_hash;
```

### Step 3: Drop Columns

For **SQLite 3.35.0+** (2021-03-12 or later):

```sql
-- Check SQLite version first: SELECT sqlite_version();
ALTER TABLE adapters DROP COLUMN aos_file_path;
ALTER TABLE adapters DROP COLUMN aos_file_hash;
```

### Step 4: Re-enable Foreign Keys

```sql
-- Restore foreign key enforcement
PRAGMA foreign_keys=ON;
```

### Step 5: Verify Rollback

```sql
-- Verify columns are removed
SELECT name, type FROM pragma_table_info('adapters')
WHERE name IN ('aos_file_path', 'aos_file_hash');
-- Should return empty result

-- Verify index is removed
SELECT name FROM pragma_index_list('adapters')
WHERE name = 'idx_adapters_aos_file_hash';
-- Should return empty result

-- Verify adapter table integrity
SELECT COUNT(*) FROM adapters;
SELECT COUNT(DISTINCT id) FROM adapters;
-- Counts should match

-- Verify foreign key constraints
PRAGMA foreign_key_check;
-- Should return empty result
```

## Automated Rollback Script

See `/Users/star/Dev/aos/scripts/rollback_migration_0079.sh` for automated execution.

### Usage

```bash
# Interactive mode (recommended) - shows backups and prompts
./scripts/rollback_migration_0079.sh

# Non-interactive mode - requires confirmation file
./scripts/rollback_migration_0079.sh --force
```

### What the Script Does

1. Connects to the database
2. Checks all pre-rollback conditions
3. Creates backup: `var/backup_0079_$(date +%Y%m%d_%H%M%S).sql`
4. Backs up `aos_adapter_metadata` table
5. Executes rollback SQL
6. Runs post-rollback validation
7. Generates rollback report

## File System Cleanup

### Identifying Orphaned Files

After rollback, identify .aos files without corresponding adapter records:

```bash
#!/bin/bash
# Find orphaned .aos files
ADAPTERS_DIR="/var/lib/adapteros/adapters"  # Adjust path as needed

for aos_file in "$ADAPTERS_DIR"/*.aos; do
    filename=$(basename "$aos_file")
    hash="${filename%.aos}"

    # Query database for adapter with this hash
    count=$(sqlite3 var/aos-cp.sqlite3 \
        "SELECT COUNT(*) FROM adapters WHERE hash_b3 = '$hash'")

    if [ "$count" -eq 0 ]; then
        echo "Orphaned: $aos_file"
    fi
done
```

### Cleanup Options

**Option A: Archive Orphaned Files**
```bash
mkdir -p var/orphaned_aos_files
find /var/lib/adapteros/adapters -name "*.aos" -type f \
    -exec mv {} var/orphaned_aos_files/ \;
```

**Option B: Delete Orphaned Files**
```bash
find /var/lib/adapteros/adapters -name "*.aos" -type f -delete
```

## Rollback Verification Checklist

After executing rollback, verify:

- [ ] Columns `aos_file_path` and `aos_file_hash` removed from `adapters` table
- [ ] Index `idx_adapters_aos_file_hash` removed
- [ ] All adapters still present in database
- [ ] No foreign key constraint violations
- [ ] `aos_adapter_metadata` table still exists (with stale data)
- [ ] Adapter counts unchanged
- [ ] Training jobs continue functioning
- [ ] Lifecycle manager continues functioning
- [ ] No errors in application logs

## Re-Application

To re-apply migration 0079 after rollback:

```bash
# Database is now at state before migration 0079
./target/release/aosctl db migrate

# System will re-execute migration 0079
# Migration is idempotent - safe to re-apply if columns don't exist
```

### Ensuring Idempotency

Migration 0079 uses `IF NOT EXISTS` clauses:
- `ALTER TABLE adapters ADD COLUMN IF NOT EXISTS aos_file_path`
- `CREATE INDEX IF NOT EXISTS idx_adapters_aos_file_hash`

This allows safe re-application.

## Emergency Procedures

### If Rollback Fails

1. **Restore from Backup**
   ```bash
   # Find latest backup
   ls -lt var/backup_0079_*.sql | head -1

   # Restore
   sqlite3 var/aos-cp.sqlite3 < var/backup_0079_TIMESTAMP.sql
   ```

2. **Manual Recovery**
   ```bash
   # If backup restore fails, use WAL recovery
   sqlite3 var/aos-cp.sqlite3 "PRAGMA journal_mode = WAL;"
   sqlite3 var/aos-cp.sqlite3 ".recover" > recovery.sql
   ```

3. **Contact Support**
   - Database corruption detected
   - Recovery procedure failed
   - Multiple rollback attempts unsuccessful

### If .aos Files Become Orphaned

1. Identify orphaned files (see above)
2. Check `aos_adapter_metadata` for source information
3. Manually relink if necessary:
   ```sql
   UPDATE aos_adapter_metadata SET aos_file_path = ?
   WHERE adapter_id = ? AND aos_file_path IS NULL;
   ```

## Communication Plan

### Before Rollback

1. Notify all active users
2. Prevent new .aos uploads
3. Allow 5 minutes for graceful shutdown of active jobs
4. Schedule rollback during maintenance window

### During Rollback

1. Database unavailable (5-10 seconds for SQLite)
2. Monitor system health
3. Verify post-rollback conditions

### After Rollback

1. Confirm all systems operational
2. Clear application caches
3. Resume normal operations
4. Monitor error logs for 30 minutes

## Testing Rollback Locally

Before production rollback, test with:

```bash
# Create test database
cp var/aos-cp.sqlite3 var/aos-cp-test.sqlite3

# Run rollback script against test database
DB_PATH="var/aos-cp-test.sqlite3" ./scripts/rollback_migration_0079.sh

# Verify success
sqlite3 var/aos-cp-test.sqlite3 \
    "SELECT * FROM pragma_table_info('adapters') WHERE name IN ('aos_file_path', 'aos_file_hash');"
# Should return 0 rows
```

## Appendix A: Migration Timeline

| Date | Event | Status |
|------|-------|--------|
| 2024-01-15 | Migration 0045: Add .aos columns | Completed |
| 2024-XX-XX | Migration 0059: Remove columns (unused) | Completed |
| 2025-11-19 | Migration 0079: Re-add columns (PRD-02) | Completed |
| TBD | Potential Rollback (if issues discovered) | Pending |

## Appendix B: Related Migrations

- **Migration 0045:** Original creation of columns and `aos_adapter_metadata` table
- **Migration 0059:** Removal of columns (marked as unused)
- **Migration 0079:** This migration - restoration of columns for PRD-02

## Appendix C: Decision Tree

**When to Rollback:**

```
Is there a critical issue with .aos uploads?
├─ YES
│  └─ Are there > 100 active .aos files?
│     ├─ YES → Contact Support, execute manual recovery
│     └─ NO → Execute rollback_migration_0079.sh
└─ NO
   └─ Monitor system, no rollback needed
```

## Version History

- **v1.0** (2025-11-19): Initial rollback procedure document

---

**Last Updated:** 2025-11-19
**Next Review:** When 0079 has been in production for 30 days or issue occurs
