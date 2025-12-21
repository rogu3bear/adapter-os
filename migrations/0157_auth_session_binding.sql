-- Strengthen session records with device binding and rotation metadata
-- Adds session_id, device_id, rot_id, refresh_hash, refresh_expires_at, locked

ALTER TABLE user_sessions ADD COLUMN session_id TEXT;
ALTER TABLE user_sessions ADD COLUMN device_id TEXT;
ALTER TABLE user_sessions ADD COLUMN rot_id TEXT;
ALTER TABLE user_sessions ADD COLUMN refresh_hash TEXT;
ALTER TABLE user_sessions ADD COLUMN refresh_expires_at TEXT;
ALTER TABLE user_sessions ADD COLUMN locked INTEGER NOT NULL DEFAULT 0;

-- Backfill session_id for existing rows
UPDATE user_sessions SET session_id = COALESCE(session_id, jti);

-- Indexes for lookup and rotation enforcement
CREATE UNIQUE INDEX IF NOT EXISTS idx_user_sessions_session_id ON user_sessions(session_id);
CREATE INDEX IF NOT EXISTS idx_user_sessions_user_id_locked ON user_sessions(user_id, locked);
CREATE INDEX IF NOT EXISTS idx_user_sessions_rot_id ON user_sessions(rot_id);

