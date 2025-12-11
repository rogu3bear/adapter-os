-- CoreML fusion pairing metadata for model+adapter fused packages

CREATE TABLE IF NOT EXISTS coreml_fusion_pairs (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    base_model_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    fused_manifest_hash TEXT NOT NULL,
    coreml_package_hash TEXT NOT NULL,
    adapter_hash_b3 TEXT,
    base_model_hash_b3 TEXT,
    metadata_path TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS coreml_fusion_pairs_unique
    ON coreml_fusion_pairs(tenant_id, base_model_id, adapter_id, coreml_package_hash);

-- Replay metadata: record which fused CoreML package was used
ALTER TABLE inference_replay_metadata
    ADD COLUMN coreml_package_hash TEXT;
