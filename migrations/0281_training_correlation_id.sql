-- Migration 0279: Add correlation_id for dataset/training job traceability

ALTER TABLE training_datasets ADD COLUMN correlation_id TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN correlation_id TEXT;
