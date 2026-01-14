-- Cryptographic Receipt Integration: Input Digest (Patent 3535886.0002 Compliance)
--
-- Adds input_digest_b3 field to inference_trace_receipts for cryptographic
-- binding of input token sequences to execution receipts.
-- This enables third-party verification that a specific output was produced
-- from a specific input under specific conditions.

-- Input token sequence digest (BLAKE3 of input token IDs)
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS input_digest_b3 BYTEA;

-- Index for verification queries
CREATE INDEX IF NOT EXISTS idx_receipts_input_digest
    ON inference_trace_receipts(input_digest_b3)
    WHERE input_digest_b3 IS NOT NULL;
