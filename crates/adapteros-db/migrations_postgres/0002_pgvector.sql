-- pgvector setup and vector index for rag_documents
-- Idempotent: guarded with IF NOT EXISTS

-- Enable pgvector extension (requires appropriate privileges)
CREATE EXTENSION IF NOT EXISTS vector;

-- Add vector column for embeddings if absent
ALTER TABLE rag_documents
    ADD COLUMN IF NOT EXISTS embedding vector(3584);

-- Create vector index for similarity search (HNSW or IVFFlat)
-- Prefer HNSW for quality; fallback IVFFlat left as optional alternative
CREATE INDEX IF NOT EXISTS rag_documents_embedding_hnsw_idx
    ON rag_documents USING hnsw (embedding vector_cosine_ops);

-- Optional: IVFFlat index (commented out by default)
-- CREATE INDEX IF NOT EXISTS rag_documents_embedding_ivfflat_idx
--     ON rag_documents USING ivfflat (embedding vector_cosine_ops)
--     WITH (lists = 100);

