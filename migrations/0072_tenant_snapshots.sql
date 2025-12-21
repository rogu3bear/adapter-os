-- Migration 0072: Tenant State Snapshots
-- Purpose: Track tenant state snapshots for point-in-time recovery and state auditing
-- Evidence: Supports tenant isolation and snapshot-based recovery workflows
-- Renumbered from crate migration 0066 to resolve conflict with root migration 0066 (stack_versioning)

-- Create tenant snapshots table for state tracking
CREATE TABLE IF NOT EXISTS tenant_snapshots (
    tenant_id TEXT NOT NULL,
    state_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (tenant_id, created_at),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Index for efficient tenant-based queries
CREATE INDEX IF NOT EXISTS idx_tenant_snapshots_tenant ON tenant_snapshots(tenant_id);

-- Index for temporal queries (find snapshots within time range)
CREATE INDEX IF NOT EXISTS idx_tenant_snapshots_created ON tenant_snapshots(created_at DESC);

-- Index for hash-based lookups (find when specific state occurred)
CREATE INDEX IF NOT EXISTS idx_tenant_snapshots_hash ON tenant_snapshots(state_hash);
