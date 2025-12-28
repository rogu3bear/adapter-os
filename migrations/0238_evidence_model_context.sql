-- Add model/adapter context columns to inference_evidence for explainability.
--
-- These columns capture the model and adapter state at inference time, enabling
-- accurate audit trails even if the active workspace state changes later.
-- This fixes the evidence integrity issue where evidence bundles could become
-- misleading if workspace active state changed after inference.

ALTER TABLE inference_evidence ADD COLUMN base_model_id TEXT;
ALTER TABLE inference_evidence ADD COLUMN adapter_ids TEXT;  -- JSON array of adapter IDs
ALTER TABLE inference_evidence ADD COLUMN manifest_hash TEXT;

-- Index for querying evidence by model
CREATE INDEX IF NOT EXISTS idx_inference_evidence_base_model
    ON inference_evidence(tenant_id, base_model_id)
    WHERE base_model_id IS NOT NULL;
