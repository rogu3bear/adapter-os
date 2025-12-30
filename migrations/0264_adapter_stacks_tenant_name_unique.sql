-- Migration: 0264
-- Purpose: Allow identical stack names across tenants while enforcing tenant-scoped uniqueness

-- Drop dependent triggers and views before table recreation
DROP TRIGGER IF EXISTS trg_chat_sessions_stack_tenant_check;
DROP TRIGGER IF EXISTS trg_chat_sessions_stack_tenant_check_update;
DROP TRIGGER IF EXISTS trg_routing_decisions_stack_tenant_check;
DROP TRIGGER IF EXISTS trg_routing_decisions_stack_tenant_check_update;

DROP VIEW IF EXISTS routing_decisions_enriched;
DROP VIEW IF EXISTS recent_stack_lifecycle_changes;
DROP VIEW IF EXISTS stacks_lifecycle_summary;
DROP VIEW IF EXISTS active_stacks_with_version;

-- Recreate adapter_stacks without global UNIQUE(name)
CREATE TABLE adapter_stacks_new (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    adapter_ids_json TEXT NOT NULL,
    workflow_type TEXT,
    version TEXT NOT NULL DEFAULT '1.0.0',
    lifecycle_state TEXT NOT NULL DEFAULT 'active',
    created_by TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    determinism_mode TEXT,
    routing_determinism_mode TEXT,
    metadata_json TEXT,
    CONSTRAINT valid_workflow_type CHECK (
        workflow_type IS NULL OR
        workflow_type IN ('Parallel', 'UpstreamDownstream', 'Sequential')
    ),
    CONSTRAINT fk_adapter_stacks_tenant FOREIGN KEY (tenant_id)
        REFERENCES tenants(id) ON DELETE CASCADE,
    CONSTRAINT unique_adapter_stacks_tenant_name UNIQUE (tenant_id, name)
);

-- Copy existing data
INSERT INTO adapter_stacks_new (
    id,
    tenant_id,
    name,
    description,
    adapter_ids_json,
    workflow_type,
    version,
    lifecycle_state,
    created_by,
    created_at,
    updated_at,
    determinism_mode,
    routing_determinism_mode,
    metadata_json
)
SELECT
    id,
    tenant_id,
    name,
    description,
    adapter_ids_json,
    workflow_type,
    version,
    lifecycle_state,
    created_by,
    created_at,
    updated_at,
    determinism_mode,
    routing_determinism_mode,
    metadata_json
FROM adapter_stacks;

-- Swap tables
DROP TABLE adapter_stacks;
ALTER TABLE adapter_stacks_new RENAME TO adapter_stacks;

-- Recreate indexes
CREATE INDEX idx_adapter_stacks_name ON adapter_stacks(name);
CREATE INDEX idx_adapter_stacks_created_at ON adapter_stacks(created_at DESC);
CREATE INDEX idx_adapter_stacks_tenant ON adapter_stacks(tenant_id);
CREATE UNIQUE INDEX idx_adapter_stacks_tenant_id_composite
    ON adapter_stacks(tenant_id, id);
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_determinism_mode
    ON adapter_stacks(determinism_mode)
    WHERE determinism_mode IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_tenant_name_active
    ON adapter_stacks(tenant_id, name, lifecycle_state)
    WHERE lifecycle_state = 'active';

-- Recreate stack name validation trigger
CREATE TRIGGER IF NOT EXISTS validate_stack_name_format
BEFORE INSERT ON adapter_stacks
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.name NOT GLOB 'stack.[a-z0-9]*[a-z0-9]'
            AND NEW.name NOT GLOB 'stack.[a-z0-9]*[a-z0-9].[a-z0-9]*[a-z0-9]'
        THEN RAISE(ABORT, 'Invalid stack name format: must match stack.{namespace}[.{identifier}]')
    END;

    SELECT CASE
        WHEN length(NEW.name) > 100
        THEN RAISE(ABORT, 'Stack name exceeds 100 character limit')
    END;

    SELECT CASE
        WHEN NEW.name LIKE '%---%'
        THEN RAISE(ABORT, 'Stack name cannot contain consecutive hyphens')
    END;

    SELECT CASE
        WHEN NEW.name IN ('stack.safe-default', 'stack.system')
        THEN RAISE(ABORT, 'Stack name is reserved')
    END;
END;

