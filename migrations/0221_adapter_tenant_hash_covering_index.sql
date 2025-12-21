-- Convert idx_adapters_tenant_hash_active to a covering index
-- Migration: 0220
-- Purpose: Optimize adapter hash resolution by including frequently accessed columns
-- to eliminate bookmark lookups.

DROP INDEX IF EXISTS idx_adapters_tenant_hash_active;

CREATE INDEX IF NOT EXISTS idx_adapters_tenant_hash_active
    ON adapters(tenant_id, hash_b3, active, id, name, tier, lifecycle_state)
    WHERE active = 1;

ANALYZE adapters;
