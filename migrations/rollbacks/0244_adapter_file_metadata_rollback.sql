-- Rollback for 0244_adapter_file_metadata.sql
-- Note: Requires SQLite 3.35.0+ for DROP COLUMN support

-- Drop indexes first
DROP INDEX IF EXISTS idx_aos_adapter_metadata_modified_at;
DROP INDEX IF EXISTS idx_aos_adapter_metadata_file_size;

-- Drop columns added by the migration
ALTER TABLE aos_adapter_metadata DROP COLUMN tier;
ALTER TABLE aos_adapter_metadata DROP COLUMN category;
ALTER TABLE aos_adapter_metadata DROP COLUMN base_model;
ALTER TABLE aos_adapter_metadata DROP COLUMN manifest_schema_version;
ALTER TABLE aos_adapter_metadata DROP COLUMN segment_count;
ALTER TABLE aos_adapter_metadata DROP COLUMN file_modified_at;
ALTER TABLE aos_adapter_metadata DROP COLUMN file_size_bytes;
