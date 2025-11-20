-- Stack versioning for telemetry correlation (PRD-03)
-- Migration: 0079
-- Enables tracking which stack version handled each inference request
--
-- Version increments are handled at application level in update_stack()
-- to avoid trigger recursion issues and maintain explicit control.

-- Add version column to adapter_stacks table
ALTER TABLE adapter_stacks ADD COLUMN version INTEGER NOT NULL DEFAULT 1;

-- Add index for version lookups
CREATE INDEX idx_adapter_stacks_version ON adapter_stacks(id, version);

-- Create view for active stacks with their current versions
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
