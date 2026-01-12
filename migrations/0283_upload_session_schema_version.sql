-- Add schema version to upload sessions for forward/backward compatibility detection.
-- Sessions with mismatched versions should be treated as stale.
ALTER TABLE dataset_upload_sessions
    ADD COLUMN schema_version INTEGER NOT NULL DEFAULT 1;
