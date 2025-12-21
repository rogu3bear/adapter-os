-- Tenant-Scoped Routing Decisions Analytics Optimization
-- Migration: 0216
-- Purpose: Add composite index for tenant-specific router analytics and debugging queries
--
-- This index eliminates "USE TEMP B-TREE FOR ORDER BY" in EXPLAIN QUERY PLAN
-- and ensures routing decision lookups remain sub-millisecond during high-throughput
-- inference periods. It supports real-time router performance monitoring per tenant.
--
-- Query Patterns Supported:
-- 1. Router analytics: SELECT * FROM routing_decisions WHERE tenant_id = ? AND stack_id = ? ORDER BY timestamp DESC
-- 2. Stack debugging: SELECT * FROM routing_decisions WHERE tenant_id = ? ORDER BY timestamp DESC, stack_id
-- 3. Performance monitoring: SELECT stack_id, COUNT(*) FROM routing_decisions WHERE tenant_id = ? GROUP BY stack_id ORDER BY timestamp DESC

CREATE INDEX IF NOT EXISTS idx_routing_decisions_tenant_timestamp_stack
    ON routing_decisions(tenant_id, timestamp DESC, stack_id);

-- Update query planner statistics for routing_decisions table
ANALYZE routing_decisions;


