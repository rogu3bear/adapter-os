-- Repository training policies for self-hosting agent orchestration
-- Stores per-repo backend preferences, dataset gates, and trust thresholds

CREATE TABLE IF NOT EXISTS repository_training_policies (
    repo_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    preferred_backends_json TEXT NOT NULL,
    allowed_dataset_types_json TEXT NOT NULL,
    trust_states_json TEXT NOT NULL,
    coreml_allowed INTEGER NOT NULL DEFAULT 1,
    coreml_required INTEGER NOT NULL DEFAULT 0,
    pinned_dataset_version_ids_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (repo_id, tenant_id),
    FOREIGN KEY (repo_id) REFERENCES repositories(repo_id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_repo_training_policies_repo
    ON repository_training_policies(repo_id);
