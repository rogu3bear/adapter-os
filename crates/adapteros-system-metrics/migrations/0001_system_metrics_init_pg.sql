-- System Metrics Schema for PostgreSQL
--
-- Provides storage for system resource monitoring, health checks, and threshold violations
-- Aligns with AdapterOS deterministic state tracking requirements

CREATE TABLE IF NOT EXISTS system_metrics (
    id                  SERIAL PRIMARY KEY,
    timestamp           BIGINT NOT NULL,
    cpu_usage           DOUBLE PRECISION NOT NULL,
    memory_usage        DOUBLE PRECISION NOT NULL,
    disk_read_bytes     BIGINT NOT NULL,
    disk_write_bytes    BIGINT NOT NULL,
    network_rx_bytes    BIGINT NOT NULL,
    network_tx_bytes    BIGINT NOT NULL,
    gpu_utilization     DOUBLE PRECISION,
    gpu_memory_used     BIGINT,
    uptime_seconds      BIGINT,
    process_count       INTEGER,
    load_1min           DOUBLE PRECISION,
    load_5min           DOUBLE PRECISION,
    load_15min          DOUBLE PRECISION,
    created_at          TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_system_metrics_timestamp ON system_metrics(timestamp);

CREATE TABLE IF NOT EXISTS system_health_checks (
    id              SERIAL PRIMARY KEY,
    timestamp       BIGINT NOT NULL,
    status          TEXT NOT NULL,  -- overall status
    check_name      TEXT NOT NULL,
    check_status    TEXT NOT NULL,  -- individual check status
    message         TEXT,
    value           DOUBLE PRECISION,
    threshold       DOUBLE PRECISION,
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_health_checks_timestamp ON system_health_checks(timestamp);

CREATE TABLE IF NOT EXISTS threshold_violations (
    id                  SERIAL PRIMARY KEY,
    timestamp           BIGINT NOT NULL,
    metric_name         TEXT NOT NULL,
    current_value       DOUBLE PRECISION NOT NULL,
    threshold_value     DOUBLE PRECISION NOT NULL,
    severity            TEXT NOT NULL,
    resolved_at         BIGINT,
    created_at          TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_threshold_violations_timestamp ON threshold_violations(timestamp);
CREATE INDEX IF NOT EXISTS idx_threshold_violations_resolved ON threshold_violations(resolved_at) WHERE resolved_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS metrics_aggregations (
    id                      SERIAL PRIMARY KEY,
    window_start            BIGINT NOT NULL,
    window_end              BIGINT NOT NULL,
    window_type             TEXT NOT NULL,  -- hourly, daily, etc.
    avg_cpu_usage           DOUBLE PRECISION,
    max_cpu_usage           DOUBLE PRECISION,
    avg_memory_usage        DOUBLE PRECISION,
    max_memory_usage        DOUBLE PRECISION,
    total_disk_read         BIGINT,
    total_disk_write        BIGINT,
    total_network_rx        BIGINT,
    total_network_tx        BIGINT,
    sample_count            INTEGER NOT NULL,
    created_at              TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_aggregations_window ON metrics_aggregations(window_start, window_type);

CREATE TABLE IF NOT EXISTS system_metrics_config (
    id              SERIAL PRIMARY KEY,
    config_key      TEXT UNIQUE NOT NULL,
    config_value    TEXT NOT NULL,
    updated_at      BIGINT NOT NULL,
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_config_key ON system_metrics_config(config_key);
