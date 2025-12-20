-- Security Tracking Columns
-- Migration: 0213
-- Purpose: Add columns for password rotation, token rotation, and last login tracking
-- PRD: Security Hardening 3.1

-- Track when password was last rotated (NULL = never rotated since account creation)
ALTER TABLE users ADD COLUMN password_rotated_at TEXT;

-- Track when user's tokens were last rotated/invalidated (NULL = never rotated)
ALTER TABLE users ADD COLUMN token_rotated_at TEXT;

-- Track last successful login timestamp
ALTER TABLE users ADD COLUMN last_login_at TEXT;

-- Index for querying users who haven't rotated passwords recently
CREATE INDEX IF NOT EXISTS idx_users_password_rotated_at ON users(password_rotated_at);

-- Index for querying users by last login (inactive users)
CREATE INDEX IF NOT EXISTS idx_users_last_login_at ON users(last_login_at);
