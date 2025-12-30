-- Rollback for 0255_training_job_metadata_columns.sql
-- Note: Requires SQLite 3.35.0+ for DROP COLUMN support

-- Drop columns added by the migration (in reverse order)
ALTER TABLE repository_training_jobs DROP COLUMN dataset_hash_b3;
ALTER TABLE repository_training_jobs DROP COLUMN backend_device;
ALTER TABLE repository_training_jobs DROP COLUMN backend_reason;
ALTER TABLE repository_training_jobs DROP COLUMN backend;
ALTER TABLE repository_training_jobs DROP COLUMN scope;
ALTER TABLE repository_training_jobs DROP COLUMN lora_strength;
ALTER TABLE repository_training_jobs DROP COLUMN lora_tier;
