-- Migration: Adapter Performance Tracking
-- Purpose: Track adapter performance metrics for routing optimization and monitoring
-- Policy Compliance: Telemetry Ruleset (#5) - Canonical structured events
-- Created: 2025-11-22

-- Adapter performance metrics table - tracks inference latency and throughput
CREATE TABLE IF NOT EXISTS adapter_performance_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    adapter_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,

    -- Request context
    request_id TEXT,
    session_id TEXT,

    -- Latency metrics (microseconds for precision)
    inference_latency_us INTEGER NOT NULL,
    preprocessing_latency_us INTEGER,
    postprocessing_latency_us INTEGER,
    total_latency_us INTEGER NOT NULL,

    -- Throughput metrics
    tokens_processed INTEGER,
    tokens_per_second REAL,

    -- Memory metrics
    memory_used_bytes INTEGER,
    peak_memory_bytes INTEGER,

    -- Quality indicators
    gate_score REAL, -- Router gate value when selected
    confidence REAL, -- Output confidence if available

    -- Metadata
    backend_type TEXT, -- 'coreml', 'mlx', 'metal'
    batch_size INTEGER DEFAULT 1,
    recorded_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for performance queries
CREATE INDEX IF NOT EXISTS idx_adapter_perf_adapter_id ON adapter_performance_metrics(adapter_id);
CREATE INDEX IF NOT EXISTS idx_adapter_perf_tenant_id ON adapter_performance_metrics(tenant_id);
CREATE INDEX IF NOT EXISTS idx_adapter_perf_recorded_at ON adapter_performance_metrics(recorded_at DESC);
CREATE INDEX IF NOT EXISTS idx_adapter_perf_request_id ON adapter_performance_metrics(request_id) WHERE request_id IS NOT NULL;

-- Composite index for adapter performance analysis
CREATE INDEX IF NOT EXISTS idx_adapter_perf_analysis
    ON adapter_performance_metrics(adapter_id, recorded_at DESC, inference_latency_us);

-- Aggregated performance summary table - updated periodically for dashboard queries
CREATE TABLE IF NOT EXISTS adapter_performance_summary (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    adapter_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,

    -- Time window
    window_start TIMESTAMP NOT NULL,
    window_end TIMESTAMP NOT NULL,
    window_duration_seconds INTEGER NOT NULL,

    -- Aggregated latency stats
    request_count INTEGER NOT NULL DEFAULT 0,
    avg_latency_us REAL,
    p50_latency_us INTEGER,
    p95_latency_us INTEGER,
    p99_latency_us INTEGER,
    min_latency_us INTEGER,
    max_latency_us INTEGER,

    -- Throughput stats
    total_tokens_processed INTEGER DEFAULT 0,
    avg_tokens_per_second REAL,

    -- Memory stats
    avg_memory_used_bytes INTEGER,
    max_memory_used_bytes INTEGER,

    -- Quality stats
    avg_gate_score REAL,
    avg_confidence REAL,

    -- Error tracking
    error_count INTEGER DEFAULT 0,
    error_rate REAL, -- error_count / request_count

    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    UNIQUE(adapter_id, window_start, window_end)
);

-- Indexes for summary queries
CREATE INDEX IF NOT EXISTS idx_adapter_summary_adapter_id ON adapter_performance_summary(adapter_id);
CREATE INDEX IF NOT EXISTS idx_adapter_summary_window ON adapter_performance_summary(window_start DESC, window_end DESC);
CREATE INDEX IF NOT EXISTS idx_adapter_summary_tenant ON adapter_performance_summary(tenant_id, window_start DESC);

-- View for latest adapter performance (last 5 minutes)
CREATE VIEW IF NOT EXISTS adapter_recent_performance AS
SELECT
    adapter_id,
    tenant_id,
    COUNT(*) as request_count,
    AVG(inference_latency_us) as avg_latency_us,
    MIN(inference_latency_us) as min_latency_us,
    MAX(inference_latency_us) as max_latency_us,
    SUM(tokens_processed) as total_tokens,
    AVG(tokens_per_second) as avg_throughput,
    AVG(gate_score) as avg_gate_score,
    MAX(recorded_at) as last_request_at
FROM adapter_performance_metrics
WHERE recorded_at > datetime('now', '-5 minutes')
GROUP BY adapter_id, tenant_id;

-- View for adapter performance trends (hourly)
CREATE VIEW IF NOT EXISTS adapter_hourly_performance AS
SELECT
    adapter_id,
    tenant_id,
    strftime('%Y-%m-%d %H:00:00', recorded_at) as hour,
    COUNT(*) as request_count,
    AVG(inference_latency_us) as avg_latency_us,
    AVG(tokens_per_second) as avg_throughput
FROM adapter_performance_metrics
WHERE recorded_at > datetime('now', '-24 hours')
GROUP BY adapter_id, tenant_id, strftime('%Y-%m-%d %H:00:00', recorded_at)
ORDER BY hour DESC;
