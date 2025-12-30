-- Rollback Migration 0253: Lifecycle Audit Enrichment
-- Purpose: Remove enrichment columns from adapter_lifecycle_history
-- Date: 2025-12-29
--
-- Dependencies to handle:
-- - Indexes on automation_source and policy_rule_id
-- - policy_rule_id references lifecycle_rules(id) (no formal FK constraint)
-- - No other tables reference these columns
--
-- WARNING: This will lose automation source, policy linkage, precondition,
-- and context data for all lifecycle history records.
-- Backup data before executing if needed.

-- Step 1: Drop indexes first
DROP INDEX IF EXISTS idx_alh_policy_rule;
DROP INDEX IF EXISTS idx_alh_automation;

-- Step 2: Drop the columns
-- Note: Requires SQLite 3.35.0+ for DROP COLUMN support
ALTER TABLE adapter_lifecycle_history DROP COLUMN context_json;
ALTER TABLE adapter_lifecycle_history DROP COLUMN preconditions_json;
ALTER TABLE adapter_lifecycle_history DROP COLUMN policy_rule_id;
ALTER TABLE adapter_lifecycle_history DROP COLUMN automation_source;
