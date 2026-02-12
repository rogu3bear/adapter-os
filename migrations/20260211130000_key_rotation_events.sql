-- Key rotation event persistence
--
-- Rotation events were previously in-memory only (RotationDaemon.history Vec),
-- lost on process restart. This table persists them for audit trail completeness.

CREATE TABLE IF NOT EXISTS key_rotation_events (
    id TEXT PRIMARY KEY NOT NULL,
    key_fingerprint TEXT NOT NULL,
    rotation_type TEXT NOT NULL CHECK(rotation_type IN ('scheduled', 'manual', 'compromise', 'policy_enforced')),
    rotated_at TEXT NOT NULL,              -- ISO 8601 timestamp
    rotated_by TEXT NOT NULL,              -- actor identity (daemon, admin user, etc.)
    prev_key_fingerprint TEXT,             -- NULL for initial rotation
    deks_reencrypted INTEGER NOT NULL DEFAULT 0,
    metadata TEXT                          -- JSON blob for extra context
);

CREATE INDEX IF NOT EXISTS idx_key_rotation_events_fingerprint
    ON key_rotation_events(key_fingerprint);
CREATE INDEX IF NOT EXISTS idx_key_rotation_events_rotated_at
    ON key_rotation_events(rotated_at);
CREATE INDEX IF NOT EXISTS idx_key_rotation_events_type
    ON key_rotation_events(rotation_type);
