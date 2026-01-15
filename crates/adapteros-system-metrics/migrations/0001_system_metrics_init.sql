-- System Metrics Schema
-- 
-- Provides storage for system resource monitoring, health checks, and threshold violations
-- Aligns with adapterOS deterministic state tracking requirements

CREATE TABLE IF NOT EXISTS system_metrics (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp           INTEGER NOT NULL,
    cpu_usage           REAL NOT NULL,
    memory_usage        REAL NOT NULL,
    disk_read_bytes     INTEGER NOT NULL,
    disk_write_bytes    INTEGER NOT NULL,
    network_rx_bytes    INTEGER NOT NULL,
    network_tx_bytes    INTEGER NOT NULL,
    gpu_utilization     REAL,
    gpu_memory_used     INTEGER,
    uptime_seconds      INTEGER,
    process_count       INTEGER,
    load_1min           REAL,
    load_5min           REAL,
    load_15min          REAL,
    created_at          INTEGER DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_system_metrics_timestamp ON system_metrics(timestamp);

CREATE TABLE IF NOT EXISTS system_health_checks (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp       INTEGER NOT NULL,
    status          TEXT NOT NULL,  -- overall status
    check_name      TEXT NOT NULL,
    check_status    TEXT NOT NULL,  -- individual check status
    message         TEXT,
    value           REAL,
    threshold       REAL,
    created_at      INTEGER DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_health_checks_timestamp ON system_health_checks(timestamp);

CREATE TABLE IF NOT EXISTS threshold_violations (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp           INTEGER NOT NULL,
    metric_name         TEXT NOT NULL,
    current_value       REAL NOT NULL,
    threshold_value     REAL NOT NULL,
    severity            TEXT NOT NULL,
    resolved_at         INTEGER,
    created_at          INTEGER DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_threshold_violations_timestamp ON threshold_violations(timestamp);
CREATE INDEX IF NOT EXISTS idx_threshold_violations_resolved ON threshold_violations(resolved_at);

CREATE TABLE IF NOT EXISTS metrics_aggregations (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    window_start            INTEGER NOT NULL,
    window_end              INTEGER NOT NULL,
    window_type             TEXT NOT NULL,  -- hourly, daily, etc.
    avg_cpu_usage           REAL,
    max_cpu_usage           REAL,
    avg_memory_usage        REAL,
    max_memory_usage        REAL,
    total_disk_read         INTEGER,
    total_disk_write        INTEGER,
    total_network_rx        INTEGER,
    total_network_tx        INTEGER,
    sample_count            INTEGER NOT NULL,
    created_at              INTEGER DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_aggregations_window ON metrics_aggregations(window_start, window_type);

CREATE TABLE IF NOT EXISTS system_metrics_config (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    config_key      TEXT UNIQUE NOT NULL,
    config_value    TEXT NOT NULL,
    updated_at      INTEGER NOT NULL,
    created_at      INTEGER DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_config_key ON system_metrics_config(config_key);

