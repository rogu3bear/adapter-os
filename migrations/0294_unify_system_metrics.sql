-- Migration 0294: Unify System Metrics Schema
--
-- Adds missing columns to `system_metrics` that were present in `adapteros-system-metrics` 
-- private schema but missing from the main database schema.
-- This allows `adapteros-system-metrics` to fully switch to using `adapteros-db`.
--
-- Missing columns identified:
-- - network_rx_packets (INTEGER)
-- - network_tx_packets (INTEGER)

ALTER TABLE system_metrics ADD COLUMN network_rx_packets INTEGER DEFAULT 0;
ALTER TABLE system_metrics ADD COLUMN network_tx_packets INTEGER DEFAULT 0;

-- Ensure indexes exist for these new columns if they are queried frequently
CREATE INDEX IF NOT EXISTS idx_system_metrics_network_packets ON system_metrics(network_rx_packets, network_tx_packets);
