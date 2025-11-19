-- Dashboard configuration table for per-user widget customization
-- Supports show/hide widgets and custom ordering
CREATE TABLE IF NOT EXISTS dashboard_configs (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id TEXT NOT NULL,
    widget_id TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    position INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Unique constraint: one config entry per user per widget
CREATE UNIQUE INDEX IF NOT EXISTS idx_dashboard_configs_user_widget
    ON dashboard_configs(user_id, widget_id);

-- Index for fast user lookups
CREATE INDEX IF NOT EXISTS idx_dashboard_configs_user
    ON dashboard_configs(user_id);

-- Index for ordering widgets
CREATE INDEX IF NOT EXISTS idx_dashboard_configs_position
    ON dashboard_configs(user_id, position);
