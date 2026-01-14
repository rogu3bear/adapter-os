-- Patent 3535886.0002 Compliance: Tenant Cryptographic Binding
--
-- Adds tenant-specific HMAC binding fields to inference_trace_receipts
-- for multi-tenant isolation with cryptographic proof.

-- Tenant ID for multi-tenant isolation
ALTER TABLE inference_trace_receipts
    ADD COLUMN tenant_id TEXT;

-- Tenant binding MAC (HMAC-SHA256 of receipt_digest || tenant_id)
ALTER TABLE inference_trace_receipts
    ADD COLUMN tenant_binding_mac BLOB;

-- Timestamp when tenant binding was created
ALTER TABLE inference_trace_receipts
    ADD COLUMN tenant_bound_at TEXT;

-- Index for querying by tenant binding
CREATE INDEX IF NOT EXISTS idx_receipts_tenant_binding
    ON inference_trace_receipts(tenant_id, tenant_binding_mac)
    WHERE tenant_binding_mac IS NOT NULL;
