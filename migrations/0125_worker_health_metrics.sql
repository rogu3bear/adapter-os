-- Worker Health, Hung Detection & Log Centralization
-- Adds health tracking columns to workers table and creates worker_incidents table

-- Add health status tracking columns to workers table
ALTER TABLE workers ADD COLUMN health_status TEXT DEFAULT 'unknown'
  CHECK(health_status IN ('healthy', 'degraded', 'crashed', 'unknown'));

ALTER TABLE workers ADD COLUMN avg_latency_ms REAL;
ALTER TABLE workers ADD COLUMN latency_samples INTEGER DEFAULT 0;
ALTER TABLE workers ADD COLUMN last_response_at TEXT;
ALTER TABLE workers ADD COLUMN consecutive_slow_responses INTEGER DEFAULT 0;
ALTER TABLE workers ADD COLUMN consecutive_failures INTEGER DEFAULT 0;

-- Worker incidents table for structured error channel and incident tracking
CREATE TABLE IF NOT EXISTS worker_incidents (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    incident_type TEXT NOT NULL CHECK(incident_type IN ('fatal', 'crash', 'hung', 'degraded', 'recovered')),
    reason TEXT NOT NULL,
    backtrace_snippet TEXT,
    latency_at_incident_ms REAL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Index for querying incidents by worker (most common query pattern)
CREATE INDEX IF NOT EXISTS idx_worker_incidents_worker ON worker_incidents(worker_id, created_at DESC);

-- Index for filtering by incident type (for dashboards/monitoring)
CREATE INDEX IF NOT EXISTS idx_worker_incidents_type ON worker_incidents(incident_type);

-- Index for tenant-scoped queries
CREATE INDEX IF NOT EXISTS idx_worker_incidents_tenant ON worker_incidents(tenant_id, created_at DESC);

-- Index on workers health_status for routing queries
CREATE INDEX IF NOT EXISTS idx_workers_health_status ON workers(health_status);
