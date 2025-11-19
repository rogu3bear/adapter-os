-- Tenant security enhancements
-- Migration: 0078
-- Created: 2025-11-19
-- Purpose: Enhanced tenant isolation and security controls

-- Add tenant_id to users table for tenant-scoped users
ALTER TABLE users ADD COLUMN tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE;

-- Create index for tenant-scoped user queries
CREATE INDEX IF NOT EXISTS idx_users_tenant_id ON users(tenant_id);

-- Tenant security settings
CREATE TABLE IF NOT EXISTS tenant_security_settings (
    tenant_id TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    require_mfa INTEGER NOT NULL DEFAULT 0,
    session_timeout_minutes INTEGER NOT NULL DEFAULT 480,  -- 8 hours
    max_concurrent_sessions INTEGER NOT NULL DEFAULT 10,
    ip_allowlist_enabled INTEGER NOT NULL DEFAULT 0,
    rate_limit_requests_per_minute INTEGER NOT NULL DEFAULT 1000,
    password_expiry_days INTEGER,   -- NULL = no expiry
    min_password_length INTEGER NOT NULL DEFAULT 12,
    require_password_complexity INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_by TEXT
);

-- Tenant API keys (for service-to-service authentication)
CREATE TABLE IF NOT EXISTS tenant_api_keys (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    key_hash TEXT NOT NULL UNIQUE,  -- BLAKE3 hash of the API key
    name TEXT NOT NULL,              -- Human-readable name for the key
    permissions_json TEXT NOT NULL,  -- JSON array of permissions
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT NOT NULL,
    expires_at TEXT,                 -- Optional expiry
    last_used_at TEXT,
    last_used_from_ip TEXT
);

CREATE INDEX IF NOT EXISTS idx_tenant_api_keys_tenant ON tenant_api_keys(tenant_id);
CREATE INDEX IF NOT EXISTS idx_tenant_api_keys_active ON tenant_api_keys(active);
CREATE INDEX IF NOT EXISTS idx_tenant_api_keys_expires ON tenant_api_keys(expires_at);
