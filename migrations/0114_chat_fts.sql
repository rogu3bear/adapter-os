-- Migration 0114: Full-Text Search

-- FTS for sessions
CREATE VIRTUAL TABLE chat_sessions_fts USING fts5(
    session_id UNINDEXED,
    tenant_id UNINDEXED,
    name,
    description,
    content='',
    tokenize='porter unicode61 remove_diacritics 1'
);

-- FTS for messages
CREATE VIRTUAL TABLE chat_messages_fts USING fts5(
    message_id UNINDEXED,
    session_id UNINDEXED,
    tenant_id UNINDEXED,
    content,
    content='',
    tokenize='porter unicode61 remove_diacritics 1'
);

-- Sync triggers for sessions
-- Note: No rowid - chat_sessions uses TEXT PRIMARY KEY (no implicit rowid)
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

-- Sync triggers for messages
-- Note: No rowid - chat_messages uses TEXT PRIMARY KEY (no implicit rowid)
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

-- Backfill existing data (no rowid - FTS5 generates internal rowid automatically)
INSERT INTO chat_sessions_fts(session_id, tenant_id, name, description)
SELECT id, tenant_id, name, COALESCE(description, '') FROM chat_sessions;

INSERT INTO chat_messages_fts(message_id, session_id, tenant_id, content)
SELECT m.id, m.session_id, cs.tenant_id, m.content
FROM chat_messages m JOIN chat_sessions cs ON cs.id = m.session_id;
