-- Allow MLX backend in models.backend CHECK constraint while preserving existing data

PRAGMA foreign_keys = OFF;

CREATE TABLE models_new (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    hash_b3 TEXT UNIQUE NOT NULL,
    license_hash_b3 TEXT,
    config_hash_b3 TEXT NOT NULL,
    tokenizer_hash_b3 TEXT NOT NULL,
    tokenizer_cfg_hash_b3 TEXT NOT NULL,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    model_type TEXT DEFAULT 'base_model',
    model_path TEXT,
    config TEXT,
    status TEXT DEFAULT 'available',
    tenant_id TEXT DEFAULT 'default',
    updated_at TEXT DEFAULT (datetime('now')),
    adapter_path TEXT,
    backend TEXT NOT NULL DEFAULT 'metal' CHECK(backend IN ('metal', 'mlx-ffi', 'mlx')),
    quantization TEXT,
    last_error TEXT,
    size_bytes INTEGER,
    format TEXT,
    capabilities TEXT,
    import_status TEXT DEFAULT 'available' CHECK(import_status IN ('importing', 'available', 'failed')),
    import_error TEXT,
    imported_at TEXT,
    imported_by TEXT,
    weights_hash_b3 TEXT,
    license_text TEXT,
    model_card_hash_b3 TEXT
);

INSERT INTO models_new (
    id,
    name,
    hash_b3,
    license_hash_b3,
    config_hash_b3,
    tokenizer_hash_b3,
    tokenizer_cfg_hash_b3,
    metadata_json,
    created_at,
    model_type,
    model_path,
    config,
    status,
    tenant_id,
    updated_at,
    adapter_path,
    backend,
    quantization,
    last_error,
    size_bytes,
    format,
    capabilities,
    import_status,
    import_error,
    imported_at,
    imported_by,
    weights_hash_b3,
    license_text,
    model_card_hash_b3
)
SELECT
    id,
    name,
    hash_b3,
    license_hash_b3,
    config_hash_b3,
    tokenizer_hash_b3,
    tokenizer_cfg_hash_b3,
    metadata_json,
    created_at,
    model_type,
    model_path,
    config,
    status,
    tenant_id,
    updated_at,
    adapter_path,
    backend,
    quantization,
    last_error,
    size_bytes,
    format,
    capabilities,
    import_status,
    import_error,
    imported_at,
    imported_by,
    weights_hash_b3,
    license_text,
    model_card_hash_b3
FROM models;

DROP TABLE models;
ALTER TABLE models_new RENAME TO models;

CREATE INDEX IF NOT EXISTS idx_models_import_status ON models(import_status);
CREATE INDEX IF NOT EXISTS idx_models_format ON models(format);
CREATE INDEX IF NOT EXISTS idx_models_backend ON models(backend);
CREATE INDEX IF NOT EXISTS idx_models_tenant_model ON models(tenant_id, id);
CREATE INDEX IF NOT EXISTS idx_models_tenant_id ON models(tenant_id);

PRAGMA foreign_keys = ON;

