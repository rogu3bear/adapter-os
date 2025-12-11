-- Add routing-specific determinism mode for stacks (deterministic/adaptive)
ALTER TABLE adapter_stacks ADD COLUMN routing_determinism_mode TEXT;

