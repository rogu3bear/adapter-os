-- Add step/epoch tracking to metrics
-- "loss at epoch 3, step 1200" not "whichever timestamp that might be"
ALTER TABLE repository_training_metrics ADD COLUMN step INTEGER DEFAULT 0;
ALTER TABLE repository_training_metrics ADD COLUMN epoch INTEGER;  -- nullable for non-epoch metrics

-- Add composite index for efficient time-series queries
CREATE INDEX IF NOT EXISTS idx_training_metrics_job_step
    ON repository_training_metrics(training_job_id, step);
CREATE INDEX IF NOT EXISTS idx_training_metrics_job_epoch
    ON repository_training_metrics(training_job_id, epoch);

-- Composite index for typical query pattern: get all metrics for job ordered by step
CREATE INDEX IF NOT EXISTS idx_training_metrics_job_step_name
    ON repository_training_metrics(training_job_id, step, metric_name);
