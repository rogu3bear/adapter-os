-- Adapter Registry Database Schema
-- Simplified schema for CLI adapter management

-- Adapters table: basic adapter registry
CREATE TABLE adapters (
    id TEXT PRIMARY KEY,
    hash TEXT NOT NULL,
    tier TEXT NOT NULL,
    rank INTEGER NOT NULL,
    acl TEXT NOT NULL,
    activation_pct REAL DEFAULT 0.0,
    registered_at TEXT NOT NULL
);

-- Tenants table: tenant registry for ACL
CREATE TABLE tenants (
    id TEXT PRIMARY KEY,
    uid INTEGER NOT NULL,
    gid INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

-- Models table: base model registry
CREATE TABLE models (
    name TEXT PRIMARY KEY,
    config_hash TEXT NOT NULL,
    tokenizer_hash TEXT NOT NULL,
    tokenizer_cfg_hash TEXT NOT NULL,
    weights_hash TEXT NOT NULL,
    license_hash TEXT NOT NULL,
    license_text TEXT NOT NULL,
    model_card_hash TEXT,
    created_at INTEGER NOT NULL
);

-- Checkpoints table: plan checkpoints
CREATE TABLE checkpoints (
    cpid TEXT PRIMARY KEY,
    plan_id TEXT NOT NULL,
    manifest_hash TEXT NOT NULL,
    promoted_at TEXT NOT NULL,
    status TEXT NOT NULL
);

-- Indexes for performance
CREATE INDEX idx_adapters_tier ON adapters(tier);
CREATE INDEX idx_adapters_activation ON adapters(activation_pct);
CREATE INDEX idx_models_config_hash ON models(config_hash);
CREATE INDEX idx_models_tokenizer_hash ON models(tokenizer_hash);
CREATE INDEX idx_models_weights_hash ON models(weights_hash);
CREATE INDEX idx_models_created_at ON models(created_at);
