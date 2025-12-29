-- Training Job Datasets Junction Table
-- Migration: 0241
-- Purpose: Enable many-to-many relationship between training jobs and datasets
--
-- This migration creates a junction table to link training jobs to multiple datasets,
-- supporting multi-dataset training runs while preserving the existing single-dataset
-- provenance columns (dataset_id, dataset_version_id) for backward compatibility.
--
-- Evidence: Feature requirement for tracking which datasets are used by which training jobs
-- Pattern: Junction table for many-to-many relationship with audit metadata

CREATE TABLE IF NOT EXISTS training_job_datasets (
    id TEXT PRIMARY KEY,
    training_job_id TEXT NOT NULL REFERENCES repository_training_jobs(id) ON DELETE CASCADE,
    dataset_id TEXT NOT NULL REFERENCES training_datasets(id) ON DELETE CASCADE,
    dataset_version_id TEXT REFERENCES training_dataset_versions(id) ON DELETE SET NULL,
    -- Role of this dataset in the training job (e.g., 'primary', 'validation', 'supplementary')
    role TEXT NOT NULL DEFAULT 'primary',
    -- Ordering for datasets when order matters (e.g., curriculum learning)
    ordinal INTEGER NOT NULL DEFAULT 0,
    -- Optional weight for this dataset in the training mix
    weight REAL DEFAULT 1.0,
    -- Snapshot of dataset hash at link time for reproducibility
    hash_b3_at_link TEXT,
    -- Tenant isolation
    tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE,
    -- Audit fields
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT,
    -- Metadata for additional link-specific configuration
    metadata_json TEXT,
    UNIQUE(training_job_id, dataset_id)
);

-- Index for querying all datasets used by a training job
CREATE INDEX IF NOT EXISTS idx_tjd_training_job ON training_job_datasets(training_job_id);

-- Index for querying all training jobs that used a dataset
CREATE INDEX IF NOT EXISTS idx_tjd_dataset ON training_job_datasets(dataset_id);

-- Index for querying by dataset version
CREATE INDEX IF NOT EXISTS idx_tjd_dataset_version ON training_job_datasets(dataset_version_id);

-- Index for tenant-scoped queries
CREATE INDEX IF NOT EXISTS idx_tjd_tenant ON training_job_datasets(tenant_id);

-- Composite index for common query pattern: datasets for a job ordered by role and ordinal
CREATE INDEX IF NOT EXISTS idx_tjd_job_role_ordinal ON training_job_datasets(training_job_id, role, ordinal);
