-- Migration 0011: System Metrics Storage
-- Adds tables for storing system metrics data with proper indexing

CREATE TABLE system_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    cpu_usage REAL NOT NULL,
    memory_usage REAL NOT NULL,
    disk_read_bytes INTEGER NOT NULL,
    disk_write_bytes INTEGER NOT NULL,
    network_rx_bytes INTEGER NOT NULL,
    network_tx_bytes INTEGER NOT NULL,
    gpu_utilization REAL,
    gpu_memory_used INTEGER,
    uptime_seconds INTEGER NOT NULL,
    process_count INTEGER NOT NULL,
    load_1min REAL NOT NULL,
    load_5min REAL NOT NULL,
    load_15min REAL NOT NULL,
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);

-- Indexes for efficient querying
CREATE INDEX idx_system_metrics_timestamp ON system_metrics(timestamp);
CREATE INDEX idx_system_metrics_cpu ON system_metrics(cpu_usage);
CREATE INDEX idx_system_metrics_memory ON system_metrics(memory_usage);
CREATE INDEX idx_system_metrics_disk ON system_metrics(disk_read_bytes, disk_write_bytes);
CREATE INDEX idx_system_metrics_network ON system_metrics(network_rx_bytes, network_tx_bytes);
CREATE INDEX idx_system_metrics_gpu ON system_metrics(gpu_utilization);
CREATE INDEX idx_system_metrics_created_at ON system_metrics(created_at);

-- System health check results
CREATE TABLE system_health_checks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('healthy', 'warning', 'critical')),
    check_name TEXT NOT NULL,
    check_status TEXT NOT NULL CHECK (check_status IN ('healthy', 'warning', 'critical')),
    message TEXT NOT NULL,
    value REAL,
    threshold REAL,
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);

-- Indexes for health checks
CREATE INDEX idx_health_checks_timestamp ON system_health_checks(timestamp);
CREATE INDEX idx_health_checks_status ON system_health_checks(status);
CREATE INDEX idx_health_checks_name ON system_health_checks(check_name);

-- Threshold violations
CREATE TABLE threshold_violations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    metric_name TEXT NOT NULL,
    current_value REAL NOT NULL,
    threshold_value REAL NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('warning', 'critical')),
    resolved_at INTEGER,
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);

-- Indexes for violations
CREATE INDEX idx_violations_timestamp ON threshold_violations(timestamp);
CREATE INDEX idx_violations_metric ON threshold_violations(metric_name);
CREATE INDEX idx_violations_severity ON threshold_violations(severity);
CREATE INDEX idx_violations_resolved ON threshold_violations(resolved_at);

-- Metrics aggregation cache for performance
CREATE TABLE metrics_aggregations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    window_start INTEGER NOT NULL,
    window_end INTEGER NOT NULL,
    window_type TEXT NOT NULL CHECK (window_type IN ('hour', 'day', 'week')),
    avg_cpu_usage REAL NOT NULL,
    max_cpu_usage REAL NOT NULL,
    avg_memory_usage REAL NOT NULL,
    max_memory_usage REAL NOT NULL,
    total_disk_read INTEGER NOT NULL,
    total_disk_write INTEGER NOT NULL,
    total_network_rx INTEGER NOT NULL,
    total_network_tx INTEGER NOT NULL,
    sample_count INTEGER NOT NULL,
    created_at INTEGER DEFAULT (strftime('%s', 'now')),
    UNIQUE(window_start, window_end, window_type)
);

-- Indexes for aggregations
CREATE INDEX idx_aggregations_window ON metrics_aggregations(window_start, window_end);
CREATE INDEX idx_aggregations_type ON metrics_aggregations(window_type);
CREATE INDEX idx_aggregations_created_at ON metrics_aggregations(created_at);

-- Configuration for system metrics collection
CREATE TABLE system_metrics_config (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    config_key TEXT NOT NULL UNIQUE,
    config_value TEXT NOT NULL,
    updated_at INTEGER DEFAULT (strftime('%s', 'now'))
);

-- Insert default configuration
INSERT INTO system_metrics_config (config_key, config_value) VALUES
    ('collection_interval_secs', '30'),
    ('sampling_rate', '0.05'),
    ('enable_gpu_metrics', 'true'),
    ('enable_disk_metrics', 'true'),
    ('enable_network_metrics', 'true'),
    ('retention_days', '30'),
    ('cpu_warning_threshold', '70.0'),
    ('cpu_critical_threshold', '90.0'),
    ('memory_warning_threshold', '80.0'),
    ('memory_critical_threshold', '95.0'),
    ('disk_warning_threshold', '85.0'),
    ('disk_critical_threshold', '95.0'),
    ('gpu_warning_threshold', '80.0'),
    ('gpu_critical_threshold', '95.0'),
    ('min_memory_headroom', '15.0');

-- Index for config
CREATE INDEX idx_config_key ON system_metrics_config(config_key);
