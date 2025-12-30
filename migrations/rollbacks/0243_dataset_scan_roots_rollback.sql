-- Rollback for 0243_dataset_scan_roots.sql
-- WARNING: This will delete all data in the affected tables

-- Drop indexes first (in reverse order of creation)
DROP INDEX IF EXISTS idx_dsr_repo;
DROP INDEX IF EXISTS idx_dsr_content_hash;
DROP INDEX IF EXISTS idx_dsr_dataset_ordinal;
DROP INDEX IF EXISTS idx_dsr_path;
DROP INDEX IF EXISTS idx_dsr_tenant;
DROP INDEX IF EXISTS idx_dsr_session;
DROP INDEX IF EXISTS idx_dsr_dataset_version;
DROP INDEX IF EXISTS idx_dsr_dataset;

-- Drop the table
DROP TABLE IF EXISTS dataset_scan_roots;
