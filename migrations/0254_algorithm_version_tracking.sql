-- Algorithm Version Tracking for Cross-Version Replay
--
-- Enables reproduction of training runs by recording which algorithm
-- versions were used to generate dataset hashes and seeds.
--
-- Evidence: Determinism Policy - cross-version replay support
-- Pattern: Explicit version columns for type safety and queryability
--
-- Version columns are nullable to support legacy data without version info.
-- NULL values are treated as version 1 (initial/legacy version).

-- =============================================================================
-- Add version columns to training jobs
-- =============================================================================

ALTER TABLE repository_training_jobs
    ADD COLUMN hkdf_algorithm_version INTEGER;

ALTER TABLE repository_training_jobs
    ADD COLUMN parser_algorithm_version INTEGER;

ALTER TABLE repository_training_jobs
    ADD COLUMN path_normalization_version INTEGER;

-- =============================================================================
-- Add version columns to dataset_hash_inputs
-- =============================================================================

ALTER TABLE dataset_hash_inputs
    ADD COLUMN hkdf_version INTEGER;

ALTER TABLE dataset_hash_inputs
    ADD COLUMN parser_version INTEGER;

ALTER TABLE dataset_hash_inputs
    ADD COLUMN path_normalization_version INTEGER;

-- =============================================================================
-- Add version columns to training_datasets
-- =============================================================================

ALTER TABLE training_datasets
    ADD COLUMN hkdf_algorithm_version INTEGER;

ALTER TABLE training_datasets
    ADD COLUMN parser_algorithm_version INTEGER;

ALTER TABLE training_datasets
    ADD COLUMN path_normalization_version INTEGER;

-- =============================================================================
-- Indexes for version mismatch detection queries
-- =============================================================================

-- Index for finding training jobs with specific algorithm versions
CREATE INDEX IF NOT EXISTS idx_training_jobs_algo_versions
    ON repository_training_jobs(hkdf_algorithm_version, parser_algorithm_version, path_normalization_version)
    WHERE hkdf_algorithm_version IS NOT NULL;

-- Index for finding datasets by algorithm version
CREATE INDEX IF NOT EXISTS idx_training_datasets_algo_versions
    ON training_datasets(hkdf_algorithm_version, parser_algorithm_version, path_normalization_version)
    WHERE hkdf_algorithm_version IS NOT NULL;

-- Index for dataset hash inputs version queries
CREATE INDEX IF NOT EXISTS idx_dataset_hash_inputs_versions
    ON dataset_hash_inputs(hkdf_version, parser_version, path_normalization_version)
    WHERE hkdf_version IS NOT NULL;
