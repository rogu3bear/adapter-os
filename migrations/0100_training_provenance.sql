-- Training Job Provenance Enhancement
-- Migration: 0100
-- Purpose: Add complete provenance tracking to training jobs
--
-- This migration adds columns to repository_training_jobs for:
-- - dataset_id: Direct link to training dataset used
-- - base_model_id: Reference to base model trained from
-- - collection_id: Document collection used for training
-- - tenant_id: Explicit tenant isolation (fixes current pattern-matching approach)
-- - build_id: Git commit + version at training time for reproducibility
-- - source_documents_json: Immutable snapshot of document hashes at job start
--
-- Evidence: This addresses the gaps identified in training job provenance:
-- - Training jobs had no direct link to datasets
-- - Base model version was only tracked on adapters, not jobs
-- - Tenant isolation used LIKE pattern matching instead of FK
-- - No build/CI context for reproducibility

-- Add provenance columns to training jobs
ALTER TABLE repository_training_jobs ADD COLUMN dataset_id TEXT REFERENCES training_datasets(id) ON DELETE SET NULL;
ALTER TABLE repository_training_jobs ADD COLUMN base_model_id TEXT REFERENCES models(id) ON DELETE SET NULL;
ALTER TABLE repository_training_jobs ADD COLUMN collection_id TEXT REFERENCES document_collections(id) ON DELETE SET NULL;
ALTER TABLE repository_training_jobs ADD COLUMN tenant_id TEXT REFERENCES tenants(id) ON DELETE SET NULL;
ALTER TABLE repository_training_jobs ADD COLUMN build_id TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN source_documents_json TEXT;

-- Create indices for provenance queries
CREATE INDEX IF NOT EXISTS idx_training_jobs_dataset ON repository_training_jobs(dataset_id);
CREATE INDEX IF NOT EXISTS idx_training_jobs_base_model ON repository_training_jobs(base_model_id);
CREATE INDEX IF NOT EXISTS idx_training_jobs_collection ON repository_training_jobs(collection_id);
CREATE INDEX IF NOT EXISTS idx_training_jobs_tenant ON repository_training_jobs(tenant_id);
CREATE INDEX IF NOT EXISTS idx_training_jobs_build ON repository_training_jobs(build_id);

-- Composite index for tenant+status queries (common pattern)
CREATE INDEX IF NOT EXISTS idx_training_jobs_tenant_status ON repository_training_jobs(tenant_id, status);

-- Composite index for dataset provenance queries
CREATE INDEX IF NOT EXISTS idx_training_jobs_dataset_status ON repository_training_jobs(dataset_id, status);

-- Backfill existing jobs with default values (per user preference)
-- Note: These defaults indicate data predates provenance tracking
UPDATE repository_training_jobs SET base_model_id = 'unknown' WHERE base_model_id IS NULL;
UPDATE repository_training_jobs SET build_id = 'pre-0100-unknown' WHERE build_id IS NULL;
UPDATE repository_training_jobs SET tenant_id = 'default' WHERE tenant_id IS NULL;
