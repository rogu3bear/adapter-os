-- Trusted adapter signing keys for signature verification during import
-- Each tenant can register one or more Ed25519 public keys for adapter signing
-- Keys can be revoked without deletion to maintain audit trail

CREATE TABLE IF NOT EXISTS trusted_adapter_keys (
    tenant_id TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key_hex TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    revoked_at TEXT,
    PRIMARY KEY (tenant_id, key_id)
);

-- Index for efficient tenant lookups
CREATE INDEX IF NOT EXISTS idx_trusted_adapter_keys_tenant
    ON trusted_adapter_keys(tenant_id);

-- Index for finding active (non-revoked) keys
CREATE INDEX IF NOT EXISTS idx_trusted_adapter_keys_active
    ON trusted_adapter_keys(tenant_id, revoked_at)
    WHERE revoked_at IS NULL;
