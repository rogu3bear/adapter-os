-- JWT security infrastructure
-- Migration: 0066
-- Created: 2025-11-19
-- Purpose: Token revocation tracking and security enhancements

-- Token revocation blacklist
CREATE TABLE IF NOT EXISTS revoked_tokens (
    jti TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    revoked_at TEXT NOT NULL DEFAULT (datetime('now')),
    revoked_by TEXT,           -- User ID who revoked it (for admin revocation)
    reason TEXT,                -- Reason for revocation (logout, compromise, etc.)
    expires_at TEXT NOT NULL    -- Original token expiry (for cleanup)
);

CREATE INDEX IF NOT EXISTS idx_revoked_tokens_user_id ON revoked_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_revoked_tokens_tenant_id ON revoked_tokens(tenant_id);
CREATE INDEX IF NOT EXISTS idx_revoked_tokens_expires_at ON revoked_tokens(expires_at);

-- IP allowlist/denylist
CREATE TABLE IF NOT EXISTS ip_access_control (
    id TEXT PRIMARY KEY,
    ip_address TEXT NOT NULL,
    ip_range TEXT,              -- CIDR notation for range (e.g., "192.168.1.0/24")
    list_type TEXT NOT NULL CHECK(list_type IN ('allow', 'deny')),
    tenant_id TEXT,              -- NULL = global rule, specific tenant otherwise
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_by TEXT NOT NULL,
    expires_at TEXT,             -- Optional TTL for temporary blocks
    reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_ip_access_control_ip ON ip_access_control(ip_address);
CREATE INDEX IF NOT EXISTS idx_ip_access_control_tenant ON ip_access_control(tenant_id);
CREATE INDEX IF NOT EXISTS idx_ip_access_control_list_type ON ip_access_control(list_type);
CREATE UNIQUE INDEX IF NOT EXISTS idx_ip_access_control_unique ON ip_access_control(ip_address, tenant_id, list_type) WHERE active = 1;

-- Rate limiting buckets (per tenant)
CREATE TABLE IF NOT EXISTS rate_limit_buckets (
    tenant_id TEXT PRIMARY KEY,
    requests_count INTEGER NOT NULL DEFAULT 0,
    window_start TEXT NOT NULL DEFAULT (datetime('now')),
    window_size_seconds INTEGER NOT NULL DEFAULT 60,
    max_requests INTEGER NOT NULL DEFAULT 1000,  -- Default: 1000 requests per minute
    last_updated TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_rate_limit_window_start ON rate_limit_buckets(window_start);

-- Authentication attempts tracking (for brute force protection)
CREATE TABLE IF NOT EXISTS auth_attempts (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    ip_address TEXT NOT NULL,
    success INTEGER NOT NULL,  -- 0 = failure, 1 = success
    attempted_at TEXT NOT NULL DEFAULT (datetime('now')),
    failure_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_auth_attempts_email ON auth_attempts(email);
CREATE INDEX IF NOT EXISTS idx_auth_attempts_ip ON auth_attempts(ip_address);
CREATE INDEX IF NOT EXISTS idx_auth_attempts_time ON auth_attempts(attempted_at);
CREATE INDEX IF NOT EXISTS idx_auth_attempts_email_time ON auth_attempts(email, attempted_at DESC);
CREATE INDEX IF NOT EXISTS idx_auth_attempts_ip_time ON auth_attempts(ip_address, attempted_at DESC);

-- User sessions (for tracking active sessions and forced logout)
CREATE TABLE IF NOT EXISTS user_sessions (
    jti TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    last_activity TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_user_sessions_user_id ON user_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_user_sessions_tenant_id ON user_sessions(tenant_id);
CREATE INDEX IF NOT EXISTS idx_user_sessions_expires_at ON user_sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_user_sessions_user_active ON user_sessions(user_id, last_activity DESC);
