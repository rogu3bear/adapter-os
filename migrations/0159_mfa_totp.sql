-- MFA (TOTP + backup codes) support
-- Migration: 0157
-- Adds per-user MFA state and storage for encrypted TOTP secret and hashed backup codes

ALTER TABLE users ADD COLUMN mfa_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN mfa_secret_enc TEXT;
ALTER TABLE users ADD COLUMN mfa_backup_codes_json TEXT;
ALTER TABLE users ADD COLUMN mfa_enrolled_at TEXT;
ALTER TABLE users ADD COLUMN mfa_last_verified_at TEXT;
ALTER TABLE users ADD COLUMN mfa_recovery_last_used_at TEXT;

CREATE INDEX IF NOT EXISTS idx_users_mfa_enabled ON users(mfa_enabled);


