-- Migration 0018: Advanced Process Control
-- Adds tables for bulk operations, templates, auto-scaling, and process dependencies
-- Citation: docs/architecture.md, docs/runaway-prevention.md

-- Process templates table for reusable worker configurations
CREATE TABLE IF NOT EXISTS process_templates (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    description TEXT,
    tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE,
    config_json TEXT NOT NULL,
    plan_id TEXT REFERENCES plans(id),
    auto_scaling_config_json TEXT,
    dependencies_json TEXT,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_process_templates_tenant_id ON process_templates(tenant_id);
CREATE INDEX IF NOT EXISTS idx_process_templates_name ON process_templates(name);

-- Process bulk operations table
CREATE TABLE IF NOT EXISTS process_bulk_operations (
    id TEXT PRIMARY KEY,
    operation_type TEXT NOT NULL CHECK(operation_type IN ('start','stop','restart','migrate','scale')),
    tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE,
    target_workers_json TEXT NOT NULL,
    config_json TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','running','completed','failed','cancelled')),
    progress_json TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    error_message TEXT,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_bulk_operations_tenant_id ON process_bulk_operations(tenant_id);
CREATE INDEX IF NOT EXISTS idx_bulk_operations_status ON process_bulk_operations(status);
CREATE INDEX IF NOT EXISTS idx_bulk_operations_type ON process_bulk_operations(operation_type);

-- Process auto-scaling rules table
CREATE TABLE IF NOT EXISTS process_auto_scaling_rules (
    id TEXT PRIMARY KEY,
    tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE,
    rule_name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    metric_type TEXT NOT NULL CHECK(metric_type IN ('cpu','memory','requests','latency','custom')),
    threshold_value REAL NOT NULL,
    threshold_duration_seconds INTEGER NOT NULL DEFAULT 300,
    scale_action TEXT NOT NULL CHECK(scale_action IN ('scale_up','scale_down','scale_out','scale_in')),
    scale_factor REAL NOT NULL DEFAULT 1.0,
    min_workers INTEGER NOT NULL DEFAULT 1,
    max_workers INTEGER NOT NULL DEFAULT 10,
    cooldown_seconds INTEGER NOT NULL DEFAULT 600,
    last_triggered_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_auto_scaling_rules_tenant_id ON process_auto_scaling_rules(tenant_id);
CREATE INDEX IF NOT EXISTS idx_auto_scaling_rules_enabled ON process_auto_scaling_rules(enabled);

-- Process dependencies table
CREATE TABLE IF NOT EXISTS process_dependencies (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    depends_on_worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    dependency_type TEXT NOT NULL CHECK(dependency_type IN ('startup','shutdown','health','data')),
    condition_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_process_dependencies_worker_id ON process_dependencies(worker_id);
CREATE INDEX IF NOT EXISTS idx_process_dependencies_depends_on ON process_dependencies(depends_on_worker_id);

-- Process migration tracking table
CREATE TABLE IF NOT EXISTS process_migrations (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    source_node_id TEXT NOT NULL REFERENCES nodes(id),
    target_node_id TEXT NOT NULL REFERENCES nodes(id),
    migration_type TEXT NOT NULL CHECK(migration_type IN ('live','cold','warm')),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','preparing','migrating','completed','failed','rolled_back')),
    migration_config_json TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    error_message TEXT,
    rollback_data_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_process_migrations_worker_id ON process_migrations(worker_id);
CREATE INDEX IF NOT EXISTS idx_process_migrations_status ON process_migrations(status);
CREATE INDEX IF NOT EXISTS idx_process_migrations_source_node ON process_migrations(source_node_id);
CREATE INDEX IF NOT EXISTS idx_process_migrations_target_node ON process_migrations(target_node_id);

-- Process orchestration workflows table
CREATE TABLE IF NOT EXISTS process_orchestration_workflows (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    tenant_id TEXT REFERENCES tenants(id) ON DELETE CASCADE,
    workflow_type TEXT NOT NULL CHECK(workflow_type IN ('deployment','scaling','maintenance','recovery')),
    steps_json TEXT NOT NULL,
    triggers_json TEXT,
    status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','active','paused','completed','failed')),
    last_executed_at TEXT,
    execution_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_orchestration_workflows_tenant_id ON process_orchestration_workflows(tenant_id);
CREATE INDEX IF NOT EXISTS idx_orchestration_workflows_status ON process_orchestration_workflows(status);
CREATE INDEX IF NOT EXISTS idx_orchestration_workflows_type ON process_orchestration_workflows(workflow_type);
