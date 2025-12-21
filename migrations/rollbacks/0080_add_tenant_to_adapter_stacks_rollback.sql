-- Rollback for adding tenant_id to adapter_stacks (0080)
-- Migration: 0080_add_tenant_to_adapter_stacks_rollback.sql
-- Created: 2025-11-19
-- Purpose: Remove tenant_id column and related indexes

-- Remove the tenant index first
DROP INDEX IF EXISTS idx_adapter_stacks_tenant;

-- Remove the tenant_id column
-- Warning: This will lose tenant association data
ALTER TABLE adapter_stacks DROP COLUMN tenant_id;

