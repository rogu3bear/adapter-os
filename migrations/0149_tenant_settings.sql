-- Tenant Settings for Stack and Pin Defaults
-- Provides configurable behavior for stack/adapter inheritance

CREATE TABLE IF NOT EXISTS tenant_settings (
    tenant_id TEXT PRIMARY KEY,

    -- Stack inheritance on chat session creation
    -- When TRUE (1): new chat sessions inherit stack_id from tenants.default_stack_id
    use_default_stack_on_chat_create INTEGER NOT NULL DEFAULT 0,

    -- Stack fallback on inference with session
    -- When TRUE (1): inference with session_id falls back to tenant default stack
    -- Only applies when no adapters/stack specified in request
    use_default_stack_on_infer_session INTEGER NOT NULL DEFAULT 0,

    -- Extensible JSON for experimental flags
    -- Structure: {"flag_name": true/false, ...}
    settings_json TEXT DEFAULT '{}',

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Index for efficient lookup
CREATE INDEX IF NOT EXISTS idx_tenant_settings_tenant
    ON tenant_settings(tenant_id);

-- Trigger to auto-update updated_at
CREATE TRIGGER IF NOT EXISTS tenant_settings_updated_at
    AFTER UPDATE ON tenant_settings
    FOR EACH ROW
    BEGIN
        UPDATE tenant_settings SET updated_at = datetime('now')
        WHERE tenant_id = NEW.tenant_id;
    END;
