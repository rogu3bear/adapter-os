-- Migration 0246: Add branch and commit tracking to training_datasets
-- Enables creating datasets for specific branch runs
--
-- This migration adds branch/commit information to training_datasets to support:
-- - Associating datasets with specific branch runs
-- - Querying datasets by branch for branch-specific training workflows
-- - Tracking which commit a dataset was generated from
--
-- The branch column stores the git branch name (e.g., "main", "feature/xyz")
-- The commit_sha column stores the git commit hash at dataset creation time

-- Add branch column to training_datasets
ALTER TABLE training_datasets
ADD COLUMN branch TEXT;

-- Add commit_sha column to training_datasets
ALTER TABLE training_datasets
ADD COLUMN commit_sha TEXT;

-- Index for efficient branch lookups
CREATE INDEX IF NOT EXISTS idx_training_datasets_branch
    ON training_datasets(branch)
    WHERE branch IS NOT NULL;

-- Index for commit_sha lookups
CREATE INDEX IF NOT EXISTS idx_training_datasets_commit_sha
    ON training_datasets(commit_sha)
    WHERE commit_sha IS NOT NULL;

-- Composite index for tenant + branch queries (common pattern)
CREATE INDEX IF NOT EXISTS idx_training_datasets_tenant_branch
    ON training_datasets(tenant_id, branch)
    WHERE branch IS NOT NULL;

-- Composite index for repo_slug + branch queries (finding datasets for a repo branch)
CREATE INDEX IF NOT EXISTS idx_training_datasets_repo_branch
    ON training_datasets(repo_slug, branch)
    WHERE repo_slug IS NOT NULL AND branch IS NOT NULL;
