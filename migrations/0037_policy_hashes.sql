-- Policy Hash Storage
-- 
-- Stores baseline hashes for policy packs to detect runtime mutations.
-- Part of Determinism Ruleset #2: refuse to serve if policy hashes don't match.
--
-- Fields:
--   policy_pack_id: Identifier for the policy pack (e.g., "Egress", "Determinism")
--   baseline_hash: BLAKE3 hash of the policy pack configuration (hex-encoded)
--   cpid: Control Plane ID this hash is associated with (nullable for global policies)
--   signer_pubkey: Ed25519 public key of the signer (hex-encoded, nullable)
--   created_at: Unix timestamp when the hash was first registered
--   updated_at: Unix timestamp when the hash was last updated

CREATE TABLE IF NOT EXISTS policy_hashes (
    policy_pack_id TEXT NOT NULL,
    baseline_hash TEXT NOT NULL,
    cpid TEXT,
    signer_pubkey TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (policy_pack_id, cpid)
);

-- Index for efficient CPID-based queries
CREATE INDEX IF NOT EXISTS idx_policy_hashes_cpid ON policy_hashes(cpid);

-- Index for temporal queries and cleanup operations
CREATE INDEX IF NOT EXISTS idx_policy_hashes_updated ON policy_hashes(updated_at);

-- Index for signer-based auditing
CREATE INDEX IF NOT EXISTS idx_policy_hashes_signer ON policy_hashes(signer_pubkey) WHERE signer_pubkey IS NOT NULL;

