-- Enclave operation audit trail
-- This migration adds audit logging for all enclave operations (seal/unseal/sign)
-- and key lifecycle tracking for age-based warnings

-- Enclave operation audit trail
CREATE TABLE enclave_operations (
    id TEXT PRIMARY KEY,
    timestamp INTEGER NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN ('sign','seal','unseal','get_public_key')),
    requester TEXT,
    artifact_hash TEXT,
    result TEXT NOT NULL CHECK(result IN ('success','error')),
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_enclave_ops_timestamp ON enclave_operations(timestamp DESC);
CREATE INDEX idx_enclave_ops_operation ON enclave_operations(operation);
CREATE INDEX idx_enclave_ops_result ON enclave_operations(result);

-- Key metadata for age tracking
CREATE TABLE key_metadata (
    key_label TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    source TEXT NOT NULL CHECK(source IN ('keychain','manual')),
    key_type TEXT NOT NULL CHECK(key_type IN ('signing','encryption')),
    last_checked TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_key_metadata_created_at ON key_metadata(created_at);

