-- Evidence and inference trace performance indexes
-- Addresses audit finding: improper indexing causes timeouts under realistic data growth
--
-- These composite indexes optimize the most common query patterns:
-- 1. Listing evidence by dataset with temporal ordering
-- 2. Listing evidence by adapter with temporal ordering
-- 3. Looking up inference traces by tenant+request with ordering

-- Composite index for dataset evidence queries
-- Optimizes: SELECT ... FROM evidence_entries WHERE dataset_id = ? ORDER BY created_at DESC, id DESC
CREATE INDEX IF NOT EXISTS idx_evidence_entries_dataset_created_id
    ON evidence_entries(dataset_id, created_at DESC, id DESC);

-- Composite index for adapter evidence queries
-- Optimizes: SELECT ... FROM evidence_entries WHERE adapter_id = ? ORDER BY created_at DESC, id DESC
CREATE INDEX IF NOT EXISTS idx_evidence_entries_adapter_created_id
    ON evidence_entries(adapter_id, created_at DESC, id DESC);

-- Composite index for inference trace lookups with ordering
-- Optimizes: SELECT ... FROM inference_traces WHERE tenant_id = ? AND request_id = ? ORDER BY created_at DESC
CREATE INDEX IF NOT EXISTS idx_inference_traces_tenant_request_created
    ON inference_traces(tenant_id, request_id, created_at DESC, trace_id DESC);

-- Composite index for replay metadata tenant listing
-- Optimizes: SELECT ... FROM inference_replay_metadata WHERE tenant_id = ? ORDER BY created_at DESC
CREATE INDEX IF NOT EXISTS idx_replay_metadata_tenant_created
    ON inference_replay_metadata(tenant_id, created_at DESC, id DESC);
