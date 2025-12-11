-- Align inference trace tables with per-token evidence and receipts.
-- Adds status tracking, fusion metadata, and JSON storage for decisions.

BEGIN;

-- Rebuild inference_traces with explicit status and trace_id PK
CREATE TABLE inference_traces_new (
    trace_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    request_id TEXT,
    context_digest BLOB NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    status TEXT NOT NULL DEFAULT 'running',
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

INSERT INTO inference_traces_new (
    trace_id,
    tenant_id,
    request_id,
    context_digest,
    created_at,
    status
)
SELECT
    id AS trace_id,
    tenant_id,
    request_id,
    context_digest,
    created_at,
    COALESCE(
        (
            SELECT 'completed'
            FROM inference_trace_receipts r
            WHERE r.trace_id = t.id
            LIMIT 1
        ),
        'running'
    ) AS status
FROM inference_traces t;

DROP TABLE inference_traces;
ALTER TABLE inference_traces_new RENAME TO inference_traces;

CREATE INDEX idx_inference_traces_tenant_request
    ON inference_traces (tenant_id, request_id);

-- Rebuild inference_trace_tokens with JSON columns and fusion metadata
CREATE TABLE inference_trace_tokens_new (
    trace_id TEXT NOT NULL,
    token_index INTEGER NOT NULL,
    selected_adapter_ids JSON NOT NULL,
    gates_q15 JSON NOT NULL,
    decision_hash BLOB NOT NULL,
    policy_mask_digest BLOB,
    backend_id TEXT,
    kernel_version_id TEXT,
    fusion_interval_id TEXT,
    fused_weight_hash BLOB,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (trace_id, token_index),
    FOREIGN KEY (trace_id) REFERENCES inference_traces(trace_id) ON DELETE CASCADE
);

DROP TABLE IF EXISTS inference_trace_tokens;
ALTER TABLE inference_trace_tokens_new RENAME TO inference_trace_tokens;

CREATE INDEX idx_inference_trace_tokens_trace
    ON inference_trace_tokens (trace_id);

-- Rebuild inference_trace_receipts with aligned columns
CREATE TABLE inference_trace_receipts_new (
    trace_id TEXT PRIMARY KEY,
    run_head_hash BLOB NOT NULL,
    output_digest BLOB NOT NULL,
    receipt_digest BLOB NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (trace_id) REFERENCES inference_traces(trace_id) ON DELETE CASCADE
);

INSERT INTO inference_trace_receipts_new (
    trace_id,
    run_head_hash,
    output_digest,
    receipt_digest,
    created_at
)
SELECT
    trace_id,
    run_head_hash,
    output_digest,
    receipt_digest,
    created_at
FROM inference_trace_receipts;

DROP TABLE inference_trace_receipts;
ALTER TABLE inference_trace_receipts_new RENAME TO inference_trace_receipts;

COMMIT;
