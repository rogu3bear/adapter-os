-- Stack versioning for telemetry correlation (PRD-03)
-- Enables tracking which stack version handled each inference request

-- Add version column to adapter_stacks table
ALTER TABLE adapter_stacks ADD COLUMN version INTEGER NOT NULL DEFAULT 1;

-- Add index for version lookups
CREATE INDEX idx_adapter_stacks_version ON adapter_stacks(id, version);

-- Create view for active stacks with their current versions
CREATE VIEW IF NOT EXISTS active_stacks_with_version AS
SELECT
    id,
    name,
    version,
    adapter_ids_json,
    workflow_type,
    created_at,
    updated_at
FROM adapter_stacks;

-- Add trigger to auto-increment version on stack updates
-- This ensures every modification creates a new version for audit trail
CREATE TRIGGER IF NOT EXISTS auto_increment_stack_version
AFTER UPDATE ON adapter_stacks
FOR EACH ROW
WHEN NEW.adapter_ids_json != OLD.adapter_ids_json
    OR (NEW.workflow_type IS NOT NULL AND OLD.workflow_type IS NOT NULL AND NEW.workflow_type != OLD.workflow_type)
    OR (NEW.workflow_type IS NULL AND OLD.workflow_type IS NOT NULL)
    OR (NEW.workflow_type IS NOT NULL AND OLD.workflow_type IS NULL)
BEGIN
    UPDATE adapter_stacks
    SET version = version + 1,
        updated_at = datetime('now')
    WHERE id = NEW.id;
END;
