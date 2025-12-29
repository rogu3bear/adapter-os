-- Dataset Scan Roots Table
-- Migration: 0242
-- Purpose: Track scan roots (directories) associated with training datasets
--
-- This migration creates a junction table to link training datasets to their
-- source scan roots. When a dataset is created from code ingestion, each
-- scanned directory is recorded for provenance tracking.
--
-- Evidence: Feature requirement for multi-scan-root support in dataset creation
-- Pattern: Junction table for one-to-many relationship with provenance metadata

CREATE TABLE IF NOT EXISTS dataset_scan_roots (
    id TEXT PRIMARY KEY,
    dataset_id TEXT NOT NULL REFERENCES training_datasets(id) ON DELETE CASCADE,
    dataset_version_id TEXT REFERENCES training_dataset_versions(id) ON DELETE SET NULL,
    -- Session that created this scan root entry (for atomic rollback)
    session_id TEXT,
    -- Absolute or relative path to the scan root directory
    path TEXT NOT NULL,
    -- Optional label describing this scan root's role (e.g., "main", "lib", "tests")
    label TEXT,
    -- Number of files processed from this scan root
    file_count INTEGER,
    -- Total bytes ingested from this scan root
    byte_count INTEGER,
    -- BLAKE3 hash of the scan root's content at ingestion time
    content_hash_b3 TEXT,
    -- Timestamp when this scan root was processed
    scanned_at TEXT,
    -- Ordering for scan roots when order matters (e.g., priority scanning)
    ordinal INTEGER NOT NULL DEFAULT 0,
    -- Git repository information
    repo_name TEXT,
    repo_slug TEXT,
    commit_sha TEXT,
    branch TEXT,
    remote_url TEXT,
    -- Tenant isolation
    tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE,
    -- Audit fields
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT,
    -- Metadata for additional scan-root-specific configuration
    metadata_json TEXT
);

-- Index for querying all scan roots for a dataset
CREATE INDEX IF NOT EXISTS idx_dsr_dataset ON dataset_scan_roots(dataset_id);

-- Index for querying by dataset version
CREATE INDEX IF NOT EXISTS idx_dsr_dataset_version ON dataset_scan_roots(dataset_version_id);

-- Index for querying by session (for atomic rollback)
CREATE INDEX IF NOT EXISTS idx_dsr_session ON dataset_scan_roots(session_id);

-- Index for tenant-scoped queries
CREATE INDEX IF NOT EXISTS idx_dsr_tenant ON dataset_scan_roots(tenant_id);

-- Index for path lookups (finding datasets by source location)
CREATE INDEX IF NOT EXISTS idx_dsr_path ON dataset_scan_roots(path);

-- Composite index for common query pattern: scan roots for a dataset ordered by ordinal
CREATE INDEX IF NOT EXISTS idx_dsr_dataset_ordinal ON dataset_scan_roots(dataset_id, ordinal);

-- Index for finding scan roots by content hash (deduplication)
CREATE INDEX IF NOT EXISTS idx_dsr_content_hash ON dataset_scan_roots(content_hash_b3) WHERE content_hash_b3 IS NOT NULL;

-- Index for repository lookups
CREATE INDEX IF NOT EXISTS idx_dsr_repo ON dataset_scan_roots(repo_slug, commit_sha) WHERE repo_slug IS NOT NULL;
