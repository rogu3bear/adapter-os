-- Add base_model_id and backend_version to inference_replay_metadata

ALTER TABLE inference_replay_metadata ADD COLUMN base_model_id TEXT;
ALTER TABLE inference_replay_metadata ADD COLUMN backend_version TEXT;

-- Optional indexes for lookup by base_model_id or backend_version (null-friendly)
CREATE INDEX IF NOT EXISTS idx_replay_metadata_base_model
    ON inference_replay_metadata(base_model_id);
CREATE INDEX IF NOT EXISTS idx_replay_metadata_backend_version
    ON inference_replay_metadata(backend_version);

