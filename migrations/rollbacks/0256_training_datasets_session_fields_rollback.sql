-- Rollback Migration 0256: Remove session fields from training_datasets

-- Drop indexes first
DROP INDEX IF EXISTS idx_training_datasets_session_name;
DROP INDEX IF EXISTS idx_training_datasets_tenant_session;
DROP INDEX IF EXISTS idx_training_datasets_session_id;

-- Drop columns (SQLite requires table recreation for DROP COLUMN in older versions,
-- but newer SQLite 3.35+ supports DROP COLUMN directly)
ALTER TABLE training_datasets DROP COLUMN session_tags;
ALTER TABLE training_datasets DROP COLUMN session_name;
ALTER TABLE training_datasets DROP COLUMN session_id;
