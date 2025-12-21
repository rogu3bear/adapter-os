-- Seed base model entry for Qwen2.5-7B-Instruct-4bit (MLX)

INSERT INTO models (
    id,
    name,
    hash_b3,
    config_hash_b3,
    tokenizer_hash_b3,
    tokenizer_cfg_hash_b3,
    model_type,
    model_path,
    status,
    tenant_id,
    backend,
    format,
    quantization,
    import_status,
    weights_hash_b3,
    capabilities,
    imported_at
)
SELECT
    'Qwen2.5-7B-Instruct-4bit',
    'Qwen2.5-7B-Instruct-4bit',
    '53227c71512c207e0fa10c8aee0f36116fcd7637d31949c41e694d74284b1a29',
    '6d3fc2d709ca3ee85a59cdd28b72e9c75576dc5c6a94b8ba75e8c044fc654d15',
    'c7ec05642c4277aa1d9de231c93e0d0303cdb97751e06b53e8543499b225df5b',
    'cda39ba010797e1871bc35bcf0190d85de4fe223c9a6100e7b10c171dd4613b1',
    'base_model',
    'var/models/Qwen2.5-7B-Instruct-4bit',
    'available',
    'system',
    'mlx',
    'mlx',
    'int4',
    'available',
    '53227c71512c207e0fa10c8aee0f36116fcd7637d31949c41e694d74284b1a29',
    '["chat"]',
    datetime('now')
WHERE NOT EXISTS (
    SELECT 1 FROM models WHERE id = 'Qwen2.5-7B-Instruct-4bit'
);

