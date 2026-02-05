-- Migration: 20260204140100
-- Purpose: Add inference_verdicts table for tracking verdict evaluations on inference traces
-- Created: 2026-02-04

CREATE TABLE inference_verdicts (
    id TEXT PRIMARY KEY NOT NULL,
    tenant_id TEXT NOT NULL,
    inference_id TEXT NOT NULL,  -- FK to inference_traces(trace_id)
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    -- Verdict
    verdict TEXT NOT NULL CHECK (verdict IN ('high', 'medium', 'low', 'paused')),
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),

    -- Evaluation details
    evaluator_type TEXT NOT NULL CHECK (evaluator_type IN ('rule', 'human', 'model')),
    evaluator_id TEXT,  -- User ID if human, rule ID if rule, model ID if model

    -- Warnings (stored as digest for privacy, or JSON if detailed)
    warnings_digest_b3 TEXT,
    warnings_json TEXT,  -- Optional detailed warnings

    -- Context
    extraction_confidence_score REAL,  -- From ExtractionConfidence if available
    trust_state TEXT,  -- From dataset trust if applicable

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (inference_id) REFERENCES inference_traces(trace_id) ON DELETE CASCADE
);

-- Indexes
CREATE INDEX idx_inference_verdicts_tenant ON inference_verdicts(tenant_id, created_at DESC);
CREATE INDEX idx_inference_verdicts_inference ON inference_verdicts(inference_id);
CREATE INDEX idx_inference_verdicts_verdict ON inference_verdicts(tenant_id, verdict);
CREATE UNIQUE INDEX idx_inference_verdicts_unique_latest ON inference_verdicts(inference_id, evaluator_type);
