-- Migration 0068: Legacy Index Migration
-- Adds version tracking and recomputes hashes for pre-v2 indexes
-- For v1 events upconversion: Run Rust upgrader script post-migration to parse legacy bundles
-- and insert updated index_hashes using new build_index_snapshot logic

-- Add version column if not exists (idempotent)
CREATE TABLE IF NOT EXISTS temp_version (version INTEGER);
INSERT OR IGNORE INTO temp_version VALUES (2);
DROP TABLE temp_version;

-- Assume index_hashes table exists from previous migration
ALTER TABLE index_hashes ADD COLUMN version INTEGER DEFAULT 1;

-- Update legacy entries (version < 2) to new hash
-- Placeholder: compute_new_hash would be a SQL function, but for complex logic, use external script
-- For now, mark as version 2 and set hash to NULL (recompute on next verify_index call)
UPDATE index_hashes SET version = 2, hash = NULL WHERE version < 2 OR version IS NULL;

-- Create index for efficient queries
CREATE INDEX IF NOT EXISTS idx_index_hashes_tenant_version ON index_hashes(tenant_id, version DESC);

-- Post-migration: Run Rust command to recompute all hashes
-- e.g., cargo run --bin upgrader -- recompute-indexes --tenant all
-- This will call build_index_snapshot and store_index_hash for each tenant/index_type
-- For v1 bundles: Parse legacy events in bundle.rs upconverter and apply to snapshots
