-- Migration 0141: Add composite index for policy audit chain verification
-- Purpose: Optimize tenant-scoped chain traversal and Merkle chain verification queries
-- PRD: PRD-06 (Policy audit Merkle chain)
-- Created: 2025-12-03

-- Composite index for efficient tenant + chain_sequence queries.
-- Supports: WHERE tenant_id = ? ORDER BY chain_sequence ASC/DESC
-- Without this index, chain verification queries do full table scans.
CREATE INDEX IF NOT EXISTS idx_pad_tenant_chain
ON policy_audit_decisions(tenant_id, chain_sequence);

-- Update query planner statistics after index creation
ANALYZE policy_audit_decisions;
