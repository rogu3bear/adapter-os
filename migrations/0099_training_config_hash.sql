-- Training Configuration Hash for Reproducibility
-- Migration: 0099
-- Purpose: Add Blake3 hash of training configuration for deterministic reproducibility tracking
--
-- Enables:
-- - Detecting identical training configurations across jobs
-- - Verifying reproducibility of training runs
-- - Auditing training parameter changes
--
-- The config_hash_b3 column stores a BLAKE3 hash of normalized training hyperparameters
-- (rank, alpha, learning_rate, batch_size, epochs, hidden_dim) to enable:
-- 1. Fast lookup of training jobs with identical configurations
-- 2. Reproducibility verification across training runs
-- 3. Configuration drift detection

-- Add config_hash_b3 column to repository_training_jobs
ALTER TABLE repository_training_jobs ADD COLUMN config_hash_b3 TEXT;

-- Create index for config hash lookups (find jobs with same config)
CREATE INDEX IF NOT EXISTS idx_training_jobs_config_hash ON repository_training_jobs(config_hash_b3);

-- Create composite index for efficient reproducibility queries
CREATE INDEX IF NOT EXISTS idx_training_jobs_config_status ON repository_training_jobs(config_hash_b3, status);
