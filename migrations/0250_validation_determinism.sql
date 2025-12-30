-- Add determinism recording columns to dataset version validations.
-- Supports tracking validation seed derivation for reproducibility audits.

ALTER TABLE dataset_version_validations ADD COLUMN validation_seed_hex TEXT;
ALTER TABLE dataset_version_validations ADD COLUMN determinism_mode TEXT;
ALTER TABLE dataset_version_validations ADD COLUMN validation_hash_b3 TEXT;
ALTER TABLE dataset_version_validations ADD COLUMN is_deterministic INTEGER DEFAULT 0;

-- Index for filtering deterministic validation runs
CREATE INDEX idx_dvv_determinism ON dataset_version_validations(is_deterministic) WHERE is_deterministic = 1;
