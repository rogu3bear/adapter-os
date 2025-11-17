CREATE TABLE IF NOT EXISTS tenant_snapshots (
    tenant_id TEXT NOT NULL,
    state_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (tenant_id, created_at)
);

-- Index for quick lookup
CREATE INDEX IF NOT EXISTS idx_tenant_snapshots_tenant ON tenant_snapshots(tenant_id);
