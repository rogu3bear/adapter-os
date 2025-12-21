-- Migration 0160: Canonical chat domain model (DB-first)
-- Adds source-aware chat session fields, message sequencing, and chat_provenance.
-- Aligns chat tables with multi-tenant, document, and training-job entrypoints.

-- Extend chat_sessions with canonical fields
ALTER TABLE chat_sessions ADD COLUMN title TEXT; -- Optional display title (defaults to name)
ALTER TABLE chat_sessions ADD COLUMN created_by TEXT REFERENCES users(id); -- Creator user
ALTER TABLE chat_sessions ADD COLUMN document_id TEXT REFERENCES documents(id) ON DELETE SET NULL;
ALTER TABLE chat_sessions
    ADD COLUMN source_type TEXT NOT NULL DEFAULT 'general'
    CHECK (source_type IN ('general', 'document', 'owner_system', 'training_job'));
ALTER TABLE chat_sessions ADD COLUMN source_ref_id TEXT; -- e.g., training_job_id
ALTER TABLE chat_sessions ADD COLUMN tags_json TEXT; -- Optional inline tags (JSON array)
ALTER TABLE chat_sessions ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'));

-- Backfill new chat_sessions fields
UPDATE chat_sessions SET title = COALESCE(title, name);
UPDATE chat_sessions SET created_by = COALESCE(created_by, user_id);
UPDATE chat_sessions SET updated_at = COALESCE(updated_at, last_activity_at, created_at);
UPDATE chat_sessions
SET source_type = 'document'
WHERE document_id IS NOT NULL AND source_type = 'general';

-- Indexes for new chat_sessions dimensions
CREATE INDEX IF NOT EXISTS idx_chat_sessions_source ON chat_sessions(tenant_id, source_type);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_document ON chat_sessions(document_id);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_updated_at ON chat_sessions(updated_at DESC);

-- Extend chat_messages with tenant scoping, sequencing, and timestamps
ALTER TABLE chat_messages ADD COLUMN tenant_id TEXT REFERENCES tenants(id);
ALTER TABLE chat_messages ADD COLUMN sequence INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chat_messages ADD COLUMN created_at TEXT NOT NULL DEFAULT (datetime('now'));

-- Backfill chat_messages tenant_id and created_at from existing data
UPDATE chat_messages
SET tenant_id = (
    SELECT cs.tenant_id FROM chat_sessions cs WHERE cs.id = chat_messages.session_id
)
WHERE tenant_id IS NULL;

UPDATE chat_messages
SET created_at = COALESCE(created_at, timestamp, datetime('now'));

-- Backfill deterministic per-session sequencing (0-based, ordered by timestamp then id)
WITH ordered AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY session_id
               ORDER BY timestamp ASC, id ASC
           ) - 1 AS seq
    FROM chat_messages
)
UPDATE chat_messages
SET sequence = (SELECT seq FROM ordered WHERE ordered.id = chat_messages.id);

-- Indexes for chat_messages sequencing and tenant scoping
CREATE INDEX IF NOT EXISTS idx_chat_messages_session_seq
    ON chat_messages(session_id, sequence);
CREATE INDEX IF NOT EXISTS idx_chat_messages_tenant
    ON chat_messages(tenant_id);

-- Canonical chat_provenance table
CREATE TABLE IF NOT EXISTS chat_provenance (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    message_id TEXT,
    tenant_id TEXT NOT NULL,
    inference_call_id TEXT,
    payload_snapshot TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (message_id) REFERENCES chat_messages(id) ON DELETE SET NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_chat_provenance_session
    ON chat_provenance(session_id, created_at);
CREATE INDEX IF NOT EXISTS idx_chat_provenance_message
    ON chat_provenance(message_id);
CREATE INDEX IF NOT EXISTS idx_chat_provenance_tenant
    ON chat_provenance(tenant_id);


