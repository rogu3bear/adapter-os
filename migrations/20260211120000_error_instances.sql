-- First-class error objects (ErrorInstance + ErrorBucket)
--
-- Design goals:
-- - Searchable/linkable errors with stable IDs (err-*)
-- - Deterministic fingerprinting for dedupe/grouping
-- - Tenant-scoped querying

CREATE TABLE IF NOT EXISTS error_instances (
    id TEXT PRIMARY KEY NOT NULL,
    created_at_unix_ms INTEGER NOT NULL,
    tenant_id TEXT NOT NULL,

    -- source: ui | api | worker
    source TEXT NOT NULL,

    -- stable, human-defined error code (uses API ErrorResponse.code for now)
    error_code TEXT NOT NULL,

    -- kind: network | auth | validation | server | decode | timeout | worker | unknown
    kind TEXT NOT NULL,

    -- severity: info | warn | error | fatal
    severity TEXT NOT NULL,

    -- user-safe message
    message_user TEXT NOT NULL,
    -- diagnostic message (sanitized)
    message_dev TEXT,

    -- stable dedupe key (hash)
    fingerprint TEXT NOT NULL,

    -- tags (JSON object, whitelisted/sanitized)
    tags_json TEXT NOT NULL,

    -- correlation
    session_id TEXT,
    request_id TEXT,
    diag_trace_id TEXT,
    otel_trace_id TEXT,
    http_method TEXT,
    http_path TEXT,
    http_status INTEGER,
    run_id TEXT,
    receipt_hash TEXT,
    route_digest TEXT
);

CREATE INDEX IF NOT EXISTS idx_error_instances_tenant_created
    ON error_instances(tenant_id, created_at_unix_ms DESC);
CREATE INDEX IF NOT EXISTS idx_error_instances_tenant_fingerprint
    ON error_instances(tenant_id, fingerprint);
CREATE INDEX IF NOT EXISTS idx_error_instances_tenant_code_created
    ON error_instances(tenant_id, error_code, created_at_unix_ms DESC);
CREATE INDEX IF NOT EXISTS idx_error_instances_tenant_request_id
    ON error_instances(tenant_id, request_id);
CREATE INDEX IF NOT EXISTS idx_error_instances_tenant_diag_trace_id
    ON error_instances(tenant_id, diag_trace_id);
CREATE INDEX IF NOT EXISTS idx_error_instances_tenant_session_id
    ON error_instances(tenant_id, session_id);

CREATE TABLE IF NOT EXISTS error_buckets (
    fingerprint TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL,
    error_code TEXT NOT NULL,
    kind TEXT NOT NULL,
    severity TEXT NOT NULL,
    first_seen_unix_ms INTEGER NOT NULL,
    last_seen_unix_ms INTEGER NOT NULL,
    count INTEGER NOT NULL,
    sample_error_ids_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_error_buckets_tenant_last_seen
    ON error_buckets(tenant_id, last_seen_unix_ms DESC);
CREATE INDEX IF NOT EXISTS idx_error_buckets_tenant_code_last_seen
    ON error_buckets(tenant_id, error_code, last_seen_unix_ms DESC);

