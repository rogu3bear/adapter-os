-- Migration: Federation Bundle Signatures
-- Purpose: Add cross-host signature chain for telemetry bundles
-- Policy Compliance: Determinism Ruleset (#2), Isolation Ruleset (#8), Telemetry Ruleset (#9)

-- Federation bundle signatures table - stores cross-host signature records
CREATE TABLE IF NOT EXISTS federation_bundle_signatures (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    host_id TEXT NOT NULL,
    bundle_hash TEXT NOT NULL,
    signature TEXT NOT NULL, -- Ed25519 signature (hex)
    prev_host_hash TEXT, -- Previous bundle hash for chain verification
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    verified INTEGER NOT NULL DEFAULT 0 -- Boolean: 0 = false, 1 = true
);

-- Index for bundle hash lookups
CREATE INDEX IF NOT EXISTS idx_federation_bundle_hash 
    ON federation_bundle_signatures(bundle_hash);

-- Index for host chain queries
CREATE INDEX IF NOT EXISTS idx_federation_host_created 
    ON federation_bundle_signatures(host_id, created_at DESC);

-- Index for verification status
CREATE INDEX IF NOT EXISTS idx_federation_verified 
    ON federation_bundle_signatures(verified);

-- Composite index for host-bundle lookups
CREATE INDEX IF NOT EXISTS idx_federation_host_bundle 
    ON federation_bundle_signatures(host_id, bundle_hash);

