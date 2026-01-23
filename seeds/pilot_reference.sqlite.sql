-- Deterministic Pilot Demo Seed (SQLite)
--
-- Entity IDs (UUIDs)
--   tenant_id:        00000000-0000-4000-8000-000000000001
--   admin_user_id:    00000000-0000-4000-8000-000000000002
--   base_model_id:    00000000-0000-4000-8000-000000000003
--   repo_id:          00000000-0000-4000-8000-000000000004
--   adapter_version:  00000000-0000-4000-8000-000000000005
--   training_job_id:  00000000-0000-4000-8000-000000000006
--   stack_id:         00000000-0000-4000-8000-000000000007
--   git_repo_row_id:  00000000-0000-4000-8000-000000000008
--
-- Fixed timestamp (ISO-8601)
--   2025-01-01T00:00:00Z

PRAGMA foreign_keys = ON;

BEGIN;

-- ---------------------------------------------------------------------------
-- Tenant + admin user
-- ---------------------------------------------------------------------------

INSERT OR IGNORE INTO tenants (
    id,
    name,
    itar_flag,
    status,
    created_at,
    updated_at,
    multi_tenant_mode,
    determinism_mode,
    default_stack_id,
    default_pinned_adapter_ids
) VALUES (
    '00000000-0000-4000-8000-000000000001',
    'pilot-demo',
    0,
    'active',
    '2025-01-01T00:00:00Z',
    '2025-01-01T00:00:00Z',
    'disabled',
    'strict',
    '00000000-0000-4000-8000-000000000007',
    '[]'
);

INSERT OR IGNORE INTO users (
    id,
    tenant_id,
    email,
    display_name,
    pw_hash,
    role,
    disabled,
    created_at
) VALUES (
    '00000000-0000-4000-8000-000000000002',
    '00000000-0000-4000-8000-000000000001',
    'demo@example.com',
    'Pilot Demo Admin',
    '$argon2id$v=19$m=65536,t=3,p=1$cGlsb3QtZGVtby1zYWx0IQ$gKkg46uD2KECmzp5uOW9hFxHBwbw6OhtgM8NnyFsgg8',
    'admin',
    0,
    '2025-01-01T00:00:00Z'
);

-- ---------------------------------------------------------------------------
-- Base model (so /v1/models is non-empty)
-- ---------------------------------------------------------------------------

INSERT OR IGNORE INTO models (
    id,
    name,
    hash_b3,
    config_hash_b3,
    tokenizer_hash_b3,
    tokenizer_cfg_hash_b3,
    metadata_json,
    created_at,
    model_type,
    tenant_id,
    updated_at,
    backend,
    quantization,
    size_bytes,
    format,
    capabilities,
    import_status,
    imported_at,
    imported_by
) VALUES (
    '00000000-0000-4000-8000-000000000003',
    'qwen2.5-7b-demo',
    '804433d69235fb8f3eaffb970da94d40947626be1d8f3afddf53fdd2e8de338c',
    '80f422ee4a86b97aa5fbb0f8d8a85f3ffd5edabaa1a6ab1667c149b435981d88',
    'bd1e84a77b58b19df9c17358303523cc01bcfeb1186fcfadfa6e1bbbbeeebfe9',
    '60c3ecc97b9dd5ebd6c9e3865fcf62356927fb5cf2d1aa9a17e69c23fcb9f7da',
    '{"size_bytes": 1024, "quant": "q4_0", "source": "seed-demo"}',
    '2025-01-01T00:00:00Z',
    'base_model',
    '00000000-0000-4000-8000-000000000001',
    '2025-01-01T00:00:00Z',
    'metal',
    'q4_0',
    1024,
    'safetensors',
    '["chat"]',
    'available',
    '2025-01-01T00:00:00Z',
    'seed-demo'
);

-- ---------------------------------------------------------------------------
-- Stack
-- ---------------------------------------------------------------------------

INSERT OR IGNORE INTO adapter_stacks (
    id,
    tenant_id,
    name,
    description,
    adapter_ids_json,
    workflow_type,
    version,
    lifecycle_state,
    determinism_mode,
    routing_determinism_mode,
    created_by,
    created_at,
    updated_at
) VALUES (
    '00000000-0000-4000-8000-000000000007',
    '00000000-0000-4000-8000-000000000001',
    'stack.pilot-demo',
    'Deterministic pilot demo stack',
    '[]',
    'Parallel',
    '1.0.0',
    'active',
    'strict',
    'deterministic',
    'seed-demo',
    '2025-01-01T00:00:00Z',
    '2025-01-01T00:00:00Z'
);

