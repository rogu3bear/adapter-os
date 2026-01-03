-- Add utf8_healing column to inference_replay_metadata for deterministic replay
-- This field is required for replay since UTF-8 token healing affects output bytes
ALTER TABLE inference_replay_metadata ADD COLUMN utf8_healing INTEGER DEFAULT NULL;

-- Index for filtering replays by healing mode
CREATE INDEX IF NOT EXISTS idx_inference_replay_metadata_utf8_healing
    ON inference_replay_metadata(utf8_healing) WHERE utf8_healing IS NOT NULL;
