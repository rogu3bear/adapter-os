-- Migration: Code Intelligence Tables
-- Adds support for repository tracking, commit metadata, and code-specific policies

-- Repositories table: tracks registered code repositories
CREATE TABLE IF NOT EXISTS repositories (
    id TEXT PRIMARY KEY NOT NULL,
    repo_id TEXT UNIQUE NOT NULL,
    path TEXT NOT NULL,
    languages TEXT NOT NULL,  -- JSON array of languages
    default_branch TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'registered' CHECK(status IN ('registered','scanning','ready','error')),
    frameworks_json TEXT,  -- JSON array of detected frameworks
    file_count INTEGER,
    symbol_count INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_repositories_repo_id ON repositories(repo_id);
CREATE INDEX IF NOT EXISTS idx_repositories_status ON repositories(status);

-- Commits table: stores commit metadata and analysis results
CREATE TABLE IF NOT EXISTS commits (
    id TEXT PRIMARY KEY NOT NULL,
    repo_id TEXT NOT NULL,
    sha TEXT NOT NULL,
    author TEXT NOT NULL,
    date TEXT NOT NULL,
    message TEXT NOT NULL,
    branch TEXT,
    changed_files_json TEXT NOT NULL,  -- JSON array of changed files
    impacted_symbols_json TEXT,  -- JSON array of impacted symbols
    test_results_json TEXT,  -- JSON object with test results
    ephemeral_adapter_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(repo_id, sha),
    FOREIGN KEY (repo_id) REFERENCES repositories(repo_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_commits_repo_id ON commits(repo_id);
CREATE INDEX IF NOT EXISTS idx_commits_sha ON commits(sha);
CREATE INDEX IF NOT EXISTS idx_commits_repo_sha ON commits(repo_id, sha);

-- Code policies table: code-specific policy configurations per tenant
CREATE TABLE IF NOT EXISTS code_policies (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    evidence_config_json TEXT NOT NULL,  -- Evidence requirements
    auto_apply_config_json TEXT NOT NULL,  -- Auto-apply settings
    path_permissions_json TEXT NOT NULL,  -- Path allowlist/denylist
    secret_patterns_json TEXT NOT NULL,  -- Secret detection patterns
    patch_limits_json TEXT NOT NULL,  -- Patch size/complexity limits
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, active)  -- Only one active policy per tenant
);

CREATE INDEX IF NOT EXISTS idx_code_policies_tenant ON code_policies(tenant_id);
CREATE INDEX IF NOT EXISTS idx_code_policies_active ON code_policies(active);