-- Recreate cross-tenant adapter validation triggers
CREATE TRIGGER IF NOT EXISTS trg_adapter_stacks_cross_tenant_insert
BEFORE INSERT ON adapter_stacks
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM json_each(NEW.adapter_ids_json) AS ae
    LEFT JOIN adapters a ON ae.value = a.id
    WHERE a.id IS NOT NULL AND a.tenant_id != NEW.tenant_id
)
BEGIN
    SELECT RAISE(ABORT, 'Tenant isolation violation: all adapters in adapter_ids_json must belong to the same tenant as the stack');
END;

CREATE TRIGGER IF NOT EXISTS trg_adapter_stacks_cross_tenant_update_adapters
BEFORE UPDATE OF adapter_ids_json ON adapter_stacks
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM json_each(NEW.adapter_ids_json) AS ae
    LEFT JOIN adapters a ON ae.value = a.id
    WHERE a.id IS NOT NULL AND a.tenant_id != NEW.tenant_id
)
BEGIN
    SELECT RAISE(ABORT, 'Tenant isolation violation: all adapters in adapter_ids_json must belong to the same tenant as the stack');
END;

CREATE TRIGGER IF NOT EXISTS trg_adapter_stacks_cross_tenant_update_tenant
BEFORE UPDATE OF tenant_id ON adapter_stacks
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
    FROM json_each(NEW.adapter_ids_json) AS ae
    LEFT JOIN adapters a ON ae.value = a.id
    WHERE a.id IS NOT NULL AND a.tenant_id != NEW.tenant_id
)
BEGIN
    SELECT RAISE(ABORT, 'Tenant isolation violation: all adapters in adapter_ids_json must belong to the same tenant as the stack');
END;

-- Recreate tenant isolation triggers for stack references
CREATE TRIGGER IF NOT EXISTS trg_chat_sessions_stack_tenant_check
BEFORE INSERT ON chat_sessions
FOR EACH ROW
WHEN NEW.stack_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_stacks WHERE id = NEW.stack_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: chat_session.stack_id references stack from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_chat_sessions_stack_tenant_check_update
BEFORE UPDATE OF stack_id ON chat_sessions
FOR EACH ROW
WHEN NEW.stack_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_stacks WHERE id = NEW.stack_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: chat_session.stack_id references stack from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_routing_decisions_stack_tenant_check
BEFORE INSERT ON routing_decisions
FOR EACH ROW
WHEN NEW.stack_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_stacks WHERE id = NEW.stack_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: routing_decision.stack_id references stack from different tenant')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_routing_decisions_stack_tenant_check_update
BEFORE UPDATE OF stack_id ON routing_decisions
FOR EACH ROW
WHEN NEW.stack_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT tenant_id FROM adapter_stacks WHERE id = NEW.stack_id) != NEW.tenant_id
        THEN RAISE(ABORT, 'Tenant mismatch: routing_decision.stack_id references stack from different tenant')
    END;
END;

-- Recreate dependent views
CREATE VIEW IF NOT EXISTS routing_decisions_enriched AS
SELECT
    rd.*,
    s.name AS stack_name,
    s.workflow_type,
    COUNT(DISTINCT json_extract(value, '$.adapter_idx')) AS num_candidates
FROM routing_decisions rd
LEFT JOIN adapter_stacks s ON rd.stack_id = s.id,
     json_each(rd.candidate_adapters) AS candidate
GROUP BY rd.id;

CREATE VIEW IF NOT EXISTS recent_stack_lifecycle_changes AS
SELECT
    svh.id,
    svh.stack_id,
    s.name AS stack_name,
    svh.version,
    svh.previous_lifecycle_state,
    svh.lifecycle_state,
    svh.reason,
    svh.initiated_by,
    svh.created_at
FROM stack_version_history svh
LEFT JOIN adapter_stacks s ON svh.stack_id = s.id
WHERE svh.created_at >= datetime('now', '-30 days')
ORDER BY svh.created_at DESC;

CREATE VIEW IF NOT EXISTS stacks_lifecycle_summary AS
SELECT
    s.id AS stack_id,
    s.name,
    s.tenant_id,
    s.lifecycle_state,
    s.version,
    COUNT(svh.id) AS total_transitions,
    MAX(svh.created_at) AS last_transition_at
FROM adapter_stacks s
LEFT JOIN stack_version_history svh ON s.id = svh.stack_id
GROUP BY s.id, s.name, s.tenant_id, s.lifecycle_state, s.version;

CREATE VIEW IF NOT EXISTS active_stacks_with_version AS
SELECT
    id,
    tenant_id,
    name,
    version,
    adapter_ids_json,
    workflow_type,
    created_at,
    updated_at
FROM adapter_stacks;
