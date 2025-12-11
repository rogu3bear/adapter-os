-- Core entity model: AdapterRepository, AdapterVersion, AdapterVersionRuntimeState
-- Adds repository/version tables, runtime projection, guardrails, and backfill from existing adapters.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS adapter_repositories (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    base_model_id TEXT,
    default_branch TEXT NOT NULL DEFAULT 'main',
    archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    description TEXT,
    UNIQUE (tenant_id, name),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (base_model_id) REFERENCES models(id)
);

CREATE TABLE IF NOT EXISTS adapter_versions (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    version TEXT NOT NULL,
    branch TEXT NOT NULL,
    aos_path TEXT,
    aos_hash TEXT,
    manifest_schema_version TEXT,
    parent_version_id TEXT,
    code_commit_sha TEXT,
    data_spec_hash TEXT,
    release_state TEXT NOT NULL CHECK (release_state IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')),
    metrics_snapshot_id TEXT,
    evaluation_summary TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (repo_id) REFERENCES adapter_repositories(id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_version_id) REFERENCES adapter_versions(id)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_adapter_versions_repo_branch_version
    ON adapter_versions(repo_id, branch, version);
CREATE INDEX IF NOT EXISTS idx_adapter_versions_release_state
    ON adapter_versions(release_state);
CREATE INDEX IF NOT EXISTS idx_adapter_versions_repo
    ON adapter_versions(repo_id);
CREATE INDEX IF NOT EXISTS idx_adapter_versions_tenant
    ON adapter_versions(tenant_id);

CREATE TABLE IF NOT EXISTS adapter_version_runtime_state (
    version_id TEXT PRIMARY KEY,
    runtime_state TEXT NOT NULL CHECK (runtime_state IN ('unloaded', 'loading', 'warm', 'hot', 'error')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    worker_id TEXT,
    last_error TEXT,
    FOREIGN KEY (version_id) REFERENCES adapter_versions(id) ON DELETE CASCADE
);

-- Prevent deleting a repository that still has an active version
CREATE TRIGGER IF NOT EXISTS adapter_repo_block_delete_active
BEFORE DELETE ON adapter_repositories
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM adapter_versions
    WHERE repo_id = OLD.id AND release_state = 'active'
)
BEGIN
    SELECT RAISE(ABORT, 'cannot delete repository with active versions');
END;

-- Prevent artifact mutation once a version reaches ready/active/retired states
CREATE TRIGGER IF NOT EXISTS adapter_version_artifact_immutable
BEFORE UPDATE ON adapter_versions
FOR EACH ROW
WHEN OLD.release_state IN ('ready', 'active', 'deprecated', 'retired', 'failed')
     AND (
        COALESCE(OLD.aos_hash, '') <> COALESCE(NEW.aos_hash, '') OR
        COALESCE(OLD.aos_path, '') <> COALESCE(NEW.aos_path, '') OR
        COALESCE(OLD.manifest_schema_version, '') <> COALESCE(NEW.manifest_schema_version, '')
     )
BEGIN
    SELECT RAISE(ABORT, 'adapter artifact immutable after ready');
END;

-- Backfill repositories from existing adapters
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
)
SELECT DISTINCT
    COALESCE(a.repo_id, a.adapter_id, a.id),
    a.tenant_id,
    a.name,
    a.base_model_id,
    'main',
    CASE WHEN a.archived_at IS NOT NULL THEN 1 ELSE 0 END,
    a.archived_by,
    COALESCE(a.created_at, datetime('now')),
    a.intent
FROM adapters a;

-- Backfill versions from existing adapters
INSERT INTO adapter_versions (
    id,
    repo_id,
    tenant_id,
    version,
    branch,
    aos_path,
    aos_hash,
    manifest_schema_version,
    parent_version_id,
    code_commit_sha,
    data_spec_hash,
    release_state,
    metrics_snapshot_id,
    evaluation_summary,
    created_at
)
SELECT
    lower(hex(randomblob(16))),
    COALESCE(a.repo_id, a.adapter_id, a.id),
    a.tenant_id,
    a.version,
    'main',
    a.aos_file_path,
    COALESCE(a.aos_file_hash, a.hash_b3),
    a.manifest_schema_version,
    NULL,
    a.commit_sha,
    NULL,
    CASE
        WHEN a.lifecycle_state IN ('active', 'deprecated', 'retired') THEN a.lifecycle_state
        WHEN a.lifecycle_state = 'training' THEN 'training'
        WHEN a.lifecycle_state = 'ready' THEN 'ready'
        ELSE 'draft'
    END,
    NULL,
    NULL,
    COALESCE(a.created_at, datetime('now'))
FROM adapters a;

-- Seed runtime state projection from adapter load_state
INSERT OR IGNORE INTO adapter_version_runtime_state (
    version_id,
    runtime_state,
    updated_at,
    worker_id,
    last_error
)
SELECT
    av.id,
    CASE
        WHEN a.load_state IS NULL THEN 'unloaded'
        WHEN lower(a.load_state) IN ('unloaded', 'loading', 'warm', 'hot', 'error') THEN lower(a.load_state)
        ELSE 'unloaded'
    END,
    COALESCE(a.updated_at, datetime('now')),
    NULL,
    NULL
FROM adapters a
JOIN adapter_versions av
    ON av.repo_id = COALESCE(a.repo_id, a.adapter_id, a.id)
   AND av.tenant_id = a.tenant_id
   AND av.version = a.version
   AND av.branch = 'main';

-- Extend training jobs with repository/version alignment fields
ALTER TABLE repository_training_jobs ADD COLUMN base_version_id TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN target_branch TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN produced_version_id TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN hyperparameters_json TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN data_spec_json TEXT;
ALTER TABLE repository_training_jobs ADD COLUMN metrics_snapshot_id TEXT;
