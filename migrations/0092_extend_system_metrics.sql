-- Migration 0092: Extend System Metrics Table
-- Purpose: Add missing columns for time series metrics handler
-- System, Nodes, Workers, Memory, Metrics
-- Citation: Based on metrics_time_series.rs requirements and 0011_system_metrics.sql

-- Add missing columns to system_metrics table
ALTER TABLE system_metrics ADD COLUMN disk_usage_percent REAL DEFAULT 0.0;
ALTER TABLE system_metrics ADD COLUMN network_bandwidth_mbps REAL DEFAULT 0.0;
ALTER TABLE system_metrics ADD COLUMN gpu_memory_total INTEGER;

-- Update indexes for new columns
CREATE INDEX IF NOT EXISTS idx_system_metrics_disk_usage ON system_metrics(disk_usage_percent);
CREATE INDEX IF NOT EXISTS idx_system_metrics_network_bandwidth ON system_metrics(network_bandwidth_mbps);
CREATE INDEX IF NOT EXISTS idx_system_metrics_gpu_memory_total ON system_metrics(gpu_memory_total);
