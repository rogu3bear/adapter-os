-- AdapterOS PostgreSQL Schema (Consolidated Init)
-- Purpose: Provide a Postgres-native schema parallel to SQLite migrations/
-- Notes:
--   - Uses TIMESTAMPTZ, BOOLEAN, and proper FK constraints
--   - Keeps table/column names aligned with application code
--   - Idempotent via IF NOT EXISTS

-- Users
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    pw_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('admin','operator','sre','compliance','auditor','viewer')),
    disabled BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Tenants
CREATE TABLE IF NOT EXISTS tenants (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    org_id TEXT NOT NULL,
    isolation_mode TEXT NOT NULL DEFAULT 'process' CHECK(isolation_mode IN ('process','container','vm')),
    max_memory_gb INTEGER NOT NULL DEFAULT 64,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','suspended')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Nodes
CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    hostname TEXT UNIQUE NOT NULL,
    metal_family TEXT,
    memory_gb INTEGER,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','online','offline','maintenance')),
    last_heartbeat TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_nodes_tenant ON nodes(tenant_id);
CREATE INDEX IF NOT EXISTS idx_nodes_status ON nodes(status);

-- Adapters (PostgreSQL representation matching SQLite schema for M0 compatibility)
CREATE TABLE IF NOT EXISTS adapters (
    id TEXT PRIMARY KEY,
    adapter_id TEXT,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    tier TEXT NOT NULL CHECK(tier IN ('persistent','warm','ephemeral')),
    hash_b3 TEXT UNIQUE NOT NULL,
    rank INTEGER NOT NULL,
    alpha REAL NOT NULL DEFAULT 1.0,
    targets_json TEXT NOT NULL DEFAULT '[]',
    acl_json TEXT,
    languages_json TEXT,
    framework TEXT,
    -- Code intelligence fields
    category TEXT NOT NULL DEFAULT 'code',
    scope TEXT NOT NULL DEFAULT 'global',
    framework_id TEXT,
    framework_version TEXT,
    repo_id TEXT,
    commit_sha TEXT,
    intent TEXT,
    expires_at TEXT,
    -- Lifecycle state management
    current_state TEXT NOT NULL DEFAULT 'unloaded',
    pinned INTEGER NOT NULL DEFAULT 0,
    memory_bytes BIGINT NOT NULL DEFAULT 0,
    last_activated TEXT,
    activation_count BIGINT NOT NULL DEFAULT 0,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, name)
);
CREATE INDEX IF NOT EXISTS idx_adapters_adapter_id ON adapters(adapter_id);
CREATE INDEX IF NOT EXISTS idx_adapters_tenant ON adapters(tenant_id);
CREATE INDEX IF NOT EXISTS idx_adapters_active ON adapters(active);
CREATE INDEX IF NOT EXISTS idx_adapters_category ON adapters(category);
CREATE INDEX IF NOT EXISTS idx_adapters_scope ON adapters(scope);
CREATE INDEX IF NOT EXISTS idx_adapters_state ON adapters(current_state);
CREATE INDEX IF NOT EXISTS idx_adapters_rank ON adapters(rank DESC, created_at DESC);

-- RAG: Documents (base without pgvector column; added in 0002)
CREATE TABLE IF NOT EXISTS rag_documents (
    id BIGSERIAL PRIMARY KEY,
    doc_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    text TEXT NOT NULL,
    rev TEXT NOT NULL,
    effectivity TEXT NOT NULL,
    source_type TEXT NOT NULL,
    superseded_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(doc_id, tenant_id)
);
CREATE INDEX IF NOT EXISTS idx_rag_documents_tenant ON rag_documents(tenant_id);
CREATE INDEX IF NOT EXISTS idx_rag_documents_doc_id ON rag_documents(doc_id);
CREATE INDEX IF NOT EXISTS idx_rag_documents_tenant_superseded ON rag_documents(tenant_id, superseded_by);
CREATE INDEX IF NOT EXISTS idx_rag_documents_tenant_doc_id_sorted ON rag_documents(tenant_id, doc_id ASC);

-- RAG: Embedding models
CREATE TABLE IF NOT EXISTS rag_embedding_models (
    id BIGSERIAL PRIMARY KEY,
    model_hash TEXT NOT NULL UNIQUE,
    model_name TEXT NOT NULL,
    dimension INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

-- RAG: Document <-> Model mapping
CREATE TABLE IF NOT EXISTS rag_document_embeddings (
    id BIGSERIAL PRIMARY KEY,
    doc_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    model_hash TEXT NOT NULL REFERENCES rag_embedding_models(model_hash),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT fk_rag_doc_ref FOREIGN KEY (doc_id, tenant_id)
        REFERENCES rag_documents(doc_id, tenant_id) ON DELETE CASCADE,
    UNIQUE(doc_id, tenant_id, model_hash)
);
CREATE INDEX IF NOT EXISTS idx_rag_document_embeddings_model ON rag_document_embeddings(model_hash);

-- RAG: Document revisions
CREATE TABLE IF NOT EXISTS rag_document_revisions (
    id BIGSERIAL PRIMARY KEY,
    doc_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    rev TEXT NOT NULL,
    superseded_by TEXT,
    superseded_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(doc_id, tenant_id, rev)
);
CREATE INDEX IF NOT EXISTS idx_rag_document_revisions_lookup ON rag_document_revisions(doc_id, tenant_id, rev);

-- RAG: Retrieval audit
CREATE TABLE IF NOT EXISTS rag_retrieval_audit (
    id BIGSERIAL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    query_hash TEXT NOT NULL,
    retrieved_doc_ids JSONB NOT NULL,
    retrieved_scores JSONB NOT NULL,
    top_k INTEGER NOT NULL,
    embedding_model_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_rag_retrieval_audit_tenant ON rag_retrieval_audit(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_rag_retrieval_audit_query ON rag_retrieval_audit(query_hash);

-- Adapter activations (M0 compatibility)
CREATE TABLE IF NOT EXISTS adapter_activations (
    id TEXT PRIMARY KEY,
    adapter_id TEXT NOT NULL,
    request_id TEXT,
    gate_value DOUBLE PRECISION NOT NULL,
    selected INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_adapter_activations_adapter_id ON adapter_activations(adapter_id);
CREATE INDEX IF NOT EXISTS idx_adapter_activations_created_at ON adapter_activations(created_at DESC);

