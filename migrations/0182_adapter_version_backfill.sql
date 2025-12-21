-- Backfill adapter version metadata defaults (no-op for dataset links due to lack of historical dataset_version_ids).

PRAGMA foreign_keys = ON;

-- Ensure training_backend/coreml flags are populated for legacy rows.
UPDATE adapter_versions
SET training_backend = COALESCE(training_backend, 'unknown'),
    coreml_used = COALESCE(coreml_used, 0)
WHERE training_backend IS NULL OR coreml_used IS NULL;
