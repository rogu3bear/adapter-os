-- Migration: Add base model import tracking and onboarding journey support
-- Citation: Policy Pack #8 (Isolation) - per-tenant operations
-- Pattern from: migrations/0028_base_model_status.sql

-- Track model imports
CREATE TABLE IF NOT EXISTS base_model_imports (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    model_name TEXT NOT NULL,
    weights_path TEXT NOT NULL,
    config_path TEXT NOT NULL,
    tokenizer_path TEXT NOT NULL,
    tokenizer_config_path TEXT,
    status TEXT NOT NULL CHECK(status IN ('uploading', 'validating', 'importing', 'completed', 'failed')),
    progress INTEGER DEFAULT 0,
    error_message TEXT,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    created_by TEXT NOT NULL,
    metadata_json TEXT,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_base_model_imports_tenant ON base_model_imports(tenant_id);
CREATE INDEX idx_base_model_imports_status ON base_model_imports(status);
CREATE INDEX idx_base_model_imports_created_by ON base_model_imports(created_by);

-- Track user onboarding journey
CREATE TABLE IF NOT EXISTS onboarding_journeys (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    journey_type TEXT NOT NULL DEFAULT 'cursor_integration',
    step_completed TEXT NOT NULL CHECK(step_completed IN ('model_imported', 'model_loaded', 'cursor_configured', 'first_inference')),
    step_data TEXT,
    completed_at TEXT NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_onboarding_journeys_tenant ON onboarding_journeys(tenant_id);
CREATE INDEX idx_onboarding_journeys_user ON onboarding_journeys(user_id);
CREATE INDEX idx_onboarding_journeys_type ON onboarding_journeys(journey_type);

-- Extend base_model_status table to link to imports
ALTER TABLE base_model_status ADD COLUMN import_id TEXT;

