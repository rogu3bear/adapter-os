-- Migration 0226: Add tenant isolation triggers for base_model_id references
--
-- Problem: adapters.base_model_id and adapter_repositories.base_model_id both reference
-- models(id), but there's no trigger enforcement that the referenced model belongs to
-- the same tenant. This permits cross-tenant base model references.
--
-- Fix: Add trigger-based guards (similar to migration 0131/0211 pattern).
-- Required for PRD-RECT-004 tenant isolation rectification.

PRAGMA foreign_keys = ON;

-- Guard: adapters.base_model_id tenant must match models.tenant_id (insert)
CREATE TRIGGER IF NOT EXISTS trg_adapters_base_model_tenant_check
BEFORE INSERT ON adapters
FOR EACH ROW
WHEN NEW.base_model_id IS NOT NULL
  AND (SELECT tenant_id FROM models WHERE id = NEW.base_model_id) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Tenant mismatch: adapters.base_model_id references model from different tenant');
END;

-- Guard: adapters.base_model_id tenant must match models.tenant_id (update)
CREATE TRIGGER IF NOT EXISTS trg_adapters_base_model_tenant_check_update
BEFORE UPDATE OF base_model_id ON adapters
FOR EACH ROW
WHEN NEW.base_model_id IS NOT NULL
  AND (SELECT tenant_id FROM models WHERE id = NEW.base_model_id) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Tenant mismatch: adapters.base_model_id references model from different tenant');
END;

-- Guard: adapter_repositories.base_model_id tenant must match models.tenant_id (insert)
CREATE TRIGGER IF NOT EXISTS trg_adapter_repositories_base_model_tenant_check
BEFORE INSERT ON adapter_repositories
FOR EACH ROW
WHEN NEW.base_model_id IS NOT NULL
  AND (SELECT tenant_id FROM models WHERE id = NEW.base_model_id) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Tenant mismatch: adapter_repositories.base_model_id references model from different tenant');
END;

-- Guard: adapter_repositories.base_model_id tenant must match models.tenant_id (update)
CREATE TRIGGER IF NOT EXISTS trg_adapter_repositories_base_model_tenant_check_update
BEFORE UPDATE OF base_model_id ON adapter_repositories
FOR EACH ROW
WHEN NEW.base_model_id IS NOT NULL
  AND (SELECT tenant_id FROM models WHERE id = NEW.base_model_id) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Tenant mismatch: adapter_repositories.base_model_id references model from different tenant');
END;
