-- Migration: 0218
-- Purpose: Accelerate tenant-scoped stack lookups used during inference routing.

CREATE INDEX IF NOT EXISTS idx_adapter_stacks_tenant_name_active
    ON adapter_stacks(tenant_id, name, lifecycle_state)
    WHERE lifecycle_state = 'active';

ANALYZE adapter_stacks;
