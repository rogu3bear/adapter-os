-- Storage reconciliation issues
CREATE TABLE IF NOT EXISTS storage_reconciliation_issues (
    id TEXT PRIMARY KEY,
    tenant_id TEXT,
    owner_type TEXT NOT NULL, -- dataset|adapter|orphan
    owner_id TEXT,
    version_id TEXT,
    issue_type TEXT NOT NULL, -- missing_file|orphan_file|hash_mismatch
    severity TEXT NOT NULL,   -- warning|error
    path TEXT NOT NULL,
    expected_hash TEXT,
    actual_hash TEXT,
    message TEXT,
    detected_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_storage_issues_owner ON storage_reconciliation_issues(owner_type, owner_id);
CREATE INDEX IF NOT EXISTS idx_storage_issues_tenant ON storage_reconciliation_issues(tenant_id);

