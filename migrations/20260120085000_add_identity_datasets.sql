-- Add identity configuration to dataset_versions for identity-based routing
-- Identity datasets store persona information and routing rules
ALTER TABLE training_dataset_versions
ADD COLUMN identity_config JSONB;
-- Index for querying identity datasets
CREATE INDEX idx_identity_datasets ON training_dataset_versions ((identity_config->>'type'))
WHERE identity_config IS NOT NULL;