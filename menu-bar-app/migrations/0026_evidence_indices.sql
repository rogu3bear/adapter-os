-- Evidence index metadata and state tracking
-- This migration adds tables for managing evidence indices per tenant

-- Evidence index metadata
CREATE TABLE IF NOT EXISTS evidence_indices (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    repo_id TEXT NOT NULL,
    index_type TEXT NOT NULL CHECK(index_type IN ('symbol','test','doc','vector')),
    index_path TEXT NOT NULL,
    last_updated TEXT NOT NULL,
    document_count INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'ready' CHECK(status IN ('building','ready','error')),
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_evidence_indices_tenant ON evidence_indices(tenant_id);
CREATE INDEX idx_evidence_indices_repo ON evidence_indices(repo_id);
CREATE INDEX idx_evidence_indices_type ON evidence_indices(index_type);
CREATE INDEX idx_evidence_indices_status ON evidence_indices(status);

-- File-to-index mapping for incremental updates
CREATE TABLE IF NOT EXISTS evidence_file_tracking (
    id TEXT PRIMARY KEY NOT NULL,
    index_id TEXT NOT NULL REFERENCES evidence_indices(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    last_indexed TEXT NOT NULL DEFAULT (datetime('now')),
    file_hash TEXT NOT NULL,
    UNIQUE(index_id, file_path)
);

CREATE INDEX idx_evidence_file_tracking_index ON evidence_file_tracking(index_id);
CREATE INDEX idx_evidence_file_tracking_path ON evidence_file_tracking(file_path);
CREATE INDEX idx_evidence_file_tracking_commit ON evidence_file_tracking(commit_sha);