-- ---------------------------------------------------------------------------
-- Adapter repository + adapter version
-- ---------------------------------------------------------------------------

INSERT OR IGNORE INTO adapter_repositories (
    id,
    tenant_id,
    name,
    base_model_id,
    default_branch,
    archived,
    created_by,
    created_at,
    description
) VALUES (
    '00000000-0000-4000-8000-000000000004',
    '00000000-0000-4000-8000-000000000001',
    'pilot-demo-repo',
    '00000000-0000-4000-8000-000000000003',
    'main',
    0,
    '00000000-0000-4000-8000-000000000002',
    '2025-01-01T00:00:00Z',
    'Deterministic pilot demo adapter repository'
);

INSERT OR IGNORE INTO adapter_versions (
    id,
    repo_id,
    tenant_id,
    version,
    branch,
    branch_classification,
    aos_hash,
    manifest_schema_version,
    code_commit_sha,
    training_backend,
    coreml_used,
    adapter_trust_state,
    release_state,
    evaluation_summary,
    created_at
) VALUES (
    '00000000-0000-4000-8000-000000000005',
    '00000000-0000-4000-8000-000000000004',
    '00000000-0000-4000-8000-000000000001',
    '1.0.0',
    'main',
    'protected',
    '4c91d6316af88bb65a56b16196d1fb9e8eff5da995160f1937ee3ede298d7a56',
    '1.0.0',
    'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef',
    'metal',
    0,
    'allowed',
    'ready',
    'Seeded pilot demo adapter version',
    '2025-01-01T00:00:00Z'
);

INSERT OR IGNORE INTO adapter_version_runtime_state (
    version_id,
    runtime_state,
    updated_at,
    worker_id,
    last_error
) VALUES (
    '00000000-0000-4000-8000-000000000005',
    'unloaded',
    '2025-01-01T00:00:00Z',
    NULL,
    NULL
);

-- ---------------------------------------------------------------------------
-- Training job (completed) + required FK parent (git_repositories.repo_id)
-- ---------------------------------------------------------------------------

INSERT OR IGNORE INTO git_repositories (
    id,
    repo_id,
    path,
    branch,
    analysis_json,
    evidence_json,
    security_scan_json,
    status,
    created_at,
    created_by
) VALUES (
    '00000000-0000-4000-8000-000000000008',
    '00000000-0000-4000-8000-000000000004',
    '/repos/pilot-demo',
    'main',
    '{}',
    '{}',
    '{}',
    'ready',
    '2025-01-01T00:00:00Z',
    '00000000-0000-4000-8000-000000000002'
);

INSERT OR IGNORE INTO repository_training_jobs (
    id,
    repo_id,
    training_config_json,
    status,
    progress_json,
    started_at,
    completed_at,
    created_by,
    created_at,
    config_hash_b3,
    tenant_id,
    base_model_id,
    stack_id,
    produced_version_id,
    adapter_name
) VALUES (
    '00000000-0000-4000-8000-000000000006',
    '00000000-0000-4000-8000-000000000004',
    '{"rank":8,"alpha":16.0,"epochs":1,"learning_rate":0.0005,"batch_size":4,"base_model_id":"00000000-0000-4000-8000-000000000003"}',
    'completed',
    '{"progress_pct":100.0,"current_epoch":1,"total_epochs":1,"current_loss":0.0,"learning_rate":0.0005,"tokens_per_second":10.0,"error_message":null}',
    '2025-01-01T00:00:00Z',
    '2025-01-01T00:00:00Z',
    '00000000-0000-4000-8000-000000000002',
    '2025-01-01T00:00:00Z',
    '6b3b8948d5055a3a18be2d15075fe7e690dd631ea4c4dd94a0c24594dce80d79',
    '00000000-0000-4000-8000-000000000001',
    '00000000-0000-4000-8000-000000000003',
    '00000000-0000-4000-8000-000000000007',
    '00000000-0000-4000-8000-000000000005',
    'pilot-demo-adapter'
);

COMMIT;
