-- Rollback for 0262_session_codebase_binding.sql
-- Note: Requires SQLite 3.35.0+ for DROP COLUMN support

-- Drop indexes first
DROP INDEX IF EXISTS idx_sessions_codebase_tenant;
DROP INDEX IF EXISTS idx_sessions_codebase_adapter;

-- Drop the column added by the migration
ALTER TABLE chat_sessions DROP COLUMN codebase_adapter_id;
