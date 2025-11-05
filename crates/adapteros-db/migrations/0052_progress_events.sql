-- Progress Events Table
--
-- Stores historical progress tracking data for long-running operations.
-- Supports querying by tenant, operation type, status, and time ranges.
-- Evidence: Progress tracking APIs require persistent storage for historical data

CREATE TABLE progress_events (
    id TEXT PRIMARY KEY NOT NULL,
    operation_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    event_type TEXT NOT NULL, -- 'operation:load', 'training:train', etc.
    progress_pct REAL NOT NULL DEFAULT 0.0 CHECK(progress_pct >= 0.0 AND progress_pct <= 100.0),
    status TEXT NOT NULL CHECK(status IN ('running', 'completed', 'failed', 'cancelled')),
    message TEXT,
    metadata TEXT, -- JSON string for additional context
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes for efficient querying
CREATE INDEX idx_progress_events_tenant_id ON progress_events(tenant_id);
CREATE INDEX idx_progress_events_operation_id ON progress_events(operation_id);
CREATE INDEX idx_progress_events_event_type ON progress_events(event_type);
CREATE INDEX idx_progress_events_status ON progress_events(status);
CREATE INDEX idx_progress_events_created_at ON progress_events(created_at);
CREATE INDEX idx_progress_events_updated_at ON progress_events(updated_at);

-- Composite indexes for common query patterns
CREATE INDEX idx_progress_events_tenant_status ON progress_events(tenant_id, status);
CREATE INDEX idx_progress_events_tenant_created ON progress_events(tenant_id, created_at DESC);
CREATE INDEX idx_progress_events_operation_updated ON progress_events(operation_id, updated_at DESC);

-- Partial index for active operations (faster counts)
CREATE INDEX idx_progress_events_active ON progress_events(status) WHERE status = 'running';
