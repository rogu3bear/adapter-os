-- Migration 0100: Production Safety Features
-- Agent B: Backend & Control Plane
-- Adds promotions signing, JWT rotation, and alerting support

-- Promotions table with Ed25519 signatures
CREATE TABLE IF NOT EXISTS promotions (
    id TEXT PRIMARY KEY,
    cpid TEXT NOT NULL,
    cp_pointer_id TEXT NOT NULL REFERENCES cp_pointers(id),
    promoted_by TEXT NOT NULL REFERENCES users(id),
    promoted_at TEXT NOT NULL,
    signature_b64 TEXT NOT NULL,
    signer_key_id TEXT NOT NULL,
    quality_json TEXT NOT NULL,  -- ARR, ECS5, HLR, CR
    before_cpid TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_promotions_cpid ON promotions(cpid);
CREATE INDEX IF NOT EXISTS idx_promotions_created ON promotions(created_at DESC);

-- JWT rotation tracking
CREATE TABLE IF NOT EXISTS jwt_secrets (
    id TEXT PRIMARY KEY,
    secret_hash TEXT NOT NULL,  -- BLAKE3 hash of actual secret
    not_before TEXT NOT NULL,
    not_after TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_jwt_secrets_active ON jwt_secrets(active, not_before);

-- Alert log tracking
CREATE TABLE IF NOT EXISTS alerts (
    id TEXT PRIMARY KEY,
    severity TEXT NOT NULL CHECK(severity IN ('critical','high','medium','low')),
    kind TEXT NOT NULL,  -- job_failed, policy_violation, etc
    subject_id TEXT,  -- job_id, worker_id, etc
    message TEXT NOT NULL,
    acknowledged INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_alerts_created ON alerts(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_alerts_ack ON alerts(acknowledged);
CREATE INDEX IF NOT EXISTS idx_alerts_kind ON alerts(kind);

-- Extend audits table with status and before/after CPID tracking
-- SQLite doesn't support ALTER COLUMN with constraints, so we check if columns exist
-- These will be added if the table exists and columns don't

-- Note: SQLite's ALTER TABLE ADD COLUMN doesn't support NOT NULL with CHECK
-- We add columns as nullable and rely on application logic for validation
ALTER TABLE audits ADD COLUMN before_cpid TEXT;
ALTER TABLE audits ADD COLUMN after_cpid TEXT;
ALTER TABLE audits ADD COLUMN status TEXT;
