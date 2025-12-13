-- Adapter version lifecycle history with actor/reason metadata
-- Tracks all release state transitions and links optional training jobs

PRAGMA foreign_keys = ON;

-- Drop views that depend on the old adapter_version_history schema
-- (created in migrations 0071 and 0107 with different column names)
DROP VIEW IF EXISTS recent_adapter_lifecycle_changes;
DROP VIEW IF EXISTS adapters_lifecycle_summary;

-- Drop legacy history table to align with new schema.
DROP TABLE IF EXISTS adapter_version_history;

CREATE TABLE IF NOT EXISTS adapter_version_history (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    version_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    old_state TEXT,
    new_state TEXT NOT NULL,
    actor TEXT,
    reason TEXT,
    train_job_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (repo_id) REFERENCES adapter_repositories(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (version_id) REFERENCES adapter_versions(id) ON DELETE CASCADE,
    FOREIGN KEY (train_job_id) REFERENCES repository_training_jobs(id) ON DELETE SET NULL,
    CHECK (new_state IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')),
    CHECK (
        old_state IS NULL OR
        old_state IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
    )
);

CREATE INDEX IF NOT EXISTS idx_adapter_version_history_version
    ON adapter_version_history(version_id);
CREATE INDEX IF NOT EXISTS idx_adapter_version_history_repo_branch_created
    ON adapter_version_history(repo_id, branch, created_at DESC);

-- Tenant guard: history tenant must match version tenant
CREATE TRIGGER IF NOT EXISTS trg_adapter_version_history_tenant_match
BEFORE INSERT ON adapter_version_history
FOR EACH ROW
WHEN (
    SELECT tenant_id FROM adapter_versions WHERE id = NEW.version_id
) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'adapter_version_history.tenant_id must match adapter_versions.tenant_id');
END;

-- Repo guard: history repo must match version repo
CREATE TRIGGER IF NOT EXISTS trg_adapter_version_history_repo_match
BEFORE INSERT ON adapter_version_history
FOR EACH ROW
WHEN (
    SELECT repo_id FROM adapter_versions WHERE id = NEW.version_id
) != NEW.repo_id
BEGIN
    SELECT RAISE(ABORT, 'adapter_version_history.repo_id must match adapter_versions.repo_id');
END;

-- Branch guard: history branch must match version branch
CREATE TRIGGER IF NOT EXISTS trg_adapter_version_history_branch_match
BEFORE INSERT ON adapter_version_history
FOR EACH ROW
WHEN (
    SELECT branch FROM adapter_versions WHERE id = NEW.version_id
) != NEW.branch
BEGIN
    SELECT RAISE(ABORT, 'adapter_version_history.branch must match adapter_versions.branch');
END;
