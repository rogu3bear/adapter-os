-- Migration 0138: Adapter Archive and Garbage Collection Support
--
-- Adds columns and indexes for adapter archival lifecycle:
-- - archived_at: Timestamp when adapter was archived (tenant cascade or manual)
-- - purged_at: Timestamp when .aos file was deleted (GC operation)
-- - archived_by: User/system that initiated archive
-- - archive_reason: Human-readable reason for archival
--
-- Lifecycle model:
--   lifecycle_state: draft -> active -> deprecated -> retired (semantic behavior)
--   archived_at: NULL (active) -> timestamp (archived, file may exist)
--   purged_at: NULL (file exists) -> timestamp (file deleted, record kept for audit)
--
-- Invariants:
-- - purged_at can only be set if archived_at is set
-- - purged adapters have aos_file_path = NULL
-- - archived/purged adapters cannot be loaded for inference
--
-- PRD Reference: 3A (flat storage layout) and 4C (archive + GC model)

-- Add archive/purge lifecycle columns
ALTER TABLE adapters ADD COLUMN archived_at TEXT DEFAULT NULL;
ALTER TABLE adapters ADD COLUMN archived_by TEXT DEFAULT NULL;
ALTER TABLE adapters ADD COLUMN archive_reason TEXT DEFAULT NULL;
ALTER TABLE adapters ADD COLUMN purged_at TEXT DEFAULT NULL;

-- Index for GC queries: find archived adapters by age
CREATE INDEX IF NOT EXISTS idx_adapters_archived_at
    ON adapters(archived_at)
    WHERE archived_at IS NOT NULL;

-- Composite index for active (non-archived) adapters - common query pattern
CREATE INDEX IF NOT EXISTS idx_adapters_active_not_archived
    ON adapters(tenant_id, lifecycle_state)
    WHERE archived_at IS NULL AND active = 1;

-- Index for finding purge candidates (archived but not yet purged, with file path)
CREATE INDEX IF NOT EXISTS idx_adapters_archive_gc_candidates
    ON adapters(tenant_id, archived_at)
    WHERE archived_at IS NOT NULL AND purged_at IS NULL AND aos_file_path IS NOT NULL;

-- Trigger to enforce purge invariants:
-- 1. Cannot purge if not archived
-- 2. Must clear aos_file_path when purging
CREATE TRIGGER IF NOT EXISTS validate_adapter_purge
BEFORE UPDATE ON adapters
FOR EACH ROW
WHEN NEW.purged_at IS NOT NULL AND OLD.purged_at IS NULL
BEGIN
    SELECT CASE
        WHEN NEW.archived_at IS NULL
        THEN RAISE(ABORT, 'Cannot purge adapter that is not archived')
        WHEN NEW.aos_file_path IS NOT NULL
        THEN RAISE(ABORT, 'Must clear aos_file_path when purging adapter')
    END;
END;

-- Trigger to prevent loading purged adapters
-- Restricts load_state changes for purged adapters to only 'cold', 'unloaded', or 'error'
CREATE TRIGGER IF NOT EXISTS prevent_purged_adapter_load
BEFORE UPDATE ON adapters
FOR EACH ROW
WHEN OLD.purged_at IS NOT NULL
  AND NEW.load_state IS NOT NULL
  AND NEW.load_state != OLD.load_state
BEGIN
    SELECT CASE
        WHEN NEW.load_state NOT IN ('cold', 'unloaded', 'error')
        THEN RAISE(ABORT, 'Cannot load purged adapter - .aos file has been deleted')
    END;
END;

-- Trigger to prevent re-archiving already archived adapters with different timestamp
-- (allows updating archive_reason but not changing archived_at once set)
CREATE TRIGGER IF NOT EXISTS prevent_archive_timestamp_change
BEFORE UPDATE ON adapters
FOR EACH ROW
WHEN OLD.archived_at IS NOT NULL
  AND NEW.archived_at IS NOT NULL
  AND OLD.archived_at != NEW.archived_at
BEGIN
    SELECT RAISE(ABORT, 'Cannot change archived_at timestamp once set');
END;
