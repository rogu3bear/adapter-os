-- Add configuration identifiers to inference traces for observability.

ALTER TABLE inference_traces ADD COLUMN stack_id TEXT;
ALTER TABLE inference_traces ADD COLUMN model_id TEXT;
ALTER TABLE inference_traces ADD COLUMN policy_id TEXT;
