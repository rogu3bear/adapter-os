-- Adapter version CoreML + dataset lineage
-- Adds backend/coreml metadata to adapter_versions, dataset links, and repo policies.

PRAGMA foreign_keys = ON;

ALTER TABLE adapter_versions ADD COLUMN training_backend TEXT;
ALTER TABLE adapter_versions ADD COLUMN coreml_used INTEGER NOT NULL DEFAULT 0 CHECK (coreml_used IN (0, 1));
ALTER TABLE adapter_versions ADD COLUMN coreml_device_type TEXT;

-- Repository-level training backend/coreml preferences
CREATE TABLE IF NOT EXISTS adapter_repository_policies (
    repo_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    preferred_backends_json TEXT,
    coreml_allowed INTEGER NOT NULL DEFAULT 1 CHECK (coreml_allowed IN (0, 1)),
    coreml_required INTEGER NOT NULL DEFAULT 0 CHECK (coreml_required IN (0, 1)),
    autopromote_coreml INTEGER NOT NULL DEFAULT 0 CHECK (autopromote_coreml IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (repo_id) REFERENCES adapter_repositories(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_adapter_repo_policy_tenant ON adapter_repository_policies(tenant_id);

-- Link adapter versions to the dataset versions used for training.
CREATE TABLE IF NOT EXISTS adapter_version_dataset_versions (
    adapter_version_id TEXT NOT NULL,
    dataset_version_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    PRIMARY KEY (adapter_version_id, dataset_version_id),
    FOREIGN KEY (adapter_version_id) REFERENCES adapter_versions(id) ON DELETE CASCADE,
    FOREIGN KEY (dataset_version_id) REFERENCES training_dataset_versions(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_av_dataset_versions_adapter ON adapter_version_dataset_versions(adapter_version_id);
CREATE INDEX IF NOT EXISTS idx_av_dataset_versions_dataset ON adapter_version_dataset_versions(dataset_version_id);

-- Enforce tenant consistency between adapter version and dataset version.
CREATE TRIGGER IF NOT EXISTS trg_av_dataset_versions_tenant_check
BEFORE INSERT ON adapter_version_dataset_versions
FOR EACH ROW
BEGIN
    -- Ensure dataset_version exists and tenant matches
    SELECT
        CASE
            WHEN NOT EXISTS (
                SELECT 1 FROM training_dataset_versions dv
                WHERE dv.id = NEW.dataset_version_id
                  AND dv.tenant_id = NEW.tenant_id
            ) THEN RAISE(ABORT, 'dataset_version tenant mismatch or missing')
        END;

    -- Ensure adapter_version tenant matches
    SELECT
        CASE
            WHEN NOT EXISTS (
                SELECT 1 FROM adapter_versions av
                WHERE av.id = NEW.adapter_version_id
                  AND av.tenant_id = NEW.tenant_id
            ) THEN RAISE(ABORT, 'adapter_version tenant mismatch')
        END;
END;
