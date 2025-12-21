-- Rollback for Migration 0072: Tenant State Snapshots
-- Purpose: Remove tenant_snapshots table and associated indexes
-- Warning: This will permanently delete all tenant snapshot history

-- Drop indexes first (cascading order)
DROP INDEX IF EXISTS idx_tenant_snapshots_hash;
DROP INDEX IF EXISTS idx_tenant_snapshots_created;
DROP INDEX IF EXISTS idx_tenant_snapshots_tenant;

-- Drop the main table
DROP TABLE IF EXISTS tenant_snapshots;
