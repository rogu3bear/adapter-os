-- Rollback for Migration 0220: Convert idx_adapters_tenant_hash_active to a covering index

DROP INDEX IF EXISTS idx_adapters_tenant_hash_active;

-- Restore original definition from migration 0210
CREATE INDEX IF NOT EXISTS idx_adapters_tenant_hash_active
    ON adapters(tenant_id, hash_b3, active)
    WHERE active = 1;

ANALYZE adapters;
