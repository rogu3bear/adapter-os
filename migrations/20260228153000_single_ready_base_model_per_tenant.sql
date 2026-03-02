-- Migration: Enforce single loaded base model per tenant.
-- Purpose:
--   1) Repair legacy multi-ready rows by demoting all but the most recent ready row.
--   2) Prevent future multi-loaded states via partial unique index.

-- Canonicalize remaining legacy aliases before enforcing uniqueness.
UPDATE base_model_status
SET status = 'ready'
WHERE status = 'loaded';

-- Keep at most one ready model per tenant (most recent wins).
WITH ranked_ready AS (
    SELECT
        id,
        ROW_NUMBER() OVER (
            PARTITION BY tenant_id
            ORDER BY
                COALESCE(updated_at, loaded_at, created_at, datetime('now')) DESC,
                id DESC
        ) AS rn
    FROM base_model_status
    WHERE status = 'ready'
)
UPDATE base_model_status
SET status = 'no-model',
    error_message = NULL,
    memory_usage_mb = NULL,
    unloaded_at = COALESCE(unloaded_at, datetime('now')),
    updated_at = datetime('now')
WHERE id IN (SELECT id FROM ranked_ready WHERE rn > 1);

-- Hard guard: a tenant can have only one loaded/ready model row.
CREATE UNIQUE INDEX IF NOT EXISTS idx_base_model_status_single_ready_per_tenant
    ON base_model_status(tenant_id)
    WHERE status IN ('ready', 'loaded');
