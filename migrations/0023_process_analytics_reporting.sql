-- Migration 0023: Process Analytics and Reporting
-- Adds tables for performance analytics, cost analysis, usage reports, trend analysis, and custom dashboards
-- Citation: docs/architecture.md, docs/control-plane.md

-- Process performance analytics table
CREATE TABLE IF NOT EXISTS process_performance_analytics (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    worker_id TEXT REFERENCES workers(id) ON DELETE CASCADE,
    metric_name TEXT NOT NULL,
    metric_type TEXT NOT NULL CHECK(metric_type IN ('latency','throughput','cpu_usage','memory_usage','gpu_usage','error_rate','success_rate','queue_depth')),
    metric_value REAL NOT NULL,
    metric_unit TEXT,
    aggregation_period TEXT NOT NULL CHECK(aggregation_period IN ('minute','hour','day','week','month')),
    timestamp TEXT NOT NULL,
    dimensions_json TEXT,
    tags_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_performance_analytics_tenant_id ON process_performance_analytics(tenant_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_performance_analytics_worker_id ON process_performance_analytics(worker_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_performance_analytics_metric ON process_performance_analytics(metric_name, metric_type);
CREATE INDEX IF NOT EXISTS idx_performance_analytics_period ON process_performance_analytics(aggregation_period, timestamp DESC);

-- Process cost analysis table
CREATE TABLE IF NOT EXISTS process_cost_analysis (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    worker_id TEXT REFERENCES workers(id) ON DELETE CASCADE,
    cost_category TEXT NOT NULL CHECK(cost_category IN ('compute','storage','network','license','support','maintenance')),
    cost_amount REAL NOT NULL,
    cost_currency TEXT NOT NULL DEFAULT 'USD',
    billing_period TEXT NOT NULL CHECK(billing_period IN ('hourly','daily','weekly','monthly','yearly')),
    resource_type TEXT NOT NULL CHECK(resource_type IN ('cpu','memory','gpu','storage','bandwidth','api_calls')),
    resource_quantity REAL NOT NULL,
    resource_unit TEXT NOT NULL,
    unit_cost REAL NOT NULL,
    cost_breakdown_json TEXT,
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_cost_analysis_tenant_id ON process_cost_analysis(tenant_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_cost_analysis_worker_id ON process_cost_analysis(worker_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_cost_analysis_category ON process_cost_analysis(cost_category);
CREATE INDEX IF NOT EXISTS idx_cost_analysis_period ON process_cost_analysis(billing_period, timestamp DESC);

-- Process usage reports table
CREATE TABLE IF NOT EXISTS process_usage_reports (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    report_name TEXT NOT NULL,
    report_type TEXT NOT NULL CHECK(report_type IN ('summary','detailed','trend','comparison','forecast')),
    report_period_start TEXT NOT NULL,
    report_period_end TEXT NOT NULL,
    report_data_json TEXT NOT NULL,
    report_metadata_json TEXT,
    generated_by TEXT REFERENCES users(id),
    generated_at TEXT NOT NULL DEFAULT (datetime('now')),
    report_status TEXT NOT NULL DEFAULT 'completed' CHECK(report_status IN ('generating','completed','failed')),
    file_path TEXT,
    file_size_bytes INTEGER,
    download_count INTEGER NOT NULL DEFAULT 0,
    last_downloaded_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_usage_reports_tenant_id ON process_usage_reports(tenant_id, generated_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_reports_type ON process_usage_reports(report_type);
CREATE INDEX IF NOT EXISTS idx_usage_reports_period ON process_usage_reports(report_period_start, report_period_end);
CREATE INDEX IF NOT EXISTS idx_usage_reports_status ON process_usage_reports(report_status);

-- Process trend analysis table
CREATE TABLE IF NOT EXISTS process_trend_analysis (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    trend_name TEXT NOT NULL,
    trend_type TEXT NOT NULL CHECK(trend_type IN ('performance','cost','usage','capacity','reliability','efficiency')),
    metric_name TEXT NOT NULL,
    analysis_period TEXT NOT NULL CHECK(analysis_period IN ('week','month','quarter','year')),
    trend_direction TEXT NOT NULL CHECK(trend_direction IN ('increasing','decreasing','stable','volatile')),
    trend_strength REAL NOT NULL,
    confidence_level REAL NOT NULL,
    data_points_json TEXT NOT NULL,
    forecast_data_json TEXT,
    insights_json TEXT,
    recommendations_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_trend_analysis_tenant_id ON process_trend_analysis(tenant_id);
CREATE INDEX IF NOT EXISTS idx_trend_analysis_type ON process_trend_analysis(trend_type);
CREATE INDEX IF NOT EXISTS idx_trend_analysis_metric ON process_trend_analysis(metric_name);
CREATE INDEX IF NOT EXISTS idx_trend_analysis_period ON process_trend_analysis(analysis_period);

-- Process custom dashboards table
CREATE TABLE IF NOT EXISTS process_custom_dashboards (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    dashboard_name TEXT NOT NULL,
    dashboard_description TEXT,
    dashboard_config_json TEXT NOT NULL,
    dashboard_layout_json TEXT NOT NULL,
    dashboard_filters_json TEXT,
    dashboard_refresh_interval_seconds INTEGER NOT NULL DEFAULT 300,
    is_public INTEGER NOT NULL DEFAULT 0,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_accessed_at TEXT,
    access_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_custom_dashboards_tenant_id ON process_custom_dashboards(tenant_id);
CREATE INDEX IF NOT EXISTS idx_custom_dashboards_public ON process_custom_dashboards(is_public);
CREATE INDEX IF NOT EXISTS idx_custom_dashboards_default ON process_custom_dashboards(is_default);
CREATE INDEX IF NOT EXISTS idx_custom_dashboards_created_by ON process_custom_dashboards(created_by);

-- Process dashboard widgets table
CREATE TABLE IF NOT EXISTS process_dashboard_widgets (
    id TEXT PRIMARY KEY,
    dashboard_id TEXT NOT NULL REFERENCES process_custom_dashboards(id) ON DELETE CASCADE,
    widget_name TEXT NOT NULL,
    widget_type TEXT NOT NULL CHECK(widget_type IN ('chart','table','metric','gauge','map','text','image')),
    widget_config_json TEXT NOT NULL,
    widget_position_json TEXT NOT NULL,
    widget_size_json TEXT NOT NULL,
    widget_data_source_json TEXT,
    widget_filters_json TEXT,
    widget_order INTEGER NOT NULL,
    is_visible INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_dashboard_widgets_dashboard_id ON process_dashboard_widgets(dashboard_id, widget_order);
CREATE INDEX IF NOT EXISTS idx_dashboard_widgets_type ON process_dashboard_widgets(widget_type);
CREATE INDEX IF NOT EXISTS idx_dashboard_widgets_visible ON process_dashboard_widgets(is_visible);

-- Process analytics queries table
CREATE TABLE IF NOT EXISTS process_analytics_queries (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    query_name TEXT NOT NULL,
    query_description TEXT,
    query_sql TEXT NOT NULL,
    query_parameters_json TEXT,
    query_result_cache_json TEXT,
    query_execution_time_ms INTEGER,
    query_result_count INTEGER,
    last_executed_at TEXT,
    execution_count INTEGER NOT NULL DEFAULT 0,
    average_execution_time_ms REAL,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_analytics_queries_tenant_id ON process_analytics_queries(tenant_id);
CREATE INDEX IF NOT EXISTS idx_analytics_queries_name ON process_analytics_queries(query_name);
CREATE INDEX IF NOT EXISTS idx_analytics_queries_execution ON process_analytics_queries(last_executed_at DESC);

-- Process analytics alerts table
CREATE TABLE IF NOT EXISTS process_analytics_alerts (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    alert_name TEXT NOT NULL,
    alert_type TEXT NOT NULL CHECK(alert_type IN ('threshold','anomaly','trend','forecast','comparison')),
    alert_condition_json TEXT NOT NULL,
    alert_severity TEXT NOT NULL CHECK(alert_severity IN ('low','medium','high','critical')),
    alert_channels_json TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    last_triggered_at TEXT,
    trigger_count INTEGER NOT NULL DEFAULT 0,
    last_resolved_at TEXT,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_analytics_alerts_tenant_id ON process_analytics_alerts(tenant_id);
CREATE INDEX IF NOT EXISTS idx_analytics_alerts_type ON process_analytics_alerts(alert_type);
CREATE INDEX IF NOT EXISTS idx_analytics_alerts_severity ON process_analytics_alerts(alert_severity);
CREATE INDEX IF NOT EXISTS idx_analytics_alerts_active ON process_analytics_alerts(is_active);

-- Process analytics exports table
CREATE TABLE IF NOT EXISTS process_analytics_exports (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    export_name TEXT NOT NULL,
    export_type TEXT NOT NULL CHECK(export_type IN ('csv','json','xlsx','pdf','png','svg')),
    export_query_json TEXT NOT NULL,
    export_filters_json TEXT,
    export_format_config_json TEXT,
    export_status TEXT NOT NULL DEFAULT 'pending' CHECK(export_status IN ('pending','processing','completed','failed')),
    file_path TEXT,
    file_size_bytes INTEGER,
    download_url TEXT,
    expires_at TEXT,
    requested_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    download_count INTEGER NOT NULL DEFAULT 0,
    last_downloaded_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_analytics_exports_tenant_id ON process_analytics_exports(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_analytics_exports_type ON process_analytics_exports(export_type);
CREATE INDEX IF NOT EXISTS idx_analytics_exports_status ON process_analytics_exports(export_status);
CREATE INDEX IF NOT EXISTS idx_analytics_exports_expires ON process_analytics_exports(expires_at);
