-- Migration 0260: Add algorithm version fields to dataset_scan_roots
--
-- Purpose: Link scan roots to the algorithm versions used during scanning.
-- This enables cross-version replay and reproducibility verification.
--
-- Evidence: Determinism Policy - algorithm version tracking at scan root level
-- Pattern: Consistent with migration 0254 algorithm version fields on training_datasets

-- HKDF algorithm version used for seed derivation during this scan
ALTER TABLE dataset_scan_roots ADD COLUMN hkdf_algorithm_version INTEGER;

-- Parser algorithm version used for file parsing during this scan
ALTER TABLE dataset_scan_roots ADD COLUMN parser_algorithm_version INTEGER;

-- Path normalization version used for deterministic ordering
ALTER TABLE dataset_scan_roots ADD COLUMN path_normalization_version INTEGER;

-- Codegraph version used for code analysis (if applicable)
ALTER TABLE dataset_scan_roots ADD COLUMN codegraph_version TEXT;

-- =============================================================================
-- Indexes for algorithm version queries
-- =============================================================================

-- Index for finding scan roots with specific algorithm versions
CREATE INDEX IF NOT EXISTS idx_dsr_algo_versions
    ON dataset_scan_roots(
        hkdf_algorithm_version,
        parser_algorithm_version,
        path_normalization_version
    )
    WHERE hkdf_algorithm_version IS NOT NULL;

-- Index for codegraph version queries
CREATE INDEX IF NOT EXISTS idx_dsr_codegraph_version
    ON dataset_scan_roots(codegraph_version)
    WHERE codegraph_version IS NOT NULL;

-- =============================================================================
-- Backfill from metadata_json if algorithm versions were stored there
-- =============================================================================

-- Note: Algorithm versions are typically stored at the dataset level in migration 0254,
-- not at the scan root level in metadata_json. This backfill is a fallback for any
-- custom metadata that might have been stored.

UPDATE dataset_scan_roots
SET
    hkdf_algorithm_version = CAST(json_extract(metadata_json, '$.hkdf_version') AS INTEGER),
    parser_algorithm_version = CAST(json_extract(metadata_json, '$.parser_version') AS INTEGER),
    path_normalization_version = CAST(json_extract(metadata_json, '$.path_normalization_version') AS INTEGER),
    codegraph_version = json_extract(metadata_json, '$.codegraph_version')
WHERE metadata_json IS NOT NULL
  AND hkdf_algorithm_version IS NULL
  AND (json_extract(metadata_json, '$.hkdf_version') IS NOT NULL
    OR json_extract(metadata_json, '$.parser_version') IS NOT NULL
    OR json_extract(metadata_json, '$.path_normalization_version') IS NOT NULL
    OR json_extract(metadata_json, '$.codegraph_version') IS NOT NULL);
