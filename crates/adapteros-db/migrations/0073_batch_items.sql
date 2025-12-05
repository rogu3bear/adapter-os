-- Batch Items Table
-- Stores individual batch item requests and responses
CREATE TABLE batch_items (
    id TEXT PRIMARY KEY NOT NULL,
    batch_job_id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending', 'running', 'completed', 'failed', 'timeout')),
    request_json TEXT NOT NULL,
    response_json TEXT,
    error_code TEXT,
    error_message TEXT,
    latency_ms INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    FOREIGN KEY (batch_job_id) REFERENCES batch_jobs(id) ON DELETE CASCADE
);

CREATE INDEX idx_batch_items_job_id ON batch_items(batch_job_id);
CREATE INDEX idx_batch_items_status ON batch_items(status);
CREATE INDEX idx_batch_items_job_status ON batch_items(batch_job_id, status);
