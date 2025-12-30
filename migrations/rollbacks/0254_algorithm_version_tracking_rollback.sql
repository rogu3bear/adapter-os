-- Rollback Migration 0254: Algorithm Version Tracking
-- Purpose: Remove algorithm version columns from training jobs, datasets, and hash inputs
-- Date: 2025-12-29
--
-- Dependencies to handle:
-- - Indexes on version columns in three tables
-- - Affects repository_training_jobs, dataset_hash_inputs, training_datasets
-- - No foreign key dependencies from other tables
--
-- CRITICAL: This must be run BEFORE 0252_dataset_hash_inputs_rollback.sql
-- since it modifies dataset_hash_inputs table.
--
-- WARNING: This will lose algorithm version tracking data across all tables.
-- Backup data before executing if needed.

-- =============================================================================
-- Step 1: Drop indexes first
-- =============================================================================
DROP INDEX IF EXISTS idx_dataset_hash_inputs_versions;
DROP INDEX IF EXISTS idx_training_datasets_algo_versions;
DROP INDEX IF EXISTS idx_training_jobs_algo_versions;

-- =============================================================================
-- Step 2: Drop columns from training_datasets
-- =============================================================================
ALTER TABLE training_datasets DROP COLUMN path_normalization_version;
ALTER TABLE training_datasets DROP COLUMN parser_algorithm_version;
ALTER TABLE training_datasets DROP COLUMN hkdf_algorithm_version;

-- =============================================================================
-- Step 3: Drop columns from dataset_hash_inputs
-- =============================================================================
ALTER TABLE dataset_hash_inputs DROP COLUMN path_normalization_version;
ALTER TABLE dataset_hash_inputs DROP COLUMN parser_version;
ALTER TABLE dataset_hash_inputs DROP COLUMN hkdf_version;

-- =============================================================================
-- Step 4: Drop columns from repository_training_jobs
-- =============================================================================
ALTER TABLE repository_training_jobs DROP COLUMN path_normalization_version;
ALTER TABLE repository_training_jobs DROP COLUMN parser_algorithm_version;
ALTER TABLE repository_training_jobs DROP COLUMN hkdf_algorithm_version;
