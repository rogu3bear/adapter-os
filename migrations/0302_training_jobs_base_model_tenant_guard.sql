-- Migration 0302: Enforce tenant isolation for repository_training_jobs -> models
--
-- Problem: repository_training_jobs.base_model_id references models(id), but that FK
-- does not enforce tenant ownership. A job could reference a model from another tenant.
--
-- Fix: Add trigger-based guards (SQLite cannot add composite FKs via ALTER).
-- Allow same-tenant models, shared system models, and legacy NULL-tenant models.

PRAGMA foreign_keys = ON;

CREATE TRIGGER IF NOT EXISTS trg_training_jobs_base_model_tenant_match_insert
BEFORE INSERT ON repository_training_jobs
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
  SELECT RAISE(ABORT, 'Tenant mismatch: repository_training_jobs.base_model_id references model from different tenant');
END;

CREATE TRIGGER IF NOT EXISTS trg_training_jobs_base_model_tenant_match_update_base_model
BEFORE UPDATE OF base_model_id ON repository_training_jobs
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
  SELECT RAISE(ABORT, 'Tenant mismatch: repository_training_jobs.base_model_id references model from different tenant');
END;

CREATE TRIGGER IF NOT EXISTS trg_training_jobs_base_model_tenant_match_update_tenant
BEFORE UPDATE OF tenant_id ON repository_training_jobs
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
  SELECT RAISE(ABORT, 'Tenant mismatch: repository_training_jobs.base_model_id references model from different tenant');
END;
