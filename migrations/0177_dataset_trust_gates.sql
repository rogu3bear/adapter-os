-- Dataset trust and validation gates
-- Introduces versioned datasets with validation + safety status and training-time trust gating.

CREATE TABLE IF NOT EXISTS training_dataset_versions (
    id TEXT PRIMARY KEY,
    dataset_id TEXT NOT NULL REFERENCES training_datasets(id) ON DELETE CASCADE,
    tenant_id TEXT,
    version_number INTEGER NOT NULL,
    version_label TEXT,
    storage_path TEXT NOT NULL,
    hash_b3 TEXT NOT NULL,
    manifest_path TEXT,
    manifest_json TEXT,
    validation_status TEXT NOT NULL DEFAULT 'pending', -- structural tier
    validation_errors_json TEXT,
    pii_status TEXT NOT NULL DEFAULT 'unknown',
    toxicity_status TEXT NOT NULL DEFAULT 'unknown',
    leak_status TEXT NOT NULL DEFAULT 'unknown',
    anomaly_status TEXT NOT NULL DEFAULT 'unknown',
    overall_safety_status TEXT NOT NULL DEFAULT 'unknown',
    trust_state TEXT NOT NULL DEFAULT 'unknown',
    overall_trust_status TEXT NOT NULL DEFAULT 'unknown',
    sensitivity TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT,
    locked_at TEXT,
    soft_deleted_at TEXT,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_tdv_dataset_version_number
    ON training_dataset_versions(dataset_id, version_number);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tdv_dataset_version_label
    ON training_dataset_versions(dataset_id, version_label)
    WHERE version_label IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tdv_tenant_dataset ON training_dataset_versions(tenant_id, dataset_id);
CREATE INDEX IF NOT EXISTS idx_tdv_hash ON training_dataset_versions(hash_b3);
CREATE INDEX IF NOT EXISTS idx_tdv_trust_state ON training_dataset_versions(trust_state);

-- Validation runs per version (structural + semantic/safety tiers)
CREATE TABLE IF NOT EXISTS dataset_version_validations (
    id TEXT PRIMARY KEY,
    dataset_version_id TEXT NOT NULL REFERENCES training_dataset_versions(id) ON DELETE CASCADE,
    tier TEXT NOT NULL, -- tier1_structural | tier2_safety
    status TEXT NOT NULL, -- pending | running | valid | invalid | failed | warn | block
    signal TEXT, -- pii | toxicity | leak | anomaly | structural
    validation_errors_json TEXT,
    sample_row_ids_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT
);
CREATE INDEX IF NOT EXISTS idx_dvv_dataset_version ON dataset_version_validations(dataset_version_id);
CREATE INDEX IF NOT EXISTS idx_dvv_tier_status ON dataset_version_validations(tier, status);

-- Admin overrides for trust gating (audit-only, does not mutate validation rows)
CREATE TABLE IF NOT EXISTS dataset_version_overrides (
    id TEXT PRIMARY KEY,
    dataset_version_id TEXT NOT NULL REFERENCES training_dataset_versions(id) ON DELETE CASCADE,
    override_state TEXT NOT NULL, -- allowed | allowed_with_warning | blocked | needs_approval
    reason TEXT,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_dvo_dataset_version ON dataset_version_overrides(dataset_version_id);

-- Training jobs reference dataset versions for provenance and trust gating
ALTER TABLE repository_training_jobs ADD COLUMN dataset_version_id TEXT;
CREATE INDEX IF NOT EXISTS idx_training_jobs_dataset_version ON repository_training_jobs(dataset_version_id);

