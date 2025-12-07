-- API keys (hashed storage)
-- Stores per-tenant API keys with scoped roles and one-way BLAKE3 hashes
CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    scopes TEXT NOT NULL, -- JSON array of Role strings
    hash TEXT NOT NULL,   -- BLAKE3 hex digest of the token
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    revoked_at TEXT,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE ON UPDATE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE ON UPDATE CASCADE,
    CONSTRAINT api_keys_hash_length CHECK (length(hash) = 64)
);

-- Uniqueness and lookup indexes
CREATE UNIQUE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_tenant ON api_keys(tenant_id, revoked_at);
CREATE INDEX IF NOT EXISTS idx_api_keys_user ON api_keys(user_id, revoked_at);
CREATE INDEX IF NOT EXISTS idx_api_keys_created_at ON api_keys(created_at DESC);

-- Tenant isolation guard: user must belong to the same tenant
CREATE TRIGGER IF NOT EXISTS api_keys_user_tenant_guard
BEFORE INSERT ON api_keys
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NOT EXISTS (
            SELECT 1 FROM users u
            WHERE u.id = NEW.user_id
              AND u.tenant_id = NEW.tenant_id
        )
        THEN RAISE(ABORT, 'user does not belong to tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS api_keys_user_tenant_guard_update
BEFORE UPDATE ON api_keys
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.user_id <> OLD.user_id OR NEW.tenant_id <> OLD.tenant_id THEN
            CASE
                WHEN NOT EXISTS (
                    SELECT 1 FROM users u
                    WHERE u.id = NEW.user_id
                      AND u.tenant_id = NEW.tenant_id
                )
                THEN RAISE(ABORT, 'user does not belong to tenant')
            END
    END;
END;



