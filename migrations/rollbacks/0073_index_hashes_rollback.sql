-- Rollback for Migration 0073: Index Hash Tracking
-- Purpose: Remove index_hashes table and associated indexes
-- Warning: This will permanently delete all index hash integrity data
-- Note: Migration 0074 depends on this table, rollback 0074 first before rolling back 0073

-- Drop indexes first (cascading order)
DROP INDEX IF EXISTS idx_index_hashes_type;
DROP INDEX IF EXISTS idx_index_hashes_updated;
DROP INDEX IF EXISTS idx_index_hashes_tenant;

-- Drop the main table
DROP TABLE IF EXISTS index_hashes;
