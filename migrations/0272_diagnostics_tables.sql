-- Diagnostic Runs Table
-- Tracks each inference request's diagnostic session
-- Evidence: Diagnostics Runtime Infrastructure

CREATE TABLE IF NOT EXISTS diag_runs (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    trace_id TEXT NOT NULL,
    started_at_unix_ms INTEGER NOT NULL,
    completed_at_unix_ms INTEGER,
    request_hash TEXT NOT NULL,
    manifest_hash TEXT,
    status TEXT NOT NULL CHECK(status IN ('running', 'completed', 'failed', 'cancelled')) DEFAULT 'running',
    dropped_events_count INTEGER NOT NULL DEFAULT 0,
    total_events_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for diag_runs
CREATE INDEX IF NOT EXISTS idx_diag_runs_tenant_id ON diag_runs(tenant_id);
CREATE INDEX IF NOT EXISTS idx_diag_runs_trace_id ON diag_runs(trace_id);
CREATE INDEX IF NOT EXISTS idx_diag_runs_tenant_trace ON diag_runs(tenant_id, trace_id);
CREATE INDEX IF NOT EXISTS idx_diag_runs_tenant_created ON diag_runs(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_diag_runs_status ON diag_runs(status);

-- Trigger to auto-update updated_at on diag_runs
CREATE TRIGGER IF NOT EXISTS update_diag_runs_timestamp
AFTER UPDATE ON diag_runs
FOR EACH ROW
BEGIN
    UPDATE diag_runs SET updated_at = datetime('now') WHERE id = NEW.id;
END;


-- Diagnostic Events Table
-- Stores individual diagnostic events for each run
-- Enforces tenant isolation via foreign key

CREATE TABLE IF NOT EXISTS diag_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tenant_id TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES diag_runs(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    mono_us INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    severity TEXT NOT NULL CHECK(severity IN ('trace', 'debug', 'info', 'warn', 'error')),
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    -- Enforce sequence uniqueness per run
    UNIQUE(run_id, seq)
);

-- Indexes for diag_events
CREATE INDEX IF NOT EXISTS idx_diag_events_tenant_id ON diag_events(tenant_id);
CREATE INDEX IF NOT EXISTS idx_diag_events_run_id ON diag_events(run_id);
CREATE INDEX IF NOT EXISTS idx_diag_events_tenant_run ON diag_events(tenant_id, run_id);
CREATE INDEX IF NOT EXISTS idx_diag_events_run_seq ON diag_events(run_id, seq);
CREATE INDEX IF NOT EXISTS idx_diag_events_tenant_created ON diag_events(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_diag_events_event_type ON diag_events(event_type);
