-- Rollback Migration 0070: Routing Decisions Table for Router & Stack Visibility
-- Purpose: Reverse the creation of routing_decisions table and its supporting structures
-- Author: Migration Rollback System
-- Date: 2025-11-19
--
-- Dependencies to handle:
-- - Views depend on routing_decisions table
-- - Indexes optimize query patterns
-- - Foreign keys reference tenants and adapter_stacks tables

-- Step 1: Drop dependent views first
DROP VIEW IF EXISTS routing_decisions_low_entropy;
DROP VIEW IF EXISTS routing_decisions_high_overhead;
DROP VIEW IF EXISTS routing_decisions_enriched;

-- Step 2: Drop indexes (they will be recreated if table is recreated)
DROP INDEX IF EXISTS idx_routing_decisions_timestamp;
DROP INDEX IF EXISTS idx_routing_decisions_request_id;
DROP INDEX IF EXISTS idx_routing_decisions_stack_id;
DROP INDEX IF EXISTS idx_routing_decisions_tenant_timestamp;

-- Step 3: Drop the main table
-- Foreign key constraints reference:
-- - tenants(id) ON DELETE CASCADE
-- - adapter_stacks(id) ON DELETE SET NULL
DROP TABLE IF EXISTS routing_decisions;
