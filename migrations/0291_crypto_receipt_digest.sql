-- Phase 3: Crypto Receipt Digest Storage (Patent 3535886.0002 Compliance)
--
-- Adds crypto_receipt_digest_b3 column to store ReceiptGenerator output
-- alongside the legacy SqlTraceSink receipt_digest for parallel validation
-- and eventual migration.

-- Crypto receipt digest (BLAKE3 from ReceiptGenerator)
-- This enables dual-write during transition and offline verification
ALTER TABLE inference_trace_receipts
    ADD COLUMN crypto_receipt_digest_b3 BLOB;

-- Flag indicating parity between legacy and crypto receipt digests
-- NULL = not checked, 1 = match, 0 = mismatch
ALTER TABLE inference_trace_receipts
    ADD COLUMN receipt_parity_verified INTEGER;

-- Index for verification queries
CREATE INDEX IF NOT EXISTS idx_receipts_crypto_digest
    ON inference_trace_receipts(crypto_receipt_digest_b3)
    WHERE crypto_receipt_digest_b3 IS NOT NULL;

-- Index for finding receipts needing parity verification
CREATE INDEX IF NOT EXISTS idx_receipts_parity_unverified
    ON inference_trace_receipts(receipt_parity_verified)
    WHERE receipt_parity_verified IS NULL;
