-- Add cross-run lineage columns for Patent 3535886.0002 Claims 7-8 compliance.
-- These fields enable temporal ordering verification of inference runs within a session.
--
-- previous_receipt_digest: Links to the prior receipt in the same tenant/session chain.
--                         NULL for first receipt in a session.
-- session_sequence: Monotonically increasing counter within a session, starting at 0.
--
-- Together with the evidence envelope chain, these fields provide cryptographic proof
-- of inference ordering without requiring access to the full trace.

ALTER TABLE inference_trace_receipts ADD COLUMN previous_receipt_digest BLOB;
ALTER TABLE inference_trace_receipts ADD COLUMN session_sequence INTEGER DEFAULT 0;

-- Index for efficient session chain traversal
CREATE INDEX IF NOT EXISTS idx_receipt_session_chain
    ON inference_trace_receipts(tenant_id, session_sequence);

-- Index for chain verification queries
CREATE INDEX IF NOT EXISTS idx_receipt_previous_digest
    ON inference_trace_receipts(previous_receipt_digest)
    WHERE previous_receipt_digest IS NOT NULL;
