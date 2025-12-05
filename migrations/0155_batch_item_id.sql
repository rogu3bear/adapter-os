-- Migration: Add batch_item_id to inference_replay_metadata
--
-- Purpose: Track which batch item this inference belongs to (for batch requests)
-- This enables filtering and tracking of batch inference replay metadata.

ALTER TABLE inference_replay_metadata
ADD COLUMN batch_item_id TEXT;

-- Index for batch item queries
CREATE INDEX IF NOT EXISTS idx_replay_metadata_batch_item
ON inference_replay_metadata(batch_item_id)
WHERE batch_item_id IS NOT NULL;
