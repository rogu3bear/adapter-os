-- Migration 0256: Add session fields to training_datasets for lineage tracking
--
-- Purpose: Promote session metadata from metadata_json to indexed columns
-- for efficient querying and to close the lineage gap between codebase
-- ingestion and dataset version tracking.
--
-- Evidence: PRD-RECT-001 tenant isolation, session-scoped operations
-- Pattern: Column promotion from metadata_json for performance
--
-- Fields are nullable to support legacy datasets created before this migration.

-- Session ID that created this dataset (for atomic rollback/grouping)
ALTER TABLE training_datasets ADD COLUMN session_id TEXT;

-- Human-readable session name for display/filtering
ALTER TABLE training_datasets ADD COLUMN session_name TEXT;

-- Comma-separated tags for session categorization
ALTER TABLE training_datasets ADD COLUMN session_tags TEXT;

-- =============================================================================
-- Indexes for session-based queries
-- =============================================================================

-- Index for finding all datasets in a session (critical for rollback)
CREATE INDEX IF NOT EXISTS idx_training_datasets_session_id
    ON training_datasets(session_id)
    WHERE session_id IS NOT NULL;

-- Composite index for tenant + session queries (common access pattern)
CREATE INDEX IF NOT EXISTS idx_training_datasets_tenant_session
    ON training_datasets(tenant_id, session_id)
    WHERE session_id IS NOT NULL;

-- Index for session name lookups
CREATE INDEX IF NOT EXISTS idx_training_datasets_session_name
    ON training_datasets(session_name)
    WHERE session_name IS NOT NULL;

-- =============================================================================
-- Backfill session fields from metadata_json
-- =============================================================================

-- Backfill session_id from metadata_json
UPDATE training_datasets
SET session_id = json_extract(metadata_json, '$.session_id')
WHERE session_id IS NULL
  AND metadata_json IS NOT NULL
  AND json_extract(metadata_json, '$.session_id') IS NOT NULL;

-- Backfill session_name from metadata_json
UPDATE training_datasets
SET session_name = json_extract(metadata_json, '$.session_name')
WHERE session_name IS NULL
  AND metadata_json IS NOT NULL
  AND json_extract(metadata_json, '$.session_name') IS NOT NULL;

-- Backfill session_tags from metadata_json
UPDATE training_datasets
SET session_tags = json_extract(metadata_json, '$.session_tags')
WHERE session_tags IS NULL
  AND metadata_json IS NOT NULL
  AND json_extract(metadata_json, '$.session_tags') IS NOT NULL;
