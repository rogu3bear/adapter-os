-- Extend repository_training_jobs table with additional fields for TrainingService integration
-- Adds adapter_name, template_id, created_at, and metadata_json for artifact tracking

-- Add adapter_name column
ALTER TABLE repository_training_jobs ADD COLUMN adapter_name TEXT;

-- Add template_id column
ALTER TABLE repository_training_jobs ADD COLUMN template_id TEXT;

-- Add created_at column (currently only has started_at)
ALTER TABLE repository_training_jobs ADD COLUMN created_at TEXT DEFAULT (datetime('now'));

-- Backfill created_at for existing rows (use started_at if available, otherwise current time)
UPDATE repository_training_jobs 
SET created_at = COALESCE(started_at, datetime('now'))
WHERE created_at IS NULL;

-- Add metadata_json column for artifact_path, adapter_id, weights_hash_b3
ALTER TABLE repository_training_jobs ADD COLUMN metadata_json TEXT;

-- Create index for adapter_name lookups
CREATE INDEX IF NOT EXISTS idx_training_jobs_adapter_name ON repository_training_jobs(adapter_name);

-- Create index for template_id lookups
CREATE INDEX IF NOT EXISTS idx_training_jobs_template_id ON repository_training_jobs(template_id);

-- Create index for created_at sorting
CREATE INDEX IF NOT EXISTS idx_training_jobs_created_at ON repository_training_jobs(created_at DESC);

