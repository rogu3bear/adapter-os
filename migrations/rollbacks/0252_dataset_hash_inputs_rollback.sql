-- Rollback Migration 0252: Dataset Hash Inputs
-- Purpose: Drop the dataset_hash_inputs table and all its supporting structures
-- Date: 2025-12-29
--
-- Dependencies to handle:
-- - Migration 0252 adds columns to this table (must rollback 0252 first!)
-- - Indexes on content_hash_b3, commit_sha, repo_slug, tenant_id, dataset_id
-- - Foreign key from dataset_id to training_datasets (CASCADE)
-- - Foreign key from tenant_id to tenants (CASCADE)
-- - UNIQUE constraint on (dataset_id, content_hash_b3)
--
-- CRITICAL: Run 0254_algorithm_version_tracking_rollback.sql BEFORE this rollback!
--
-- WARNING: This will permanently delete all dataset hash input records.
-- Backup data before executing if needed.

-- Step 1: Drop all indexes
DROP INDEX IF EXISTS idx_dhi_dataset;
DROP INDEX IF EXISTS idx_dhi_tenant;
DROP INDEX IF EXISTS idx_dhi_repo_slug;
DROP INDEX IF EXISTS idx_dhi_commit;
DROP INDEX IF EXISTS idx_dhi_content_hash;

-- Step 2: Drop the table (CASCADE will handle FK cleanup)
DROP TABLE IF EXISTS dataset_hash_inputs;
