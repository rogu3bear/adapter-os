CREATE TABLE dataset_upload_sessions (
    session_id TEXT PRIMARY KEY,
    session_key TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    dataset_id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    normalized_file_name TEXT NOT NULL,
    total_size_bytes INTEGER NOT NULL,
    chunk_size_bytes INTEGER NOT NULL,
    content_type TEXT NOT NULL,
    expected_file_hash_b3 TEXT,
    actual_file_hash_b3 TEXT,
    received_chunks_json TEXT NOT NULL DEFAULT '{}',
    received_chunks_count INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'initiated' CHECK (status IN ('initiated','uploading','complete','failed')),
    error_message TEXT,
    temp_dir TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_dataset_upload_sessions_key
    ON dataset_upload_sessions (tenant_id, workspace_id, session_key);

CREATE INDEX idx_dataset_upload_sessions_status
    ON dataset_upload_sessions (status, created_at);

CREATE INDEX idx_dataset_upload_sessions_dataset
    ON dataset_upload_sessions (dataset_id);
