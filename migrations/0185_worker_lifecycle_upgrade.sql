-- Upgrade worker lifecycle statuses to created/registered/healthy/draining/stopped/error
-- and migrate existing rows from legacy starting/serving/crashed values.

PRAGMA foreign_keys=off;

-- Recreate workers table with new status enum
CREATE TABLE IF NOT EXISTS workers_new (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    plan_id TEXT NOT NULL REFERENCES plans(id),
    uds_path TEXT NOT NULL,
    pid INTEGER,
    status TEXT NOT NULL DEFAULT 'created' CHECK(status IN ('created','registered','healthy','draining','stopped','error')),
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
    consecutive_failures INTEGER DEFAULT 0
);

INSERT INTO workers_new (
    id, tenant_id, node_id, plan_id, uds_path, pid, status,
    memory_headroom_pct, k_current, adapters_loaded_json, started_at, last_seen_at,
    manifest_hash_b3, schema_version, api_version, registered_at, last_transition_at,
    last_transition_reason, health_status, avg_latency_ms, latency_samples,
    last_response_at, consecutive_slow_responses, consecutive_failures
)
SELECT
    id,
    tenant_id,
    node_id,
    plan_id,
    uds_path,
    pid,
    CASE status
        WHEN 'starting' THEN 'created'
        WHEN 'serving' THEN 'healthy'
        WHEN 'crashed' THEN 'error'
        ELSE status
    END AS status,
    memory_headroom_pct,
    k_current,
    adapters_loaded_json,
    started_at,
    last_seen_at,
    manifest_hash_b3,
    schema_version,
    api_version,
    registered_at,
    last_transition_at,
    last_transition_reason,
    health_status,
    avg_latency_ms,
    latency_samples,
    last_response_at,
    consecutive_slow_responses,
    consecutive_failures
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

-- Recreate worker_status_history with new status enum
CREATE TABLE IF NOT EXISTS worker_status_history_new (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    from_status TEXT,
    to_status TEXT NOT NULL CHECK(to_status IN ('created','registered','healthy','draining','stopped','error')),
    reason TEXT NOT NULL,
    actor TEXT,
    valid_transition INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO worker_status_history_new (
    id, worker_id, tenant_id, from_status, to_status, reason, actor, valid_transition, created_at
)
SELECT
    id,
    worker_id,
    tenant_id,
    CASE from_status
        WHEN 'starting' THEN 'created'
        WHEN 'serving' THEN 'healthy'
        WHEN 'crashed' THEN 'error'
        ELSE from_status
    END AS from_status,
    CASE to_status
        WHEN 'starting' THEN 'created'
        WHEN 'serving' THEN 'healthy'
        WHEN 'crashed' THEN 'error'
        ELSE to_status
    END AS to_status,
    reason,
    actor,
    valid_transition,
    created_at
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


