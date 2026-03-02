-- Canonical runtime tables required by DB/API read paths (Postgres parity).
-- Adds telemetry_events and request_log with contract columns.

CREATE TABLE IF NOT EXISTS telemetry_events (
    id TEXT PRIMARY KEY,
    worker_id TEXT,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    event_data TEXT NOT NULL DEFAULT '{}',
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    source TEXT,
    user_id TEXT,
    session_id TEXT,
    metadata TEXT,
    tags TEXT,
    priority TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_telemetry_events_tenant_timestamp
    ON telemetry_events (tenant_id, timestamp DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_telemetry_events_tenant_event_type_timestamp
    ON telemetry_events (tenant_id, event_type, timestamp DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_telemetry_events_worker_event_type_timestamp
    ON telemetry_events (worker_id, event_type, timestamp DESC)
    WHERE worker_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS request_log (
    id TEXT PRIMARY KEY,
    tenant_id TEXT,
    request_id TEXT,
    method TEXT,
    path TEXT,
    route TEXT,
    status TEXT NOT NULL DEFAULT 'completed',
    status_code INTEGER,
    latency_ms INTEGER,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_request_log_timestamp
    ON request_log (timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_request_log_tenant_timestamp
    ON request_log (tenant_id, timestamp DESC)
    WHERE tenant_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_request_log_status
    ON request_log (status);

CREATE INDEX IF NOT EXISTS idx_request_log_status_code
    ON request_log (status_code)
    WHERE status_code IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_request_log_latency_ms
    ON request_log (latency_ms DESC)
    WHERE latency_ms IS NOT NULL;
