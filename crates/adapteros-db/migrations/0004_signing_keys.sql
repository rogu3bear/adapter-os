-- Add signing public key storage for bundle verification
-- This migration adds cryptographic public key tracking to CP pointers

ALTER TABLE cp_pointers ADD COLUMN signing_public_key TEXT;

-- Add index for key lookups
CREATE INDEX idx_cp_pointers_signing_key ON cp_pointers(signing_public_key);

-- Add table for tracking bundle signatures
CREATE TABLE bundle_signatures (
    id TEXT PRIMARY KEY,
    bundle_hash_b3 TEXT NOT NULL,
    cpid TEXT NOT NULL,
    signature_hex TEXT NOT NULL,
    public_key_hex TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(bundle_hash_b3)
);

CREATE INDEX idx_bundle_signatures_cpid ON bundle_signatures(cpid);
CREATE INDEX idx_bundle_signatures_bundle_hash ON bundle_signatures(bundle_hash_b3);

