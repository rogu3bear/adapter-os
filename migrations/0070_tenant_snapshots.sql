-- Tenant Snapshot & Hydration Support (PRD 2)
-- Stores deterministic snapshot hashes for tenant state reconstruction

-- Table for storing tenant snapshot hashes
CREATE TABLE IF NOT EXISTS tenant_snapshots (
    tenant_id TEXT NOT NULL,
    state_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (tenant_id, state_hash)
);

-- Index for retrieving latest snapshot hash per tenant
CREATE INDEX IF NOT EXISTS idx_tenant_snapshots_latest
ON tenant_snapshots(tenant_id, created_at DESC);

-- Table for router policies (referenced in TenantStateSnapshot)
CREATE TABLE IF NOT EXISTS router_policies (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    rules_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, name)
);

-- Table for tenant configs (referenced in TenantStateSnapshot)
CREATE TABLE IF NOT EXISTS tenant_configs (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, key)
);

-- Index for config key lookups
CREATE INDEX IF NOT EXISTS idx_tenant_configs_tenant
ON tenant_configs(tenant_id, key);
