-- Allow training dataset upload jobs in the generic jobs table.
--
-- SQLite cannot ALTER a CHECK constraint in-place, so we recreate the table.
-- Keep the schema identical aside from the expanded `kind` enum.

PRAGMA foreign_keys=off;
BEGIN;

CREATE TABLE IF NOT EXISTS jobs_new (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('build_plan','audit','replay','node_command','training_dataset_from_upload')),
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

INSERT INTO jobs_new (id, kind, tenant_id, user_id, payload_json, status, result_json, logs_path, created_at, started_at, finished_at)
SELECT id, kind, tenant_id, user_id, payload_json, status, result_json, logs_path, created_at, started_at, finished_at
FROM jobs;

DROP TABLE jobs;
ALTER TABLE jobs_new RENAME TO jobs;

CREATE INDEX IF NOT EXISTS idx_jobs_status_created_at ON jobs(status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_tenant_id ON jobs(tenant_id);

COMMIT;
PRAGMA foreign_keys=on;

