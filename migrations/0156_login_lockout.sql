-- Login lockout and rate limit state
-- Migration: 0156
-- Purpose: Track failed login attempts and lockout metadata per user

ALTER TABLE users ADD COLUMN failed_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN last_failed_at TEXT;
ALTER TABLE users ADD COLUMN lockout_until TEXT;

CREATE INDEX IF NOT EXISTS idx_users_lockout_until ON users(lockout_until);
CREATE INDEX IF NOT EXISTS idx_users_failed_attempts ON users(failed_attempts);

