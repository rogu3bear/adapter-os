-- Rollback Migration 0250: Validation Determinism
-- Purpose: Remove determinism tracking columns from dataset_version_validations
-- Date: 2025-12-29
--
-- Dependencies to handle:
-- - Index on is_deterministic column
-- - No foreign key dependencies from other tables
--
-- WARNING: This will lose validation determinism data.
-- Backup data before executing if needed.

-- Step 1: Drop the index first
DROP INDEX IF EXISTS idx_dvv_determinism;

-- Step 2: Drop the columns
-- Note: Requires SQLite 3.35.0+ for DROP COLUMN support
ALTER TABLE dataset_version_validations DROP COLUMN is_deterministic;
ALTER TABLE dataset_version_validations DROP COLUMN validation_hash_b3;
ALTER TABLE dataset_version_validations DROP COLUMN determinism_mode;
ALTER TABLE dataset_version_validations DROP COLUMN validation_seed_hex;
