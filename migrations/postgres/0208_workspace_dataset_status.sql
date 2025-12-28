-- Add workspace scoping + dataset status fields

ALTER TABLE training_datasets
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'uploaded'
        CHECK (status IN ('uploaded', 'processing', 'ready', 'failed'));

ALTER TABLE training_datasets
    ADD COLUMN IF NOT EXISTS workspace_id TEXT REFERENCES workspaces(id) ON DELETE SET NULL;

ALTER TABLE training_datasets
    ADD COLUMN IF NOT EXISTS dataset_hash_b3 TEXT;

UPDATE training_datasets
SET dataset_hash_b3 = COALESCE(dataset_hash_b3, hash_b3),
    status = 'ready'
WHERE dataset_hash_b3 IS NULL
   OR status IS NULL;

CREATE INDEX IF NOT EXISTS idx_training_datasets_workspace_id
    ON training_datasets(workspace_id);

CREATE INDEX IF NOT EXISTS idx_training_datasets_status
    ON training_datasets(status);

CREATE INDEX IF NOT EXISTS idx_training_datasets_dataset_hash
    ON training_datasets(dataset_hash_b3);
