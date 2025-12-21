-- Capture trust_state snapshot at training time for adapter lineage.
-- Adds read-only mirror column; manifest remains source of historical facts.

ALTER TABLE adapter_version_dataset_versions
ADD COLUMN trust_at_training_time TEXT;

-- Backfill using current trust_state for existing links to avoid NULL drift.
UPDATE adapter_version_dataset_versions AS avdv
SET trust_at_training_time = (
    SELECT trust_state
    FROM training_dataset_versions tdv
    WHERE tdv.id = avdv.dataset_version_id
)
WHERE trust_at_training_time IS NULL;

CREATE INDEX IF NOT EXISTS idx_avdv_trust_at_training_time
    ON adapter_version_dataset_versions(trust_at_training_time);
