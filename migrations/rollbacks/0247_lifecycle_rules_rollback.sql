-- Rollback for 0247_lifecycle_rules.sql
-- WARNING: This will delete all data in the affected tables

-- Drop indexes first
DROP INDEX IF EXISTS idx_lifecycle_rules_active_by_type;
DROP INDEX IF EXISTS idx_lifecycle_rules_priority;
DROP INDEX IF EXISTS idx_lifecycle_rules_enabled;
DROP INDEX IF EXISTS idx_lifecycle_rules_rule_type;
DROP INDEX IF EXISTS idx_lifecycle_rules_scope_target;
DROP INDEX IF EXISTS idx_lifecycle_rules_scope;

-- Drop the table
DROP TABLE IF EXISTS lifecycle_rules;
