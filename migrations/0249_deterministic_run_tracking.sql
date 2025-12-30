-- Deterministic run tracking for training jobs
--
-- Adds columns to track determinism configuration for reproducible training runs.
-- - is_deterministic_run: Whether the job explicitly requested deterministic execution
-- - global_seed_hex: BLAKE3 hash of the global seed used for all RNG sources
-- - determinism_config_json: Full determinism configuration snapshot (seed sources, overrides, etc.)
-- - seed_mode: Seed derivation strategy (best_effort, strict, disabled)
--
-- Evidence: Determinism Policy - reproducibility tracking
-- Pattern: Training job provenance extension

ALTER TABLE repository_training_jobs ADD COLUMN is_deterministic_run INTEGER DEFAULT 0;
ALTER TABLE repository_training_jobs ADD COLUMN global_seed_hex TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN determinism_config_json TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN seed_mode TEXT DEFAULT 'best_effort';

-- Index for querying deterministic runs by dataset
-- Supports get_deterministic_runs_for_dataset queries
CREATE INDEX IF NOT EXISTS idx_training_jobs_deterministic_dataset
    ON repository_training_jobs(dataset_id, is_deterministic_run)
    WHERE is_deterministic_run = 1;
