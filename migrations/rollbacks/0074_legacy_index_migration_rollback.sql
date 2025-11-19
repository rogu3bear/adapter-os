-- Rollback for Migration 0074: Legacy Index Migration with Version Tracking
-- Purpose: Remove version tracking from index_hashes and restore pre-migration state
-- Warning: This will restore index_hashes to v1 schema without version tracking

-- Drop version-specific indexes
DROP INDEX IF EXISTS idx_index_hashes_null_hash;
DROP INDEX IF EXISTS idx_index_hashes_tenant_version;

-- Remove version column (SQLite requires table recreation for column removal)
-- Step 1: Create temporary table without version column
CREATE TABLE IF NOT EXISTS index_hashes_temp (
    tenant_id TEXT NOT NULL,
    index_type TEXT NOT NULL,
    hash TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (tenant_id, index_type),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Step 2: Copy data (restore original hashes if available, otherwise keep current)
INSERT INTO index_hashes_temp (tenant_id, index_type, hash, updated_at)
SELECT tenant_id, index_type,
       CASE WHEN hash IS NULL THEN 'ROLLBACK_PLACEHOLDER' ELSE hash END,
       updated_at
FROM index_hashes;

-- Step 3: Drop old table
DROP TABLE index_hashes;

-- Step 4: Rename temp table to original name
ALTER TABLE index_hashes_temp RENAME TO index_hashes;

-- Step 5: Recreate indexes from migration 0073
CREATE INDEX IF NOT EXISTS idx_index_hashes_tenant ON index_hashes(tenant_id);
CREATE INDEX IF NOT EXISTS idx_index_hashes_updated ON index_hashes(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_index_hashes_type ON index_hashes(index_type);

-- Post-rollback instructions:
-- Manual hash recomputation may be required for entries with ROLLBACK_PLACEHOLDER
--   cargo run --bin aosctl -- recompute-indexes --tenant all
