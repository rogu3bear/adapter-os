-- Admin tenant access control
-- Migration: 0116
-- Created: 2025-11-27
-- Purpose: Fix tenant isolation bypass by tracking per-tenant admin access

-- Track which tenants an admin user can access
-- Non-admin users don't need entries here (they use their own tenant_id)
CREATE TABLE IF NOT EXISTS user_tenant_access (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    granted_by TEXT REFERENCES users(id),
    granted_at TEXT NOT NULL DEFAULT (datetime('now')),
    reason TEXT,  -- Audit trail: why was access granted
    expires_at TEXT,  -- Optional expiry for temporary access
    UNIQUE(user_id, tenant_id)
);

CREATE INDEX IF NOT EXISTS idx_user_tenant_access_user ON user_tenant_access(user_id);
CREATE INDEX IF NOT EXISTS idx_user_tenant_access_tenant ON user_tenant_access(tenant_id);
CREATE INDEX IF NOT EXISTS idx_user_tenant_access_expires ON user_tenant_access(expires_at) WHERE expires_at IS NOT NULL;

-- Audit log for tenant access violations
-- This tracks all cross-tenant access attempts (successful and failed)
CREATE TABLE IF NOT EXISTS tenant_access_audit (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    user_email TEXT NOT NULL,
    user_role TEXT NOT NULL,
    user_tenant_id TEXT NOT NULL,
    resource_tenant_id TEXT NOT NULL,
    access_granted INTEGER NOT NULL,  -- 1 if allowed, 0 if denied
    reason TEXT,  -- Why access was granted/denied
    request_path TEXT,  -- Which endpoint was accessed
    timestamp TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_tenant_access_audit_user ON tenant_access_audit(user_id);
CREATE INDEX IF NOT EXISTS idx_tenant_access_audit_timestamp ON tenant_access_audit(timestamp);
CREATE INDEX IF NOT EXISTS idx_tenant_access_audit_denied ON tenant_access_audit(access_granted) WHERE access_granted = 0;
