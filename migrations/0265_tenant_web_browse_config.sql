-- Tenant Web Browse Configuration
-- Migration: 0265
-- Created: 2025-12-30
-- Purpose: Add per-tenant configuration for web browsing capabilities (live data policy)

-- Configuration table for tenant web browsing permissions
CREATE TABLE IF NOT EXISTS tenant_web_browse_config (
    tenant_id TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL DEFAULT 0,

    -- Rate limiting
    requests_per_minute INTEGER NOT NULL DEFAULT 10,
    requests_per_day INTEGER NOT NULL DEFAULT 100,

    -- Feature toggles
    enable_web_search INTEGER NOT NULL DEFAULT 1,
    enable_page_fetch INTEGER NOT NULL DEFAULT 0,
    enable_image_search INTEGER NOT NULL DEFAULT 0,

    -- Allowed providers (JSON array of provider names: brave, bing, google)
    allowed_search_providers TEXT NOT NULL DEFAULT '["brave"]',

    -- Domain allowlist/blocklist (JSON arrays)
    allowed_domains TEXT NOT NULL DEFAULT '[]',
    blocked_domains TEXT NOT NULL DEFAULT '["localhost", "127.0.0.1", "*.local", "*.internal"]',

    -- Cache settings
    cache_ttl_seconds INTEGER NOT NULL DEFAULT 3600,

    -- Response limits
    max_results_per_query INTEGER NOT NULL DEFAULT 10,
    max_page_content_kb INTEGER NOT NULL DEFAULT 100,

    -- Security settings
    https_only INTEGER NOT NULL DEFAULT 1,
    max_concurrent_requests INTEGER NOT NULL DEFAULT 3,
    request_timeout_secs INTEGER NOT NULL DEFAULT 10,

    -- Fallback behavior when browsing fails or disabled
    -- Values: 'deny', 'warn_and_allow', 'allow_silent'
    fallback_behavior TEXT NOT NULL DEFAULT 'warn_and_allow'
        CHECK(fallback_behavior IN ('deny', 'warn_and_allow', 'allow_silent')),

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Rate limiting tracking table
CREATE TABLE IF NOT EXISTS tenant_web_browse_usage (
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    usage_date TEXT NOT NULL,  -- YYYY-MM-DD format
    usage_minute TEXT NOT NULL,  -- HH:MM format
    requests_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, usage_date, usage_minute)
);

-- Daily usage aggregation for quota tracking
CREATE TABLE IF NOT EXISTS tenant_web_browse_daily_usage (
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    usage_date TEXT NOT NULL,  -- YYYY-MM-DD format
    total_requests INTEGER NOT NULL DEFAULT 0,
    total_searches INTEGER NOT NULL DEFAULT 0,
    total_page_fetches INTEGER NOT NULL DEFAULT 0,
    total_image_searches INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, usage_date)
);

-- Web browse results cache
CREATE TABLE IF NOT EXISTS web_browse_cache (
    cache_key TEXT PRIMARY KEY,  -- SHA256 of (query_type, query, params)
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    query_type TEXT NOT NULL CHECK(query_type IN ('search', 'page_fetch', 'image_search')),
    query TEXT NOT NULL,
    response_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL
);

-- Indexes for efficient lookups
CREATE INDEX IF NOT EXISTS idx_web_browse_config_enabled
    ON tenant_web_browse_config(enabled) WHERE enabled = 1;

CREATE INDEX IF NOT EXISTS idx_web_browse_usage_tenant_date
    ON tenant_web_browse_usage(tenant_id, usage_date);

CREATE INDEX IF NOT EXISTS idx_web_browse_daily_usage_tenant
    ON tenant_web_browse_daily_usage(tenant_id, usage_date);

CREATE INDEX IF NOT EXISTS idx_web_browse_cache_tenant
    ON web_browse_cache(tenant_id);

CREATE INDEX IF NOT EXISTS idx_web_browse_cache_expires
    ON web_browse_cache(expires_at);

-- Trigger to update updated_at on config changes
CREATE TRIGGER IF NOT EXISTS update_web_browse_config_timestamp
AFTER UPDATE ON tenant_web_browse_config
FOR EACH ROW
BEGIN
    UPDATE tenant_web_browse_config SET updated_at = datetime('now') WHERE tenant_id = NEW.tenant_id;
END;

-- Trigger to aggregate minute usage into daily usage
CREATE TRIGGER IF NOT EXISTS aggregate_web_browse_daily_usage
AFTER INSERT ON tenant_web_browse_usage
FOR EACH ROW
BEGIN
    INSERT INTO tenant_web_browse_daily_usage (tenant_id, usage_date, total_requests)
    VALUES (NEW.tenant_id, NEW.usage_date, NEW.requests_count)
    ON CONFLICT(tenant_id, usage_date) DO UPDATE SET
        total_requests = total_requests + NEW.requests_count;
END;
