-- Migration 0127: Replay Executions Audit Trail (PRD-02)
-- Records each replay attempt with match analysis and divergence details
-- Multiple executions can exist per original inference

CREATE TABLE IF NOT EXISTS replay_executions (
    id TEXT PRIMARY KEY,
    original_inference_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    replay_mode TEXT NOT NULL,           -- exact, approximate, degraded

    -- Replay inputs (snapshot at execution time)
    prompt_text TEXT NOT NULL,
    sampling_params_json TEXT NOT NULL,
    backend TEXT NOT NULL,
    manifest_hash TEXT NOT NULL,
    router_seed TEXT,
    adapter_ids_json TEXT,

    -- Results
    response_text TEXT,
    response_truncated INTEGER NOT NULL DEFAULT 0,
    tokens_generated INTEGER,
    latency_ms INTEGER,

    -- Match analysis
    match_status TEXT NOT NULL,          -- exact, semantic, divergent, error
    divergence_details_json TEXT,        -- {position, backend_changed, manifest_changed, reasons}
    rag_reproducibility_score REAL,      -- 0.0-1.0 (null if no RAG)
    missing_doc_ids_json TEXT,           -- Docs that were unavailable during replay

    -- Metadata
    executed_at TEXT NOT NULL DEFAULT (datetime('now')),
    executed_by TEXT,                    -- User ID who triggered replay
    trigger_type TEXT NOT NULL DEFAULT 'user',  -- user, token, system
    error_message TEXT,                  -- Error details if match_status = 'error'

    FOREIGN KEY (original_inference_id) REFERENCES inference_replay_metadata(inference_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id)
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_replay_exec_inference ON replay_executions(original_inference_id);
CREATE INDEX IF NOT EXISTS idx_replay_exec_tenant ON replay_executions(tenant_id);
CREATE INDEX IF NOT EXISTS idx_replay_exec_status ON replay_executions(match_status);
CREATE INDEX IF NOT EXISTS idx_replay_exec_executed ON replay_executions(executed_at DESC);
