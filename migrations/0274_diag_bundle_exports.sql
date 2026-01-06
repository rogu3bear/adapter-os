-- Diagnostic bundle exports table
-- Stores metadata for signed bundle exports

CREATE TABLE IF NOT EXISTS diag_bundle_exports (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id),
    run_id TEXT NOT NULL REFERENCES diag_runs(id),
    trace_id TEXT NOT NULL,

    -- Bundle file info
    format TEXT NOT NULL CHECK(format IN ('tar.zst', 'zip')),
    file_path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    bundle_hash TEXT NOT NULL,  -- BLAKE3 hex

    -- Signature info
    merkle_root TEXT NOT NULL,  -- BLAKE3 hex
    signature TEXT NOT NULL,    -- Ed25519 hex
    public_key TEXT NOT NULL,   -- Ed25519 hex
    key_id TEXT NOT NULL,       -- kid-{hash}

    -- Manifest (JSON)
    manifest_json TEXT NOT NULL,

    -- Evidence inclusion
    evidence_included INTEGER NOT NULL DEFAULT 0,

    -- Identity snapshot
    request_hash TEXT,
    decision_chain_hash TEXT,
    backend_identity_hash TEXT,
    model_identity_hash TEXT,
    adapter_stack_ids TEXT,  -- JSON array
    code_identity TEXT,

    -- Status
    status TEXT NOT NULL DEFAULT 'completed' CHECK(status IN ('pending', 'completed', 'failed', 'expired')),
    expires_at TEXT,  -- ISO 8601, for cleanup

    -- Audit
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_diag_bundle_exports_tenant_id ON diag_bundle_exports(tenant_id);
CREATE INDEX IF NOT EXISTS idx_diag_bundle_exports_run_id ON diag_bundle_exports(run_id);
CREATE INDEX IF NOT EXISTS idx_diag_bundle_exports_trace_id ON diag_bundle_exports(trace_id);
CREATE INDEX IF NOT EXISTS idx_diag_bundle_exports_bundle_hash ON diag_bundle_exports(bundle_hash);
CREATE INDEX IF NOT EXISTS idx_diag_bundle_exports_created_at ON diag_bundle_exports(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_diag_bundle_exports_expires_at ON diag_bundle_exports(expires_at);
