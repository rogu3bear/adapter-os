-- Storage migration compat:
-- - repository_training_jobs.code_commit_sha is referenced by KV migration tooling
-- - auth_sessions compatibility view for legacy user_sessions schema

ALTER TABLE repository_training_jobs
ADD COLUMN code_commit_sha TEXT;

-- Prefer canonical auth_sessions relation; provide a view when only user_sessions exists.
-- If auth_sessions already exists as a table, this is a no-op.
CREATE VIEW IF NOT EXISTS auth_sessions AS
SELECT
    jti,
    COALESCE(session_id, jti) AS session_id,
    user_id,
    tenant_id,
    device_id,
    rot_id,
    refresh_hash,
    refresh_expires_at,
    ip_address,
    user_agent,
    created_at,
    last_activity,
    CAST(
        CASE
            WHEN typeof(expires_at) = 'integer' THEN expires_at
            ELSE strftime('%s', expires_at)
        END AS INTEGER
    ) AS expires_at,
    COALESCE(locked, 0) AS locked
FROM user_sessions;
