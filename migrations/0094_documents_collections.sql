-- Migration 0091: Document Collections and Inference Evidence
-- Supports document-centric UX with provenance tracking

-- First-class document entity
CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    file_path TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    mime_type TEXT NOT NULL,
    page_count INTEGER,
    status TEXT NOT NULL DEFAULT 'processing',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    metadata_json TEXT,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id)
);

CREATE INDEX IF NOT EXISTS idx_documents_tenant ON documents(tenant_id);
CREATE INDEX IF NOT EXISTS idx_documents_status ON documents(status);
CREATE INDEX IF NOT EXISTS idx_documents_content_hash ON documents(content_hash);

-- Document chunks for RAG
CREATE TABLE IF NOT EXISTS document_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    page_number INTEGER,
    start_offset INTEGER,
    end_offset INTEGER,
    chunk_hash TEXT NOT NULL,
    text_preview TEXT,
    embedding_json TEXT,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
    UNIQUE(document_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_document_chunks_document ON document_chunks(document_id);
CREATE INDEX IF NOT EXISTS idx_document_chunks_hash ON document_chunks(chunk_hash);

-- Document collections
CREATE TABLE IF NOT EXISTS document_collections (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id)
);

CREATE INDEX IF NOT EXISTS idx_document_collections_tenant ON document_collections(tenant_id);

-- Many-to-many: collections <-> documents
CREATE TABLE IF NOT EXISTS collection_documents (
    collection_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (collection_id, document_id),
    FOREIGN KEY (collection_id) REFERENCES document_collections(id) ON DELETE CASCADE,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

-- Inference evidence for provenance tracking
CREATE TABLE IF NOT EXISTS inference_evidence (
    id TEXT PRIMARY KEY,
    inference_id TEXT NOT NULL,
    session_id TEXT,
    message_id TEXT,
    document_id TEXT NOT NULL,
    chunk_id TEXT NOT NULL,
    page_number INTEGER,
    document_hash TEXT NOT NULL,
    chunk_hash TEXT NOT NULL,
    relevance_score REAL NOT NULL,
    rank INTEGER NOT NULL,
    context_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (document_id) REFERENCES documents(id),
    FOREIGN KEY (chunk_id) REFERENCES document_chunks(id),
    FOREIGN KEY (session_id) REFERENCES chat_sessions(id)
);

CREATE INDEX IF NOT EXISTS idx_inference_evidence_inference ON inference_evidence(inference_id);
CREATE INDEX IF NOT EXISTS idx_inference_evidence_session ON inference_evidence(session_id);
CREATE INDEX IF NOT EXISTS idx_inference_evidence_document ON inference_evidence(document_id);
CREATE INDEX IF NOT EXISTS idx_inference_evidence_context ON inference_evidence(context_hash);

-- Adapter training snapshots for immutable provenance
-- Note: No foreign key on adapter_id to preserve snapshots even if adapter is deleted
CREATE TABLE IF NOT EXISTS adapter_training_snapshots (
    id TEXT PRIMARY KEY,
    adapter_id TEXT NOT NULL,
    training_job_id TEXT NOT NULL,
    collection_id TEXT,
    documents_json TEXT NOT NULL,
    chunk_manifest_hash TEXT NOT NULL,
    chunking_config_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (collection_id) REFERENCES document_collections(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_adapter_training_snapshots_adapter ON adapter_training_snapshots(adapter_id);
CREATE INDEX IF NOT EXISTS idx_adapter_training_snapshots_collection ON adapter_training_snapshots(collection_id);
