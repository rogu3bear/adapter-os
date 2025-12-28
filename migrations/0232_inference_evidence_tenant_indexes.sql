-- Tenant isolation indexes for inference_evidence table
-- Addresses audit finding: cross-workspace evidence export vulnerability
--
-- These indexes optimize tenant-scoped evidence queries and enforce
-- efficient workspace isolation for evidence retrieval operations.

-- Composite index for tenant + inference_id queries
-- Optimizes: SELECT ... FROM inference_evidence WHERE tenant_id = ? AND inference_id = ?
CREATE INDEX IF NOT EXISTS idx_inference_evidence_tenant_inference
    ON inference_evidence(tenant_id, inference_id);

-- Composite index for tenant + message_id queries
-- Optimizes: SELECT ... FROM inference_evidence WHERE tenant_id = ? AND message_id = ?
CREATE INDEX IF NOT EXISTS idx_inference_evidence_tenant_message
    ON inference_evidence(tenant_id, message_id);

-- Composite index for tenant + session_id queries with temporal ordering
-- Optimizes: SELECT ... FROM inference_evidence WHERE tenant_id = ? AND session_id = ? ORDER BY created_at DESC
CREATE INDEX IF NOT EXISTS idx_inference_evidence_tenant_session
    ON inference_evidence(tenant_id, session_id, created_at DESC);
