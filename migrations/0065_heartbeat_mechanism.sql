-- Heartbeat mechanism for lifecycle management
-- Adds last_heartbeat tracking to critical tables

-- Add heartbeat columns to adapters table
ALTER TABLE adapters ADD COLUMN last_heartbeat INTEGER;

-- Add heartbeat columns to workers table (if exists)
-- Note: workers table may not exist in all deployments
-- ALTER TABLE workers ADD COLUMN last_heartbeat INTEGER;

-- Note: training_jobs table doesn't exist in base schema (only repository_training_jobs)
-- Commenting out training_jobs references until table is created
-- ALTER TABLE training_jobs ADD COLUMN last_heartbeat INTEGER;

-- Create index for heartbeat-based queries
CREATE INDEX IF NOT EXISTS idx_adapters_heartbeat ON adapters(last_heartbeat) WHERE last_heartbeat IS NOT NULL;
-- CREATE INDEX IF NOT EXISTS idx_training_jobs_heartbeat ON training_jobs(last_heartbeat) WHERE last_heartbeat IS NOT NULL;

-- View for stale adapters (no heartbeat in 5 minutes)
CREATE VIEW IF NOT EXISTS stale_adapters AS
SELECT
    id,
    name,
    load_state,
    last_heartbeat,
    (strftime('%s', 'now') - last_heartbeat) AS seconds_since_heartbeat
FROM adapters
WHERE last_heartbeat IS NOT NULL
  AND (strftime('%s', 'now') - last_heartbeat) > 300;

-- View for stale training jobs commented out (training_jobs table doesn't exist)
-- CREATE VIEW IF NOT EXISTS stale_training_jobs AS
-- SELECT
--     id,
--     status,
--     last_heartbeat,
--     (strftime('%s', 'now') - last_heartbeat) AS seconds_since_heartbeat
-- FROM training_jobs
-- WHERE last_heartbeat IS NOT NULL
--   AND status IN ('pending', 'running')
--   AND (strftime('%s', 'now') - last_heartbeat) > 300;
