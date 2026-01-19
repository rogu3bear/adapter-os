-- Cancellation Receipt Generation for Audit Trail Completeness
--
-- When inference is cancelled (client disconnect, timeout, policy violation),
-- the worker generates a cryptographic receipt capturing:
-- - Partial output digest (BLAKE3 hash of tokens generated)
-- - Cancellation source and metadata
-- - Signed receipt for verification
--
-- This ensures audit trail completeness for partial outputs.

CREATE TABLE IF NOT EXISTS cancellation_receipts (
    -- Primary key: unique identifier for this receipt
    id TEXT PRIMARY KEY NOT NULL,

    -- Trace correlation: links to inference_traces if the trace was started
    trace_id TEXT NOT NULL,

    -- Cryptographic fields
    partial_output_digest BLOB NOT NULL,           -- BLAKE3 hash of partial output tokens
    partial_output_count INTEGER NOT NULL,         -- Number of tokens before cancellation
    stop_reason TEXT NOT NULL DEFAULT 'CANCELLED', -- Always CANCELLED for this table
    cancellation_source TEXT NOT NULL,             -- CLIENT_DISCONNECT, REQUEST_TIMEOUT, etc.
    cancelled_at_token INTEGER NOT NULL,           -- Token index at cancellation
    receipt_digest BLOB NOT NULL,                  -- Final BLAKE3 receipt digest
    signature BLOB,                                -- Ed25519 signature (NULL in dev mode)

    -- Context fields for verification
    equipment_profile_digest BLOB,                 -- Equipment profile digest
    context_digest BLOB,                           -- Model + adapter config digest

    -- Multi-tenant isolation
    tenant_id TEXT,

    -- Timestamps
    cancelled_at TEXT,                             -- ISO 8601 timestamp when cancelled
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),

    -- Foreign key constraint (trace may not exist if cancelled very early)
    -- Note: We don't enforce FK since trace may not have been created
    CONSTRAINT cancellation_receipts_trace_id_unique UNIQUE (trace_id)
);

-- Index for querying by trace_id (most common lookup pattern)
CREATE INDEX IF NOT EXISTS idx_cancellation_receipts_trace_id
    ON cancellation_receipts(trace_id);

-- Index for tenant-scoped queries
CREATE INDEX IF NOT EXISTS idx_cancellation_receipts_tenant_id
    ON cancellation_receipts(tenant_id)
    WHERE tenant_id IS NOT NULL;

-- Index for querying by cancellation source (operational metrics)
CREATE INDEX IF NOT EXISTS idx_cancellation_receipts_source
    ON cancellation_receipts(cancellation_source);

-- Index for time-range queries (audit trail queries)
CREATE INDEX IF NOT EXISTS idx_cancellation_receipts_created_at
    ON cancellation_receipts(created_at);

-- Composite index for tenant + time queries
CREATE INDEX IF NOT EXISTS idx_cancellation_receipts_tenant_time
    ON cancellation_receipts(tenant_id, created_at)
    WHERE tenant_id IS NOT NULL;
