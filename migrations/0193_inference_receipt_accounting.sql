-- Token accounting fields on receipts (logical vs billed, cache reuse).
-- NOTE: These columns are now created by migration 0192_inference_trace_v2.sql
-- when it rebuilds the inference_trace_receipts table. This migration is
-- kept for backward compatibility with databases that applied 0193 before 0192
-- was updated to include these columns.

-- SQLite doesn't support "IF NOT EXISTS" for ALTER TABLE, and these columns
-- already exist in the new schema from 0192. This migration is now a no-op.

-- Historical note: Originally this migration added:
--   logical_prompt_tokens, prefix_cached_token_count, billed_input_tokens,
--   logical_output_tokens, billed_output_tokens, signature, attestation
-- to inference_trace_receipts. Now included in 0192.





