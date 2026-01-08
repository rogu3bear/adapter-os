-- Client Error Observability Tables
-- Migration: 0277
-- Purpose: Persistent storage for client-side errors, alert rules, and alert history
-- Created: 2026-01-08

-- =============================================================================
-- Table: client_errors
-- Stores client-side error events reported from the WASM UI
-- =============================================================================
CREATE TABLE IF NOT EXISTS client_errors (
    id TEXT PRIMARY KEY NOT NULL,                    -- UUIDv7
    tenant_id TEXT NOT NULL,                         -- Tenant isolation
    user_id TEXT,                                    -- NULL for anonymous errors
    error_type TEXT NOT NULL,                        -- 'Network', 'Http', 'Validation', etc.
    message TEXT NOT NULL,                           -- Truncated to 2000 chars
    code TEXT,                                       -- Error code if available
    failure_code TEXT,                               -- Structured failure code
    http_status INTEGER,                             -- HTTP status if applicable
    page TEXT,                                       -- Route where error occurred
    user_agent TEXT NOT NULL,                        -- Browser user agent
    client_timestamp TEXT NOT NULL,                  -- ISO 8601 from client
    details_json TEXT,                               -- Additional JSON context
    ip_address TEXT,                                 -- Client IP (optional)
    session_id TEXT,                                 -- Auth session if available
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Primary query indexes
CREATE INDEX IF NOT EXISTS idx_client_errors_tenant_id ON client_errors(tenant_id);
CREATE INDEX IF NOT EXISTS idx_client_errors_created_at ON client_errors(created_at);
CREATE INDEX IF NOT EXISTS idx_client_errors_error_type ON client_errors(error_type);
CREATE INDEX IF NOT EXISTS idx_client_errors_http_status ON client_errors(http_status);
CREATE INDEX IF NOT EXISTS idx_client_errors_user_id ON client_errors(user_id);
CREATE INDEX IF NOT EXISTS idx_client_errors_page ON client_errors(page);

-- Composite indexes for dashboard queries
CREATE INDEX IF NOT EXISTS idx_client_errors_tenant_created
    ON client_errors(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_client_errors_tenant_type_created
    ON client_errors(tenant_id, error_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_client_errors_tenant_status
    ON client_errors(tenant_id, http_status);

-- =============================================================================
-- Table: error_alert_rules
-- Configurable alert rules for error thresholds
-- =============================================================================
CREATE TABLE IF NOT EXISTS error_alert_rules (
    id TEXT PRIMARY KEY NOT NULL,                    -- UUIDv7
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    error_type_pattern TEXT,                         -- NULL = all types, or specific type
    http_status_pattern TEXT,                        -- e.g., '4xx', '5xx', '500'
    page_pattern TEXT,                               -- Glob pattern for pages
    threshold_count INTEGER NOT NULL DEFAULT 5,      -- Errors before alert fires
    threshold_window_minutes INTEGER NOT NULL DEFAULT 5, -- Time window for counting
    cooldown_minutes INTEGER NOT NULL DEFAULT 15,    -- Min time between alerts
    severity TEXT NOT NULL DEFAULT 'warning'         -- Alert severity level
        CHECK(severity IN ('info', 'warning', 'error', 'critical')),
    is_active INTEGER NOT NULL DEFAULT 1,            -- 0 = disabled, 1 = enabled
    notification_channels_json TEXT,                 -- JSON array of notification channels
    created_by TEXT,                                 -- User who created the rule
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_error_alert_rules_tenant
    ON error_alert_rules(tenant_id);
CREATE INDEX IF NOT EXISTS idx_error_alert_rules_active
    ON error_alert_rules(tenant_id, is_active);

-- Auto-update trigger for updated_at
CREATE TRIGGER IF NOT EXISTS update_error_alert_rules_timestamp
AFTER UPDATE ON error_alert_rules
FOR EACH ROW
BEGIN
    UPDATE error_alert_rules SET updated_at = datetime('now') WHERE id = NEW.id;
END;

-- =============================================================================
-- Table: error_alert_history
-- History of triggered alerts
-- =============================================================================
CREATE TABLE IF NOT EXISTS error_alert_history (
    id TEXT PRIMARY KEY NOT NULL,                    -- UUIDv7
    rule_id TEXT NOT NULL REFERENCES error_alert_rules(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    triggered_at TEXT NOT NULL DEFAULT (datetime('now')),
    error_count INTEGER NOT NULL,                    -- Count when alert fired
    sample_error_ids_json TEXT,                      -- JSON array of sample error IDs
    acknowledged_at TEXT,                            -- When alert was acknowledged
    acknowledged_by TEXT,                            -- User who acknowledged
    resolved_at TEXT,                                -- When alert was resolved
    resolution_note TEXT                             -- Resolution notes
);

CREATE INDEX IF NOT EXISTS idx_error_alert_history_tenant
    ON error_alert_history(tenant_id);
CREATE INDEX IF NOT EXISTS idx_error_alert_history_rule
    ON error_alert_history(rule_id);
CREATE INDEX IF NOT EXISTS idx_error_alert_history_triggered
    ON error_alert_history(tenant_id, triggered_at DESC);
CREATE INDEX IF NOT EXISTS idx_error_alert_history_unresolved
    ON error_alert_history(tenant_id, resolved_at) WHERE resolved_at IS NULL;
