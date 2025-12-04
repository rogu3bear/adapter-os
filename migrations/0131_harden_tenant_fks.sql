-- Migration: 0131_harden_tenant_fks.sql
-- Purpose: Harden tenant isolation with REAL composite FK constraints
-- Created: 2025-12-02
--
-- This migration properly enforces tenant isolation at the schema level by:
-- 1. Creating composite unique indexes on parent tables
-- 2. Recreating child tables with tenant_id NOT NULL and composite FKs
-- 3. FAILING if orphaned rows exist (no silent corruption)
--
-- SQLite doesn't support ALTER TABLE ADD CONSTRAINT, so we recreate tables.

-- ========================================
-- PHASE 1: Orphan detection - FAIL EARLY
-- These queries will cause the migration to fail if orphans exist
-- Using division-by-zero trick: 1/(1-has_orphans) fails when has_orphans=1
-- ========================================

-- Create temp table to store orphan check results (will fail with constraint if orphans exist)
CREATE TABLE _orphan_check (
    check_name TEXT PRIMARY KEY,
    orphan_count INTEGER NOT NULL CHECK(orphan_count = 0)  -- FAILS if orphan_count > 0
);

-- 1a. Check for orphaned document_chunks (no parent document)
-- If orphans exist, this INSERT violates the CHECK constraint and migration fails
INSERT INTO _orphan_check (check_name, orphan_count)
SELECT 'document_chunks', COUNT(*)
FROM document_chunks dc
LEFT JOIN documents d ON dc.document_id = d.id
WHERE d.id IS NULL;

-- 1b. Check for orphaned inference_evidence (no parent document)
INSERT INTO _orphan_check (check_name, orphan_count)
SELECT 'inference_evidence', COUNT(*)
FROM inference_evidence ie
LEFT JOIN documents d ON ie.document_id = d.id
WHERE d.id IS NULL;

-- 1c. Check for orphaned adapter_training_snapshots (no parent training job)
INSERT INTO _orphan_check (check_name, orphan_count)
SELECT 'adapter_training_snapshots', COUNT(*)
FROM adapter_training_snapshots ats
LEFT JOIN repository_training_jobs j ON ats.training_job_id = j.id
WHERE j.id IS NULL;

-- 1d. Check for cross-tenant collection_documents (collection and document have different tenants)
INSERT INTO _orphan_check (check_name, orphan_count)
SELECT 'cross_tenant_collection_documents', COUNT(*)
FROM collection_documents cd
JOIN document_collections c ON cd.collection_id = c.id
JOIN documents d ON cd.document_id = d.id
WHERE c.tenant_id != d.tenant_id;

-- If we get here, no orphans exist. Drop the check table.
DROP TABLE _orphan_check;

-- IMPORTANT: If migration fails above, run these queries to find orphans:
-- SELECT dc.id, dc.document_id FROM document_chunks dc LEFT JOIN documents d ON dc.document_id = d.id WHERE d.id IS NULL;
-- SELECT ie.id, ie.document_id FROM inference_evidence ie LEFT JOIN documents d ON ie.document_id = d.id WHERE d.id IS NULL;
-- SELECT ats.id, ats.training_job_id FROM adapter_training_snapshots ats LEFT JOIN repository_training_jobs j ON ats.training_job_id = j.id WHERE j.id IS NULL;
-- SELECT cd.*, c.tenant_id as coll_tenant, d.tenant_id as doc_tenant FROM collection_documents cd JOIN document_collections c ON cd.collection_id = c.id JOIN documents d ON cd.document_id = d.id WHERE c.tenant_id != d.tenant_id;

-- ========================================
-- PHASE 2: Create composite unique indexes on parent tables
-- These enable proper composite FK constraints
-- ========================================

