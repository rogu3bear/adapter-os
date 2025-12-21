-- Migration 0223: Enforce tenant isolation for repository_training_jobs -> adapters
--
-- Problem: repository_training_jobs has both adapter_id and tenant_id, but the FK on adapter_id
-- does not enforce that the referenced adapter row belongs to the same tenant.
-- This permits cross-tenant references.
--
-- Fix: Add trigger-based guards (SQLite cannot add composite FKs via ALTER).
--
-- PRD-RECT-004: Training job tenant guard triggers

PRAGMA foreign_keys = ON;

-- Guard: training job tenant must match adapter tenant (insert)
CREATE TRIGGER IF NOT EXISTS trg_training_jobs_adapter_tenant_match_insert
BEFORE INSERT ON repository_training_jobs
FOR EACH ROW
WHEN NEW.adapter_id IS NOT NULL
  AND (
    SELECT tenant_id FROM adapters WHERE id = NEW.adapter_id
  ) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Tenant mismatch: repository_training_jobs.tenant_id must match adapters.tenant_id');
END;

-- Guard: training job tenant must match adapter tenant (update adapter_id)
CREATE TRIGGER IF NOT EXISTS trg_training_jobs_adapter_tenant_match_update_adapter
BEFORE UPDATE OF adapter_id ON repository_training_jobs
FOR EACH ROW
WHEN NEW.adapter_id IS NOT NULL
  AND (
    SELECT tenant_id FROM adapters WHERE id = NEW.adapter_id
  ) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Tenant mismatch: repository_training_jobs.tenant_id must match adapters.tenant_id');
END;

-- Guard: training job tenant must match adapter tenant (update tenant_id)
CREATE TRIGGER IF NOT EXISTS trg_training_jobs_adapter_tenant_match_update_tenant
BEFORE UPDATE OF tenant_id ON repository_training_jobs
FOR EACH ROW
WHEN NEW.adapter_id IS NOT NULL
  AND (
    SELECT tenant_id FROM adapters WHERE id = NEW.adapter_id
  ) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'Tenant mismatch: repository_training_jobs.tenant_id must match adapters.tenant_id');
END;
