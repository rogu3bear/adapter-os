-- Expand jobs.kind CHECK constraint to allow new job types
-- (e.g. 'training_dataset_from_upload' for the async upload pipeline).
--
-- SQLite does not support ALTER CONSTRAINT, so we use the standard
-- rename-copy-drop pattern (same as 20260205120000_drop_legacy_id_columns.sql).

-- Step 1: Rename existing table
ALTER TABLE jobs RENAME TO _jobs_old;

-- Step 2: Create new table without restrictive kind CHECK
CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    tenant_id TEXT REFERENCES tenants(id) ON DELETE SET NULL,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    payload_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','running','finished','failed','cancelled')),
    result_json TEXT,
    logs_path TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    finished_at TEXT
);

-- Step 3: Copy existing data
INSERT INTO jobs (id, kind, tenant_id, user_id, payload_json, status, result_json, logs_path, created_at, started_at, finished_at)
SELECT id, kind, tenant_id, user_id, payload_json, status, result_json, logs_path, created_at, started_at, finished_at
FROM _jobs_old;

-- Step 4: Drop old table
DROP TABLE _jobs_old;

-- Step 5: Recreate index
CREATE INDEX IF NOT EXISTS idx_jobs_status_created_at ON jobs(status, created_at DESC);