CREATE UNIQUE INDEX IF NOT EXISTS idx_documents_tenant_id_composite
    ON documents(tenant_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_document_collections_tenant_id_composite
    ON document_collections(tenant_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_training_jobs_tenant_id_composite
    ON repository_training_jobs(tenant_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_adapters_tenant_id_composite
    ON adapters(tenant_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_training_datasets_tenant_id_composite
    ON training_datasets(tenant_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_adapter_stacks_tenant_id_composite
    ON adapter_stacks(tenant_id, id);

-- ========================================
-- PHASE 3: Recreate document_chunks with tenant_id and composite FK
-- ========================================

-- 3a. Create new table with proper constraints
CREATE TABLE document_chunks_new (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    page_number INTEGER,
    start_offset INTEGER,
    end_offset INTEGER,
    chunk_hash TEXT NOT NULL,
    text_preview TEXT,
    embedding_json TEXT,
    -- Composite FK: ensures chunk belongs to same tenant as document
    FOREIGN KEY (tenant_id, document_id) REFERENCES documents(tenant_id, id) ON DELETE CASCADE,
    UNIQUE(document_id, chunk_index)
);

-- 3b. Copy data with tenant_id derived from parent document
INSERT INTO document_chunks_new (
    id, tenant_id, document_id, chunk_index, page_number,
    start_offset, end_offset, chunk_hash, text_preview, embedding_json
)
SELECT
    dc.id,
    d.tenant_id,  -- Derived from parent
    dc.document_id,
    dc.chunk_index,
    dc.page_number,
    dc.start_offset,
    dc.end_offset,
    dc.chunk_hash,
    dc.text_preview,
    dc.embedding_json
FROM document_chunks dc
INNER JOIN documents d ON dc.document_id = d.id;

-- 3c. Drop old table and rename
DROP TABLE document_chunks;
ALTER TABLE document_chunks_new RENAME TO document_chunks;

-- 3d. Recreate indexes
CREATE INDEX idx_document_chunks_document ON document_chunks(document_id);
CREATE INDEX idx_document_chunks_hash ON document_chunks(chunk_hash);
CREATE INDEX idx_document_chunks_tenant ON document_chunks(tenant_id);

-- ========================================
-- PHASE 4: Recreate inference_evidence with tenant_id and composite FK
-- ========================================

-- 4a. Create new table with proper constraints
CREATE TABLE inference_evidence_new (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
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
    -- Composite FK: ensures evidence belongs to same tenant as document
    FOREIGN KEY (tenant_id, document_id) REFERENCES documents(tenant_id, id),
    FOREIGN KEY (session_id) REFERENCES chat_sessions(id)
);

-- 4b. Copy data with tenant_id derived from parent document
INSERT INTO inference_evidence_new (
    id, tenant_id, inference_id, session_id, message_id, document_id, chunk_id,
    page_number, document_hash, chunk_hash, relevance_score, rank, context_hash, created_at
)
SELECT
    ie.id,
    d.tenant_id,  -- Derived from parent
    ie.inference_id,
    ie.session_id,
    ie.message_id,
    ie.document_id,
    ie.chunk_id,
    ie.page_number,
    ie.document_hash,
    ie.chunk_hash,
    ie.relevance_score,
    ie.rank,
    ie.context_hash,
    ie.created_at
FROM inference_evidence ie
INNER JOIN documents d ON ie.document_id = d.id;

-- 4c. Drop old table and rename
DROP TABLE inference_evidence;
ALTER TABLE inference_evidence_new RENAME TO inference_evidence;

-- 4d. Recreate indexes
CREATE INDEX idx_inference_evidence_inference ON inference_evidence(inference_id);
CREATE INDEX idx_inference_evidence_session ON inference_evidence(session_id);
CREATE INDEX idx_inference_evidence_document ON inference_evidence(document_id);
CREATE INDEX idx_inference_evidence_context ON inference_evidence(context_hash);
CREATE INDEX idx_inference_evidence_tenant ON inference_evidence(tenant_id);

-- ========================================
-- PHASE 5: Recreate adapter_training_snapshots with tenant_id and composite FK
-- ========================================

-- 5a. Create new table with proper constraints
CREATE TABLE adapter_training_snapshots_new (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    training_job_id TEXT NOT NULL,
    collection_id TEXT,
    documents_json TEXT NOT NULL,
    chunk_manifest_hash TEXT NOT NULL,
    chunking_config_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    -- Composite FK on collection (if present, must be same tenant)
    FOREIGN KEY (tenant_id, collection_id) REFERENCES document_collections(tenant_id, id) ON DELETE SET NULL
);

-- 5b. Copy data with tenant_id derived from training job
INSERT INTO adapter_training_snapshots_new (
    id, tenant_id, adapter_id, training_job_id, collection_id,
    documents_json, chunk_manifest_hash, chunking_config_json, created_at
)
SELECT
    ats.id,
    j.tenant_id,  -- Derived from parent training job
    ats.adapter_id,
    ats.training_job_id,
    ats.collection_id,
    ats.documents_json,
    ats.chunk_manifest_hash,
    ats.chunking_config_json,
    ats.created_at
FROM adapter_training_snapshots ats
INNER JOIN repository_training_jobs j ON ats.training_job_id = j.id;

-- 5c. Drop old table and rename
DROP TABLE adapter_training_snapshots;
ALTER TABLE adapter_training_snapshots_new RENAME TO adapter_training_snapshots;

-- 5d. Recreate indexes
CREATE INDEX idx_adapter_training_snapshots_adapter ON adapter_training_snapshots(adapter_id);
CREATE INDEX idx_adapter_training_snapshots_collection ON adapter_training_snapshots(collection_id);
CREATE INDEX idx_adapter_training_snapshots_tenant ON adapter_training_snapshots(tenant_id);

-- ========================================
-- PHASE 6: Recreate collection_documents with composite FK
-- ========================================

-- 6a. Create new table with proper constraints
CREATE TABLE collection_documents_new (
    tenant_id TEXT NOT NULL,
    collection_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (collection_id, document_id),
    -- Both collection and document must belong to same tenant
    FOREIGN KEY (tenant_id, collection_id) REFERENCES document_collections(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, document_id) REFERENCES documents(tenant_id, id) ON DELETE CASCADE
);

-- 6b. Copy data with tenant_id from collection (or document)
INSERT INTO collection_documents_new (tenant_id, collection_id, document_id, added_at)
SELECT
    c.tenant_id,
    cd.collection_id,
    cd.document_id,
    cd.added_at
FROM collection_documents cd
INNER JOIN document_collections c ON cd.collection_id = c.id
INNER JOIN documents d ON cd.document_id = d.id
WHERE c.tenant_id = d.tenant_id;  -- Only copy if tenants match

-- 6c. Drop old table and rename
DROP TABLE collection_documents;
ALTER TABLE collection_documents_new RENAME TO collection_documents;

-- ========================================
-- PHASE 7: Add tenant_id check constraint on existing tables
-- For tables that already have tenant_id but weak FKs
-- ========================================

-- Note: SQLite CHECK constraints can't reference other tables,
-- so we use triggers for cross-table validation

-- 7a. Trigger: adapters.primary_dataset_id must match tenant
CREATE TRIGGER IF NOT EXISTS trg_adapters_primary_dataset_tenant_check
BEFORE INSERT ON adapters
FOR EACH ROW
WHEN NEW.primary_dataset_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.primary_dataset_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: adapter.primary_dataset_id references dataset from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_adapters_primary_dataset_tenant_check_update
BEFORE UPDATE OF primary_dataset_id ON adapters
FOR EACH ROW
WHEN NEW.primary_dataset_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.primary_dataset_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: adapter.primary_dataset_id references dataset from different tenant')
    END;
END;

-- 7b. Trigger: adapters.eval_dataset_id must match tenant
CREATE TRIGGER IF NOT EXISTS trg_adapters_eval_dataset_tenant_check
BEFORE INSERT ON adapters
FOR EACH ROW
WHEN NEW.eval_dataset_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.eval_dataset_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: adapter.eval_dataset_id references dataset from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_adapters_eval_dataset_tenant_check_update
BEFORE UPDATE OF eval_dataset_id ON adapters
FOR EACH ROW
WHEN NEW.eval_dataset_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.eval_dataset_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: adapter.eval_dataset_id references dataset from different tenant')
    END;
END;

-- 7c. Trigger: training_jobs.collection_id must match tenant
CREATE TRIGGER IF NOT EXISTS trg_training_jobs_collection_tenant_check
BEFORE INSERT ON repository_training_jobs
FOR EACH ROW
WHEN NEW.collection_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM document_collections WHERE id = NEW.collection_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: training_job.collection_id references collection from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_training_jobs_collection_tenant_check_update
BEFORE UPDATE OF collection_id ON repository_training_jobs
FOR EACH ROW
WHEN NEW.collection_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM document_collections WHERE id = NEW.collection_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: training_job.collection_id references collection from different tenant')
    END;
END;

-- 7d. Trigger: training_jobs.dataset_id must match tenant
CREATE TRIGGER IF NOT EXISTS trg_training_jobs_dataset_tenant_check
BEFORE INSERT ON repository_training_jobs
FOR EACH ROW
WHEN NEW.dataset_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: training_job.dataset_id references dataset from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_training_jobs_dataset_tenant_check_update
BEFORE UPDATE OF dataset_id ON repository_training_jobs
FOR EACH ROW
WHEN NEW.dataset_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: training_job.dataset_id references dataset from different tenant')
    END;
END;

-- 7e. Trigger: chat_sessions.stack_id must match tenant
CREATE TRIGGER IF NOT EXISTS trg_chat_sessions_stack_tenant_check
BEFORE INSERT ON chat_sessions
FOR EACH ROW
WHEN NEW.stack_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_stacks WHERE id = NEW.stack_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: chat_session.stack_id references stack from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_chat_sessions_stack_tenant_check_update
BEFORE UPDATE OF stack_id ON chat_sessions
FOR EACH ROW
WHEN NEW.stack_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_stacks WHERE id = NEW.stack_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: chat_session.stack_id references stack from different tenant')
    END;
END;

-- 7f. Trigger: chat_sessions.collection_id must match tenant
CREATE TRIGGER IF NOT EXISTS trg_chat_sessions_collection_tenant_check
BEFORE INSERT ON chat_sessions
FOR EACH ROW
WHEN NEW.collection_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM document_collections WHERE id = NEW.collection_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: chat_session.collection_id references collection from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_chat_sessions_collection_tenant_check_update
BEFORE UPDATE OF collection_id ON chat_sessions
FOR EACH ROW
WHEN NEW.collection_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM document_collections WHERE id = NEW.collection_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: chat_session.collection_id references collection from different tenant')
    END;
END;

-- 7g. Trigger: routing_decisions.stack_id must match tenant
CREATE TRIGGER IF NOT EXISTS trg_routing_decisions_stack_tenant_check
BEFORE INSERT ON routing_decisions
FOR EACH ROW
WHEN NEW.stack_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_stacks WHERE id = NEW.stack_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: routing_decision.stack_id references stack from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_routing_decisions_stack_tenant_check_update
BEFORE UPDATE OF stack_id ON routing_decisions
FOR EACH ROW
WHEN NEW.stack_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_stacks WHERE id = NEW.stack_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: routing_decision.stack_id references stack from different tenant')
    END;
END;

-- ========================================
-- PHASE 8: Additional FK enforcement
-- ========================================

-- 8a. Trigger: adapters.training_job_id must match tenant
CREATE TRIGGER IF NOT EXISTS trg_adapters_training_job_tenant_check
BEFORE INSERT ON adapters
FOR EACH ROW
WHEN NEW.training_job_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM repository_training_jobs WHERE id = NEW.training_job_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: adapter.training_job_id references training job from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_adapters_training_job_tenant_check_update
BEFORE UPDATE OF training_job_id ON adapters
FOR EACH ROW
WHEN NEW.training_job_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM repository_training_jobs WHERE id = NEW.training_job_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: adapter.training_job_id references training job from different tenant')
    END;
END;

-- 8b. Trigger: pinned_adapters.adapter_pk must match tenant
CREATE TRIGGER IF NOT EXISTS trg_pinned_adapters_tenant_check
BEFORE INSERT ON pinned_adapters
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapters WHERE id = NEW.adapter_pk) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: pinned_adapter.adapter_pk references adapter from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_pinned_adapters_tenant_check_update
BEFORE UPDATE OF adapter_pk ON pinned_adapters
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapters WHERE id = NEW.adapter_pk) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: pinned_adapter.adapter_pk references adapter from different tenant')
    END;
END;

-- ========================================
-- PHASE 9: Add tenant_id to tables missing it + triggers
-- ========================================

-- 9a. Add tenant_id to dataset_adapter_links (currently has none)
ALTER TABLE dataset_adapter_links ADD COLUMN tenant_id TEXT;

-- Backfill tenant_id from adapters (use adapter as source since it's more restrictive)
UPDATE dataset_adapter_links SET tenant_id = (
    SELECT a.tenant_id FROM adapters a WHERE a.id = dataset_adapter_links.adapter_id
);

-- Note: Rows with NULL tenant_id after backfill are orphaned (adapter was deleted)
-- Don't make NOT NULL immediately - let app code handle cleanup

-- Trigger: dataset_adapter_links must match tenant of both dataset and adapter
CREATE TRIGGER IF NOT EXISTS trg_dataset_adapter_links_tenant_check
BEFORE INSERT ON dataset_adapter_links
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapters WHERE id = NEW.adapter_id) !=
             (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id)
        THEN RAISE(ABORT, 'Tenant mismatch: dataset_adapter_link references adapter and dataset from different tenants')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_dataset_adapter_links_tenant_check_update
BEFORE UPDATE ON dataset_adapter_links
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapters WHERE id = NEW.adapter_id) !=
             (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id)
        THEN RAISE(ABORT, 'Tenant mismatch: dataset_adapter_link references adapter and dataset from different tenants')
    END;
END;

CREATE INDEX IF NOT EXISTS idx_dataset_adapter_links_tenant ON dataset_adapter_links(tenant_id);

-- 9b. Add tenant_id to adapter_version_history (currently has none)
ALTER TABLE adapter_version_history ADD COLUMN tenant_id TEXT;

-- Backfill tenant_id from adapters
UPDATE adapter_version_history SET tenant_id = (
    SELECT a.tenant_id FROM adapters a WHERE a.id = adapter_version_history.adapter_pk
);

-- Trigger: adapter_version_history must match tenant of adapter
CREATE TRIGGER IF NOT EXISTS trg_adapter_version_history_tenant_check
BEFORE INSERT ON adapter_version_history
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapters WHERE id = NEW.adapter_pk) IS NULL
        THEN RAISE(ABORT, 'Invalid adapter_pk: adapter does not exist')
        WHEN NEW.tenant_id IS NOT NULL AND
             (SELECT tenant_id FROM adapters WHERE id = NEW.adapter_pk) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: adapter_version_history.tenant_id does not match adapter tenant')
    END;
END;

CREATE INDEX IF NOT EXISTS idx_adapter_version_history_tenant ON adapter_version_history(tenant_id);

-- 9c. Add tenant_id to evidence_entries (currently has none)
ALTER TABLE evidence_entries ADD COLUMN tenant_id TEXT;

-- Backfill tenant_id from training_datasets (for entries with dataset_id)
UPDATE evidence_entries SET tenant_id = (
    SELECT td.tenant_id FROM training_datasets td WHERE td.id = evidence_entries.dataset_id
) WHERE dataset_id IS NOT NULL AND tenant_id IS NULL;

-- Backfill tenant_id from adapters (for entries with only adapter_id)
UPDATE evidence_entries SET tenant_id = (
    SELECT a.tenant_id FROM adapters a WHERE a.id = evidence_entries.adapter_id
) WHERE adapter_id IS NOT NULL AND tenant_id IS NULL;

-- Trigger: evidence_entries must match tenant of dataset (if present)
CREATE TRIGGER IF NOT EXISTS trg_evidence_entries_dataset_tenant_check
BEFORE INSERT ON evidence_entries
FOR EACH ROW
WHEN NEW.dataset_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: evidence_entry.dataset_id references dataset from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_evidence_entries_dataset_tenant_check_update
BEFORE UPDATE OF dataset_id ON evidence_entries
FOR EACH ROW
WHEN NEW.dataset_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: evidence_entry.dataset_id references dataset from different tenant')
    END;
END;

-- Trigger: evidence_entries must match tenant of adapter (if present)
CREATE TRIGGER IF NOT EXISTS trg_evidence_entries_adapter_tenant_check
BEFORE INSERT ON evidence_entries
FOR EACH ROW
WHEN NEW.adapter_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapters WHERE id = NEW.adapter_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: evidence_entry.adapter_id references adapter from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_evidence_entries_adapter_tenant_check_update
BEFORE UPDATE OF adapter_id ON evidence_entries
FOR EACH ROW
WHEN NEW.adapter_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapters WHERE id = NEW.adapter_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: evidence_entry.adapter_id references adapter from different tenant')
    END;
END;

CREATE INDEX IF NOT EXISTS idx_evidence_entries_tenant ON evidence_entries(tenant_id);

-- 9d. Add tenant_id to dataset_files (currently has none)
ALTER TABLE dataset_files ADD COLUMN tenant_id TEXT;

-- Backfill tenant_id from training_datasets
UPDATE dataset_files SET tenant_id = (
    SELECT td.tenant_id FROM training_datasets td WHERE td.id = dataset_files.dataset_id
);

-- Trigger: dataset_files must match tenant of dataset
CREATE TRIGGER IF NOT EXISTS trg_dataset_files_tenant_check
BEFORE INSERT ON dataset_files
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) IS NULL
        THEN RAISE(ABORT, 'Invalid dataset_id: dataset does not exist')
        WHEN NEW.tenant_id IS NOT NULL AND
             (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: dataset_file.tenant_id does not match dataset tenant')
    END;
END;

CREATE INDEX IF NOT EXISTS idx_dataset_files_tenant ON dataset_files(tenant_id);

-- 9e. Add tenant_id to dataset_statistics (currently has none)
ALTER TABLE dataset_statistics ADD COLUMN tenant_id TEXT;

-- Backfill tenant_id from training_datasets
UPDATE dataset_statistics SET tenant_id = (
    SELECT td.tenant_id FROM training_datasets td WHERE td.id = dataset_statistics.dataset_id
);

-- Trigger: dataset_statistics must match tenant of dataset
CREATE TRIGGER IF NOT EXISTS trg_dataset_statistics_tenant_check
BEFORE INSERT ON dataset_statistics
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) IS NULL
        THEN RAISE(ABORT, 'Invalid dataset_id: dataset does not exist')
        WHEN NEW.tenant_id IS NOT NULL AND
             (SELECT tenant_id FROM training_datasets WHERE id = NEW.dataset_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: dataset_statistics.tenant_id does not match dataset tenant')
    END;
END;

CREATE INDEX IF NOT EXISTS idx_dataset_statistics_tenant ON dataset_statistics(tenant_id);
