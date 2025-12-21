-- Embedding Model Versioning and Re-embedding Pipeline
-- Adds deprecation tracking and re-embedding job support

-- Add deprecation tracking to embedding models
ALTER TABLE rag_embedding_models ADD COLUMN deprecated_at TEXT;
ALTER TABLE rag_embedding_models ADD COLUMN successor_model_hash TEXT;
ALTER TABLE rag_embedding_models ADD COLUMN description TEXT;

-- Create re-embedding job queue for model migrations
CREATE TABLE IF NOT EXISTS rag_reembedding_jobs (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source_model_hash TEXT NOT NULL,
    target_model_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    total_docs INTEGER NOT NULL DEFAULT 0,
    processed_docs INTEGER NOT NULL DEFAULT 0,
    failed_docs INTEGER NOT NULL DEFAULT 0,
    skipped_docs INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    completed_at TEXT,
    last_processed_doc_id TEXT,

    FOREIGN KEY (tenant_id) REFERENCES tenants(id)
);

-- Index for finding active/pending jobs
CREATE INDEX IF NOT EXISTS idx_reembedding_jobs_status
    ON rag_reembedding_jobs(status, created_at);

-- Index for tenant-specific job queries
CREATE INDEX IF NOT EXISTS idx_reembedding_jobs_tenant
    ON rag_reembedding_jobs(tenant_id, status);

-- Track which documents have been re-embedded for each job
CREATE TABLE IF NOT EXISTS rag_reembedding_progress (
    job_id TEXT NOT NULL,
    doc_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'completed', 'failed', 'skipped')),
    error_message TEXT,
    processed_at TEXT,

    PRIMARY KEY (job_id, doc_id),
    FOREIGN KEY (job_id) REFERENCES rag_reembedding_jobs(id) ON DELETE CASCADE
);
