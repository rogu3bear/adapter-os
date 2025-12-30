-- Migration: Add missing metadata columns to repository_training_jobs for API contract alignment
-- Evidence: API type drift analysis - TrainingJobResponse declares fields with no DB columns
-- Pattern: Schema extension for API contract compliance
--
-- Note: category, description, language, symbol_targets_json, framework_id, framework_version,
-- api_patterns_json, repo_scope, file_patterns_json, exclude_patterns_json
-- were already added in migration 0117_training_job_category_metadata.sql

-- LoRA configuration fields (new)
ALTER TABLE repository_training_jobs ADD COLUMN lora_tier TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN lora_strength REAL;
ALTER TABLE repository_training_jobs ADD COLUMN scope TEXT;

-- Backend execution fields (new)
ALTER TABLE repository_training_jobs ADD COLUMN backend TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN backend_reason TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN backend_device TEXT;

-- Dataset hash for provenance (new)
ALTER TABLE repository_training_jobs ADD COLUMN dataset_hash_b3 TEXT;
