-- Migration: 0195
-- Purpose: Add KV quota and residency tracking fields for evidence/receipts
-- PRD: KvResidencyAndQuotas v1

-- Tenant KV quota configuration
ALTER TABLE tenants ADD COLUMN max_kv_cache_bytes INTEGER DEFAULT NULL;
ALTER TABLE tenants ADD COLUMN kv_residency_policy_id TEXT DEFAULT 'kv_residency_v1';

-- Receipt KV fields
ALTER TABLE inference_trace_receipts ADD COLUMN tenant_kv_quota_bytes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE inference_trace_receipts ADD COLUMN tenant_kv_bytes_used INTEGER NOT NULL DEFAULT 0;
ALTER TABLE inference_trace_receipts ADD COLUMN kv_evictions INTEGER NOT NULL DEFAULT 0;
ALTER TABLE inference_trace_receipts ADD COLUMN kv_residency_policy_id TEXT;
ALTER TABLE inference_trace_receipts ADD COLUMN kv_quota_enforced INTEGER NOT NULL DEFAULT 0;

-- Index for residency policy queries
CREATE INDEX IF NOT EXISTS idx_inference_trace_receipts_kv_policy
    ON inference_trace_receipts (kv_residency_policy_id)
    WHERE kv_residency_policy_id IS NOT NULL;

-- Index for quota enforcement audits
CREATE INDEX IF NOT EXISTS idx_inference_trace_receipts_kv_quota
    ON inference_trace_receipts (tenant_kv_quota_bytes, tenant_kv_bytes_used)
    WHERE kv_quota_enforced = 1;
