-- Add compile flags hash tracking for deterministic replay verification
-- This enables detection of toolchain drift between original inference and replay

ALTER TABLE inference_replay_metadata
ADD COLUMN backend_compile_flags_hash TEXT;

-- Index for efficient verification queries
CREATE INDEX IF NOT EXISTS idx_replay_metadata_compile_flags_hash
ON inference_replay_metadata(backend_compile_flags_hash);
