-- Database Critical Fixes
-- Migration: 0105
-- Created: 2025-11-27
-- Purpose: Fix critical database integrity issues
--   1. Add FK cascade constraint on adapter_stacks.tenant_id
--   2. Add CHECK constraint for adapters.activation_count >= 0
--   3. Add FK cascade constraint on adapter_activations.adapter_id
--
-- CRITICAL FIXES:
-- - FK cascade prevents orphaned stacks when tenant deleted
-- - CHECK constraint prevents negative activation counts
-- - FK cascade prevents orphaned activations when adapter deleted

-- ============================================================================
-- Fix 1: Add FK constraint to adapter_stacks.tenant_id with CASCADE
-- ============================================================================
-- SQLite doesn't support adding FK constraints to existing columns directly.
-- We must recreate the table with the FK constraint.

-- Step 0: Drop all dependent views (will recreate after table recreation)
DROP VIEW IF EXISTS routing_decisions_enriched;
DROP VIEW IF EXISTS recent_stack_lifecycle_changes;
DROP VIEW IF EXISTS stacks_lifecycle_summary;
DROP VIEW IF EXISTS active_stacks_with_version;

-- Step 1: Create new table with FK constraint
CREATE TABLE adapter_stacks_new (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    name TEXT UNIQUE NOT NULL,
    description TEXT,
    adapter_ids_json TEXT NOT NULL,
    workflow_type TEXT,
    version TEXT NOT NULL DEFAULT '1.0.0',
    lifecycle_state TEXT NOT NULL DEFAULT 'active',
    created_by TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    CONSTRAINT valid_workflow_type CHECK (
        workflow_type IS NULL OR
        workflow_type IN ('Parallel', 'UpstreamDownstream', 'Sequential')
    ),
    CONSTRAINT fk_adapter_stacks_tenant FOREIGN KEY (tenant_id)
        REFERENCES tenants(id) ON DELETE CASCADE
);

-- Step 2: Copy data from old table
INSERT INTO adapter_stacks_new (
    id, tenant_id, name, description, adapter_ids_json, workflow_type,
    version, lifecycle_state, created_by, created_at, updated_at
)
SELECT
    id, tenant_id, name, description, adapter_ids_json, workflow_type,
    version, lifecycle_state, created_by, created_at, updated_at
FROM adapter_stacks;

-- Step 3: Drop old table
DROP TABLE adapter_stacks;

-- Step 4: Rename new table
ALTER TABLE adapter_stacks_new RENAME TO adapter_stacks;

-- Step 5: Recreate indexes
CREATE INDEX idx_adapter_stacks_name ON adapter_stacks(name);
CREATE INDEX idx_adapter_stacks_created_at ON adapter_stacks(created_at DESC);
CREATE INDEX idx_adapter_stacks_tenant ON adapter_stacks(tenant_id);

-- Step 5b: Recreate all dependent views that were dropped in Step 0

-- From 0070_routing_decisions.sql
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

-- From 0071_lifecycle_version_history.sql
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

-- From 0079_stack_versioning.sql
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

-- Step 6: Recreate validation trigger
CREATE TRIGGER IF NOT EXISTS validate_stack_name_format
BEFORE INSERT ON adapter_stacks
FOR EACH ROW
BEGIN
    -- Validate format: stack.{namespace}[.{identifier}]
    SELECT CASE
        WHEN NEW.name NOT GLOB 'stack.[a-z0-9]*[a-z0-9]'
            AND NEW.name NOT GLOB 'stack.[a-z0-9]*[a-z0-9].[a-z0-9]*[a-z0-9]'
        THEN RAISE(ABORT, 'Invalid stack name format: must match stack.{namespace}[.{identifier}]')
    END;

    -- Validate max length
    SELECT CASE
        WHEN length(NEW.name) > 100
        THEN RAISE(ABORT, 'Stack name exceeds 100 character limit')
    END;

    -- Validate no consecutive hyphens
    SELECT CASE
        WHEN NEW.name LIKE '%---%'
        THEN RAISE(ABORT, 'Stack name cannot contain consecutive hyphens')
    END;

    -- Reject reserved stack names
    SELECT CASE
        WHEN NEW.name IN ('stack.safe-default', 'stack.system')
        THEN RAISE(ABORT, 'Stack name is reserved')
    END;
END;

-- ============================================================================
-- Fix 2: Add CHECK constraint for adapters.activation_count >= 0
-- ============================================================================
-- SQLite doesn't support adding CHECK constraints to existing columns.
-- We add a trigger to enforce this constraint.

CREATE TRIGGER IF NOT EXISTS enforce_activation_count_non_negative
BEFORE UPDATE ON adapters
FOR EACH ROW
WHEN NEW.activation_count < 0
BEGIN
    SELECT RAISE(ABORT, 'activation_count cannot be negative');
END;

-- Also enforce on INSERT
CREATE TRIGGER IF NOT EXISTS enforce_activation_count_non_negative_insert
BEFORE INSERT ON adapters
FOR EACH ROW
WHEN NEW.activation_count < 0
BEGIN
    SELECT RAISE(ABORT, 'activation_count cannot be negative');
END;

-- ============================================================================
-- Fix 3: Ensure adapter_activations has FK cascade
-- ============================================================================
-- The adapter_activations table was created in migration 0082 with FK constraint.
-- Verify it has ON DELETE CASCADE (it already does per migration 0082)
--
-- Current schema from 0082:
--   FOREIGN KEY (adapter_id) REFERENCES adapters(id) ON DELETE CASCADE
--
-- No changes needed - FK cascade already present in migration 0082.

-- ============================================================================
-- Verification Queries (run after migration)
-- ============================================================================
-- Verify FK constraint on adapter_stacks:
--   SELECT sql FROM sqlite_master WHERE name='adapter_stacks';
-- Should show: FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
--
-- Verify triggers exist:
--   SELECT name FROM sqlite_master WHERE type='trigger'
--     AND (name LIKE '%activation_count%' OR name LIKE '%validate_stack%');
-- Should show: enforce_activation_count_non_negative,
--              enforce_activation_count_non_negative_insert,
--              validate_stack_name_format
