-- Rollback Migration 0248: Training Datasets Branch/Commit Tracking
-- Purpose: Remove branch and commit_sha columns from training_datasets
-- Date: 2025-12-29
--
-- Dependencies to handle:
-- - Indexes on branch and commit_sha columns
-- - No foreign key dependencies from other tables
--
-- WARNING: This will lose branch/commit tracking data for all datasets.
-- Backup data before executing if needed.

-- Step 1: Drop indexes first (they reference the columns)
DROP INDEX IF EXISTS idx_training_datasets_repo_branch;
DROP INDEX IF EXISTS idx_training_datasets_tenant_branch;
DROP INDEX IF EXISTS idx_training_datasets_commit_sha;
DROP INDEX IF EXISTS idx_training_datasets_branch;

-- Step 2: Drop the columns
-- Note: Requires SQLite 3.35.0+ for DROP COLUMN support
ALTER TABLE training_datasets DROP COLUMN commit_sha;
ALTER TABLE training_datasets DROP COLUMN branch;
