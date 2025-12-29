-- Training Dataset Rows Tables
-- Migration: 0244
-- Purpose: Create tables for storing training dataset rows
--
-- This migration creates two tables:
-- 1. training_dataset_rows: General-purpose training rows (prompt/response pairs)
-- 2. codebase_dataset_rows: Specialized rows for code-extracted training data
--
-- The training_dataset_rows table stores CanonicalRow-like data for general datasets
-- created from uploads, synthetic generation, or other non-codebase sources.
--
-- The codebase_dataset_rows table stores rows extracted during code ingestion with
-- full provenance tracking (file paths, line numbers, symbols, etc.)

-- ============================================================================
-- General Training Dataset Rows
-- ============================================================================
-- Stores prompt/response training pairs for general-purpose datasets.
-- Maps to CanonicalRow API format for compatibility with training workers.

CREATE TABLE IF NOT EXISTS training_dataset_rows (
    id TEXT PRIMARY KEY,
    -- Parent references
    dataset_id TEXT NOT NULL REFERENCES training_datasets(id) ON DELETE CASCADE,
    dataset_version_id TEXT REFERENCES training_dataset_versions(id) ON DELETE SET NULL,
    -- Session for atomic operations (ingestion runs, bulk uploads)
    session_id TEXT,
    -- Core training data (matches CanonicalRow schema)
    prompt TEXT NOT NULL,
    response TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1.0,
    -- Data split for train/eval/test separation
    split TEXT NOT NULL DEFAULT 'train',
    -- Sample role: positive (teach knowledge) or negative (teach abstention)
    sample_role TEXT NOT NULL DEFAULT 'positive',
    -- Content hash for deduplication (BLAKE3 of prompt:response:weight)
    content_hash_b3 TEXT NOT NULL,
    -- Source tracking
    source_type TEXT, -- upload, synthetic, api, pipeline
    source_file TEXT, -- Original file name if from upload
    source_line INTEGER, -- Line number in source file if applicable
    -- Tenant isolation
    tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE,
    -- Additional metadata (JSON)
    metadata_json TEXT,
    -- Audit fields
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT
);

-- Indexes for training_dataset_rows
CREATE INDEX IF NOT EXISTS idx_tdr_dataset ON training_dataset_rows(dataset_id);
CREATE INDEX IF NOT EXISTS idx_tdr_version ON training_dataset_rows(dataset_version_id);
CREATE INDEX IF NOT EXISTS idx_tdr_session ON training_dataset_rows(session_id);
CREATE INDEX IF NOT EXISTS idx_tdr_tenant ON training_dataset_rows(tenant_id);
CREATE INDEX IF NOT EXISTS idx_tdr_split ON training_dataset_rows(dataset_id, split);
CREATE INDEX IF NOT EXISTS idx_tdr_content_hash ON training_dataset_rows(content_hash_b3);
CREATE INDEX IF NOT EXISTS idx_tdr_source_type ON training_dataset_rows(source_type) WHERE source_type IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tdr_dataset_role ON training_dataset_rows(dataset_id, sample_role);

-- ============================================================================
-- Codebase Dataset Rows
-- ============================================================================
-- Stores prompt/response training pairs extracted from code during ingestion.
-- Includes full provenance tracking for reproducibility and lineage.

CREATE TABLE IF NOT EXISTS codebase_dataset_rows (
    id TEXT PRIMARY KEY,
    -- Parent references
    dataset_id TEXT NOT NULL REFERENCES training_datasets(id) ON DELETE CASCADE,
    dataset_version_id TEXT REFERENCES training_dataset_versions(id) ON DELETE SET NULL,
    -- Session for atomic operations (ingestion runs)
    session_id TEXT,
    -- Core training data
    prompt TEXT NOT NULL,
    response TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1.0,
    sample_role TEXT NOT NULL DEFAULT 'positive',
    -- Code symbol metadata
    symbol_kind TEXT, -- function, struct, enum, module, etc.
    language TEXT, -- rust, python, typescript, etc.
    file_path TEXT, -- Relative path within repository
    start_line INTEGER,
    end_line INTEGER,
    qualified_name TEXT, -- Fully qualified symbol name (e.g., crate::module::function)
    -- Repository metadata
    commit_sha TEXT,
    repo_name TEXT,
    repo_slug TEXT,
    repo_identifier TEXT, -- Unique repo identifier (owner/repo or path)
    project_name TEXT,
    -- Quality indicators
    has_docstring INTEGER NOT NULL DEFAULT 0,
    -- Content hash for deduplication
    content_hash_b3 TEXT NOT NULL,
    -- Additional metadata
    metadata_json TEXT,
    -- Tenant isolation
    tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE,
    -- Audit fields
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for codebase_dataset_rows
CREATE INDEX IF NOT EXISTS idx_cdr_dataset ON codebase_dataset_rows(dataset_id);
CREATE INDEX IF NOT EXISTS idx_cdr_version ON codebase_dataset_rows(dataset_version_id);
CREATE INDEX IF NOT EXISTS idx_cdr_session ON codebase_dataset_rows(session_id);
CREATE INDEX IF NOT EXISTS idx_cdr_tenant ON codebase_dataset_rows(tenant_id);
CREATE INDEX IF NOT EXISTS idx_cdr_content_hash ON codebase_dataset_rows(content_hash_b3);
CREATE INDEX IF NOT EXISTS idx_cdr_file ON codebase_dataset_rows(dataset_id, file_path);
CREATE INDEX IF NOT EXISTS idx_cdr_symbol ON codebase_dataset_rows(dataset_id, qualified_name);
CREATE INDEX IF NOT EXISTS idx_cdr_repo ON codebase_dataset_rows(repo_identifier) WHERE repo_identifier IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_cdr_language ON codebase_dataset_rows(language) WHERE language IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_cdr_dataset_role ON codebase_dataset_rows(dataset_id, sample_role);
