-- Migration: 0197
-- Purpose: Add prefix KV cache support with config-based templates and receipt fields
-- PRD: PrefixKvCache v1

-- Prefix templates table for config-based prefix detection
CREATE TABLE IF NOT EXISTS prefix_templates (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    mode TEXT NOT NULL,                          -- 'system', 'user', 'builder', 'audit', or custom
    template_text TEXT NOT NULL,                 -- The actual prefix text to cache
    template_hash_b3 TEXT NOT NULL,              -- BLAKE3 hash of template_text
    priority INTEGER NOT NULL DEFAULT 0,         -- Higher priority templates matched first
    enabled BOOLEAN NOT NULL DEFAULT TRUE,       -- disabled or enabled
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for efficient lookup by tenant + mode
CREATE INDEX IF NOT EXISTS idx_prefix_templates_tenant_mode
    ON prefix_templates (tenant_id, mode)
    WHERE enabled = TRUE;

-- Index for cache invalidation on template changes
CREATE INDEX IF NOT EXISTS idx_prefix_templates_hash
    ON prefix_templates (template_hash_b3);

-- Receipt prefix KV cache fields
ALTER TABLE inference_trace_receipts
    ADD COLUMN IF NOT EXISTS prefix_kv_key_b3 TEXT,
    ADD COLUMN IF NOT EXISTS prefix_cache_hit BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS prefix_kv_bytes BIGINT NOT NULL DEFAULT 0;

-- Index for prefix cache hit analysis
CREATE INDEX IF NOT EXISTS idx_inference_trace_receipts_prefix_hit
    ON inference_trace_receipts (prefix_cache_hit, prefix_kv_key_b3)
    WHERE prefix_kv_key_b3 IS NOT NULL;

-- Index for prefix KV key lookups (for cache stats)
CREATE INDEX IF NOT EXISTS idx_inference_trace_receipts_prefix_key
    ON inference_trace_receipts (prefix_kv_key_b3)
    WHERE prefix_kv_key_b3 IS NOT NULL;
