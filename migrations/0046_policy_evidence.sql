-- Policy evidence tracking table
-- Stores evidence records for model provenance, router decisions, and kernel audits

CREATE TABLE IF NOT EXISTS policy_evidence (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    timestamp INTEGER NOT NULL,  -- Nanoseconds since epoch
    model_id TEXT NOT NULL,
    model_path TEXT NOT NULL,
    model_hash TEXT NOT NULL,
    model_load_timestamp INTEGER NOT NULL,
    quantization_hash TEXT,
    active_loras_json TEXT NOT NULL,  -- JSON array of adapter IDs
    router_scores_q15_json TEXT NOT NULL,  -- JSON array of Q15 scores
    kernel_tolerance_json TEXT NOT NULL,  -- JSON array of kernel checks
    seed_hash TEXT NOT NULL,
    metadata_json TEXT NOT NULL,  -- JSON object
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_policy_evidence_tenant ON policy_evidence(tenant_id);
CREATE INDEX idx_policy_evidence_timestamp ON policy_evidence(timestamp);
CREATE INDEX idx_policy_evidence_model ON policy_evidence(model_id);
CREATE INDEX idx_policy_evidence_model_hash ON policy_evidence(model_hash);

