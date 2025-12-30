-- Rollback Migration 0260: Remove algorithm version fields from dataset_scan_roots

-- Drop indexes
DROP INDEX IF EXISTS idx_dsr_codegraph_version;
DROP INDEX IF EXISTS idx_dsr_algo_versions;

-- Drop columns
ALTER TABLE dataset_scan_roots DROP COLUMN codegraph_version;
ALTER TABLE dataset_scan_roots DROP COLUMN path_normalization_version;
ALTER TABLE dataset_scan_roots DROP COLUMN parser_algorithm_version;
ALTER TABLE dataset_scan_roots DROP COLUMN hkdf_algorithm_version;
