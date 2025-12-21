-- Migration: 0218
-- Purpose: Implement Tenant-Based Partitioning Strategy (Clustered Indexing)
-- Target: telemetry_bundles (High volume table)
-- Strategy: Convert to WITHOUT ROWID with PRIMARY KEY (tenant_id, created_at, id) to physically partition data by tenant and time.

PRAGMA foreign_keys = OFF;

-- 1. Create new partitioned table
CREATE TABLE telemetry_bundles_partitioned (
    id TEXT NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    cpid TEXT NOT NULL,
    path TEXT NOT NULL,
    merkle_root_b3 TEXT NOT NULL,
    start_seq INTEGER NOT NULL,
    end_seq INTEGER NOT NULL,
    event_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    
    -- Partitioning Key: Cluster by tenant, then time, then ID
    -- This optimizes SELECT * ... WHERE tenant_id=? ORDER BY created_at
    -- And optimizes insertions (append-only per tenant)
    PRIMARY KEY (tenant_id, created_at, id)
) WITHOUT ROWID;

-- 2. Backfill data
INSERT INTO telemetry_bundles_partitioned (
    id, tenant_id, cpid, path, merkle_root_b3, start_seq, end_seq, event_count, created_at
)
SELECT 
    id, tenant_id, cpid, path, merkle_root_b3, start_seq, end_seq, event_count, created_at
FROM telemetry_bundles;

-- 3. Restore Constraints & Secondary Indexes

-- Original UNIQUE(path)
CREATE UNIQUE INDEX idx_telemetry_bundles_partitioned_path 
    ON telemetry_bundles_partitioned(path);

-- Maintain uniqueness of ID for Foreign Keys referencing it (audits -> telemetry_bundles(id))
CREATE UNIQUE INDEX idx_telemetry_bundles_partitioned_id 
    ON telemetry_bundles_partitioned(id);

-- Restore secondary indexes
CREATE INDEX idx_telemetry_bundles_partitioned_cpid 
    ON telemetry_bundles_partitioned(cpid);

-- Note: idx_telemetry_bundles_tenant (tenant_id, created_at DESC) is now REDUNDANT 
-- because the Primary Key (tenant_id, created_at, id) covers it.
-- We do NOT re-create it, saving space and write overhead.

-- 4. Swap Tables
DROP TABLE telemetry_bundles;
ALTER TABLE telemetry_bundles_partitioned RENAME TO telemetry_bundles;

-- 5. Rename Indexes to canonical names
DROP INDEX IF EXISTS idx_telemetry_bundles_path;
DROP INDEX IF EXISTS idx_telemetry_bundles_cpid;
DROP INDEX IF EXISTS idx_telemetry_bundles_tenant; -- Dropping old index if it exists

-- Re-create/Rename new indexes
DROP INDEX idx_telemetry_bundles_partitioned_path;
DROP INDEX idx_telemetry_bundles_partitioned_id;
DROP INDEX idx_telemetry_bundles_partitioned_cpid;

CREATE UNIQUE INDEX idx_telemetry_bundles_path ON telemetry_bundles(path);
CREATE UNIQUE INDEX idx_telemetry_bundles_id ON telemetry_bundles(id);
CREATE INDEX idx_telemetry_bundles_cpid ON telemetry_bundles(cpid);

-- 6. Analyze for Query Planner
ANALYZE telemetry_bundles;

PRAGMA foreign_keys = ON;
