-- Migration 0230: Workspace-scoped datasets with status + deterministic hash
-- Adds workspace_id, dataset_hash_b3, and lifecycle status to training_datasets.

ALTER TABLE training_datasets
    ADD COLUMN status TEXT NOT NULL DEFAULT 'uploaded'
        CHECK (status IN ('uploaded', 'processing', 'ready', 'failed'));

ALTER TABLE training_datasets
    ADD COLUMN workspace_id TEXT REFERENCES workspaces(id) ON DELETE SET NULL;

ALTER TABLE training_datasets
    ADD COLUMN dataset_hash_b3 TEXT;

-- Backfill dataset_hash_b3 and mark pre-existing rows as ready
UPDATE training_datasets
SET dataset_hash_b3 = COALESCE(dataset_hash_b3, hash_b3),
    status = 'ready';

CREATE INDEX IF NOT EXISTS idx_training_datasets_workspace_id
    ON training_datasets(workspace_id);

CREATE INDEX IF NOT EXISTS idx_training_datasets_status
    ON training_datasets(status);

CREATE INDEX IF NOT EXISTS idx_training_datasets_dataset_hash
    ON training_datasets(dataset_hash_b3);
