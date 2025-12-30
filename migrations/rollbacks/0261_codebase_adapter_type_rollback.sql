-- Rollback for 0261_codebase_adapter_type.sql
-- Note: Requires SQLite 3.35.0+ for DROP COLUMN support

-- Drop indexes first
DROP INDEX IF EXISTS idx_adapters_coreml_hash;
DROP INDEX IF EXISTS idx_adapters_versioning_threshold;
DROP INDEX IF EXISTS idx_adapters_type_tenant;
DROP INDEX IF EXISTS idx_adapters_base_adapter_id;
DROP INDEX IF EXISTS idx_adapters_codebase_session_unique;

-- Drop columns added by the migration (in reverse order)
ALTER TABLE adapters DROP COLUMN coreml_package_hash;
ALTER TABLE adapters DROP COLUMN versioning_threshold;
ALTER TABLE adapters DROP COLUMN stream_session_id;
ALTER TABLE adapters DROP COLUMN base_adapter_id;
ALTER TABLE adapters DROP COLUMN adapter_type;
