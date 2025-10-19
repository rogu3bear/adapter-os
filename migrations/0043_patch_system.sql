-- Migration: Add comprehensive patch tracking and validation system
-- Citation: Policy Pack #12 (Artifacts) - cryptographic verification requirements

-- Track patch applications with full audit trail
CREATE TABLE IF NOT EXISTS patch_applications (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    patch_id TEXT NOT NULL UNIQUE,
    patch_hash TEXT NOT NULL,
    signature TEXT NOT NULL,
    public_key TEXT NOT NULL,
    applied_by TEXT NOT NULL,
    applied_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending', 'validating', 'applying', 'completed', 'failed', 'rolled_back')),
    validation_results JSON,
    error_message TEXT,
    rollback_id TEXT,
    metadata_json TEXT,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_patch_applications_tenant ON patch_applications(tenant_id);
CREATE INDEX idx_patch_applications_status ON patch_applications(status);
CREATE INDEX idx_patch_applications_applied_at ON patch_applications(applied_at);

-- Store cryptographic signatures for patch verification
CREATE TABLE IF NOT EXISTS patch_signatures (
    patch_hash TEXT PRIMARY KEY,
    signature TEXT NOT NULL,
    public_key TEXT NOT NULL,
    signed_at TEXT NOT NULL,
    signer_identity TEXT NOT NULL,
    signature_algorithm TEXT NOT NULL DEFAULT 'ed25519'
);

CREATE INDEX idx_patch_signatures_hash ON patch_signatures(patch_hash);

-- Extend existing tables for patch relationships
-- Citation: existing pattern from migration 0028
ALTER TABLE base_model_status ADD COLUMN IF NOT EXISTS last_patch_applied TEXT;
ALTER TABLE adapter_lifecycle ADD COLUMN IF NOT EXISTS patches_applied JSON;
