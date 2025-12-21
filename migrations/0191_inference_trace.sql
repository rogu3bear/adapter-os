-- Per-token routing traces and verifiable run receipts.
CREATE TABLE inference_traces (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    request_id TEXT,
    context_digest BLOB NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_inference_traces_tenant_request
    ON inference_traces (tenant_id, request_id);

CREATE TABLE inference_trace_tokens (
    trace_id TEXT NOT NULL,
    token_index INTEGER NOT NULL,
    selected_adapter_ids BLOB NOT NULL,
    gates_q15 BLOB NOT NULL,
    decision_hash BLOB NOT NULL,
    policy_mask_digest BLOB,
    backend_id TEXT,
    kernel_version_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (trace_id, token_index),
    FOREIGN KEY (trace_id) REFERENCES inference_traces(id) ON DELETE CASCADE
);

CREATE INDEX idx_inference_trace_tokens_trace
    ON inference_trace_tokens (trace_id);

CREATE TABLE inference_trace_receipts (
    trace_id TEXT PRIMARY KEY,
    run_head_hash BLOB NOT NULL,
    output_digest BLOB NOT NULL,
    receipt_digest BLOB NOT NULL,
    signature BLOB,
    attestation BLOB,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (trace_id) REFERENCES inference_traces(id) ON DELETE CASCADE
);
