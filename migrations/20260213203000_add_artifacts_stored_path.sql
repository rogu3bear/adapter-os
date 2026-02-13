-- Allow shared system base models in tenant guard triggers.
--
-- Migration 0226 added strict tenant checks for adapters.base_model_id and
-- adapter_repositories.base_model_id. Model visibility rules already treat
-- tenant_id='system' models as shared across tenants, so strict equality
-- rejects valid references and causes late registration failures.
--
-- Keep isolation for true cross-tenant references while allowing:
-- - same-tenant models
-- - system models
-- - legacy NULL-tenant models

PRAGMA foreign_keys = ON;

DROP TRIGGER IF EXISTS trg_adapters_base_model_tenant_check;
DROP TRIGGER IF EXISTS trg_adapters_base_model_tenant_check_update;
DROP TRIGGER IF EXISTS trg_adapter_repositories_base_model_tenant_check;
DROP TRIGGER IF EXISTS trg_adapter_repositories_base_model_tenant_check_update;

CREATE TRIGGER trg_adapters_base_model_tenant_check
BEFORE INSERT ON adapters
FOR EACH ROW
WHEN NEW.base_model_id IS NOT NULL
  AND EXISTS (
    SELECT 1
    FROM models m
    WHERE m.id = NEW.base_model_id
      AND m.tenant_id IS NOT NULL
      AND m.tenant_id != 'system'
      AND m.tenant_id != NEW.tenant_id
  )
BEGIN
  SELECT RAISE(ABORT, 'Tenant mismatch: adapters.base_model_id references model from different tenant');
END;

CREATE TRIGGER trg_adapters_base_model_tenant_check_update
BEFORE UPDATE OF base_model_id ON adapters
FOR EACH ROW
WHEN NEW.base_model_id IS NOT NULL
  AND EXISTS (
    SELECT 1
    FROM models m
    WHERE m.id = NEW.base_model_id
      AND m.tenant_id IS NOT NULL
      AND m.tenant_id != 'system'
      AND m.tenant_id != NEW.tenant_id
  )
BEGIN
  SELECT RAISE(ABORT, 'Tenant mismatch: adapters.base_model_id references model from different tenant');
END;

CREATE TRIGGER trg_adapter_repositories_base_model_tenant_check
BEFORE INSERT ON adapter_repositories
FOR EACH ROW
WHEN NEW.base_model_id IS NOT NULL
  AND EXISTS (
    SELECT 1
    FROM models m
    WHERE m.id = NEW.base_model_id
      AND m.tenant_id IS NOT NULL
      AND m.tenant_id != 'system'
      AND m.tenant_id != NEW.tenant_id
  )
BEGIN
  SELECT RAISE(ABORT, 'Tenant mismatch: adapter_repositories.base_model_id references model from different tenant');
END;

CREATE TRIGGER trg_adapter_repositories_base_model_tenant_check_update
BEFORE UPDATE OF base_model_id ON adapter_repositories
FOR EACH ROW
WHEN NEW.base_model_id IS NOT NULL
  AND EXISTS (
    SELECT 1
    FROM models m
    WHERE m.id = NEW.base_model_id
      AND m.tenant_id IS NOT NULL
      AND m.tenant_id != 'system'
      AND m.tenant_id != NEW.tenant_id
  )
BEGIN
  SELECT RAISE(ABORT, 'Tenant mismatch: adapter_repositories.base_model_id references model from different tenant');
END;
