-- Add retryable flag for failed jobs
-- Determined by error type: OOM/timeout = retryable, invalid config = not retryable
ALTER TABLE repository_training_jobs ADD COLUMN retryable INTEGER DEFAULT 0;

-- Add retry reference for explicit chain tracking
-- "This is the third retry of that cursed job" - audit/evidence stays coherent
ALTER TABLE repository_training_jobs ADD COLUMN retry_of_job_id TEXT;

-- Index for finding retryable jobs (filtered index for efficiency)
CREATE INDEX IF NOT EXISTS idx_training_jobs_retryable
    ON repository_training_jobs(retryable) WHERE status = 'failed';

-- Index for traversing retry chain
CREATE INDEX IF NOT EXISTS idx_training_jobs_retry_of
    ON repository_training_jobs(retry_of_job_id);
