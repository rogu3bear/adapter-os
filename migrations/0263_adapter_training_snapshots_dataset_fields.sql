-- Migration 0263: Add dataset provenance columns to adapter_training_snapshots
--
-- Adds dataset identifiers and content hash for reproducibility and audit trails.

ALTER TABLE adapter_training_snapshots ADD COLUMN dataset_id TEXT;
ALTER TABLE adapter_training_snapshots ADD COLUMN dataset_version_id TEXT;
ALTER TABLE adapter_training_snapshots ADD COLUMN dataset_hash_b3 TEXT;

CREATE INDEX IF NOT EXISTS idx_adapter_training_snapshots_dataset_id
    ON adapter_training_snapshots(dataset_id)
    WHERE dataset_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_adapter_training_snapshots_dataset_version_id
    ON adapter_training_snapshots(dataset_version_id)
    WHERE dataset_version_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_adapter_training_snapshots_dataset_hash_b3
    ON adapter_training_snapshots(dataset_hash_b3)
    WHERE dataset_hash_b3 IS NOT NULL;
