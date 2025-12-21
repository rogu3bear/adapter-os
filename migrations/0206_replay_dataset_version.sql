-- Add dataset_version_id to inference_replay_metadata for explicit dataset pinning
-- This enables deterministic replay by pinning to a specific dataset version
-- alongside the existing rag_snapshot_hash

ALTER TABLE inference_replay_metadata
ADD COLUMN dataset_version_id TEXT;

-- Index for efficient lookups by dataset version
CREATE INDEX IF NOT EXISTS idx_replay_meta_dataset_version
ON inference_replay_metadata(dataset_version_id);
