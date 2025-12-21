-- Migration 0196: Add stop_policy_json to replay metadata
-- Stores StopPolicySpec for deterministic replay of stopping behavior
-- This ensures replayed inferences use the same stop conditions as the original

ALTER TABLE inference_replay_metadata
ADD COLUMN stop_policy_json TEXT;

-- Index not needed: stop_policy is only queried via primary key lookups
