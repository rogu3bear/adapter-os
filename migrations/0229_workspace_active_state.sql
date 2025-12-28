-- Migration 0229: Workspace active state tracking
-- Stores the currently active base model, plan, adapters, and manifest hash per workspace/tenant.

CREATE TABLE IF NOT EXISTS workspace_active_state (
    tenant_id TEXT PRIMARY KEY,
    active_base_model_id TEXT,
    active_plan_id TEXT,
    active_adapter_ids TEXT,
    manifest_hash_b3 TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (active_base_model_id) REFERENCES models(id),
    FOREIGN KEY (active_plan_id) REFERENCES plans(id)
);

CREATE TRIGGER IF NOT EXISTS trg_workspace_active_state_updated
AFTER UPDATE ON workspace_active_state
BEGIN
    UPDATE workspace_active_state
    SET updated_at = datetime('now')
    WHERE tenant_id = NEW.tenant_id;
END;
