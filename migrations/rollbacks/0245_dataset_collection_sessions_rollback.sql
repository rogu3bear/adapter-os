-- Rollback for 0245_dataset_collection_sessions.sql
-- WARNING: This will delete all data in the affected tables

-- Drop indexes for adapter_session_membership
DROP INDEX IF EXISTS idx_asm_adapter;
DROP INDEX IF EXISTS idx_asm_session;

-- Drop indexes for dataset_session_membership
DROP INDEX IF EXISTS idx_dsm_dataset;
DROP INDEX IF EXISTS idx_dsm_session;

-- Drop indexes for dataset_collection_sessions
DROP INDEX IF EXISTS idx_dcs_parent;
DROP INDEX IF EXISTS idx_dcs_started_at;
DROP INDEX IF EXISTS idx_dcs_external_id;
DROP INDEX IF EXISTS idx_dcs_tags;
DROP INDEX IF EXISTS idx_dcs_name;
DROP INDEX IF EXISTS idx_dcs_tenant_status;

-- Drop tables in reverse order (respecting foreign key dependencies)
DROP TABLE IF EXISTS adapter_session_membership;
DROP TABLE IF EXISTS dataset_session_membership;
DROP TABLE IF EXISTS dataset_collection_sessions;
