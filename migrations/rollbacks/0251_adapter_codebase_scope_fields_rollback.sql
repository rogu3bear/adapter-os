-- Rollback Migration 0251: Adapter Codebase Scope Fields
-- Purpose: Remove codebase registration columns from adapters table
-- Date: 2025-12-29
--
-- Dependencies to handle:
-- - Indexes on codebase_scope and dataset_version_id
-- - Foreign key from dataset_version_id to training_dataset_versions
-- - No other tables reference these columns
--
-- WARNING: This will lose codebase scope and dataset version linkage for all adapters.
-- Backup data before executing if needed.

-- Step 1: Drop indexes first
DROP INDEX IF EXISTS idx_adapters_dataset_version;
DROP INDEX IF EXISTS idx_adapters_codebase_scope;

-- Step 2: Drop the columns
-- Note: Requires SQLite 3.35.0+ for DROP COLUMN support
-- Note: Dropping dataset_version_id will remove the FK constraint automatically
ALTER TABLE adapters DROP COLUMN manifest_hash;
ALTER TABLE adapters DROP COLUMN registration_timestamp;
ALTER TABLE adapters DROP COLUMN dataset_version_id;
ALTER TABLE adapters DROP COLUMN codebase_scope;
