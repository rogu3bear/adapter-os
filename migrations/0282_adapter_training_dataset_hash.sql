-- Migration 0282: Add training_dataset_hash_b3 to adapters table
--
-- Purpose: Enable fast lookup of training dataset hash during inference
-- for cryptographic binding in execution receipts.
--
-- Evidence: Patent rectification - receipts must prove
-- "this output came from adapter trained on dataset X"
--
-- Pattern: Denormalized hash for inference-time performance

-- Add dedicated training_dataset_hash_b3 column to adapters table
ALTER TABLE adapters ADD COLUMN training_dataset_hash_b3 TEXT;

-- Backfill from adapter_training_lineage where available
-- Uses the primary role dataset hash for single-dataset training scenarios
UPDATE adapters SET training_dataset_hash_b3 = (
    SELECT atl.dataset_hash_b3_at_training
    FROM adapter_training_lineage atl
    WHERE atl.adapter_id = adapters.id
      AND atl.dataset_hash_b3_at_training IS NOT NULL
      AND atl.role = 'primary'
    ORDER BY atl.ordinal ASC
    LIMIT 1
)
WHERE training_dataset_hash_b3 IS NULL;

-- Create partial index for inference-time lookup
-- Only indexes rows where hash is present (space efficient)
CREATE INDEX IF NOT EXISTS idx_adapters_training_dataset_hash
    ON adapters(training_dataset_hash_b3)
    WHERE training_dataset_hash_b3 IS NOT NULL;
