-- Rollback for 0246_training_dataset_rows.sql
-- WARNING: This will delete all data in the affected tables

-- Drop indexes for codebase_dataset_rows
DROP INDEX IF EXISTS idx_cdr_dataset_role;
DROP INDEX IF EXISTS idx_cdr_language;
DROP INDEX IF EXISTS idx_cdr_repo;
DROP INDEX IF EXISTS idx_cdr_symbol;
DROP INDEX IF EXISTS idx_cdr_file;
DROP INDEX IF EXISTS idx_cdr_content_hash;
DROP INDEX IF EXISTS idx_cdr_tenant;
DROP INDEX IF EXISTS idx_cdr_session;
DROP INDEX IF EXISTS idx_cdr_version;
DROP INDEX IF EXISTS idx_cdr_dataset;

-- Drop indexes for training_dataset_rows
DROP INDEX IF EXISTS idx_tdr_dataset_role;
DROP INDEX IF EXISTS idx_tdr_source_type;
DROP INDEX IF EXISTS idx_tdr_content_hash;
DROP INDEX IF EXISTS idx_tdr_split;
DROP INDEX IF EXISTS idx_tdr_tenant;
DROP INDEX IF EXISTS idx_tdr_session;
DROP INDEX IF EXISTS idx_tdr_version;
DROP INDEX IF EXISTS idx_tdr_dataset;

-- Drop tables
DROP TABLE IF EXISTS codebase_dataset_rows;
DROP TABLE IF EXISTS training_dataset_rows;
