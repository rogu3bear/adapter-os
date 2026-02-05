-- Migration: 20260204140000
-- Purpose: Add discrepancy_cases table for tracking inference errors
--
-- This table stores user-reported discrepancies between model outputs and expected
-- ground truth. It supports privacy-conscious storage where content digests are always
-- stored, but plaintext content is only stored when explicitly opted-in (store_content=1).
--
-- Evidence: Inference Quality Tracking, Discrepancy-Based Training Loop

CREATE TABLE discrepancy_cases (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    inference_id TEXT NOT NULL,  -- FK to inference_traces(trace_id)
    run_id TEXT,  -- Optional FK to diag_runs(id)
    replay_session_id TEXT,  -- Optional FK to replay_sessions(id)

    -- Document reference
    document_id TEXT,
    document_hash_b3 TEXT,
    page_number INTEGER,
    chunk_hash_b3 TEXT,

    -- Discrepancy details
    discrepancy_type TEXT NOT NULL CHECK (discrepancy_type IN ('incorrect_answer', 'incomplete_answer', 'hallucination', 'formatting_error', 'other')),
    resolution_status TEXT NOT NULL DEFAULT 'open' CHECK (resolution_status IN ('open', 'confirmed_error', 'not_an_error', 'fixed_in_training', 'deferred')),

    -- Privacy-conscious content storage (optional, explicit opt-in)
    store_content INTEGER NOT NULL DEFAULT 0,  -- Boolean: 0=digests only, 1=store plaintext
    user_question TEXT,  -- Only stored if store_content=1
    model_answer TEXT,   -- Only stored if store_content=1
    ground_truth TEXT,   -- Only stored if store_content=1
    user_question_hash_b3 TEXT,  -- Always stored (digest)
    model_answer_hash_b3 TEXT,   -- Always stored (digest)

    -- Metadata
    reported_by TEXT,
    notes TEXT,

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (inference_id) REFERENCES inference_traces(trace_id) ON DELETE CASCADE
);

-- Primary query pattern: list open cases by tenant, most recent first
CREATE INDEX idx_discrepancy_cases_tenant_status ON discrepancy_cases(tenant_id, resolution_status, created_at DESC);

-- Lookup by inference trace
CREATE INDEX idx_discrepancy_cases_inference ON discrepancy_cases(inference_id);

-- Optional: lookup by diagnostic run (partial index for non-null only)
CREATE INDEX idx_discrepancy_cases_run ON discrepancy_cases(run_id) WHERE run_id IS NOT NULL;

-- Optional: lookup by document hash (partial index for non-null only)
CREATE INDEX idx_discrepancy_cases_document ON discrepancy_cases(document_hash_b3) WHERE document_hash_b3 IS NOT NULL;
