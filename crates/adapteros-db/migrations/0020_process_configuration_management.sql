-- Migration 0020: Process Configuration Management
-- Adds tables for configuration templates, versioning, validation, and management
-- Citation: docs/architecture.md, docs/control-plane.md

-- Process configuration templates table
CREATE TABLE IF NOT EXISTS process_config_templates (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    description TEXT,
    tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE,
    config_schema_json TEXT NOT NULL,
    default_values_json TEXT,
    validation_rules_json TEXT,
    environment_specific_configs_json TEXT,
    version TEXT NOT NULL DEFAULT '1.0.0',
    is_active INTEGER NOT NULL DEFAULT 1,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_config_templates_tenant_id ON process_config_templates(tenant_id);
CREATE INDEX IF NOT EXISTS idx_config_templates_name ON process_config_templates(name);
CREATE INDEX IF NOT EXISTS idx_config_templates_active ON process_config_templates(is_active);

-- Process configuration instances table
CREATE TABLE IF NOT EXISTS process_config_instances (
    id TEXT PRIMARY KEY,
    template_id TEXT NOT NULL REFERENCES process_config_templates(id) ON DELETE CASCADE,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    environment TEXT NOT NULL CHECK(environment IN ('development','staging','production','testing')),
    config_values_json TEXT NOT NULL,
    validation_status TEXT NOT NULL DEFAULT 'pending' CHECK(validation_status IN ('pending','valid','invalid','warning')),
    validation_errors_json TEXT,
    applied_at TEXT,
    applied_by TEXT REFERENCES users(id),
    rollback_config_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_config_instances_template_id ON process_config_instances(template_id);
CREATE INDEX IF NOT EXISTS idx_config_instances_worker_id ON process_config_instances(worker_id);
CREATE INDEX IF NOT EXISTS idx_config_instances_environment ON process_config_instances(environment);
CREATE INDEX IF NOT EXISTS idx_config_instances_validation ON process_config_instances(validation_status);

-- Process configuration history table
CREATE TABLE IF NOT EXISTS process_config_history (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL REFERENCES process_config_instances(id) ON DELETE CASCADE,
    version TEXT NOT NULL,
    config_values_json TEXT NOT NULL,
    change_type TEXT NOT NULL CHECK(change_type IN ('create','update','rollback','apply')),
    change_description TEXT,
    changed_by TEXT REFERENCES users(id),
    changed_at TEXT NOT NULL DEFAULT (datetime('now')),
    diff_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_config_history_instance_id ON process_config_history(instance_id, changed_at DESC);
CREATE INDEX IF NOT EXISTS idx_config_history_change_type ON process_config_history(change_type);
CREATE INDEX IF NOT EXISTS idx_config_history_changed_by ON process_config_history(changed_by);

-- Process configuration validation results table
CREATE TABLE IF NOT EXISTS process_config_validation_results (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL REFERENCES process_config_instances(id) ON DELETE CASCADE,
    validation_type TEXT NOT NULL CHECK(validation_type IN ('schema','business_rules','security','performance','compliance')),
    status TEXT NOT NULL CHECK(status IN ('pass','fail','warning','skipped')),
    message TEXT NOT NULL,
    details_json TEXT,
    validated_at TEXT NOT NULL DEFAULT (datetime('now')),
    validated_by TEXT REFERENCES users(id)
);

CREATE INDEX IF NOT EXISTS idx_config_validation_instance_id ON process_config_validation_results(instance_id);
CREATE INDEX IF NOT EXISTS idx_config_validation_type ON process_config_validation_results(validation_type);
CREATE INDEX IF NOT EXISTS idx_config_validation_status ON process_config_validation_results(status);

-- Process configuration deployment tracking table
CREATE TABLE IF NOT EXISTS process_config_deployments (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL REFERENCES process_config_instances(id) ON DELETE CASCADE,
    deployment_type TEXT NOT NULL CHECK(deployment_type IN ('immediate','scheduled','rolling','blue_green')),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','in_progress','completed','failed','rolled_back')),
    scheduled_at TEXT,
    started_at TEXT,
    completed_at TEXT,
    deployed_by TEXT REFERENCES users(id),
    deployment_config_json TEXT,
    rollback_plan_json TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_config_deployments_instance_id ON process_config_deployments(instance_id);
CREATE INDEX IF NOT EXISTS idx_config_deployments_status ON process_config_deployments(status);
CREATE INDEX IF NOT EXISTS idx_config_deployments_type ON process_config_deployments(deployment_type);

-- Process configuration compliance checks table
CREATE TABLE IF NOT EXISTS process_config_compliance_checks (
    id TEXT PRIMARY KEY,
    instance_id TEXT NOT NULL REFERENCES process_config_instances(id) ON DELETE CASCADE,
    compliance_standard TEXT NOT NULL CHECK(compliance_standard IN ('SOC2','ISO27001','GDPR','HIPAA','PCI_DSS','ITAR')),
    check_name TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('compliant','non_compliant','not_applicable','requires_review')),
    details_json TEXT,
    remediation_steps_json TEXT,
    checked_at TEXT NOT NULL DEFAULT (datetime('now')),
    checked_by TEXT REFERENCES users(id)
);

CREATE INDEX IF NOT EXISTS idx_config_compliance_instance_id ON process_config_compliance_checks(instance_id);
CREATE INDEX IF NOT EXISTS idx_config_compliance_standard ON process_config_compliance_checks(compliance_standard);
CREATE INDEX IF NOT EXISTS idx_config_compliance_status ON process_config_compliance_checks(status);
