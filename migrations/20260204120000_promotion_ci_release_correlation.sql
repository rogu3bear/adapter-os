-- Migration: Promotion CI verification + release correlation
-- Purpose: Add CI status fields and release correlation tracking for golden promotions
-- Created: 2026-02-04

ALTER TABLE golden_run_promotion_requests ADD COLUMN release_id TEXT;
ALTER TABLE golden_run_promotion_requests ADD COLUMN ci_status TEXT DEFAULT 'pending';
ALTER TABLE golden_run_promotion_requests ADD COLUMN ci_run_id TEXT;
ALTER TABLE golden_run_promotion_requests ADD COLUMN ci_checked_at TIMESTAMP;

UPDATE golden_run_promotion_requests
SET release_id = request_id
WHERE release_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_promotion_requests_release_id ON golden_run_promotion_requests(release_id);
CREATE INDEX IF NOT EXISTS idx_promotion_requests_ci_status ON golden_run_promotion_requests(ci_status);

CREATE TABLE IF NOT EXISTS release_correlations (
    release_id TEXT PRIMARY KEY,
    tenant_id TEXT,
    cpid TEXT,
    golden_run_id TEXT,
    promotion_request_id TEXT,
    target_stage TEXT,
    promotion_status TEXT,
    approval_signature TEXT,
    build_id TEXT,
    build_git_sha TEXT,
    ci_run_id TEXT,
    ci_status TEXT,
    ci_checked_at TIMESTAMP,
    image_digest TEXT,
    bundle_hash TEXT,
    trace_id TEXT,
    automation_workflow_id TEXT,
    automation_execution_id TEXT,
    config_deployment_id TEXT,
    trigger_id TEXT,
    ci_attestation_signature TEXT,
    ci_attestation_public_key TEXT,
    metadata_json TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_release_corr_golden_run_id ON release_correlations(golden_run_id);
CREATE INDEX IF NOT EXISTS idx_release_corr_promotion_request_id ON release_correlations(promotion_request_id);
CREATE INDEX IF NOT EXISTS idx_release_corr_ci_status ON release_correlations(ci_status);
