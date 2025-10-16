-- Migration: Federation Infrastructure
-- Purpose: Peer registry, output hashes, and host identity for cross-host verification
-- Policy Compliance: Determinism Ruleset (#2), Isolation Ruleset (#8)

-- Peer registry - stores federated host information
CREATE TABLE IF NOT EXISTS federation_peers (
    host_id TEXT PRIMARY KEY,
    pubkey TEXT NOT NULL, -- Ed25519 public key (hex)
    hostname TEXT,
    registered_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT,
    attestation_metadata TEXT, -- JSON: hardware attestation info
    active INTEGER NOT NULL DEFAULT 1, -- Boolean: 0 = inactive, 1 = active
    UNIQUE(pubkey)
);

-- Index for active peer lookups
CREATE INDEX IF NOT EXISTS idx_federation_peers_active 
    ON federation_peers(active, last_seen_at DESC);

-- Output hash comparison table - stores inference output hashes for cross-host verification
CREATE TABLE IF NOT EXISTS federation_output_hashes (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    session_id TEXT NOT NULL,
    host_id TEXT NOT NULL,
    output_hash TEXT NOT NULL, -- BLAKE3 hash of output (hex)
    input_hash TEXT NOT NULL, -- BLAKE3 hash of input (hex)
    computed_at TEXT NOT NULL DEFAULT (datetime('now')),
    deterministic INTEGER NOT NULL DEFAULT 1, -- Boolean: 0 = non-deterministic, 1 = deterministic
    FOREIGN KEY (host_id) REFERENCES federation_peers(host_id)
);

-- Index for session-based lookups
CREATE INDEX IF NOT EXISTS idx_federation_output_session 
    ON federation_output_hashes(session_id, host_id);

-- Index for cross-host comparisons
CREATE INDEX IF NOT EXISTS idx_federation_output_input_hash 
    ON federation_output_hashes(input_hash, host_id);

-- Composite index for deterministic verification
CREATE INDEX IF NOT EXISTS idx_federation_output_deterministic 
    ON federation_output_hashes(session_id, deterministic);

-- Bundle signature quorum table - tracks signature thresholds and quorum status
CREATE TABLE IF NOT EXISTS federation_bundle_quorum (
    bundle_hash TEXT PRIMARY KEY,
    required_signatures INTEGER NOT NULL, -- Quorum threshold
    collected_signatures INTEGER NOT NULL DEFAULT 0,
    quorum_reached INTEGER NOT NULL DEFAULT 0, -- Boolean: 0 = false, 1 = true
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    quorum_reached_at TEXT
);

-- Index for quorum status queries
CREATE INDEX IF NOT EXISTS idx_federation_quorum_status 
    ON federation_bundle_quorum(quorum_reached, created_at DESC);

