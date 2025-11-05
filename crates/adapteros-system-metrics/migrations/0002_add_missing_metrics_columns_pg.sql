-- Add missing columns to system_metrics table for PostgreSQL
--
-- Adds columns that exist in SystemMetricsRecord but were missing from initial schema

ALTER TABLE system_metrics ADD COLUMN IF NOT EXISTS disk_usage_percent DOUBLE PRECISION DEFAULT 0.0;
ALTER TABLE system_metrics ADD COLUMN IF NOT EXISTS network_rx_packets BIGINT DEFAULT 0;
ALTER TABLE system_metrics ADD COLUMN IF NOT EXISTS network_tx_packets BIGINT DEFAULT 0;
ALTER TABLE system_metrics ADD COLUMN IF NOT EXISTS network_bandwidth_mbps DOUBLE PRECISION DEFAULT 0.0;
ALTER TABLE system_metrics ADD COLUMN IF NOT EXISTS gpu_memory_total BIGINT;
