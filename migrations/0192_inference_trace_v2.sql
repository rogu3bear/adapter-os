-- Align inference trace tables with per-token evidence and receipts.
-- Adds status tracking, fusion metadata, and JSON storage for decisions.

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

-- Rebuild inference_trace_tokens with JSON columns, fusion metadata, and policy mask data
-- (merged from 0192_inference_trace_mask.sql)
CREATE TABLE inference_trace_tokens_new (
    trace_id TEXT NOT NULL,
    token_index INTEGER NOT NULL,
    selected_adapter_ids JSON NOT NULL,
    gates_q15 JSON NOT NULL,
    decision_hash BLOB NOT NULL,
    policy_mask_digest BLOB,
    allowed_mask BLOB,             -- Policy mask: which adapters were allowed
    policy_overrides_json TEXT,    -- JSON object of policy overrides applied
    backend_id TEXT,
    kernel_version_id TEXT,
    fusion_interval_id TEXT,
    fused_weight_hash BLOB,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (trace_id, token_index),
    FOREIGN KEY (trace_id) REFERENCES inference_traces(trace_id) ON DELETE CASCADE
);

-- Migrate existing token data (BLOB columns cast to JSON text)
INSERT INTO inference_trace_tokens_new (
    trace_id,
    token_index,
    selected_adapter_ids,
    gates_q15,
    decision_hash,
    policy_mask_digest,
    backend_id,
    kernel_version_id,
    created_at
)
SELECT
    trace_id,
    token_index,
    -- Convert BLOB to JSON (works if BLOB stored JSON text, else uses hex fallback)
    COALESCE(json(selected_adapter_ids), json_array(hex(selected_adapter_ids))),
    COALESCE(json(gates_q15), json_array(hex(gates_q15))),
    decision_hash,
    policy_mask_digest,
    backend_id,
    kernel_version_id,
    created_at
FROM inference_trace_tokens;

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
    logical_prompt_tokens INTEGER NOT NULL DEFAULT 0,
    prefix_cached_token_count INTEGER NOT NULL DEFAULT 0,
    billed_input_tokens INTEGER NOT NULL DEFAULT 0,
    logical_output_tokens INTEGER NOT NULL DEFAULT 0,
    billed_output_tokens INTEGER NOT NULL DEFAULT 0,
    signature BLOB,
    attestation BLOB,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (trace_id) REFERENCES inference_traces(trace_id) ON DELETE CASCADE
);

INSERT INTO inference_trace_receipts_new (
    trace_id,
    run_head_hash,
    output_digest,
    receipt_digest,
    logical_prompt_tokens,
    prefix_cached_token_count,
    billed_input_tokens,
    logical_output_tokens,
    billed_output_tokens,
    signature,
    attestation,
    created_at
)
SELECT
    trace_id,
    run_head_hash,
    output_digest,
    receipt_digest,
    0 AS logical_prompt_tokens,
    0 AS prefix_cached_token_count,
    0 AS billed_input_tokens,
    0 AS logical_output_tokens,
    0 AS billed_output_tokens,
    signature,
    attestation,
    created_at
FROM inference_trace_receipts;

DROP TABLE inference_trace_receipts;
ALTER TABLE inference_trace_receipts_new RENAME TO inference_trace_receipts;

