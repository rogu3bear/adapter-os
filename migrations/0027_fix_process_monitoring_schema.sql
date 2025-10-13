-- Fix process monitoring schema mismatches
-- Adds missing columns and fixes naming inconsistencies
-- Citation: Based on crates/adapteros-db/src/process_monitoring.rs analysis

-- All required columns already exist from migration 25 - no action needed

-- Column already renamed in migration 25 - no action needed
-- Citation: crates/adapteros-db/src/process_monitoring.rs L552 expects 'collected_at'

-- notification_channels and escalation_rules already exist from migration 25 - no action needed

-- Indexes already updated in migration 25 - no action needed

-- Update view to use correct column name
DROP VIEW IF EXISTS recent_health_metrics;
CREATE VIEW IF NOT EXISTS recent_health_metrics AS
SELECT 
    id,
    worker_id,
    tenant_id,
    metric_name,
    metric_value,
    metric_unit,
    tags,
    collected_at
FROM process_health_metrics
WHERE collected_at >= datetime('now', '-24 hours')
ORDER BY collected_at DESC;
