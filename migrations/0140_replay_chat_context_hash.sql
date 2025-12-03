-- Migration 0140: Add chat_context_hash to inference_replay_metadata
-- Stores BLAKE3 hash of sorted message IDs for multi-turn context verification
-- Enables deterministic replay verification for chat sessions

ALTER TABLE inference_replay_metadata
ADD COLUMN chat_context_hash TEXT;

-- Index for queries filtering by chat context
CREATE INDEX IF NOT EXISTS idx_replay_metadata_chat_context
ON inference_replay_metadata(chat_context_hash)
WHERE chat_context_hash IS NOT NULL;
