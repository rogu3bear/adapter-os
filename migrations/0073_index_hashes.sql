-- Migration 0073: Index Hash Tracking
-- Purpose: Track cryptographic hashes of indexes for integrity verification and change detection
-- Evidence: Supports deterministic execution verification and index corruption detection
-- Renumbered from crate migration 0067 to resolve conflict with root migration 0067 (add_tenant_to_adapter_stacks)

-- Create index hashes table for integrity tracking
CREATE TABLE IF NOT EXISTS index_hashes (
    tenant_id TEXT NOT NULL,
    index_type TEXT NOT NULL,
    hash TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (tenant_id, index_type),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Index for efficient tenant-based queries
CREATE INDEX IF NOT EXISTS idx_index_hashes_tenant ON index_hashes(tenant_id);

-- Index for temporal queries (find recent hash updates)
CREATE INDEX IF NOT EXISTS idx_index_hashes_updated ON index_hashes(updated_at DESC);

-- Index for index type queries (find all hashes for specific index type)
CREATE INDEX IF NOT EXISTS idx_index_hashes_type ON index_hashes(index_type);
