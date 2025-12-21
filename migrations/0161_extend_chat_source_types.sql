-- Migration 0161: Extend chat source types and add composite indexes
-- Expands chat_sessions.source_type to cover CLI/owner-system flows and
-- adds indexes aligned with new list filters.

PRAGMA foreign_keys = OFF;

-- Temporarily drop message FTS triggers that reference chat_sessions
DROP TRIGGER IF EXISTS chat_messages_fts_insert;
DROP TRIGGER IF EXISTS chat_messages_fts_update;
DROP TRIGGER IF EXISTS chat_messages_fts_delete;

-- Recreate chat_sessions with widened source_type enum
CREATE TABLE chat_sessions_new (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT,
    created_by TEXT REFERENCES users(id),
    stack_id TEXT REFERENCES adapter_stacks(id) ON DELETE SET NULL,
    collection_id TEXT REFERENCES document_collections(id) ON DELETE SET NULL,
    document_id TEXT REFERENCES documents(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    title TEXT,
    source_type TEXT NOT NULL DEFAULT 'general'
        CHECK (source_type IN ('general', 'document', 'owner_system', 'training_job', 'cli', 'cli_prompt')),
    source_ref_id TEXT,
    category_id TEXT REFERENCES chat_session_categories(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'active',
    deleted_at TEXT,
    deleted_by TEXT,
    archived_at TEXT,
    archived_by TEXT,
    archive_reason TEXT,
    retention_until TEXT,
    description TEXT,
    is_shared INTEGER NOT NULL DEFAULT 0,
    metadata_json TEXT,
    tags_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_activity_at TEXT NOT NULL DEFAULT (datetime('now')),
    pinned_adapter_ids TEXT,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Preserve existing data
INSERT INTO chat_sessions_new (
    id, tenant_id, user_id, created_by, stack_id, collection_id, document_id,
    name, title, source_type, source_ref_id, category_id, status,
    deleted_at, deleted_by, archived_at, archived_by, archive_reason,
    retention_until, description, is_shared, metadata_json, tags_json,
    created_at, updated_at, last_activity_at, pinned_adapter_ids
) SELECT
    id, tenant_id, user_id, created_by, stack_id, collection_id, document_id,
    name, title, source_type, source_ref_id, category_id, status,
    deleted_at, deleted_by, archived_at, archived_by, archive_reason,
    retention_until, description, is_shared, metadata_json, tags_json,
    created_at, updated_at, last_activity_at, pinned_adapter_ids
FROM chat_sessions;

-- Replace old table
DROP TABLE chat_sessions;
ALTER TABLE chat_sessions_new RENAME TO chat_sessions;

-- Recreate indexes (existing + new composites for list filters)
CREATE INDEX IF NOT EXISTS idx_chat_sessions_tenant ON chat_sessions(tenant_id, last_activity_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_user ON chat_sessions(user_id, last_activity_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_stack ON chat_sessions(stack_id);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_activity ON chat_sessions(last_activity_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_last_activity ON chat_sessions(last_activity_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_collection ON chat_sessions(collection_id);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_category ON chat_sessions(category_id);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_active ON chat_sessions(tenant_id, status, last_activity_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_deleted ON chat_sessions(deleted_at, retention_until);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_archived ON chat_sessions(tenant_id, archived_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_shared ON chat_sessions(tenant_id, is_shared) WHERE is_shared = 1;
CREATE INDEX IF NOT EXISTS idx_chat_sessions_source ON chat_sessions(tenant_id, source_type);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_document ON chat_sessions(document_id);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_updated_at ON chat_sessions(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_tenant_source_updated_at ON chat_sessions(tenant_id, source_type, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_tenant_document_updated_at ON chat_sessions(tenant_id, document_id, updated_at DESC);

-- Restore tenant isolation triggers from migration 0131 (stack/collection guards)
CREATE TRIGGER IF NOT EXISTS trg_chat_sessions_stack_tenant_check
BEFORE INSERT ON chat_sessions
FOR EACH ROW
WHEN NEW.stack_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_stacks WHERE id = NEW.stack_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: chat_session.stack_id references stack from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_chat_sessions_stack_tenant_check_update
BEFORE UPDATE OF stack_id ON chat_sessions
FOR EACH ROW
WHEN NEW.stack_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_stacks WHERE id = NEW.stack_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: chat_session.stack_id references stack from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_chat_sessions_collection_tenant_check
BEFORE INSERT ON chat_sessions
FOR EACH ROW
WHEN NEW.collection_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM document_collections WHERE id = NEW.collection_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: chat_session.collection_id references collection from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_chat_sessions_collection_tenant_check_update
BEFORE UPDATE OF collection_id ON chat_sessions
FOR EACH ROW
WHEN NEW.collection_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM document_collections WHERE id = NEW.collection_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: chat_session.collection_id references collection from different tenant')
    END;
END;

-- Recreate FTS sync triggers for chat_sessions
CREATE TRIGGER chat_sessions_fts_insert AFTER INSERT ON chat_sessions BEGIN
    INSERT INTO chat_sessions_fts(session_id, tenant_id, name, description)
    VALUES (NEW.id, NEW.tenant_id, NEW.name, COALESCE(NEW.description, ''));
END;

CREATE TRIGGER chat_sessions_fts_update AFTER UPDATE OF name, description ON chat_sessions BEGIN
    UPDATE chat_sessions_fts SET name = NEW.name, description = COALESCE(NEW.description, '')
    WHERE session_id = NEW.id;
END;

CREATE TRIGGER chat_sessions_fts_delete AFTER DELETE ON chat_sessions BEGIN
    DELETE FROM chat_sessions_fts WHERE session_id = OLD.id;
END;

-- Refresh FTS content to match rebuilt table
DELETE FROM chat_sessions_fts;
INSERT INTO chat_sessions_fts(session_id, tenant_id, name, description)
SELECT id, tenant_id, name, COALESCE(description, '') FROM chat_sessions;

-- Recreate message FTS triggers (definition preserved from migration 0114)
CREATE TRIGGER chat_messages_fts_insert AFTER INSERT ON chat_messages BEGIN
    INSERT INTO chat_messages_fts(message_id, session_id, tenant_id, content)
    SELECT NEW.id, NEW.session_id, cs.tenant_id, NEW.content
    FROM chat_sessions cs WHERE cs.id = NEW.session_id;
END;

CREATE TRIGGER chat_messages_fts_update AFTER UPDATE OF content ON chat_messages BEGIN
    UPDATE chat_messages_fts SET content = NEW.content WHERE message_id = NEW.id;
END;

CREATE TRIGGER chat_messages_fts_delete AFTER DELETE ON chat_messages BEGIN
    DELETE FROM chat_messages_fts WHERE message_id = OLD.id;
END;

-- Refresh message FTS content to ensure triggers and data are aligned
DELETE FROM chat_messages_fts;
INSERT INTO chat_messages_fts(message_id, session_id, tenant_id, content)
SELECT m.id, m.session_id, cs.tenant_id, m.content
FROM chat_messages m JOIN chat_sessions cs ON cs.id = m.session_id;

PRAGMA foreign_keys = ON;

