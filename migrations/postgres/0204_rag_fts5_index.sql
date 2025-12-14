-- RAG Full-Text Search Index for Hybrid Search
-- PostgreSQL version using built-in full-text search with tsvector

-- Add tsvector column for full-text search
ALTER TABLE rag_documents ADD COLUMN text_tsv tsvector;

-- Create GIN index for efficient full-text search
CREATE INDEX IF NOT EXISTS idx_rag_documents_text_tsv
    ON rag_documents USING GIN(text_tsv);

-- Populate tsvector column with existing documents
UPDATE rag_documents
SET text_tsv = to_tsvector('english', text);

-- Trigger: Update tsvector when document inserted or updated
CREATE OR REPLACE FUNCTION rag_documents_tsvector_update() RETURNS trigger AS $$
BEGIN
    NEW.text_tsv := to_tsvector('english', NEW.text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER rag_documents_tsvector_trigger
BEFORE INSERT OR UPDATE OF text ON rag_documents
FOR EACH ROW
EXECUTE FUNCTION rag_documents_tsvector_update();
