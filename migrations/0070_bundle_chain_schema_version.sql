-- Migration: Bundle Chain and Schema Versioning
-- Purpose: Add prev_bundle_hash and schema_version for deterministic hydration
-- Policy Compliance: Determinism Ruleset (#2), Telemetry Ruleset (#9)

-- Add prev_bundle_hash column for Merkle chain verification
ALTER TABLE telemetry_bundles ADD COLUMN prev_bundle_hash TEXT;

-- Add schema_version column for bundle format migrations
ALTER TABLE telemetry_bundles ADD COLUMN schema_version INTEGER NOT NULL DEFAULT 1;

-- Index for chain verification queries
CREATE INDEX IF NOT EXISTS idx_telemetry_bundles_prev_hash
    ON telemetry_bundles(prev_bundle_hash);

-- Index for schema version filtering
CREATE INDEX IF NOT EXISTS idx_telemetry_bundles_schema_version
    ON telemetry_bundles(schema_version);
