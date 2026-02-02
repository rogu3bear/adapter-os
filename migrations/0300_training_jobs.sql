-- Migration: Create training_jobs table for worker training task tracking
-- This table is referenced by system_stats.rs, tenants.rs, and workers.rs
-- but was never created (only repository_training_jobs exists)

CREATE TABLE IF NOT EXISTS training_jobs (
    id TEXT PRIMARY KEY,
    worker_id TEXT,
    tenant_id TEXT,
    dataset_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    progress REAL,
    progress_pct REAL,
    started_at TEXT,
    last_heartbeat INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (worker_id) REFERENCES workers(id) ON DELETE SET NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (dataset_id) REFERENCES training_datasets(id) ON DELETE SET NULL
);

-- Index for status queries (system_stats.rs)
CREATE INDEX IF NOT EXISTS idx_training_jobs_status ON training_jobs(status);

-- Index for tenant-scoped queries (tenants.rs)
CREATE INDEX IF NOT EXISTS idx_training_jobs_tenant_status ON training_jobs(tenant_id, status);

-- Index for worker-scoped queries (workers.rs)
CREATE INDEX IF NOT EXISTS idx_training_jobs_worker ON training_jobs(worker_id);
CREATE INDEX IF NOT EXISTS idx_training_jobs_worker_status ON training_jobs(worker_id, status);

-- Index for heartbeat-based stale detection
CREATE INDEX IF NOT EXISTS idx_training_jobs_heartbeat ON training_jobs(last_heartbeat)
    WHERE last_heartbeat IS NOT NULL;

-- View for stale training jobs (no heartbeat in 5 minutes)
CREATE VIEW IF NOT EXISTS stale_training_jobs AS
SELECT
    id,
    worker_id,
    tenant_id,
    status,
    last_heartbeat,
    (strftime('%s', 'now') - last_heartbeat) AS seconds_since_heartbeat
FROM training_jobs
WHERE last_heartbeat IS NOT NULL
  AND status IN ('pending', 'running')
  AND (strftime('%s', 'now') - last_heartbeat) > 300;
