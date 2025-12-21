-- Add synthetic_mode and data_lineage_mode to training jobs for provenance

ALTER TABLE repository_training_jobs
ADD COLUMN synthetic_mode INTEGER NOT NULL DEFAULT 0;

ALTER TABLE repository_training_jobs
ADD COLUMN data_lineage_mode TEXT;

-- Backfill lineage mode for existing rows:
-- - synthetic when dataset_id is NULL
-- - dataset_only when dataset_id is present (no version info stored historically)
UPDATE repository_training_jobs
SET data_lineage_mode = CASE
    WHEN dataset_id IS NULL THEN 'synthetic'
    ELSE 'dataset_only'
END
WHERE data_lineage_mode IS NULL;
