-- Registry Consolidation Migration
-- Adds missing columns from adapteros-registry to support consolidated database access
-- Part of the rusqlite → SQLx consolidation effort

-- Add activation_pct column for router activation percentage
ALTER TABLE adapters ADD COLUMN activation_pct REAL DEFAULT 0.0;

-- Add last_unloaded_at timestamp for eviction tracking
ALTER TABLE adapters ADD COLUMN last_unloaded_at TEXT;

-- Add weights_hash_b3 column to models table (from registry model hash verification)
ALTER TABLE models ADD COLUMN weights_hash_b3 TEXT;

-- Add license_text column to models table (from registry model metadata)
ALTER TABLE models ADD COLUMN license_text TEXT;

-- Add model_card_hash_b3 column to models table (from registry model metadata)
ALTER TABLE models ADD COLUMN model_card_hash_b3 TEXT;

-- Create checkpoints table (from registry checkpoint tracking)
CREATE TABLE IF NOT EXISTS checkpoints (
    cpid TEXT PRIMARY KEY,
    plan_id TEXT NOT NULL,
    manifest_hash_b3 TEXT NOT NULL,
    promoted_at TEXT NOT NULL DEFAULT (datetime('now')),
    status TEXT NOT NULL CHECK(status IN ('pending', 'active', 'retired'))
);

-- Index for checkpoint status queries
CREATE INDEX IF NOT EXISTS idx_checkpoints_status ON checkpoints(status);

-- Index for checkpoint plan_id queries
CREATE INDEX IF NOT EXISTS idx_checkpoints_plan_id ON checkpoints(plan_id);

-- Index for activation_pct queries (used in router selection)
CREATE INDEX IF NOT EXISTS idx_adapters_activation_pct ON adapters(activation_pct) WHERE activation_pct > 0;
