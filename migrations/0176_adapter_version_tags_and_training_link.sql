-- Adapter version tagging and training/version linkage
-- Provides tag-based resolution and links training jobs to versions

PRAGMA foreign_keys = ON;

-- Version tags keyed by repo + tag for deterministic resolution
CREATE TABLE IF NOT EXISTS adapter_version_tags (
    id TEXT PRIMARY KEY,
    version_id TEXT NOT NULL,
    repo_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    tag_name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (version_id) REFERENCES adapter_versions(id) ON DELETE CASCADE,
    FOREIGN KEY (repo_id) REFERENCES adapter_repositories(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_adapter_version_tags_repo_tag
    ON adapter_version_tags(repo_id, tag_name);

CREATE UNIQUE INDEX IF NOT EXISTS idx_adapter_version_tags_version
    ON adapter_version_tags(version_id);

-- Tenant guard: tag tenant must match version tenant
CREATE TRIGGER IF NOT EXISTS trg_adapter_version_tags_tenant_match
BEFORE INSERT ON adapter_version_tags
FOR EACH ROW
WHEN (
    SELECT tenant_id FROM adapter_versions WHERE id = NEW.version_id
) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'adapter_version_tags.tenant_id must match adapter_versions.tenant_id');
END;

-- Repo guard: tag repo must match version repo
CREATE TRIGGER IF NOT EXISTS trg_adapter_version_tags_repo_match
BEFORE INSERT ON adapter_version_tags
FOR EACH ROW
WHEN (
    SELECT repo_id FROM adapter_versions WHERE id = NEW.version_id
) != NEW.repo_id
BEGIN
    SELECT RAISE(ABORT, 'adapter_version_tags.repo_id must match adapter_versions.repo_id');
END;

-- Training job linkage to versions and source metadata
-- (columns already added in 0175; draft_version_id is new here)
ALTER TABLE repository_training_jobs ADD COLUMN draft_version_id TEXT;

CREATE INDEX IF NOT EXISTS idx_training_jobs_base_version
    ON repository_training_jobs(base_version_id);

CREATE INDEX IF NOT EXISTS idx_training_jobs_target_branch
    ON repository_training_jobs(target_branch);

CREATE INDEX IF NOT EXISTS idx_training_jobs_produced_version
    ON repository_training_jobs(produced_version_id);

CREATE INDEX IF NOT EXISTS idx_training_jobs_draft_version
    ON repository_training_jobs(draft_version_id);
