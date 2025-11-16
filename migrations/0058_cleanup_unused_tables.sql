-- Cleanup migration: Remove unused tables from migrations 0025, 0026, 0036
-- Citation: Multi-agent schema audit - Agent B findings
-- These tables were created but have zero code integration
-- Priority: LOW (cleanup only, no runtime impact)

-- ============================================================================
-- Migration 0025: Process Monitoring Tables (UNUSED)
-- ============================================================================
-- These 10 tables were created for process monitoring but have no INSERT/UPDATE
-- queries in the codebase. Struct definitions exist but no active usage.

-- Drop views that depend on these tables first
DROP VIEW IF EXISTS recent_health_metrics;

DROP TABLE IF EXISTS process_monitoring_reports;
DROP TABLE IF EXISTS process_monitoring_schedules;
DROP TABLE IF EXISTS process_monitoring_notifications;
DROP TABLE IF EXISTS process_monitoring_widgets;
DROP TABLE IF EXISTS process_monitoring_dashboards;
DROP TABLE IF EXISTS process_performance_baselines;
DROP TABLE IF EXISTS process_monitoring_rules;
DROP TABLE IF EXISTS process_health_metrics;
DROP TABLE IF EXISTS process_alerts;

-- ============================================================================
-- Migration 0026: Evidence Tracking Tables (UNUSED)
-- ============================================================================
-- These tables have zero Rust code references

DROP TABLE IF EXISTS evidence_file_tracking;
DROP TABLE IF EXISTS evidence_indices;

-- ============================================================================
-- Migration 0036: Code Intelligence Tables (UNUSED)
-- ============================================================================
-- No active code integration found

DROP TABLE IF EXISTS scan_jobs;
DROP TABLE IF EXISTS code_graph_metadata;

-- ============================================================================
-- Verification
-- ============================================================================
-- After applying this migration, verify that the following tables are still active:
-- - tick_ledger_entries (migration 0032) ✓
-- - training_datasets, dataset_files, dataset_statistics (migration 0041) ✓
-- - policy_evidence (migration 0046) ✓
-- - domain_adapters, domain_adapter_executions, domain_adapter_tests (migration 0047/0057) ✓
-- - progress_events (migration 0052) ✓

-- Note: This migration is idempotent - running it multiple times is safe
