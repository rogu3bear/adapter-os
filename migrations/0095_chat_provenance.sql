-- Migration 0092: Chat Session Provenance Extensions
-- Links chat sessions to document collections

-- Add collection_id to chat_sessions
ALTER TABLE chat_sessions ADD COLUMN collection_id TEXT REFERENCES document_collections(id);

-- Add index for collection-scoped session queries
CREATE INDEX IF NOT EXISTS idx_chat_sessions_collection ON chat_sessions(collection_id);
