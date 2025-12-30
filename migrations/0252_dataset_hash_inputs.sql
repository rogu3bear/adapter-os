-- Dataset Hash Inputs Tracking for Reproducibility
--
-- Records the structured inputs used to compute dataset content hashes,
-- enabling reproducibility, auditability, and debugging of hash mismatches.
--
-- Evidence: Set 32 Point 1 - Track dataset hash inputs in code_ingestion.rs
-- Pattern: Provenance tracking with structured inputs
--
-- Use cases:
-- 1. Debug why two ingestions of "same" code produce different hashes
-- 2. Trace from adapter back to exact source code configuration
-- 3. Detect duplicate datasets by content hash
-- 4. Audit training reproducibility

CREATE TABLE IF NOT EXISTS dataset_hash_inputs (
    id TEXT PRIMARY KEY,

    -- Link to parent dataset (optional - may be recorded before dataset created)
    dataset_id TEXT REFERENCES training_datasets(id) ON DELETE CASCADE,

    -- The computed content hash (BLAKE3, 64 hex chars)
    content_hash_b3 TEXT NOT NULL,

    -- Repository provenance
    repo_id TEXT,
    repo_slug TEXT,
    commit_sha TEXT,
    branch TEXT,
    scan_root_path TEXT,
    remote_url TEXT,

    -- Ingestion configuration that affects hash
    max_symbols INTEGER,
    include_private INTEGER,  -- boolean as 0/1
    positive_weight REAL,
    negative_weight REAL,

    -- Sample statistics
    total_samples INTEGER NOT NULL,
    positive_samples INTEGER NOT NULL,
    negative_samples INTEGER NOT NULL,

    -- Ingestion metadata
    ingestion_mode TEXT DEFAULT 'code_graph',  -- code_graph, document, synthetic, etc.
    codegraph_version TEXT,
    generator TEXT DEFAULT 'code_ingestion_pipeline',

    -- Scope configuration (JSON for extensibility)
    scope_config_json TEXT,

    -- Additional inputs for extensibility (JSON)
    additional_inputs_json TEXT,

    -- Tenant isolation
    tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE,

    -- Audit
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT,

    -- Prevent duplicate recordings for same dataset/hash combo
    UNIQUE(dataset_id, content_hash_b3)
);

-- Index for content hash lookups (find all datasets with same content)
CREATE INDEX IF NOT EXISTS idx_dhi_content_hash
    ON dataset_hash_inputs(content_hash_b3);

-- Index for commit-based provenance queries
CREATE INDEX IF NOT EXISTS idx_dhi_commit
    ON dataset_hash_inputs(commit_sha)
    WHERE commit_sha IS NOT NULL;

-- Index for repo-based queries
CREATE INDEX IF NOT EXISTS idx_dhi_repo_slug
    ON dataset_hash_inputs(repo_slug)
    WHERE repo_slug IS NOT NULL;

-- Index for tenant-scoped queries
CREATE INDEX IF NOT EXISTS idx_dhi_tenant
    ON dataset_hash_inputs(tenant_id)
    WHERE tenant_id IS NOT NULL;

-- Index for dataset lookups
CREATE INDEX IF NOT EXISTS idx_dhi_dataset
    ON dataset_hash_inputs(dataset_id)
    WHERE dataset_id IS NOT NULL;
