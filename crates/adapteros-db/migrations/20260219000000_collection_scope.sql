-- Add scope column to document_collections for Knowledge vs Session distinction.
--
-- "session" (default): collection bound to a single chat session
-- "knowledge": persistent knowledge base available across all sessions
--
-- The scope column enables the UI to present a choice when uploading documents:
-- "use in this conversation" vs "add to my knowledge".

ALTER TABLE document_collections ADD COLUMN scope TEXT NOT NULL DEFAULT 'session';

-- Index for efficient listing of knowledge collections per tenant.
CREATE INDEX IF NOT EXISTS idx_collections_tenant_scope
    ON document_collections(tenant_id, scope);
