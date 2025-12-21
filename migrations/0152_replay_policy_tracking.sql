-- Replay Metadata Policy Tracking
-- Add execution policy ID and version to replay metadata for audit trail

-- Add execution policy tracking columns
ALTER TABLE inference_replay_metadata ADD COLUMN execution_policy_id TEXT;
ALTER TABLE inference_replay_metadata ADD COLUMN execution_policy_version INTEGER;

-- Index for efficient lookup by policy
CREATE INDEX IF NOT EXISTS idx_replay_metadata_policy
    ON inference_replay_metadata(execution_policy_id)
    WHERE execution_policy_id IS NOT NULL;
