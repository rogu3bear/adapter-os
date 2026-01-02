-- Worker Lifecycle: Allow 'pending' status with nullable uds_path
--
-- Fixes schema mismatch where:
-- 1. WorkerStatus enum includes 'pending' but DB CHECK constraint did not
-- 2. pre_register_worker inserts rows without uds_path, but column was NOT NULL
--
-- This migration:
-- 1. Adds 'pending' to the status CHECK constraint
-- 2. Makes uds_path nullable (required for pending workers before socket bind)
-- 3. Adds CHECK constraint ensuring non-pending workers MUST have uds_path
--
-- The invariant enforced: A worker can only have NULL uds_path if status='pending'.
-- Once a worker transitions past pending, uds_path must be set.

PRAGMA foreign_keys=off;

-- Recreate workers table with corrected constraints
CREATE TABLE IF NOT EXISTS workers_new (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    plan_id TEXT NOT NULL REFERENCES plans(id),
    -- uds_path is now nullable: pending workers don't have a socket yet
    uds_path TEXT,
    pid INTEGER,
    -- status now includes 'pending' for pre-registered workers
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','created','registered','healthy','draining','stopped','error')),
    memory_headroom_pct REAL,
    k_current INTEGER,
    adapters_loaded_json TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT,
    manifest_hash_b3 TEXT,
    schema_version TEXT DEFAULT '1.0',
    api_version TEXT DEFAULT '1.0',
    registered_at TEXT,
    last_transition_at TEXT,
    last_transition_reason TEXT,
    health_status TEXT DEFAULT 'unknown',
    avg_latency_ms REAL,
    latency_samples INTEGER DEFAULT 0,
    last_response_at TEXT,
    consecutive_slow_responses INTEGER DEFAULT 0,
    consecutive_failures INTEGER DEFAULT 0,
    -- Additional columns from other migrations
    backend TEXT,
    model_hash_b3 TEXT,
    capabilities_json TEXT,
    -- INVARIANT: Non-pending workers MUST have a uds_path bound
    -- This prevents "half-bound" active workers
    CHECK(status = 'pending' OR uds_path IS NOT NULL)
);

-- Copy existing data (all existing rows should have uds_path since they couldn't be pending)
INSERT INTO workers_new (
    id, tenant_id, node_id, plan_id, uds_path, pid, status,
    memory_headroom_pct, k_current, adapters_loaded_json, started_at, last_seen_at,
    manifest_hash_b3, schema_version, api_version, registered_at, last_transition_at,
    last_transition_reason, health_status, avg_latency_ms, latency_samples,
    last_response_at, consecutive_slow_responses, consecutive_failures,
    backend, model_hash_b3, capabilities_json
)
SELECT
    id, tenant_id, node_id, plan_id, uds_path, pid, status,
    memory_headroom_pct, k_current, adapters_loaded_json, started_at, last_seen_at,
    manifest_hash_b3, schema_version, api_version, registered_at, last_transition_at,
    last_transition_reason, health_status, avg_latency_ms, latency_samples,
    last_response_at, consecutive_slow_responses, consecutive_failures,
    backend, model_hash_b3, capabilities_json
FROM workers;

DROP TABLE workers;
ALTER TABLE workers_new RENAME TO workers;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_workers_tenant ON workers(tenant_id);
CREATE INDEX IF NOT EXISTS idx_workers_node ON workers(node_id);
CREATE INDEX IF NOT EXISTS idx_workers_status ON workers(status);
CREATE INDEX IF NOT EXISTS idx_workers_manifest_hash ON workers(manifest_hash_b3);
CREATE INDEX IF NOT EXISTS idx_workers_routing_composite
    ON workers(status, health_status, manifest_hash_b3, tenant_id);

-- Update worker_status_history to include 'pending' in the CHECK constraint
CREATE TABLE IF NOT EXISTS worker_status_history_new (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    from_status TEXT,
    to_status TEXT NOT NULL CHECK(to_status IN ('pending','created','registered','healthy','draining','stopped','error')),
    reason TEXT NOT NULL,
    actor TEXT,
    valid_transition INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO worker_status_history_new (
    id, worker_id, tenant_id, from_status, to_status, reason, actor, valid_transition, created_at
)
SELECT
    id, worker_id, tenant_id, from_status, to_status, reason, actor, valid_transition, created_at
FROM worker_status_history;

DROP TABLE worker_status_history;
ALTER TABLE worker_status_history_new RENAME TO worker_status_history;

CREATE INDEX IF NOT EXISTS idx_worker_status_history_worker
    ON worker_status_history(worker_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_worker_status_history_tenant
    ON worker_status_history(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_worker_status_history_valid
    ON worker_status_history(valid_transition, created_at DESC);

PRAGMA foreign_keys=on;
