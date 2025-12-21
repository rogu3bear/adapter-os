-- Rollback Migration 0064: Adapter Stacks
-- Purpose: Reverse the creation of adapter_stacks table and its supporting structures
-- Author: Migration Rollback System
-- Date: 2025-11-19
--
-- Dependencies to handle:
-- - routing_decisions table (migration 0070) references adapter_stacks via foreign key
-- - This table may be referenced in other migrations after 0064
-- - Triggers validate stack naming format
-- - Indexes optimize stack lookups

-- Step 1: Drop the trigger that validates stack names
DROP TRIGGER IF EXISTS validate_stack_name_format;

-- Step 2: Drop indexes (they will be recreated if table is recreated)
DROP INDEX IF EXISTS idx_adapter_stacks_created_at;
DROP INDEX IF EXISTS idx_adapter_stacks_name;

-- Step 3: Drop the table
-- Note: Foreign key constraint from routing_decisions.stack_id will cascade to SET NULL
DROP TABLE IF EXISTS adapter_stacks;
