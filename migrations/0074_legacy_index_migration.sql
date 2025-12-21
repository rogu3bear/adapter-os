-- Migration 0074: Legacy Index Migration with Version Tracking
-- Purpose: Add version tracking to index_hashes and recompute hashes for pre-v2 indexes
-- Evidence: Supports v1 event upconversion and ensures hash consistency across schema versions
-- Renumbered from crate migration 0068 to resolve conflict with root migration 0068 (metadata_normalization)

-- Add version column to index_hashes for tracking schema evolution
ALTER TABLE index_hashes ADD COLUMN version INTEGER DEFAULT 1;

-- Update legacy entries to mark for recomputation
-- Setting hash to NULL forces recomputation on next verify_index call
UPDATE index_hashes SET version = 2, hash = NULL WHERE version < 2 OR version IS NULL;

-- Create composite index for efficient version-based queries
CREATE INDEX IF NOT EXISTS idx_index_hashes_tenant_version ON index_hashes(tenant_id, version DESC);

-- Create index for finding entries needing recomputation
CREATE INDEX IF NOT EXISTS idx_index_hashes_null_hash ON index_hashes(hash) WHERE hash IS NULL;

-- Post-migration instructions:
-- Run the following command to recompute all hashes for migrated indexes:
--   cargo run --bin aosctl -- recompute-indexes --tenant all
--
-- For v1 telemetry bundles:
--   Parse legacy events using bundle.rs upconverter
--   Apply to snapshots using build_index_snapshot logic
--   Store updated hashes via store_index_hash
