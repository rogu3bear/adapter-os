-- Migration 0022: Process Automation and Orchestration
-- Adds tables for workflow automation, scheduled operations, event-driven triggers, and dependency management
-- Citation: docs/architecture.md, docs/control-plane.md

-- Process automation workflows table
CREATE TABLE IF NOT EXISTS process_automation_workflows_v2 (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    workflow_name TEXT NOT NULL,
    workflow_type TEXT NOT NULL CHECK(workflow_type IN ('deployment','scaling','maintenance','recovery','backup','monitoring','testing')),
    description TEXT,
    workflow_definition_json TEXT NOT NULL,
    trigger_config_json TEXT,
    schedule_config_json TEXT,
    dependencies_json TEXT,
    retry_policy_json TEXT,
    timeout_seconds INTEGER NOT NULL DEFAULT 3600,
    max_concurrent_executions INTEGER NOT NULL DEFAULT 1,
    is_active INTEGER NOT NULL DEFAULT 1,
    version TEXT NOT NULL DEFAULT '1.0.0',
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_automation_workflows_tenant_id ON process_automation_workflows_v2(tenant_id);
CREATE INDEX IF NOT EXISTS idx_automation_workflows_type ON process_automation_workflows_v2(workflow_type);
CREATE INDEX IF NOT EXISTS idx_automation_workflows_active ON process_automation_workflows_v2(is_active);

-- Process automation executions table
CREATE TABLE IF NOT EXISTS process_automation_executions (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL REFERENCES process_automation_workflows_v2(id) ON DELETE CASCADE,
    execution_type TEXT NOT NULL CHECK(execution_type IN ('manual','scheduled','triggered','dependency')),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','running','completed','failed','cancelled','timeout')),
    started_at TEXT,
    completed_at TEXT,
    triggered_by TEXT REFERENCES users(id),
    trigger_event_json TEXT,
    input_parameters_json TEXT,
    output_results_json TEXT,
    error_message TEXT,
    execution_log_json TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_automation_executions_workflow_id ON process_automation_executions(workflow_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_automation_executions_status ON process_automation_executions(status);
CREATE INDEX IF NOT EXISTS idx_automation_executions_type ON process_automation_executions(execution_type);

-- Process automation steps table
CREATE TABLE IF NOT EXISTS process_automation_steps (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL REFERENCES process_automation_executions(id) ON DELETE CASCADE,
    step_name TEXT NOT NULL,
    step_type TEXT NOT NULL CHECK(step_type IN ('action','condition','loop','parallel','sequential','wait','notification')),
    step_order INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','running','completed','failed','skipped')),
    started_at TEXT,
    completed_at TEXT,
    input_data_json TEXT,
    output_data_json TEXT,
    error_message TEXT,
    step_config_json TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_automation_steps_execution_id ON process_automation_steps(execution_id, step_order);
CREATE INDEX IF NOT EXISTS idx_automation_steps_status ON process_automation_steps(status);
CREATE INDEX IF NOT EXISTS idx_automation_steps_type ON process_automation_steps(step_type);

-- Process scheduled operations table
CREATE TABLE IF NOT EXISTS process_scheduled_operations (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    operation_name TEXT NOT NULL,
    operation_type TEXT NOT NULL CHECK(operation_type IN ('backup','cleanup','health_check','scaling','maintenance','reporting')),
    schedule_cron TEXT NOT NULL,
    schedule_timezone TEXT NOT NULL DEFAULT 'UTC',
    target_workers_json TEXT,
    operation_config_json TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    last_executed_at TEXT,
    next_execution_at TEXT,
    execution_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_scheduled_operations_tenant_id ON process_scheduled_operations(tenant_id);
CREATE INDEX IF NOT EXISTS idx_scheduled_operations_type ON process_scheduled_operations(operation_type);
CREATE INDEX IF NOT EXISTS idx_scheduled_operations_active ON process_scheduled_operations(is_active);
CREATE INDEX IF NOT EXISTS idx_scheduled_operations_next_execution ON process_scheduled_operations(next_execution_at);

-- Process event triggers table
CREATE TABLE IF NOT EXISTS process_event_triggers (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    trigger_name TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK(event_type IN ('worker_started','worker_stopped','worker_crashed','config_changed','threshold_exceeded','schedule_time','webhook','api_call')),
    event_source TEXT NOT NULL,
    event_filter_json TEXT,
    trigger_actions_json TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    last_triggered_at TEXT,
    trigger_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_event_triggers_tenant_id ON process_event_triggers(tenant_id);
CREATE INDEX IF NOT EXISTS idx_event_triggers_type ON process_event_triggers(event_type);
CREATE INDEX IF NOT EXISTS idx_event_triggers_active ON process_event_triggers(is_active);

-- Process dependency management table
CREATE TABLE IF NOT EXISTS process_automation_dependencies (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    dependency_name TEXT NOT NULL,
    dependency_type TEXT NOT NULL CHECK(dependency_type IN ('worker','service','database','external_api','file_system','network')),
    dependency_id TEXT NOT NULL,
    dependency_status TEXT NOT NULL DEFAULT 'unknown' CHECK(dependency_status IN ('healthy','degraded','unhealthy','unknown')),
    health_check_url TEXT,
    health_check_interval_seconds INTEGER NOT NULL DEFAULT 300,
    timeout_seconds INTEGER NOT NULL DEFAULT 30,
    retry_count INTEGER NOT NULL DEFAULT 3,
    last_health_check_at TEXT,
    last_health_check_result TEXT,
    is_critical INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_process_dependencies_tenant_id ON process_automation_dependencies(tenant_id);
CREATE INDEX IF NOT EXISTS idx_process_dependencies_type ON process_automation_dependencies(dependency_type);
CREATE INDEX IF NOT EXISTS idx_process_dependencies_status ON process_automation_dependencies(dependency_status);
CREATE INDEX IF NOT EXISTS idx_process_dependencies_critical ON process_automation_dependencies(is_critical);

-- Process automation templates table
CREATE TABLE IF NOT EXISTS process_automation_templates (
    id TEXT PRIMARY KEY,
    template_name TEXT NOT NULL,
    template_category TEXT NOT NULL CHECK(template_category IN ('deployment','scaling','maintenance','monitoring','backup','testing')),
    description TEXT,
    template_definition_json TEXT NOT NULL,
    input_parameters_json TEXT,
    output_schema_json TEXT,
    tags_json TEXT,
    is_public INTEGER NOT NULL DEFAULT 0,
    usage_count INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_automation_templates_category ON process_automation_templates(template_category);
CREATE INDEX IF NOT EXISTS idx_automation_templates_public ON process_automation_templates(is_public);
CREATE INDEX IF NOT EXISTS idx_automation_templates_usage ON process_automation_templates(usage_count DESC);

-- Process automation notifications table
CREATE TABLE IF NOT EXISTS process_automation_notifications (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL REFERENCES process_automation_executions(id) ON DELETE CASCADE,
    notification_type TEXT NOT NULL CHECK(notification_type IN ('email','slack','webhook','sms','push')),
    recipient TEXT NOT NULL,
    subject TEXT,
    message TEXT NOT NULL,
    notification_data_json TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','sent','failed','delivered')),
    sent_at TEXT,
    delivered_at TEXT,
    error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_automation_notifications_execution_id ON process_automation_notifications(execution_id);
CREATE INDEX IF NOT EXISTS idx_automation_notifications_type ON process_automation_notifications(notification_type);
CREATE INDEX IF NOT EXISTS idx_automation_notifications_status ON process_automation_notifications(status);
