-- Per-tenant weight encryption metadata
-- Tracks which adapter weight files are encrypted and with which key version.
-- Supports coexistence: plaintext files get status='plaintext', encrypted
-- files get status='encrypted' with key fingerprint and algorithm.

CREATE TABLE IF NOT EXISTS tenant_weight_encryption_keys (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL,
    key_fingerprint TEXT NOT NULL,
    algorithm TEXT NOT NULL DEFAULT 'chacha20poly1305',
    created_at TEXT NOT NULL,
    revoked_at TEXT,
    metadata TEXT
);

CREATE INDEX IF NOT EXISTS idx_twek_tenant_id
    ON tenant_weight_encryption_keys(tenant_id);
CREATE INDEX IF NOT EXISTS idx_twek_fingerprint
    ON tenant_weight_encryption_keys(key_fingerprint);

CREATE TABLE IF NOT EXISTS encrypted_weight_files (
    id TEXT PRIMARY KEY NOT NULL,
    adapter_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    encryption_status TEXT NOT NULL CHECK(encryption_status IN ('plaintext', 'encrypted')),
    key_fingerprint TEXT,
    algorithm TEXT,
    nonce_b64 TEXT,
    original_digest_hex TEXT NOT NULL,
    encrypted_at TEXT,
    UNIQUE(adapter_id, file_path)
);

CREATE INDEX IF NOT EXISTS idx_ewf_adapter_id
    ON encrypted_weight_files(adapter_id);
CREATE INDEX IF NOT EXISTS idx_ewf_tenant_id
    ON encrypted_weight_files(tenant_id);
CREATE INDEX IF NOT EXISTS idx_ewf_status
    ON encrypted_weight_files(encryption_status);
