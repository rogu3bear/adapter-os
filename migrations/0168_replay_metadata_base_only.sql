-- Migration: Add base_only flag to inference_replay_metadata
-- Captures whether an inference ran in base-only mode (no adapters)

ALTER TABLE inference_replay_metadata
    ADD COLUMN base_only INTEGER;

