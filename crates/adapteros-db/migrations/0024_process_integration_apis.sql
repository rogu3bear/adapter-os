-- Migration 0024: Process Integration and APIs
-- Adds tables for external integrations, webhook support, API management, data synchronization, and third-party connectors
-- Citation: docs/architecture.md, docs/control-plane.md

-- Process external integrations table
CREATE TABLE IF NOT EXISTS process_external_integrations (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    integration_name TEXT NOT NULL,
    integration_type TEXT NOT NULL CHECK(integration_type IN ('api','webhook','database','message_queue','file_system','cloud_service','monitoring','logging','alerting')),
    provider TEXT NOT NULL,
    provider_version TEXT,
    connection_config_json TEXT NOT NULL,
    authentication_config_json TEXT,
    mapping_config_json TEXT,
    sync_config_json TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    last_sync_at TEXT,
    sync_status TEXT NOT NULL DEFAULT 'pending' CHECK(sync_status IN ('pending','syncing','completed','failed','paused')),
    sync_error_message TEXT,
    sync_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_external_integrations_tenant_id ON process_external_integrations(tenant_id);
CREATE INDEX IF NOT EXISTS idx_external_integrations_type ON process_external_integrations(integration_type);
CREATE INDEX IF NOT EXISTS idx_external_integrations_provider ON process_external_integrations(provider);
CREATE INDEX IF NOT EXISTS idx_external_integrations_active ON process_external_integrations(is_active);

-- Process webhook endpoints table
CREATE TABLE IF NOT EXISTS process_webhook_endpoints (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    endpoint_name TEXT NOT NULL,
    endpoint_url TEXT NOT NULL,
    endpoint_method TEXT NOT NULL CHECK(endpoint_method IN ('GET','POST','PUT','DELETE','PATCH')),
    event_types_json TEXT NOT NULL,
    authentication_type TEXT NOT NULL CHECK(authentication_type IN ('none','basic','bearer','api_key','oauth2','custom')),
    authentication_config_json TEXT,
    headers_json TEXT,
    payload_template_json TEXT,
    retry_policy_json TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    last_triggered_at TEXT,
    trigger_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_webhook_endpoints_tenant_id ON process_webhook_endpoints(tenant_id);
CREATE INDEX IF NOT EXISTS idx_webhook_endpoints_method ON process_webhook_endpoints(endpoint_method);
CREATE INDEX IF NOT EXISTS idx_webhook_endpoints_active ON process_webhook_endpoints(is_active);

-- Process API management table
CREATE TABLE IF NOT EXISTS process_api_management (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    api_name TEXT NOT NULL,
    api_version TEXT NOT NULL DEFAULT '1.0.0',
    api_description TEXT,
    api_base_url TEXT NOT NULL,
    api_endpoints_json TEXT NOT NULL,
    api_schema_json TEXT,
    authentication_required INTEGER NOT NULL DEFAULT 1,
    authentication_method TEXT NOT NULL CHECK(authentication_method IN ('api_key','oauth2','jwt','basic','none')),
    rate_limit_per_minute INTEGER NOT NULL DEFAULT 1000,
    rate_limit_per_hour INTEGER NOT NULL DEFAULT 10000,
    rate_limit_per_day INTEGER NOT NULL DEFAULT 100000,
    cors_enabled INTEGER NOT NULL DEFAULT 1,
    cors_origins_json TEXT,
    api_keys_json TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    last_accessed_at TEXT,
    access_count INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_api_management_tenant_id ON process_api_management(tenant_id);
CREATE INDEX IF NOT EXISTS idx_api_management_name ON process_api_management(api_name);
CREATE INDEX IF NOT EXISTS idx_api_management_active ON process_api_management(is_active);

-- Process data synchronization table
CREATE TABLE IF NOT EXISTS process_data_synchronization (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    sync_name TEXT NOT NULL,
    sync_type TEXT NOT NULL CHECK(sync_type IN ('bidirectional','unidirectional','replication','backup','migration')),
    source_system TEXT NOT NULL,
    target_system TEXT NOT NULL,
    source_config_json TEXT NOT NULL,
    target_config_json TEXT NOT NULL,
    sync_rules_json TEXT NOT NULL,
    sync_schedule_json TEXT,
    conflict_resolution_strategy TEXT NOT NULL CHECK(conflict_resolution_strategy IN ('source_wins','target_wins','manual','timestamp','custom')),
    sync_status TEXT NOT NULL DEFAULT 'pending' CHECK(sync_status IN ('pending','running','completed','failed','paused','cancelled')),
    last_sync_at TEXT,
    next_sync_at TEXT,
    sync_duration_ms INTEGER,
    records_synced INTEGER NOT NULL DEFAULT 0,
    records_failed INTEGER NOT NULL DEFAULT 0,
    sync_error_message TEXT,
    sync_log_json TEXT,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_data_sync_tenant_id ON process_data_synchronization(tenant_id);
CREATE INDEX IF NOT EXISTS idx_data_sync_type ON process_data_synchronization(sync_type);
CREATE INDEX IF NOT EXISTS idx_data_sync_status ON process_data_synchronization(sync_status);
CREATE INDEX IF NOT EXISTS idx_data_sync_next_sync ON process_data_synchronization(next_sync_at);

-- Process third-party connectors table
CREATE TABLE IF NOT EXISTS process_third_party_connectors (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    connector_name TEXT NOT NULL,
    connector_type TEXT NOT NULL CHECK(connector_type IN ('database','api','file','message_queue','cloud_storage','monitoring','logging','ci_cd','version_control')),
    provider_name TEXT NOT NULL,
    provider_version TEXT,
    connector_config_json TEXT NOT NULL,
    authentication_config_json TEXT,
    connection_pool_config_json TEXT,
    health_check_config_json TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    connection_status TEXT NOT NULL DEFAULT 'unknown' CHECK(connection_status IN ('connected','disconnected','error','unknown')),
    last_health_check_at TEXT,
    last_health_check_result TEXT,
    connection_count INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_third_party_connectors_tenant_id ON process_third_party_connectors(tenant_id);
CREATE INDEX IF NOT EXISTS idx_third_party_connectors_type ON process_third_party_connectors(connector_type);
CREATE INDEX IF NOT EXISTS idx_third_party_connectors_provider ON process_third_party_connectors(provider_name);
CREATE INDEX IF NOT EXISTS idx_third_party_connectors_status ON process_third_party_connectors(connection_status);

-- Process integration events table
CREATE TABLE IF NOT EXISTS process_integration_events (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    integration_id TEXT REFERENCES process_external_integrations(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL CHECK(event_type IN ('sync_started','sync_completed','sync_failed','webhook_received','webhook_sent','api_call','data_transformed','error_occurred')),
    event_source TEXT NOT NULL,
    event_data_json TEXT NOT NULL,
    event_metadata_json TEXT,
    processing_status TEXT NOT NULL DEFAULT 'pending' CHECK(processing_status IN ('pending','processing','completed','failed','retrying')),
    processing_error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    processed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_integration_events_tenant_id ON process_integration_events(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_integration_events_integration_id ON process_integration_events(integration_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_integration_events_type ON process_integration_events(event_type);
CREATE INDEX IF NOT EXISTS idx_integration_events_status ON process_integration_events(processing_status);

-- Process API keys table
CREATE TABLE IF NOT EXISTS process_api_keys (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    key_name TEXT NOT NULL,
    key_type TEXT NOT NULL CHECK(key_type IN ('api_key','oauth2','jwt','basic')),
    key_value_hash TEXT NOT NULL,
    key_permissions_json TEXT NOT NULL,
    key_scopes_json TEXT,
    rate_limit_per_minute INTEGER NOT NULL DEFAULT 1000,
    rate_limit_per_hour INTEGER NOT NULL DEFAULT 10000,
    rate_limit_per_day INTEGER NOT NULL DEFAULT 100000,
    is_active INTEGER NOT NULL DEFAULT 1,
    expires_at TEXT,
    last_used_at TEXT,
    usage_count INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_api_keys_tenant_id ON process_api_keys(tenant_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_type ON process_api_keys(key_type);
CREATE INDEX IF NOT EXISTS idx_api_keys_active ON process_api_keys(is_active);
CREATE INDEX IF NOT EXISTS idx_api_keys_expires ON process_api_keys(expires_at);

-- Process integration templates table
CREATE TABLE IF NOT EXISTS process_integration_templates (
    id TEXT PRIMARY KEY,
    template_name TEXT NOT NULL,
    template_category TEXT NOT NULL CHECK(template_category IN ('database','api','webhook','file','message_queue','cloud','monitoring','logging')),
    template_description TEXT,
    template_config_json TEXT NOT NULL,
    template_schema_json TEXT,
    template_examples_json TEXT,
    provider_name TEXT NOT NULL,
    provider_version TEXT,
    is_public INTEGER NOT NULL DEFAULT 0,
    usage_count INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_integration_templates_category ON process_integration_templates(template_category);
CREATE INDEX IF NOT EXISTS idx_integration_templates_provider ON process_integration_templates(provider_name);
CREATE INDEX IF NOT EXISTS idx_integration_templates_public ON process_integration_templates(is_public);
CREATE INDEX IF NOT EXISTS idx_integration_templates_usage ON process_integration_templates(usage_count DESC);

-- Process integration monitoring table
CREATE TABLE IF NOT EXISTS process_integration_monitoring (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    integration_id TEXT NOT NULL REFERENCES process_external_integrations(id) ON DELETE CASCADE,
    metric_name TEXT NOT NULL,
    metric_value REAL NOT NULL,
    metric_unit TEXT,
    metric_timestamp TEXT NOT NULL,
    metric_dimensions_json TEXT,
    alert_threshold_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_integration_monitoring_tenant_id ON process_integration_monitoring(tenant_id, metric_timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_integration_monitoring_integration_id ON process_integration_monitoring(integration_id, metric_timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_integration_monitoring_metric ON process_integration_monitoring(metric_name);
