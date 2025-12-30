-- Rollback Migration 0259: Remove aggregate metrics and triggers

-- Drop triggers first
DROP TRIGGER IF EXISTS trg_scan_root_update_aggregates;
DROP TRIGGER IF EXISTS trg_scan_root_delete_aggregates;
DROP TRIGGER IF EXISTS trg_scan_root_insert_aggregates;

-- Drop indexes
DROP INDEX IF EXISTS idx_training_datasets_scan_roots_hash;
DROP INDEX IF EXISTS idx_training_datasets_scan_root_count;

-- Drop columns
ALTER TABLE training_datasets DROP COLUMN scan_roots_updated_at;
ALTER TABLE training_datasets DROP COLUMN scan_roots_content_hash;
ALTER TABLE training_datasets DROP COLUMN total_scan_root_bytes;
ALTER TABLE training_datasets DROP COLUMN total_scan_root_files;
ALTER TABLE training_datasets DROP COLUMN scan_root_count;
