-- Token accounting fields on receipts (logical vs billed, cache reuse).
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS logical_prompt_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS prefix_cached_token_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS billed_input_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS logical_output_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS billed_output_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS signature BYTEA;
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS attestation BYTEA;

-- Backfill billed_input_tokens using the invariant where data is present.
UPDATE inference_trace_receipts
SET billed_input_tokens = GREATEST(logical_prompt_tokens - prefix_cached_token_count, 0),
    billed_output_tokens = CASE
        WHEN billed_output_tokens = 0 THEN logical_output_tokens
        ELSE billed_output_tokens
    END;
