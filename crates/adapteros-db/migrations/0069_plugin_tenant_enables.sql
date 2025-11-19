-- Migration 0069: Add plugin_tenant_enables table for per-tenant plugin enablement

CREATE TABLE IF NOT EXISTS plugin_tenant_enables (
    tenant_id TEXT NOT NULL,
    plugin_name TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    updated_at DATETIME NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (tenant_id, plugin_name),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Index for efficient queries
CREATE INDEX IF NOT EXISTS idx_plugin_tenant_enables_plugin ON plugin_tenant_enables(plugin_name);

