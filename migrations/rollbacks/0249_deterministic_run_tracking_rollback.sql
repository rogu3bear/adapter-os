-- Rollback Migration 0249: Deterministic Run Tracking
-- Purpose: Remove determinism tracking columns from repository_training_jobs
-- Date: 2025-12-29
--
-- Dependencies to handle:
-- - Index on dataset_id + is_deterministic_run
-- - No foreign key dependencies from other tables
--
-- WARNING: This will lose determinism configuration data for all training jobs.
-- Backup data before executing if needed.

-- Step 1: Drop the index first
DROP INDEX IF EXISTS idx_training_jobs_deterministic_dataset;

-- Step 2: Drop the columns
-- Note: Requires SQLite 3.35.0+ for DROP COLUMN support
ALTER TABLE repository_training_jobs DROP COLUMN seed_mode;
ALTER TABLE repository_training_jobs DROP COLUMN determinism_config_json;
ALTER TABLE repository_training_jobs DROP COLUMN global_seed_hex;
ALTER TABLE repository_training_jobs DROP COLUMN is_deterministic_run;
