-- Migration: Baseline version tracking for audit integrity
--
-- This migration adds tables to track baseline fingerprint versions
-- and links evidence records to specific baseline states for full
-- provenance tracking.
--
-- Related to: Drift baseline audit trail improvements

-- Table to track baseline fingerprint versions
-- Each row represents a specific baseline creation event with audit linkage
CREATE TABLE IF NOT EXISTS baseline_versions (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL DEFAULT 'system',
    fingerprint_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT,
    -- How the baseline was created: 'cli_flag', 'env_var', 'auto_legacy'
    creation_method TEXT NOT NULL,
    -- Link to the audit log entry that recorded this baseline creation
    audit_event_id TEXT,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id)
);

-- Index for efficient tenant-scoped queries
CREATE INDEX IF NOT EXISTS idx_baseline_versions_tenant
ON baseline_versions(tenant_id, created_at DESC);

-- Index for fingerprint hash lookups
CREATE INDEX IF NOT EXISTS idx_baseline_versions_hash
ON baseline_versions(fingerprint_hash);

-- Add baseline version reference to inference evidence
-- This links each evidence record to the baseline state that was active
-- when the inference was performed, enabling provenance verification
ALTER TABLE inference_evidence
ADD COLUMN baseline_version_id TEXT
REFERENCES baseline_versions(id);

-- Index for evidence-to-baseline lookups
CREATE INDEX IF NOT EXISTS idx_inference_evidence_baseline
ON inference_evidence(baseline_version_id);
