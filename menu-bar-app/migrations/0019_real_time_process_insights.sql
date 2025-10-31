-- Migration 0019: Real-Time Process Insights
-- Adds tables for real-time process monitoring, insights, and predictive analytics
-- Citation: docs/runaway-prevention.md, docs/architecture.md

-- Process insights table for storing calculated insights
CREATE TABLE IF NOT EXISTS process_insights (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    insight_type TEXT NOT NULL CHECK(insight_type IN ('performance','anomaly','trend','prediction','recommendation')),
    title TEXT NOT NULL,
    description TEXT,
    severity TEXT NOT NULL CHECK(severity IN ('info','warning','critical')),
    confidence_score REAL NOT NULL DEFAULT 0.0,
    data_json TEXT NOT NULL,
    actionable INTEGER NOT NULL DEFAULT 0,
    action_suggested TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_process_insights_worker_id ON process_insights(worker_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_process_insights_type ON process_insights(insight_type);
CREATE INDEX IF NOT EXISTS idx_process_insights_severity ON process_insights(severity);
CREATE INDEX IF NOT EXISTS idx_process_insights_actionable ON process_insights(actionable);

-- Process anomaly detection results
CREATE TABLE IF NOT EXISTS process_anomalies (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    anomaly_type TEXT NOT NULL CHECK(anomaly_type IN ('cpu_spike','memory_leak','latency_spike','error_rate','resource_exhaustion')),
    detected_at TEXT NOT NULL DEFAULT (datetime('now')),
    severity TEXT NOT NULL CHECK(severity IN ('low','medium','high','critical')),
    confidence REAL NOT NULL DEFAULT 0.0,
    baseline_value REAL,
    current_value REAL,
    threshold_value REAL,
    duration_seconds INTEGER,
    context_json TEXT,
    resolved_at TEXT,
    resolution_action TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_process_anomalies_worker_id ON process_anomalies(worker_id, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_process_anomalies_type ON process_anomalies(anomaly_type);
CREATE INDEX IF NOT EXISTS idx_process_anomalies_severity ON process_anomalies(severity);
CREATE INDEX IF NOT EXISTS idx_process_anomalies_resolved ON process_anomalies(resolved_at);

-- Process performance trends
CREATE TABLE IF NOT EXISTS process_performance_trends (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    metric_name TEXT NOT NULL,
    trend_direction TEXT NOT NULL CHECK(trend_direction IN ('improving','stable','degrading')),
    trend_strength REAL NOT NULL DEFAULT 0.0,
    period_start TEXT NOT NULL,
    period_end TEXT NOT NULL,
    baseline_value REAL,
    current_value REAL,
    change_percentage REAL,
    trend_data_json TEXT,
    forecast_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_performance_trends_worker_id ON process_performance_trends(worker_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_performance_trends_metric ON process_performance_trends(metric_name);
CREATE INDEX IF NOT EXISTS idx_performance_trends_direction ON process_performance_trends(trend_direction);

-- Process predictive analytics
CREATE TABLE IF NOT EXISTS process_predictions (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    prediction_type TEXT NOT NULL CHECK(prediction_type IN ('failure','performance','capacity','cost')),
    prediction_horizon_hours INTEGER NOT NULL,
    probability REAL NOT NULL DEFAULT 0.0,
    predicted_value REAL,
    confidence_interval_json TEXT,
    factors_json TEXT,
    mitigation_suggestions_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_process_predictions_worker_id ON process_predictions(worker_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_process_predictions_type ON process_predictions(prediction_type);
CREATE INDEX IF NOT EXISTS idx_process_predictions_probability ON process_predictions(probability);

-- Process recommendations table
CREATE TABLE IF NOT EXISTS process_recommendations (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    recommendation_type TEXT NOT NULL CHECK(recommendation_type IN ('optimization','scaling','maintenance','security','cost')),
    priority TEXT NOT NULL CHECK(priority IN ('low','medium','high','urgent')),
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    expected_impact_json TEXT,
    implementation_steps_json TEXT,
    estimated_effort_hours INTEGER,
    estimated_savings_json TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','in_progress','completed','dismissed')),
    accepted_at TEXT,
    implemented_at TEXT,
    results_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_process_recommendations_worker_id ON process_recommendations(worker_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_process_recommendations_type ON process_recommendations(recommendation_type);
CREATE INDEX IF NOT EXISTS idx_process_recommendations_priority ON process_recommendations(priority);
CREATE INDEX IF NOT EXISTS idx_process_recommendations_status ON process_recommendations(status);

-- Process alert rules for custom monitoring
CREATE TABLE IF NOT EXISTS process_alert_rules (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    rule_name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    metric_name TEXT NOT NULL,
    operator TEXT NOT NULL CHECK(operator IN ('gt','lt','eq','gte','lte','between')),
    threshold_value REAL NOT NULL,
    threshold_value_2 REAL,
    duration_seconds INTEGER NOT NULL DEFAULT 300,
    severity TEXT NOT NULL CHECK(severity IN ('info','warning','critical')),
    notification_channels_json TEXT,
    actions_json TEXT,
    cooldown_seconds INTEGER NOT NULL DEFAULT 3600,
    last_triggered_at TEXT,
    trigger_count INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_process_alert_rules_tenant_id ON process_alert_rules(tenant_id);
CREATE INDEX IF NOT EXISTS idx_process_alert_rules_enabled ON process_alert_rules(enabled);
CREATE INDEX IF NOT EXISTS idx_process_alert_rules_metric ON process_alert_rules(metric_name);

-- Process alert events
CREATE TABLE IF NOT EXISTS process_alert_events (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL REFERENCES process_alert_rules(id) ON DELETE CASCADE,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    triggered_at TEXT NOT NULL DEFAULT (datetime('now')),
    severity TEXT NOT NULL,
    metric_value REAL NOT NULL,
    threshold_value REAL NOT NULL,
    message TEXT NOT NULL,
    context_json TEXT,
    acknowledged_at TEXT,
    acknowledged_by TEXT REFERENCES users(id),
    resolved_at TEXT,
    resolved_by TEXT REFERENCES users(id),
    resolution_notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_process_alert_events_rule_id ON process_alert_events(rule_id, triggered_at DESC);
CREATE INDEX IF NOT EXISTS idx_process_alert_events_worker_id ON process_alert_events(worker_id, triggered_at DESC);
CREATE INDEX IF NOT EXISTS idx_process_alert_events_severity ON process_alert_events(severity);
CREATE INDEX IF NOT EXISTS idx_process_alert_events_acknowledged ON process_alert_events(acknowledged_at);
