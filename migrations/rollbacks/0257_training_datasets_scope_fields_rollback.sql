-- Rollback Migration 0257: Remove codebase scope fields from training_datasets

-- Drop indexes first
DROP INDEX IF EXISTS idx_training_datasets_scope_scan_root;
DROP INDEX IF EXISTS idx_training_datasets_repo_branch_commit;
DROP INDEX IF EXISTS idx_training_datasets_tenant_scope_repo;
DROP INDEX IF EXISTS idx_training_datasets_scope_repo_id;

-- Drop columns
ALTER TABLE training_datasets DROP COLUMN scope_remote_url;
ALTER TABLE training_datasets DROP COLUMN scope_scan_root;
ALTER TABLE training_datasets DROP COLUMN scope_repo;
ALTER TABLE training_datasets DROP COLUMN scope_repo_id;
