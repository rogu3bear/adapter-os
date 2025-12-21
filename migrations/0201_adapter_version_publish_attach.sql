-- Migration: 0201_adapter_version_publish_attach.sql
-- Adapter Publish + Attach Modes v1
-- Adds publish workflow, attach modes, and archive capabilities to adapter_versions

PRAGMA foreign_keys = ON;

-- Add attach_mode column: 'free' (default) or 'requires_dataset'
ALTER TABLE adapter_versions ADD COLUMN attach_mode TEXT NOT NULL DEFAULT 'free'
    CHECK (attach_mode IN ('free', 'requires_dataset'));

-- Add required_scope_dataset_version_id: FK to training_dataset_versions
-- Used when attach_mode = 'requires_dataset' to pin to a specific dataset version
ALTER TABLE adapter_versions ADD COLUMN required_scope_dataset_version_id TEXT
    REFERENCES training_dataset_versions(id) ON DELETE SET NULL;

-- Add is_archived flag: 0 = visible, 1 = archived (hidden from normal use)
-- Separate from lifecycle_state to preserve audit trail
ALTER TABLE adapter_versions ADD COLUMN is_archived INTEGER NOT NULL DEFAULT 0
    CHECK (is_archived IN (0, 1));

-- Add published_at timestamp: NULL = unpublished, non-NULL = published
ALTER TABLE adapter_versions ADD COLUMN published_at TEXT;

-- Add short_description for published adapters (max 280 chars enforced at application level)
ALTER TABLE adapter_versions ADD COLUMN short_description TEXT;

-- Index for finding published adapter versions efficiently
CREATE INDEX IF NOT EXISTS idx_adapter_versions_published
    ON adapter_versions(tenant_id, repo_id, published_at)
    WHERE published_at IS NOT NULL;

-- Index for filtering out archived versions
CREATE INDEX IF NOT EXISTS idx_adapter_versions_archived
    ON adapter_versions(tenant_id, is_archived)
    WHERE is_archived = 1;

-- Index for filtering by attach mode
CREATE INDEX IF NOT EXISTS idx_adapter_versions_attach_mode
    ON adapter_versions(tenant_id, attach_mode);

-- Trigger: When attach_mode = 'free', required_scope_dataset_version_id must be NULL
CREATE TRIGGER IF NOT EXISTS trg_adapter_version_free_mode_no_scope
BEFORE INSERT ON adapter_versions
FOR EACH ROW
WHEN NEW.attach_mode = 'free' AND NEW.required_scope_dataset_version_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'required_scope_dataset_version_id must be NULL when attach_mode is free');
END;

CREATE TRIGGER IF NOT EXISTS trg_adapter_version_free_mode_no_scope_update
BEFORE UPDATE ON adapter_versions
FOR EACH ROW
WHEN NEW.attach_mode = 'free' AND NEW.required_scope_dataset_version_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'required_scope_dataset_version_id must be NULL when attach_mode is free');
END;

-- Trigger: When attach_mode = 'requires_dataset', required_scope_dataset_version_id must be set
CREATE TRIGGER IF NOT EXISTS trg_adapter_version_requires_dataset_needs_scope
BEFORE INSERT ON adapter_versions
FOR EACH ROW
WHEN NEW.attach_mode = 'requires_dataset' AND NEW.required_scope_dataset_version_id IS NULL
BEGIN
    SELECT RAISE(ABORT, 'required_scope_dataset_version_id is required when attach_mode is requires_dataset');
END;

CREATE TRIGGER IF NOT EXISTS trg_adapter_version_requires_dataset_needs_scope_update
BEFORE UPDATE ON adapter_versions
FOR EACH ROW
WHEN NEW.attach_mode = 'requires_dataset' AND NEW.required_scope_dataset_version_id IS NULL
BEGIN
    SELECT RAISE(ABORT, 'required_scope_dataset_version_id is required when attach_mode is requires_dataset');
END;

-- Trigger: Tenant isolation for required_scope_dataset_version_id
-- Ensures the referenced dataset version belongs to the same tenant
CREATE TRIGGER IF NOT EXISTS trg_adapter_version_scope_tenant_check
BEFORE INSERT ON adapter_versions
FOR EACH ROW
WHEN NEW.required_scope_dataset_version_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN NOT EXISTS (
            SELECT 1 FROM training_dataset_versions tdv
            WHERE tdv.id = NEW.required_scope_dataset_version_id
              AND tdv.tenant_id = NEW.tenant_id
        ) THEN RAISE(ABORT, 'required_scope_dataset_version_id tenant mismatch or not found')
    END;
END;

CREATE TRIGGER IF NOT EXISTS trg_adapter_version_scope_tenant_check_update
BEFORE UPDATE ON adapter_versions
FOR EACH ROW
WHEN NEW.required_scope_dataset_version_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN NOT EXISTS (
            SELECT 1 FROM training_dataset_versions tdv
            WHERE tdv.id = NEW.required_scope_dataset_version_id
              AND tdv.tenant_id = NEW.tenant_id
        ) THEN RAISE(ABORT, 'required_scope_dataset_version_id tenant mismatch or not found')
    END;
END;
