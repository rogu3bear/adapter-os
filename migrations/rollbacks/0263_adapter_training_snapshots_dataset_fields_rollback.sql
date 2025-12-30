-- Rollback for 0263_adapter_training_snapshots_dataset_fields.sql
-- Note: Requires SQLite 3.35.0+ for DROP COLUMN support

-- Drop indexes first
DROP INDEX IF EXISTS idx_adapter_training_snapshots_dataset_hash_b3;
DROP INDEX IF EXISTS idx_adapter_training_snapshots_dataset_version_id;
DROP INDEX IF EXISTS idx_adapter_training_snapshots_dataset_id;

-- Drop columns added by the migration
ALTER TABLE adapter_training_snapshots DROP COLUMN dataset_hash_b3;
ALTER TABLE adapter_training_snapshots DROP COLUMN dataset_version_id;
ALTER TABLE adapter_training_snapshots DROP COLUMN dataset_id;
