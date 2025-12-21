-- Add effective_adapter_ids_json to inference_replay_metadata
-- This captures the resolved adapter set after stack_id resolution
-- for deterministic replay and audit purposes.
ALTER TABLE inference_replay_metadata ADD COLUMN effective_adapter_ids_json TEXT;
