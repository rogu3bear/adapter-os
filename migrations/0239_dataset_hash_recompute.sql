-- Migration 0239: Mark datasets for hash recomputation
-- Fixes bug from migration 0230 where dataset_hash_b3 was incorrectly backfilled
-- with single-file hash_b3 instead of proper manifest hash.
--
-- The actual recomputation happens in application code at server boot using BLAKE3
-- with the new algorithm that normalizes filenames (lowercase + NFD + trim).

-- Flag to mark datasets needing hash recomputation
ALTER TABLE training_datasets
ADD COLUMN hash_needs_recompute INTEGER NOT NULL DEFAULT 0;

-- Mark ALL existing datasets for recomputation (switching to v2 algorithm with normalization)
UPDATE training_datasets SET hash_needs_recompute = 1;

-- Track hash algorithm version for future-proofing
-- v1 = original (buggy backfill from 0230)
-- v2 = normalized filenames (lowercase + NFD + trim)
ALTER TABLE training_datasets
ADD COLUMN hash_algorithm_version INTEGER NOT NULL DEFAULT 1;

CREATE INDEX IF NOT EXISTS idx_training_datasets_hash_recompute
    ON training_datasets(hash_needs_recompute)
    WHERE hash_needs_recompute = 1;
