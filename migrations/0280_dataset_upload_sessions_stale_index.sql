CREATE INDEX IF NOT EXISTS idx_dataset_upload_sessions_stale
    ON dataset_upload_sessions (status, updated_at);
