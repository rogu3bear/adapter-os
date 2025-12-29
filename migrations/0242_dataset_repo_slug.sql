-- Migration 0242: Add repo_slug to training_datasets for repository tagging
-- Allows datasets to be tagged with a repository slug for filtering/querying
-- by source repository (e.g., "org/repo-name").
--
-- This enables:
-- - Finding all datasets derived from a specific repository
-- - Grouping datasets by source repo in the UI
-- - Correlating datasets with adapters trained on the same repo

-- Add repo_slug column
ALTER TABLE training_datasets
ADD COLUMN repo_slug TEXT;

-- Index for efficient repo_slug lookups
CREATE INDEX IF NOT EXISTS idx_training_datasets_repo_slug
    ON training_datasets(repo_slug)
    WHERE repo_slug IS NOT NULL;

-- Composite index for tenant + repo_slug queries
CREATE INDEX IF NOT EXISTS idx_training_datasets_tenant_repo_slug
    ON training_datasets(tenant_id, repo_slug)
    WHERE repo_slug IS NOT NULL;
