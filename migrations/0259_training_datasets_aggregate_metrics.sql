-- Migration 0259: Add aggregate scan root metrics to training_datasets
--
-- Purpose: Store aggregated metrics from scan roots at the dataset level
-- to avoid repeated aggregation queries and enable efficient filtering.
--
-- Evidence: Performance optimization for dataset list/filter operations
-- Pattern: Denormalized aggregates with triggers for consistency

-- Aggregate metrics from scan roots
ALTER TABLE training_datasets ADD COLUMN scan_root_count INTEGER DEFAULT 0;
ALTER TABLE training_datasets ADD COLUMN total_scan_root_files INTEGER DEFAULT 0;
ALTER TABLE training_datasets ADD COLUMN total_scan_root_bytes INTEGER DEFAULT 0;

-- Computed hash of all scan root content hashes (for deduplication)
ALTER TABLE training_datasets ADD COLUMN scan_roots_content_hash TEXT;

-- Timestamp of last scan root update (for staleness detection)
ALTER TABLE training_datasets ADD COLUMN scan_roots_updated_at TEXT;

-- =============================================================================
-- Indexes for aggregate metrics queries
-- =============================================================================

-- Index for filtering datasets by scan root count
CREATE INDEX IF NOT EXISTS idx_training_datasets_scan_root_count
    ON training_datasets(scan_root_count)
    WHERE scan_root_count > 0;

-- Index for content hash lookups (deduplication)
CREATE INDEX IF NOT EXISTS idx_training_datasets_scan_roots_hash
    ON training_datasets(scan_roots_content_hash)
    WHERE scan_roots_content_hash IS NOT NULL;

-- =============================================================================
-- Backfill aggregate metrics from dataset_scan_roots
-- =============================================================================

-- Update aggregate metrics for all datasets with scan roots
UPDATE training_datasets
SET
    scan_root_count = (
        SELECT COUNT(*) FROM dataset_scan_roots
        WHERE dataset_scan_roots.dataset_id = training_datasets.id
    ),
    total_scan_root_files = (
        SELECT COALESCE(SUM(file_count), 0) FROM dataset_scan_roots
        WHERE dataset_scan_roots.dataset_id = training_datasets.id
    ),
    total_scan_root_bytes = (
        SELECT COALESCE(SUM(byte_count), 0) FROM dataset_scan_roots
        WHERE dataset_scan_roots.dataset_id = training_datasets.id
    ),
    scan_roots_updated_at = datetime('now')
WHERE EXISTS (
    SELECT 1 FROM dataset_scan_roots
    WHERE dataset_scan_roots.dataset_id = training_datasets.id
);

-- =============================================================================
-- Triggers to maintain aggregate consistency
-- =============================================================================

-- Trigger: Update aggregates on scan root insert
CREATE TRIGGER IF NOT EXISTS trg_scan_root_insert_aggregates
AFTER INSERT ON dataset_scan_roots
BEGIN
    UPDATE training_datasets
    SET
        scan_root_count = COALESCE(scan_root_count, 0) + 1,
        total_scan_root_files = COALESCE(total_scan_root_files, 0) + COALESCE(NEW.file_count, 0),
        total_scan_root_bytes = COALESCE(total_scan_root_bytes, 0) + COALESCE(NEW.byte_count, 0),
        scan_roots_updated_at = datetime('now')
    WHERE id = NEW.dataset_id;
END;

-- Trigger: Update aggregates on scan root delete
CREATE TRIGGER IF NOT EXISTS trg_scan_root_delete_aggregates
AFTER DELETE ON dataset_scan_roots
BEGIN
    UPDATE training_datasets
    SET
        scan_root_count = MAX(0, COALESCE(scan_root_count, 0) - 1),
        total_scan_root_files = MAX(0, COALESCE(total_scan_root_files, 0) - COALESCE(OLD.file_count, 0)),
        total_scan_root_bytes = MAX(0, COALESCE(total_scan_root_bytes, 0) - COALESCE(OLD.byte_count, 0)),
        scan_roots_updated_at = datetime('now')
    WHERE id = OLD.dataset_id;
END;

-- Trigger: Update aggregates on scan root update (when counts change)
CREATE TRIGGER IF NOT EXISTS trg_scan_root_update_aggregates
AFTER UPDATE OF file_count, byte_count ON dataset_scan_roots
BEGIN
    UPDATE training_datasets
    SET
        total_scan_root_files = COALESCE(total_scan_root_files, 0)
            - COALESCE(OLD.file_count, 0) + COALESCE(NEW.file_count, 0),
        total_scan_root_bytes = COALESCE(total_scan_root_bytes, 0)
            - COALESCE(OLD.byte_count, 0) + COALESCE(NEW.byte_count, 0),
        scan_roots_updated_at = datetime('now')
    WHERE id = NEW.dataset_id;
END;
