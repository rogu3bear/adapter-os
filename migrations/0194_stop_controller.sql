-- Stop Controller Fields (PRD: Hard Deterministic Stop Controller)
-- Adds stop reason tracking to inference receipts for audit and cost attribution.

-- Add stop_reason_code column: enumerated stop reason (LENGTH, BUDGET_MAX, COMPLETION_CONFIDENT, REPETITION_GUARD)
ALTER TABLE inference_trace_receipts
ADD COLUMN stop_reason_code TEXT;

-- Add stop_reason_token_index column: token index at which stop was triggered
ALTER TABLE inference_trace_receipts
ADD COLUMN stop_reason_token_index INTEGER;

-- Add stop_policy_digest_b3 column: BLAKE3 digest of StopPolicySpec for audit verification
ALTER TABLE inference_trace_receipts
ADD COLUMN stop_policy_digest_b3 BLOB;

-- Index for querying by stop reason (useful for analytics and debugging)
CREATE INDEX IF NOT EXISTS idx_inference_trace_receipts_stop_reason
    ON inference_trace_receipts (stop_reason_code)
    WHERE stop_reason_code IS NOT NULL;
