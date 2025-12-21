-- Create canonical auth_sessions table and backfill from legacy user_sessions
-- Canonical relation: auth_sessions
-- Legacy (kept for compatibility this release): user_sessions

DROP VIEW IF EXISTS auth_sessions;

CREATE TABLE IF NOT EXISTS auth_sessions (
    jti TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    session_id TEXT,
    device_id TEXT,
    rot_id TEXT,
    refresh_hash TEXT,
    refresh_expires_at TEXT,
    ip_address TEXT,
    user_agent TEXT,
    created_at TEXT NOT NULL,
    last_activity TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    locked INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_auth_sessions_session_id ON auth_sessions(session_id) WHERE session_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_auth_sessions_user_id ON auth_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_auth_sessions_tenant_id ON auth_sessions(tenant_id);
CREATE INDEX IF NOT EXISTS idx_auth_sessions_expires_at ON auth_sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_auth_sessions_user_active ON auth_sessions(user_id, last_activity DESC);
CREATE INDEX IF NOT EXISTS idx_auth_sessions_rot_id ON auth_sessions(rot_id);
CREATE INDEX IF NOT EXISTS idx_auth_sessions_user_id_locked ON auth_sessions(user_id, locked);

-- Backfill from legacy table when present and canonical table is empty.
INSERT INTO auth_sessions (
    jti,
    user_id,
    tenant_id,
    session_id,
    device_id,
    rot_id,
    refresh_hash,
    refresh_expires_at,
    ip_address,
    user_agent,
    created_at,
    last_activity,
    expires_at,
    locked
)
SELECT
    jti,
    user_id,
    tenant_id,
    session_id,
    device_id,
    rot_id,
    refresh_hash,
    refresh_expires_at,
    ip_address,
    user_agent,
    created_at,
    last_activity,
    CAST(strftime('%s', expires_at) AS INTEGER),
    locked
FROM user_sessions
WHERE EXISTS (
    SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'user_sessions'
)
AND NOT EXISTS (
    SELECT 1 FROM auth_sessions WHERE auth_sessions.jti = user_sessions.jti
);

