-- Add tenant_id to adapter_stacks table
-- Migration: 0080
-- Created: 2025-11-19
-- Purpose: Enable tenant-scoped adapter stacks

ALTER TABLE adapter_stacks ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';

UPDATE adapter_stacks SET tenant_id = 'default' WHERE tenant_id IS NULL;

CREATE INDEX idx_adapter_stacks_tenant ON adapter_stacks(tenant_id);

-- Composite index for tenant + lifecycle queries on stacks
-- (moved from migration 0068 where tenant_id didn't exist yet)
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_tenant_lifecycle
    ON adapter_stacks(tenant_id, lifecycle_state);
