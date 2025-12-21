-- Migration 0118: Denormalize training job artifact metadata
-- Purpose: Enable direct column access for artifact_path, adapter_id, weights_hash_b3
-- Evidence: Making database the single source of truth for training jobs

-- Add denormalized artifact columns (currently in metadata_json)
ALTER TABLE repository_training_jobs ADD COLUMN adapter_id TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN weights_hash_b3 TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN artifact_path TEXT;

-- Add stack_id for auto-created stack reference
ALTER TABLE repository_training_jobs ADD COLUMN stack_id TEXT
    REFERENCES adapter_stacks(id) ON DELETE SET NULL;

-- Add initiated_by_role for audit trail
ALTER TABLE repository_training_jobs ADD COLUMN initiated_by_role TEXT;

-- Create index for adapter lookups
CREATE INDEX IF NOT EXISTS idx_training_jobs_adapter_id
    ON repository_training_jobs(adapter_id);

-- Backfill existing records from metadata_json
UPDATE repository_training_jobs
SET adapter_id = json_extract(metadata_json, '$.adapter_id'),
    weights_hash_b3 = json_extract(metadata_json, '$.weights_hash_b3'),
    artifact_path = json_extract(metadata_json, '$.artifact_path')
WHERE metadata_json IS NOT NULL;
