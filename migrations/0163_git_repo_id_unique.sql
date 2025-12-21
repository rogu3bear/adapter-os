-- Git repository repo_id uniqueness
-- Purpose: Ensure FK parents for training/evidence tables reference a unique key.
-- Context: repository_training_jobs.repo_id (and related tables) reference
-- git_repositories.repo_id, which must be UNIQUE for SQLite FK validation.

-- Replace the non-unique index with a unique one to satisfy FK requirements
DROP INDEX IF EXISTS idx_git_repositories_repo;

CREATE UNIQUE INDEX IF NOT EXISTS idx_git_repositories_repo
    ON git_repositories(repo_id);

