-- Migration 0113: Soft Delete and Archival

-- Add soft delete columns
ALTER TABLE chat_sessions ADD COLUMN status TEXT NOT NULL DEFAULT 'active'
    CHECK(status IN ('active', 'archived', 'deleted'));
ALTER TABLE chat_sessions ADD COLUMN deleted_at TEXT;
ALTER TABLE chat_sessions ADD COLUMN deleted_by TEXT;
ALTER TABLE chat_sessions ADD COLUMN archived_at TEXT;
ALTER TABLE chat_sessions ADD COLUMN archived_by TEXT;
ALTER TABLE chat_sessions ADD COLUMN archive_reason TEXT;
ALTER TABLE chat_sessions ADD COLUMN retention_until TEXT;
ALTER TABLE chat_sessions ADD COLUMN description TEXT;

-- Indexes for status-based queries
CREATE INDEX idx_chat_sessions_active
    ON chat_sessions(tenant_id, status, last_activity_at DESC)
    WHERE status = 'active';

CREATE INDEX idx_chat_sessions_deleted
    ON chat_sessions(deleted_at, retention_until)
    WHERE status = 'deleted';

CREATE INDEX idx_chat_sessions_archived
    ON chat_sessions(tenant_id, archived_at DESC)
    WHERE status = 'archived';

-- Soft delete messages
ALTER TABLE chat_messages ADD COLUMN deleted_at TEXT;

CREATE INDEX idx_chat_messages_active
    ON chat_messages(session_id, timestamp ASC)
    WHERE deleted_at IS NULL;

-- Retention policies
CREATE TABLE IF NOT EXISTS chat_retention_policies (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL UNIQUE,
    trash_retention_days INTEGER NOT NULL DEFAULT 30,
    auto_archive_days INTEGER DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);
