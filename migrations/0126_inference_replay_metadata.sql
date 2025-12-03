-- Migration 0126: Inference Replay Metadata (PRD-02)
-- Stores replay key and content for deterministic inference reproduction
-- One row per inference operation (1:1 relationship with inference_id)

CREATE TABLE IF NOT EXISTS inference_replay_metadata (
    id TEXT PRIMARY KEY,
    inference_id TEXT NOT NULL UNIQUE,
    tenant_id TEXT NOT NULL,

    -- Replay Key fields (required for exact reproduction)
    manifest_hash TEXT NOT NULL,
    router_seed TEXT,
    sampling_params_json TEXT NOT NULL,  -- {temperature, top_k, top_p, max_tokens, seed}
    backend TEXT NOT NULL,               -- CoreML, MLX, Metal
    sampling_algorithm_version TEXT NOT NULL DEFAULT 'v1.0.0',
    rag_snapshot_hash TEXT,              -- BLAKE3 of sorted doc hashes (null if no RAG)
    adapter_ids_json TEXT,               -- ["adapter-1", "adapter-2"] or null

    -- Stored content (64KB limit with truncation flags)
    prompt_text TEXT NOT NULL,
    prompt_truncated INTEGER NOT NULL DEFAULT 0,
    response_text TEXT,
    response_truncated INTEGER NOT NULL DEFAULT 0,

    -- RAG tracking for degraded replay
    rag_doc_ids_json TEXT,               -- Original doc IDs used for RAG retrieval

    -- Status and metrics
    replay_status TEXT NOT NULL DEFAULT 'available',  -- available, approximate, degraded, unavailable
    latency_ms INTEGER,
    tokens_generated INTEGER,

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (tenant_id) REFERENCES tenants(id)
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_replay_metadata_inference ON inference_replay_metadata(inference_id);
CREATE INDEX IF NOT EXISTS idx_replay_metadata_tenant ON inference_replay_metadata(tenant_id);
CREATE INDEX IF NOT EXISTS idx_replay_metadata_manifest ON inference_replay_metadata(manifest_hash);
CREATE INDEX IF NOT EXISTS idx_replay_metadata_status ON inference_replay_metadata(replay_status);
CREATE INDEX IF NOT EXISTS idx_replay_metadata_created ON inference_replay_metadata(created_at DESC);
