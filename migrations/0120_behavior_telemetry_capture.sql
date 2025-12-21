-- Behavior telemetry capture for training data generation
-- Stores structured adapter lifecycle events for export to training datasets

-- Create behavior_events table for capturing lifecycle transitions
CREATE TABLE IF NOT EXISTS behavior_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,  -- 'promoted', 'evicted', 'pinned', 'demoted', 'recovered', 'ttl_expired'
    adapter_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    from_state TEXT,           -- load_state before transition
    to_state TEXT,             -- load_state after transition
    activation_pct REAL,       -- Activation percentage at time of event (0.0-1.0)
    memory_mb INTEGER,         -- Memory usage in MB
    reason TEXT NOT NULL,      -- Reason for transition
    created_at TEXT DEFAULT (datetime('now')),
    metadata TEXT              -- JSON string for additional context
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_behavior_events_adapter_id ON behavior_events(adapter_id);
CREATE INDEX IF NOT EXISTS idx_behavior_events_tenant_id ON behavior_events(tenant_id);
CREATE INDEX IF NOT EXISTS idx_behavior_events_event_type ON behavior_events(event_type);
CREATE INDEX IF NOT EXISTS idx_behavior_events_created_at ON behavior_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_behavior_events_from_to_state ON behavior_events(from_state, to_state);

-- View for recent promotion events (last 30 days)
CREATE VIEW IF NOT EXISTS recent_promotions AS
SELECT 
    id, adapter_id, tenant_id, from_state, to_state, activation_pct, 
    memory_mb, reason, created_at,
    (strftime('%s', 'now') - strftime('%s', created_at)) AS age_seconds
FROM behavior_events 
WHERE event_type = 'promoted'
  AND created_at > datetime('now', '-30 days')
ORDER BY created_at DESC;

-- View for eviction patterns
CREATE VIEW IF NOT EXISTS eviction_patterns AS
SELECT 
    from_state, to_state, reason, COUNT(*) as event_count,
    AVG(activation_pct) as avg_activation,
    AVG(memory_mb) as avg_memory,
    AVG((strftime('%s', 'now') - strftime('%s', created_at))) as avg_age_days
FROM behavior_events 
WHERE event_type = 'evicted'
GROUP BY from_state, to_state, reason
ORDER BY event_count DESC;
