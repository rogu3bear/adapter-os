-- Migration 0012: Extend system_metrics table with additional observability fields

ALTER TABLE system_metrics
    ADD COLUMN disk_usage_percent REAL NOT NULL DEFAULT 0.0;

ALTER TABLE system_metrics
    ADD COLUMN network_rx_packets INTEGER NOT NULL DEFAULT 0;

ALTER TABLE system_metrics
    ADD COLUMN network_tx_packets INTEGER NOT NULL DEFAULT 0;

ALTER TABLE system_metrics
    ADD COLUMN network_bandwidth_mbps REAL NOT NULL DEFAULT 0.0;

ALTER TABLE system_metrics
    ADD COLUMN gpu_memory_total INTEGER;
