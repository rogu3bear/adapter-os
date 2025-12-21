-- Migration 0223: Validate adapter_stacks.adapter_ids_json cross-tenant isolation
--
-- Problem: adapter_stacks.adapter_ids_json is a JSON array of adapter IDs, but
-- there is no enforcement that all referenced adapters belong to the same tenant
-- as the stack. This permits cross-tenant adapter references in stacks.
--
-- Fix: Add trigger-based guards to validate that all adapters in adapter_ids_json
-- belong to the same tenant as the stack (SQLite json_each for array iteration).

PRAGMA foreign_keys = ON;

-- Guard: all adapters in adapter_ids_json must match stack tenant (insert)
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

-- Guard: all adapters in adapter_ids_json must match stack tenant (update adapter_ids_json)
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

-- Guard: all adapters in adapter_ids_json must match stack tenant (update tenant_id)
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
