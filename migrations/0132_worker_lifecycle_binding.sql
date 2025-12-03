-- PRD-01: Worker Lifecycle & Manifest Binding
-- Adds manifest binding columns and state transition tracking

-- Add manifest binding columns to workers table
ALTER TABLE workers ADD COLUMN manifest_hash_b3 TEXT;
ALTER TABLE workers ADD COLUMN schema_version TEXT DEFAULT '1.0';
ALTER TABLE workers ADD COLUMN api_version TEXT DEFAULT '1.0';
ALTER TABLE workers ADD COLUMN registered_at TEXT;
ALTER TABLE workers ADD COLUMN last_transition_at TEXT;
ALTER TABLE workers ADD COLUMN last_transition_reason TEXT;

-- Create worker_status_history table for lifecycle audit trail
CREATE TABLE IF NOT EXISTS worker_status_history (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    from_status TEXT,
    to_status TEXT NOT NULL CHECK(to_status IN ('starting','serving','draining','stopped','crashed')),
    reason TEXT NOT NULL,
    actor TEXT,
    valid_transition INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for worker_status_history
CREATE INDEX IF NOT EXISTS idx_worker_status_history_worker
    ON worker_status_history(worker_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_worker_status_history_tenant
    ON worker_status_history(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_worker_status_history_valid
    ON worker_status_history(valid_transition, created_at DESC);

-- Index for manifest-based routing queries
CREATE INDEX IF NOT EXISTS idx_workers_manifest_hash ON workers(manifest_hash_b3);

-- Composite index for routing: status + health + manifest
CREATE INDEX IF NOT EXISTS idx_workers_routing_composite
    ON workers(status, health_status, manifest_hash_b3, tenant_id);
