-- System-wide key-value settings for runtime configuration
-- Used by owner chat to store active adapter ID

CREATE TABLE IF NOT EXISTS system_settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    description TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

-- Index for fast lookups
CREATE INDEX IF NOT EXISTS idx_system_settings_key ON system_settings(key);

-- Initial settings for owner chat
INSERT OR IGNORE INTO system_settings (key, value, description) VALUES
    ('owner_chat_adapter_id', '', 'Adapter ID for documentation-aware owner chat assistant');
