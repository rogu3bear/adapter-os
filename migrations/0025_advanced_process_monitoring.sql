-- Advanced Process Monitoring and Alerting System
-- Citation: PRODUCTION_READINESS.md §4.2, CONTRIBUTING.md §3.1

-- Process monitoring rules and thresholds
CREATE TABLE process_monitoring_rules (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL,
    description TEXT,
    tenant_id TEXT NOT NULL,
    rule_type TEXT NOT NULL CHECK (rule_type IN ('cpu', 'memory', 'latency', 'error_rate', 'custom')),
    metric_name TEXT NOT NULL,
    threshold_value REAL NOT NULL,
    threshold_operator TEXT NOT NULL CHECK (threshold_operator IN ('gt', 'lt', 'eq', 'gte', 'lte')),
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'error', 'critical')),
    evaluation_window_seconds INTEGER NOT NULL DEFAULT 300,
    cooldown_seconds INTEGER NOT NULL DEFAULT 60,
    is_active BOOLEAN NOT NULL DEFAULT true,
    notification_channels JSON,
    escalation_rules JSON,
    created_by TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Process health metrics collection
CREATE TABLE process_health_metrics (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    worker_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    metric_name TEXT NOT NULL,
    metric_value REAL NOT NULL,
    metric_unit TEXT,
    tags JSON,
    collected_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (worker_id) REFERENCES workers(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Process alert instances
CREATE TABLE process_alerts (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    rule_id TEXT NOT NULL,
    worker_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    alert_type TEXT NOT NULL,
    severity TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    metric_value REAL,
    threshold_value REAL,
    status TEXT NOT NULL CHECK (status IN ('active', 'acknowledged', 'resolved', 'suppressed')),
    acknowledged_by TEXT,
    acknowledged_at TIMESTAMP,
    resolved_at TIMESTAMP,
    suppression_reason TEXT,
    suppression_until TIMESTAMP,
    escalation_level INTEGER DEFAULT 0,
    notification_sent BOOLEAN DEFAULT false,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (rule_id) REFERENCES process_monitoring_rules(id) ON DELETE CASCADE,
    FOREIGN KEY (worker_id) REFERENCES workers(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Add missing columns to existing process_anomalies table
ALTER TABLE process_anomalies ADD COLUMN tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE;
ALTER TABLE process_anomalies ADD COLUMN metric_name TEXT;
ALTER TABLE process_anomalies ADD COLUMN detected_value REAL;
ALTER TABLE process_anomalies ADD COLUMN expected_range_min REAL;
ALTER TABLE process_anomalies ADD COLUMN expected_range_max REAL;
ALTER TABLE process_anomalies ADD COLUMN confidence_score REAL DEFAULT 0.0;
ALTER TABLE process_anomalies ADD COLUMN description TEXT;
ALTER TABLE process_anomalies ADD COLUMN detection_method TEXT;
ALTER TABLE process_anomalies ADD COLUMN model_version TEXT;
ALTER TABLE process_anomalies ADD COLUMN status TEXT DEFAULT 'detected';
ALTER TABLE process_anomalies ADD COLUMN investigated_by TEXT;
ALTER TABLE process_anomalies ADD COLUMN investigation_notes TEXT;

-- Process performance baselines
CREATE TABLE process_performance_baselines (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    worker_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    metric_name TEXT NOT NULL,
    baseline_value REAL NOT NULL,
    baseline_type TEXT NOT NULL CHECK (baseline_type IN ('historical', 'statistical', 'manual')),
    calculation_period_days INTEGER NOT NULL,
    confidence_interval REAL,
    standard_deviation REAL,
    percentile_95 REAL,
    percentile_99 REAL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    calculated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP,
    FOREIGN KEY (worker_id) REFERENCES workers(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Process monitoring dashboards
CREATE TABLE process_monitoring_dashboards (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL,
    description TEXT,
    tenant_id TEXT NOT NULL,
    dashboard_config JSON NOT NULL,
    is_shared BOOLEAN NOT NULL DEFAULT false,
    created_by TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Process monitoring widgets
CREATE TABLE process_monitoring_widgets (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    dashboard_id TEXT NOT NULL,
    widget_type TEXT NOT NULL,
    widget_config JSON NOT NULL,
    position_x INTEGER NOT NULL,
    position_y INTEGER NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    refresh_interval_seconds INTEGER DEFAULT 30,
    is_visible BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (dashboard_id) REFERENCES process_monitoring_dashboards(id) ON DELETE CASCADE
);

-- Process monitoring notifications
CREATE TABLE process_monitoring_notifications (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    alert_id TEXT NOT NULL,
    notification_type TEXT NOT NULL CHECK (notification_type IN ('email', 'slack', 'webhook', 'sms', 'pagerduty')),
    recipient TEXT NOT NULL,
    message TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'sent', 'failed', 'delivered')),
    sent_at TIMESTAMP,
    delivered_at TIMESTAMP,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (alert_id) REFERENCES process_alerts(id) ON DELETE CASCADE
);

-- Process monitoring schedules
CREATE TABLE process_monitoring_schedules (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL,
    description TEXT,
    tenant_id TEXT NOT NULL,
    schedule_type TEXT NOT NULL CHECK (schedule_type IN ('interval', 'cron', 'event_driven')),
    schedule_config JSON NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    last_run_at TIMESTAMP,
    next_run_at TIMESTAMP,
    created_by TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Process monitoring reports
CREATE TABLE process_monitoring_reports (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL,
    description TEXT,
    tenant_id TEXT NOT NULL,
    report_type TEXT NOT NULL CHECK (report_type IN ('health_summary', 'performance_trends', 'anomaly_analysis', 'alert_summary')),
    report_config JSON NOT NULL,
    generated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    report_data JSON,
    file_path TEXT,
    file_size_bytes INTEGER,
    created_by TEXT,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Indexes for performance
CREATE INDEX idx_process_monitoring_rules_tenant_active ON process_monitoring_rules(tenant_id, is_active);
CREATE INDEX idx_process_health_metrics_worker_time ON process_health_metrics(worker_id, collected_at);
CREATE INDEX idx_process_health_metrics_tenant_metric ON process_health_metrics(tenant_id, metric_name, collected_at);
CREATE INDEX idx_process_alerts_tenant_status ON process_alerts(tenant_id, status);
CREATE INDEX idx_process_alerts_worker_active ON process_alerts(worker_id, status);
CREATE INDEX idx_process_anomalies_tenant_status ON process_anomalies(tenant_id, status);
CREATE INDEX idx_process_anomalies_worker_type ON process_anomalies(worker_id, anomaly_type);
CREATE INDEX idx_process_performance_baselines_worker_metric ON process_performance_baselines(worker_id, metric_name);
CREATE INDEX idx_process_monitoring_dashboards_tenant ON process_monitoring_dashboards(tenant_id);
CREATE INDEX idx_process_monitoring_widgets_dashboard ON process_monitoring_widgets(dashboard_id);
CREATE INDEX idx_process_monitoring_notifications_alert ON process_monitoring_notifications(alert_id);
CREATE INDEX idx_process_monitoring_schedules_tenant_active ON process_monitoring_schedules(tenant_id, is_active);
CREATE INDEX idx_process_monitoring_reports_tenant_type ON process_monitoring_reports(tenant_id, report_type);

-- Triggers for updated_at
CREATE TRIGGER process_monitoring_rules_updated_at 
    AFTER UPDATE ON process_monitoring_rules
    BEGIN
        UPDATE process_monitoring_rules SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
    END;

CREATE TRIGGER process_alerts_updated_at 
    AFTER UPDATE ON process_alerts
    BEGIN
        UPDATE process_alerts SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
    END;

CREATE TRIGGER process_monitoring_dashboards_updated_at 
    AFTER UPDATE ON process_monitoring_dashboards
    BEGIN
        UPDATE process_monitoring_dashboards SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
    END;

CREATE TRIGGER process_monitoring_schedules_updated_at 
    AFTER UPDATE ON process_monitoring_schedules
    BEGIN
        UPDATE process_monitoring_schedules SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
    END;
