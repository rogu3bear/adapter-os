-- Migration: 0201_adapter_version_publish_attach.sql (PostgreSQL)
-- Adapter Publish + Attach Modes v1
-- Adds publish workflow, attach modes, and archive capabilities to adapter_versions

-- Add attach_mode column: 'free' (default) or 'requires_dataset'
ALTER TABLE adapter_versions ADD COLUMN IF NOT EXISTS attach_mode TEXT NOT NULL DEFAULT 'free';
ALTER TABLE adapter_versions ADD CONSTRAINT chk_adapter_versions_attach_mode
    CHECK (attach_mode IN ('free', 'requires_dataset'));

-- Add required_scope_dataset_version_id: FK to training_dataset_versions
ALTER TABLE adapter_versions ADD COLUMN IF NOT EXISTS required_scope_dataset_version_id TEXT
    REFERENCES training_dataset_versions(id) ON DELETE SET NULL;

-- Add is_archived flag
ALTER TABLE adapter_versions ADD COLUMN IF NOT EXISTS is_archived BOOLEAN NOT NULL DEFAULT FALSE;

-- Add published_at timestamp
ALTER TABLE adapter_versions ADD COLUMN IF NOT EXISTS published_at TIMESTAMPTZ;

-- Add short_description
ALTER TABLE adapter_versions ADD COLUMN IF NOT EXISTS short_description TEXT;

-- Indexes
CREATE INDEX IF NOT EXISTS idx_adapter_versions_published
    ON adapter_versions(tenant_id, repo_id, published_at)
    WHERE published_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_adapter_versions_archived
    ON adapter_versions(tenant_id, is_archived)
    WHERE is_archived = TRUE;

CREATE INDEX IF NOT EXISTS idx_adapter_versions_attach_mode
    ON adapter_versions(tenant_id, attach_mode);

-- Function and trigger for attach mode constraints
CREATE OR REPLACE FUNCTION check_adapter_version_attach_mode()
RETURNS TRIGGER AS $$
BEGIN
    -- When attach_mode = 'free', required_scope_dataset_version_id must be NULL
    IF NEW.attach_mode = 'free' AND NEW.required_scope_dataset_version_id IS NOT NULL THEN
        RAISE EXCEPTION 'required_scope_dataset_version_id must be NULL when attach_mode is free';
    END IF;

    -- When attach_mode = 'requires_dataset', required_scope_dataset_version_id must be set
    IF NEW.attach_mode = 'requires_dataset' AND NEW.required_scope_dataset_version_id IS NULL THEN
        RAISE EXCEPTION 'required_scope_dataset_version_id is required when attach_mode is requires_dataset';
    END IF;

    -- Tenant isolation check
    IF NEW.required_scope_dataset_version_id IS NOT NULL THEN
        IF NOT EXISTS (
            SELECT 1 FROM training_dataset_versions tdv
            WHERE tdv.id = NEW.required_scope_dataset_version_id
              AND tdv.tenant_id = NEW.tenant_id
        ) THEN
            RAISE EXCEPTION 'required_scope_dataset_version_id tenant mismatch or not found';
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_adapter_version_attach_mode ON adapter_versions;
CREATE TRIGGER trg_adapter_version_attach_mode
    BEFORE INSERT OR UPDATE ON adapter_versions
    FOR EACH ROW
    EXECUTE FUNCTION check_adapter_version_attach_mode();
