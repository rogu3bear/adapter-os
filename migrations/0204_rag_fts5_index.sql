-- RAG Full-Text Search Index for Hybrid Search
-- Creates FTS5 virtual table synchronized with rag_documents

-- Create FTS5 virtual table for full-text search
CREATE VIRTUAL TABLE IF NOT EXISTS rag_documents_fts USING fts5(
    text,
    content='rag_documents',
    content_rowid='rowid',
    tokenize='porter unicode61'
);

-- Trigger: Insert into FTS when document added
CREATE TRIGGER IF NOT EXISTS rag_documents_fts_insert
AFTER INSERT ON rag_documents
BEGIN
    INSERT INTO rag_documents_fts(rowid, text) VALUES (new.rowid, new.text);
END;

-- Trigger: Remove from FTS when document deleted
CREATE TRIGGER IF NOT EXISTS rag_documents_fts_delete
AFTER DELETE ON rag_documents
BEGIN
    INSERT INTO rag_documents_fts(rag_documents_fts, rowid, text)
    VALUES('delete', old.rowid, old.text);
END;

-- Trigger: Update FTS when document updated
CREATE TRIGGER IF NOT EXISTS rag_documents_fts_update
AFTER UPDATE ON rag_documents
BEGIN
    INSERT INTO rag_documents_fts(rag_documents_fts, rowid, text)
    VALUES('delete', old.rowid, old.text);
    INSERT INTO rag_documents_fts(rowid, text) VALUES (new.rowid, new.text);
END;

-- Populate FTS index with existing documents
INSERT INTO rag_documents_fts(rowid, text)
SELECT rowid, text FROM rag_documents;
