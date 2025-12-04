-- Migration: Add replay_guarantee to inference_replay_metadata
-- Purpose: Store the computed replay guarantee level for determinism tracking

-- Add replay_guarantee to store the computed guarantee level
-- Valid values: 'exact', 'approximate', 'none'
-- - 'exact': Same backend, strict mode, all params match, no fallback
-- - 'approximate': Backend may differ, besteffort mode, or truncation
-- - 'none': Relaxed mode or incompatible configurations
ALTER TABLE inference_replay_metadata ADD COLUMN replay_guarantee TEXT DEFAULT 'none';

-- Index for querying replay metadata by guarantee level
-- Useful for finding inferences with exact replay capability
CREATE INDEX IF NOT EXISTS idx_replay_metadata_guarantee
    ON inference_replay_metadata(replay_guarantee)
    WHERE replay_guarantee IS NOT NULL;

-- Combined index for determinism analysis queries
-- Allows efficient filtering by mode + guarantee
CREATE INDEX IF NOT EXISTS idx_replay_metadata_determinism_analysis
    ON inference_replay_metadata(determinism_mode, replay_guarantee)
    WHERE determinism_mode IS NOT NULL;
