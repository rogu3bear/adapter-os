-- Migration: Model Operations Audit Trail
-- Purpose: Track model load/unload/cancel operations with audit logging
-- Evidence: Based on existing audit patterns in migrations/0008_enclave_audit.sql

-- Model operations audit trail
CREATE TABLE IF NOT EXISTS model_operations (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK(operation IN ('load', 'unload', 'cancel')),
    initiated_by TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('in_progress', 'success', 'error', 'cancelled', 'timeout')),
    error_message TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    duration_ms INTEGER,
    FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE CASCADE
);

-- Indexes for efficient queries
CREATE INDEX idx_model_ops_tenant_id ON model_operations(tenant_id);
CREATE INDEX idx_model_ops_model_id ON model_operations(model_id);
CREATE INDEX idx_model_ops_started_at ON model_operations(started_at DESC);
CREATE INDEX idx_model_ops_operation ON model_operations(operation);
CREATE INDEX idx_model_ops_status ON model_operations(status);
CREATE INDEX idx_model_ops_initiated_by ON model_operations(initiated_by);

