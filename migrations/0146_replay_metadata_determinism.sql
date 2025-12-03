-- Migration: Add determinism tracking to replay metadata
-- Purpose: Track determinism mode and fallback status for replay verification
-- PRD: PRD-CORE-02 (BackendCoordinator integration & deterministic mode)

-- Add determinism_mode to track which mode was used for the inference
-- Valid values: 'strict', 'besteffort', 'relaxed'
ALTER TABLE inference_replay_metadata ADD COLUMN determinism_mode TEXT;

-- Add fallback_triggered to track whether backend fallback was used
-- 0 = no fallback, 1 = fallback was triggered
ALTER TABLE inference_replay_metadata ADD COLUMN fallback_triggered INTEGER DEFAULT 0;

-- Index for querying replay metadata by determinism mode
CREATE INDEX IF NOT EXISTS idx_replay_metadata_determinism_mode
    ON inference_replay_metadata(determinism_mode)
    WHERE determinism_mode IS NOT NULL;

-- Index for finding replays where fallback was triggered
CREATE INDEX IF NOT EXISTS idx_replay_metadata_fallback
    ON inference_replay_metadata(fallback_triggered)
    WHERE fallback_triggered = 1;
