CREATE TABLE IF NOT EXISTS index_hashes (
    tenant_id TEXT NOT NULL,
    index_type TEXT NOT NULL,
    hash TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (tenant_id, index_type)
);

CREATE INDEX IF NOT EXISTS idx_index_hashes_tenant ON index_hashes(tenant_id);
