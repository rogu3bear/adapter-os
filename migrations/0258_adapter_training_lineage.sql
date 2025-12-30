-- Migration 0258: Create adapter_training_lineage table for reverse mapping
--
-- Purpose: Enable reverse lookups from dataset versions to trained adapters.
-- This closes the lineage gap by allowing:
-- - "Which adapters were trained on this dataset version?"
-- - "Which dataset versions contributed to this adapter?"
-- - Cascade impact analysis for dataset version updates
--
-- Evidence: PRD-RECT-001 lineage tracking requirements
-- Pattern: Junction table with audit metadata

CREATE TABLE IF NOT EXISTS adapter_training_lineage (
    id TEXT PRIMARY KEY,

    -- Adapter reference (required)
    adapter_id TEXT NOT NULL,

    -- Dataset references (at least dataset_id is required)
    dataset_id TEXT NOT NULL REFERENCES training_datasets(id) ON DELETE CASCADE,
    dataset_version_id TEXT REFERENCES training_dataset_versions(id) ON DELETE SET NULL,

    -- Training job that created this lineage (for provenance)
    training_job_id TEXT REFERENCES repository_training_jobs(id) ON DELETE SET NULL,

    -- Snapshot of dataset hash at training time (for reproducibility)
    dataset_hash_b3_at_training TEXT,

    -- Role of this dataset in training (e.g., "primary", "validation", "augmentation")
    role TEXT NOT NULL DEFAULT 'primary',

    -- Weight of this dataset in the training mix
    weight REAL,

    -- Ordering for multi-dataset training
    ordinal INTEGER NOT NULL DEFAULT 0,

    -- Tenant isolation
    tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE,

    -- Audit fields
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT,
    metadata_json TEXT,

    -- Ensure unique lineage entries per adapter + dataset + version combination
    UNIQUE(adapter_id, dataset_id, dataset_version_id)
);

-- =============================================================================
-- Indexes for lineage queries
-- =============================================================================

-- Primary lookup: find all datasets used to train an adapter
CREATE INDEX IF NOT EXISTS idx_atl_adapter_id
    ON adapter_training_lineage(adapter_id);

-- Reverse lookup: find all adapters trained on a dataset
CREATE INDEX IF NOT EXISTS idx_atl_dataset_id
    ON adapter_training_lineage(dataset_id);

-- Reverse lookup: find all adapters trained on a specific dataset version
CREATE INDEX IF NOT EXISTS idx_atl_dataset_version_id
    ON adapter_training_lineage(dataset_version_id)
    WHERE dataset_version_id IS NOT NULL;

-- Find lineage for a specific training job
CREATE INDEX IF NOT EXISTS idx_atl_training_job_id
    ON adapter_training_lineage(training_job_id)
    WHERE training_job_id IS NOT NULL;

-- Tenant-scoped queries
CREATE INDEX IF NOT EXISTS idx_atl_tenant_id
    ON adapter_training_lineage(tenant_id)
    WHERE tenant_id IS NOT NULL;

-- Hash-based lookup for reproducibility verification
CREATE INDEX IF NOT EXISTS idx_atl_dataset_hash
    ON adapter_training_lineage(dataset_hash_b3_at_training)
    WHERE dataset_hash_b3_at_training IS NOT NULL;

-- =============================================================================
-- Backfill lineage from existing adapter + training job data
-- =============================================================================

-- Populate lineage from training jobs that have both adapter_id and dataset_id
INSERT OR IGNORE INTO adapter_training_lineage (
    id,
    adapter_id,
    dataset_id,
    dataset_version_id,
    training_job_id,
    role,
    ordinal,
    tenant_id,
    created_by
)
SELECT
    lower(hex(randomblob(16))),
    j.adapter_id,
    j.dataset_id,
    j.dataset_version_id,
    j.id,
    'primary',
    0,
    j.tenant_id,
    j.created_by
FROM repository_training_jobs j
WHERE j.adapter_id IS NOT NULL
  AND j.dataset_id IS NOT NULL;

-- Also populate from adapters table dataset_version_id (migration 0251)
-- Only if not already present from training jobs
INSERT OR IGNORE INTO adapter_training_lineage (
    id,
    adapter_id,
    dataset_id,
    dataset_version_id,
    role,
    ordinal,
    tenant_id
)
SELECT
    lower(hex(randomblob(16))),
    a.id,
    v.dataset_id,
    a.dataset_version_id,
    'primary',
    0,
    a.tenant_id
FROM adapters a
JOIN training_dataset_versions v ON v.id = a.dataset_version_id
WHERE a.dataset_version_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1 FROM adapter_training_lineage atl
    WHERE atl.adapter_id = a.id
      AND atl.dataset_version_id = a.dataset_version_id
  );
