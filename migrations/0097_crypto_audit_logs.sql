-- Crypto Audit Logs with Hash Chain
-- Migration: 0097
-- Purpose: Immutable audit trail for cryptographic operations with hash chain integrity
-- PRD: PRD-SEC-01 (Cryptographic Audit Hash Chain Persistence)

-- Crypto audit logs table with hash chain
CREATE TABLE IF NOT EXISTS crypto_audit_logs (
    -- Primary identification
    id TEXT PRIMARY KEY,

    -- Hash chain fields
    entry_hash BLOB NOT NULL,           -- BLAKE3 hash of this entry (32 bytes)
    previous_hash BLOB,                 -- Hash of previous entry (NULL for first entry)
    chain_sequence INTEGER NOT NULL UNIQUE, -- Sequential number in chain (starts at 1)

    -- Audit entry metadata
    entry_type TEXT NOT NULL,           -- Operation type (e.g., "crypto.encrypt")
    timestamp INTEGER NOT NULL,         -- Unix timestamp
    signature BLOB NOT NULL,            -- Ed25519 signature (64 bytes)

    -- Crypto operation context
    key_id TEXT,                        -- Key ID involved in operation
    user_id TEXT,                       -- User who performed operation
    result TEXT NOT NULL CHECK(result IN ('success', 'failure')),
    error_message TEXT,                 -- Error message if result = failure
    metadata TEXT NOT NULL DEFAULT '{}', -- JSON metadata

    -- Timestamps
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_crypto_audit_sequence ON crypto_audit_logs(chain_sequence);
CREATE INDEX IF NOT EXISTS idx_crypto_audit_timestamp ON crypto_audit_logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_crypto_audit_operation ON crypto_audit_logs(entry_type);
CREATE INDEX IF NOT EXISTS idx_crypto_audit_key_id ON crypto_audit_logs(key_id);
CREATE INDEX IF NOT EXISTS idx_crypto_audit_user_id ON crypto_audit_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_crypto_audit_result ON crypto_audit_logs(result);

-- Hash chain integrity index (for efficient chain traversal)
CREATE INDEX IF NOT EXISTS idx_crypto_audit_chain ON crypto_audit_logs(chain_sequence, previous_hash);
