-- Add receipt batch tables for Patent 3535886.0002 Claim 12 (Batch Verification).
-- These tables enable efficient batch verification of multiple inference receipts
-- via Merkle tree aggregation.
--
-- receipt_batches: Stores batch metadata and Merkle roots
-- receipt_batch_members: Maps receipts to batches with position information

-- Receipt batch aggregation table
CREATE TABLE IF NOT EXISTS receipt_batches (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    merkle_root BLOB NOT NULL,
    receipt_count INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Index for tenant-based queries (list batches for a tenant)
CREATE INDEX IF NOT EXISTS idx_receipt_batches_tenant
    ON receipt_batches(tenant_id, created_at DESC);

-- Index for Merkle root lookups (verify batch authenticity)
CREATE INDEX IF NOT EXISTS idx_receipt_batches_merkle_root
    ON receipt_batches(merkle_root);

-- Receipt-to-batch membership table
CREATE TABLE IF NOT EXISTS receipt_batch_members (
    batch_id TEXT NOT NULL,
    trace_id TEXT NOT NULL,
    receipt_digest BLOB NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (batch_id, trace_id),
    FOREIGN KEY (batch_id) REFERENCES receipt_batches(id) ON DELETE CASCADE
);

-- Index for looking up which batch contains a receipt
CREATE INDEX IF NOT EXISTS idx_receipt_batch_members_trace
    ON receipt_batch_members(trace_id);

-- Index for looking up batches by receipt digest
CREATE INDEX IF NOT EXISTS idx_receipt_batch_members_digest
    ON receipt_batch_members(receipt_digest);
