-- Migration 0109: Fix plugin_tenant_enables FK constraint
-- Problem: Migration 0096 has FK referencing tenants(tenant_id) which doesn't exist
--          The tenants table uses 'id' as PK, not 'tenant_id'
-- Note: Migration 0069 creates the table first without FK, so 0096's CREATE TABLE IF NOT EXISTS
--       never runs. This migration ensures the FK is correctly added.

-- Step 1: Drop the table and recreate with correct FK
-- (Table may have been created by 0069 without FK, or by 0096 with bad FK reference)
DROP TABLE IF EXISTS plugin_tenant_enables;

CREATE TABLE IF NOT EXISTS plugin_tenant_enables (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    plugin_name TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 0,
    config_override_json TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
    UNIQUE(plugin_name, tenant_id),

    -- Correct FK: reference tenants.id (the actual PK)
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_plugin_tenant_enables_tenant ON plugin_tenant_enables(tenant_id);
CREATE INDEX IF NOT EXISTS idx_plugin_tenant_enables_plugin ON plugin_tenant_enables(plugin_name);
CREATE INDEX IF NOT EXISTS idx_plugin_tenant_enables_enabled ON plugin_tenant_enables(enabled);
