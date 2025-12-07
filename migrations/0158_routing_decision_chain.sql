-- Routing decision chain table for per-token cryptographic audit
CREATE TABLE routing_decision_chain (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    inference_id TEXT NOT NULL,
    request_id TEXT,
    step INTEGER NOT NULL,
    input_token_id INTEGER,
    adapter_indices TEXT NOT NULL,
    adapter_ids TEXT NOT NULL,
    gates_q15 TEXT NOT NULL,
    entropy REAL NOT NULL,
    decision_hash_json TEXT,
    previous_hash TEXT,
    entry_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_routing_decision_chain_tenant_inference_step
    ON routing_decision_chain (tenant_id, inference_id, step);

CREATE INDEX idx_routing_decision_chain_entry_hash
    ON routing_decision_chain (entry_hash);

