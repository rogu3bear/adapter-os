-- Migration 0096: Plugin configuration persistence
-- PRD-PLUG-01: Plugin system infrastructure
-- Created: 2025-11-25

-- Global plugin configuration table
CREATE TABLE IF NOT EXISTS plugin_configs (
    id TEXT PRIMARY KEY,
    plugin_name TEXT NOT NULL UNIQUE,
    enabled BOOLEAN NOT NULL DEFAULT 0,
    config_json TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now'))
);

-- Index for querying enabled plugins
CREATE INDEX IF NOT EXISTS idx_plugin_configs_enabled ON plugin_configs(enabled);

-- Per-tenant plugin enablement table
CREATE TABLE IF NOT EXISTS plugin_tenant_enables (
    id TEXT PRIMARY KEY,
    plugin_name TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 0,
    config_override_json TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
    UNIQUE(plugin_name, tenant_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(tenant_id) ON DELETE CASCADE
);

-- Index for tenant-specific plugin queries
CREATE INDEX IF NOT EXISTS idx_plugin_tenant_enables_tenant ON plugin_tenant_enables(tenant_id);
CREATE INDEX IF NOT EXISTS idx_plugin_tenant_enables_plugin ON plugin_tenant_enables(plugin_name);
CREATE INDEX IF NOT EXISTS idx_plugin_tenant_enables_enabled ON plugin_tenant_enables(enabled);

-- Trigger to update updated_at timestamp on plugin_configs
CREATE TRIGGER IF NOT EXISTS update_plugin_configs_timestamp
AFTER UPDATE ON plugin_configs
FOR EACH ROW
BEGIN
    UPDATE plugin_configs SET updated_at = strftime('%Y-%m-%d %H:%M:%S', 'now') WHERE id = NEW.id;
END;
