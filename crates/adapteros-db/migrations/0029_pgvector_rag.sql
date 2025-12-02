-- Migration: RAG Vector Integration
-- Purpose: Add RAG document storage with vector embeddings for SQLite
-- Policy Compliance: RAG Index Ruleset (#7) - per-tenant isolation, deterministic ordering
-- Determinism: Score DESC, doc_id ASC tie-breaking

-- RAG documents table with vector embeddings
CREATE TABLE IF NOT EXISTS rag_documents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    doc_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    text TEXT NOT NULL,
    -- Store embeddings as JSON array (SQLite-compatible)
    embedding_json TEXT NOT NULL,
    rev TEXT NOT NULL,
    effectivity TEXT NOT NULL,
    source_type TEXT NOT NULL,
    superseded_by TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    -- Unique constraint per tenant + document
    UNIQUE(doc_id, tenant_id)
);

-- Indices for efficient retrieval
CREATE INDEX IF NOT EXISTS idx_rag_documents_tenant 
    ON rag_documents(tenant_id);

CREATE INDEX IF NOT EXISTS idx_rag_documents_doc_id 
    ON rag_documents(doc_id);

CREATE INDEX IF NOT EXISTS idx_rag_documents_tenant_superseded 
    ON rag_documents(tenant_id, superseded_by);

-- Index for deterministic ordering (score desc handled by in-memory calculation, doc_id asc for ties)
CREATE INDEX IF NOT EXISTS idx_rag_documents_tenant_doc_id_sorted 
    ON rag_documents(tenant_id, doc_id ASC);

-- Embedding model tracking table
CREATE TABLE IF NOT EXISTS rag_embedding_models (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    model_hash TEXT NOT NULL UNIQUE,
    model_name TEXT NOT NULL,
    dimension INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

-- Track which documents use which embedding model
CREATE TABLE IF NOT EXISTS rag_document_embeddings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    doc_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    model_hash TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    FOREIGN KEY (doc_id, tenant_id) REFERENCES rag_documents(doc_id, tenant_id) ON DELETE CASCADE,
    FOREIGN KEY (model_hash) REFERENCES rag_embedding_models(model_hash),
    
    UNIQUE(doc_id, tenant_id, model_hash)
);

CREATE INDEX IF NOT EXISTS idx_rag_document_embeddings_model 
    ON rag_document_embeddings(model_hash);

-- Document supersession tracking
CREATE TABLE IF NOT EXISTS rag_document_revisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    doc_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    rev TEXT NOT NULL,
    superseded_by TEXT,
    superseded_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    UNIQUE(doc_id, tenant_id, rev)
);

CREATE INDEX IF NOT EXISTS idx_rag_document_revisions_lookup 
    ON rag_document_revisions(doc_id, tenant_id, rev);

-- Retrieval audit log (for determinism validation)
CREATE TABLE IF NOT EXISTS rag_retrieval_audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tenant_id TEXT NOT NULL,
    query_hash TEXT NOT NULL, -- BLAKE3 hash of query embedding
    retrieved_doc_ids TEXT NOT NULL, -- JSON array of doc_ids in order
    retrieved_scores TEXT NOT NULL, -- JSON array of scores
    top_k INTEGER NOT NULL,
    embedding_model_hash TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_rag_retrieval_audit_tenant 
    ON rag_retrieval_audit(tenant_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_rag_retrieval_audit_query 
    ON rag_retrieval_audit(query_hash);

