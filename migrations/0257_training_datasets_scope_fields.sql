-- Migration 0257: Add indexed codebase scope fields to training_datasets
--
-- Purpose: Promote scope fields from metadata_json to indexed columns
-- for efficient codebase adapter routing and lineage queries.
--
-- Evidence: CLI scope fields (scope_repo, scope_branch, scope_commit) used
-- in adapter manifests and routing decisions need queryable persistence.
--
-- Pattern: Column promotion from metadata_json for query performance.
-- These fields mirror the adapter manifest scope fields for lineage closure.
--
-- Note: branch and commit_sha were already added in migration 0248.
-- This migration adds the additional scope fields.

-- Repository identifier for scoped queries (e.g., "github.com/org/repo" or "repo:my_project")
ALTER TABLE training_datasets ADD COLUMN scope_repo_id TEXT;

-- Repository name (human-readable, e.g., "my-project")
ALTER TABLE training_datasets ADD COLUMN scope_repo TEXT;

-- Primary scan root path used during ingestion
ALTER TABLE training_datasets ADD COLUMN scope_scan_root TEXT;

-- Remote URL of the repository (e.g., "git@github.com:org/repo.git")
ALTER TABLE training_datasets ADD COLUMN scope_remote_url TEXT;

-- =============================================================================
-- Indexes for codebase scope queries
-- =============================================================================

-- Index for finding datasets by repository identifier
CREATE INDEX IF NOT EXISTS idx_training_datasets_scope_repo_id
    ON training_datasets(scope_repo_id)
    WHERE scope_repo_id IS NOT NULL;

-- Composite index for tenant + repo_id queries (common pattern)
CREATE INDEX IF NOT EXISTS idx_training_datasets_tenant_scope_repo
    ON training_datasets(tenant_id, scope_repo_id)
    WHERE scope_repo_id IS NOT NULL;

-- Composite index for repo + branch + commit (reproducibility queries)
-- Uses scope_repo_id with existing branch/commit_sha columns
CREATE INDEX IF NOT EXISTS idx_training_datasets_repo_branch_commit
    ON training_datasets(scope_repo_id, branch, commit_sha)
    WHERE scope_repo_id IS NOT NULL;

-- Index for scan root path lookups (finding datasets by source location)
CREATE INDEX IF NOT EXISTS idx_training_datasets_scope_scan_root
    ON training_datasets(scope_scan_root)
    WHERE scope_scan_root IS NOT NULL;

-- =============================================================================
-- Backfill scope fields from metadata_json
-- =============================================================================

-- Backfill scope_repo_id (prioritize repo_identifier, then scope_repo_id, then repo_id)
UPDATE training_datasets
SET scope_repo_id = COALESCE(
    json_extract(metadata_json, '$.repo_identifier'),
    json_extract(metadata_json, '$.scope_repo_id'),
    json_extract(metadata_json, '$.repo_id')
)
WHERE scope_repo_id IS NULL
  AND metadata_json IS NOT NULL
  AND (json_extract(metadata_json, '$.repo_identifier') IS NOT NULL
    OR json_extract(metadata_json, '$.scope_repo_id') IS NOT NULL
    OR json_extract(metadata_json, '$.repo_id') IS NOT NULL);

-- Backfill scope_repo from metadata_json
UPDATE training_datasets
SET scope_repo = COALESCE(
    json_extract(metadata_json, '$.scope_repo'),
    json_extract(metadata_json, '$.repo_name')
)
WHERE scope_repo IS NULL
  AND metadata_json IS NOT NULL
  AND (json_extract(metadata_json, '$.scope_repo') IS NOT NULL
    OR json_extract(metadata_json, '$.repo_name') IS NOT NULL);

-- Backfill scope_scan_root from metadata_json
UPDATE training_datasets
SET scope_scan_root = COALESCE(
    json_extract(metadata_json, '$.scope_scan_root'),
    json_extract(metadata_json, '$.scan_root_path'),
    json_extract(metadata_json, '$.scan_root_relative')
)
WHERE scope_scan_root IS NULL
  AND metadata_json IS NOT NULL
  AND (json_extract(metadata_json, '$.scope_scan_root') IS NOT NULL
    OR json_extract(metadata_json, '$.scan_root_path') IS NOT NULL
    OR json_extract(metadata_json, '$.scan_root_relative') IS NOT NULL);

-- Backfill scope_remote_url from metadata_json
UPDATE training_datasets
SET scope_remote_url = COALESCE(
    json_extract(metadata_json, '$.scope_remote_url'),
    json_extract(metadata_json, '$.remote_url'),
    json_extract(metadata_json, '$.repo_remote')
)
WHERE scope_remote_url IS NULL
  AND metadata_json IS NOT NULL
  AND (json_extract(metadata_json, '$.scope_remote_url') IS NOT NULL
    OR json_extract(metadata_json, '$.remote_url') IS NOT NULL
    OR json_extract(metadata_json, '$.repo_remote') IS NOT NULL);
