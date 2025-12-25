-- Migration 0227: Add policy_mask_digest to inference_replay_metadata
-- Stores the policy mask digest for deterministic replay audit trail
-- This is important for verifying that policy enforcement state is preserved during replay

ALTER TABLE inference_replay_metadata ADD COLUMN policy_mask_digest_b3 TEXT;
