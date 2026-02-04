-- Add stable_id column to adapters table for deterministic tie-breaking.
--
-- PROBLEM: All adapters currently have stable_id=0 (hardcoded in AdapterInfo constructors),
-- which breaks deterministic routing when multiple adapters have identical scores.
-- The router uses stable_id for tie-breaking (score DESC, stable_id ASC), so without
-- unique stable_ids, routing decisions become non-deterministic.
--
-- SOLUTION: Add a monotonic stable_id column per tenant. New adapters get the next
-- sequential value at registration time. Existing adapters are backfilled based on
-- creation order (created_at) within each tenant.
--
-- See: crates/adapteros-lora-router/src/types.rs (AdapterInfo.stable_id documentation)

-- Add stable_id column (nullable initially for backfill)
ALTER TABLE adapters ADD COLUMN stable_id INTEGER;

-- Backfill existing adapters with monotonic IDs per tenant based on creation order.
-- Uses ROW_NUMBER() partitioned by tenant_id, ordered by created_at.
-- This ensures deterministic assignment that preserves registration order.
UPDATE adapters
SET stable_id = (
    SELECT rn FROM (
        SELECT id, ROW_NUMBER() OVER (
            PARTITION BY tenant_id
            ORDER BY created_at ASC, id ASC
        ) as rn
        FROM adapters
    ) ranked
    WHERE ranked.id = adapters.id
);

-- Create index for efficient next-stable-id queries during registration.
-- The index on (tenant_id, stable_id DESC) allows fast MAX(stable_id) lookups.
CREATE INDEX IF NOT EXISTS idx_adapters_tenant_stable_id
    ON adapters(tenant_id, stable_id DESC);
