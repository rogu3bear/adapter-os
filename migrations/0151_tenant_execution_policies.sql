-- Tenant Execution Policies
-- Hierarchical policy model for determinism and routing enforcement
-- Contains: determinism policy, routing policy, golden verification policy

CREATE TABLE IF NOT EXISTS tenant_execution_policies (
    -- Primary key (UUID)
    id TEXT PRIMARY KEY,

    -- Tenant this policy belongs to
    tenant_id TEXT NOT NULL,

    -- Policy version for audit trail (increments on update)
    version INTEGER NOT NULL DEFAULT 1,

    -- Determinism policy as JSON
    -- Structure: {
    --   "allowed_modes": ["strict", "besteffort", "relaxed"],
    --   "default_mode": "besteffort",
    --   "require_seed": false,
    --   "allow_fallback": true,
    --   "replay_mode": "approximate"
    -- }
    determinism_policy_json TEXT NOT NULL DEFAULT '{}',

    -- Routing policy as JSON (optional)
    -- Structure: {
    --   "allowed_stack_ids": ["stack-1", "stack-2"] | null,
    --   "allowed_adapter_ids": ["adapter-1"] | null,
    --   "pin_enforcement": "warn" | "error",
    --   "require_stack": false,
    --   "require_pins": false
    -- }
    routing_policy_json TEXT,

    -- Golden verification policy as JSON (optional)
    -- Structure: {
    --   "fail_on_drift": false,
    --   "golden_baseline_id": "baseline-001" | null,
    --   "epsilon_threshold": 1e-6
    -- }
    golden_policy_json TEXT,

    -- Whether this policy is active (only one active policy per tenant)
    active INTEGER NOT NULL DEFAULT 1,

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    -- User who created/updated the policy
    created_by TEXT,

    -- Foreign key constraint
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Unique index: only one active policy per tenant
CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_exec_policies_active
    ON tenant_execution_policies(tenant_id)
    WHERE active = 1;

-- Lookup index by tenant
CREATE INDEX IF NOT EXISTS idx_tenant_exec_policies_tenant
    ON tenant_execution_policies(tenant_id);

-- Trigger to auto-update updated_at on modifications
CREATE TRIGGER IF NOT EXISTS tenant_execution_policies_updated_at
    AFTER UPDATE ON tenant_execution_policies
    FOR EACH ROW
    BEGIN
        UPDATE tenant_execution_policies SET updated_at = datetime('now')
        WHERE id = NEW.id;
    END;
