-- Add missing columns to system_metrics table for SQLite
--
-- Adds columns that exist in SystemMetricsRecord but were missing from initial schema

ALTER TABLE system_metrics ADD COLUMN disk_usage_percent REAL DEFAULT 0.0;
ALTER TABLE system_metrics ADD COLUMN network_rx_packets INTEGER DEFAULT 0;
ALTER TABLE system_metrics ADD COLUMN network_tx_packets INTEGER DEFAULT 0;
ALTER TABLE system_metrics ADD COLUMN network_bandwidth_mbps REAL DEFAULT 0.0;
ALTER TABLE system_metrics ADD COLUMN gpu_memory_total INTEGER;
