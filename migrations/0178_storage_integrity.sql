-- Storage integrity issues for datasets and adapter artifacts
CREATE TABLE IF NOT EXISTS storage_issues (
    id TEXT PRIMARY KEY,
    tenant_id TEXT,
    owner_type TEXT NOT NULL, -- dataset_version | adapter_version | orphan
    owner_id TEXT NOT NULL,
    version_id TEXT,
    issue_type TEXT NOT NULL, -- missing_bytes | hash_mismatch | orphan_bytes
    severity TEXT NOT NULL,
    location TEXT NOT NULL,
    details TEXT,
    detected_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_storage_issues_owner ON storage_issues(owner_type, owner_id);
CREATE INDEX IF NOT EXISTS idx_storage_issues_tenant ON storage_issues(tenant_id);
CREATE INDEX IF NOT EXISTS idx_storage_issues_issue ON storage_issues(issue_type);








