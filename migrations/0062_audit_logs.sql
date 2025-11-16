-- Add audit_logs table for tracking sensitive operations
-- Migration: 0062
-- Created: 2025-11-16
-- Purpose: RBAC audit trail for compliance and security

CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    user_id TEXT NOT NULL,
    user_role TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    action TEXT NOT NULL,           -- e.g., "adapter.register", "training.start", "policy.apply"
    resource_type TEXT NOT NULL,    -- e.g., "adapter", "tenant", "policy", "training_job"
    resource_id TEXT,                -- ID of the resource being acted upon
    status TEXT NOT NULL,            -- "success" or "failure"
    error_message TEXT,              -- Error details if status = "failure"
    ip_address TEXT,                 -- Client IP address (optional)
    metadata_json TEXT               -- Additional context as JSON (optional)
);

-- Indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_audit_user_id ON audit_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_logs(action);
CREATE INDEX IF NOT EXISTS idx_audit_resource_type ON audit_logs(resource_type);
CREATE INDEX IF NOT EXISTS idx_audit_resource_id ON audit_logs(resource_id);
CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_status ON audit_logs(status);
CREATE INDEX IF NOT EXISTS idx_audit_tenant_id ON audit_logs(tenant_id);

-- Composite index for common compliance queries (user actions over time)
CREATE INDEX IF NOT EXISTS idx_audit_user_timestamp ON audit_logs(user_id, timestamp DESC);

-- Composite index for resource audit trail
CREATE INDEX IF NOT EXISTS idx_audit_resource_timestamp ON audit_logs(resource_type, resource_id, timestamp DESC);
