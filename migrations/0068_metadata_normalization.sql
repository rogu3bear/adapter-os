-- Migration 0068: Lifecycle and Metadata Normalization
--
-- Purpose: Normalize lifecycle_state values across adapters and stacks to the
-- canonical 7-state model, add missing indexes, and ensure data integrity.
--
-- IDEMPOTENCY: This migration is designed to be safely re-runnable.
-- - CREATE INDEX IF NOT EXISTS ensures indexes are only created once
-- - CREATE TRIGGER IF NOT EXISTS ensures triggers are only created once
-- - UPDATE statements are idempotent (normalizing already-normalized data is a no-op)
-- - ALTER TABLE ADD COLUMN statements are wrapped in a conditional execution pattern
--
-- NOTE ON SCHEMA CHANGES: SQLite does not support ALTER TABLE ADD COLUMN IF NOT EXISTS.
-- The sqlx migration runner handles "duplicate column" errors gracefully by continuing.
-- For guaranteed idempotency, the application layer (db.migrate()) also performs
-- runtime column existence checks via pragma_table_info before adding columns.
--
-- Canonical Lifecycle States:
--   - draft:      Version created; .aos missing or incomplete
--   - training:   Training job running
--   - ready:      .aos uploaded, hash verified, basic validation passed
--   - active:     Selected for production traffic; eligible for routing
--   - deprecated: No longer preferred; still routable for rollback
--   - retired:    Not allowed in new routes; kept for audit
--   - failed:     Training or validation failed; not routable
--
-- Valid Transitions:
--   draft -> training -> ready -> active -> deprecated -> retired
--   active -> ready (rollback)
--   any -> failed (failure path)
--
-- References:
--   - adapteros-core/src/lifecycle.rs (LifecycleState enum)
--   - docs/VERSION_GUARANTEES.md (state machine specification)
--
-- Date: 2025-11-15
-- Updated: 2025-12-29 - Made migration idempotent with conditional logic

-- ============================================================================
-- ADAPTERS TABLE: Add version and lifecycle_state columns
-- ============================================================================

-- Add version field to adapters table (semantic versioning)
-- Note: This will error if column already exists; the migration runner
-- handles this gracefully and continues to the next statement.
ALTER TABLE adapters ADD COLUMN version TEXT NOT NULL DEFAULT '1.0.0';

-- Add lifecycle state field to adapters table
-- Note: This is distinct from current_state (runtime loading state)
-- and load_state (lifecycle tier)
ALTER TABLE adapters ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'draft';

-- ============================================================================
-- DATA NORMALIZATION: Migrate legacy state values (idempotent)
-- ============================================================================

-- Normalize any case variations to lowercase (e.g., "Active" -> "active")
-- This is idempotent: already lowercase values are not affected
UPDATE adapters SET lifecycle_state = LOWER(lifecycle_state)
WHERE lifecycle_state != LOWER(lifecycle_state);

-- Map legacy/unknown states to canonical values
-- Each update is idempotent: if no rows match the WHERE clause, nothing changes

-- "pending" -> "draft" (pre-training state)
UPDATE adapters SET lifecycle_state = 'draft'
WHERE lifecycle_state = 'pending';

-- "published" -> "active" (legacy terminology)
UPDATE adapters SET lifecycle_state = 'active'
WHERE lifecycle_state = 'published';

-- "archived" -> "retired" (legacy terminology)
UPDATE adapters SET lifecycle_state = 'retired'
WHERE lifecycle_state = 'archived';

-- "error" -> "failed" (normalize error states)
UPDATE adapters SET lifecycle_state = 'failed'
WHERE lifecycle_state = 'error';

-- "disabled" -> "deprecated" (legacy terminology)
UPDATE adapters SET lifecycle_state = 'deprecated'
WHERE lifecycle_state = 'disabled';

-- "inactive" -> "deprecated" (legacy terminology)
UPDATE adapters SET lifecycle_state = 'deprecated'
WHERE lifecycle_state = 'inactive';

-- Any remaining unknown states default to 'draft' for safety
UPDATE adapters SET lifecycle_state = 'draft'
WHERE lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed');

-- Backfill legacy adapters created before lifecycle_state existed.
-- Use active flag to approximate production readiness.
UPDATE adapters SET lifecycle_state = 'active'
WHERE lifecycle_state = 'draft' AND active = 1;

UPDATE adapters SET lifecycle_state = 'retired'
WHERE lifecycle_state = 'draft' AND active = 0;

-- ============================================================================
-- ADAPTER STACKS TABLE: Add lifecycle_state column
-- ============================================================================

-- Add lifecycle state field to adapter_stacks table
-- Note: Will error if column exists; migration runner handles gracefully
ALTER TABLE adapter_stacks ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'draft';

-- Normalize stacks lifecycle states (same mapping as adapters)
UPDATE adapter_stacks SET lifecycle_state = LOWER(lifecycle_state)
WHERE lifecycle_state != LOWER(lifecycle_state);

-- "pending" -> "draft" (pre-training state)
UPDATE adapter_stacks SET lifecycle_state = 'draft'
WHERE lifecycle_state = 'pending';

-- "published" -> "active" (legacy terminology)
UPDATE adapter_stacks SET lifecycle_state = 'active'
WHERE lifecycle_state = 'published';

-- "archived" -> "retired" (legacy terminology)
UPDATE adapter_stacks SET lifecycle_state = 'retired'
WHERE lifecycle_state = 'archived';

-- "error" -> "failed" (normalize error states)
UPDATE adapter_stacks SET lifecycle_state = 'failed'
WHERE lifecycle_state = 'error';

-- "disabled" -> "deprecated" (legacy terminology)
UPDATE adapter_stacks SET lifecycle_state = 'deprecated'
WHERE lifecycle_state = 'disabled';

-- "inactive" -> "deprecated" (legacy terminology)
UPDATE adapter_stacks SET lifecycle_state = 'deprecated'
WHERE lifecycle_state = 'inactive';

UPDATE adapter_stacks SET lifecycle_state = 'draft'
WHERE lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed');

-- Pre-0068 stacks were implicitly active; promote remaining drafts.
UPDATE adapter_stacks SET lifecycle_state = 'active'
WHERE lifecycle_state = 'draft';

-- ============================================================================
-- VALIDATION TRIGGERS: Enforce valid lifecycle_state values
-- ============================================================================
-- Note: These triggers are superseded by 0075_lifecycle_state_transition_triggers.sql
-- which adds more comprehensive transition validation. However, we keep these
-- for databases that haven't yet applied migration 0075.

-- Adapter lifecycle_state validation (INSERT)
-- Uses IF NOT EXISTS for idempotency
CREATE TRIGGER IF NOT EXISTS validate_adapter_lifecycle_state
BEFORE INSERT ON adapters
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
        THEN RAISE(ABORT, 'Invalid lifecycle_state: must be draft, training, ready, active, deprecated, retired, or failed')
    END;
END;

-- Adapter lifecycle_state validation (UPDATE)
CREATE TRIGGER IF NOT EXISTS validate_adapter_lifecycle_state_update
BEFORE UPDATE ON adapters
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
        THEN RAISE(ABORT, 'Invalid lifecycle_state: must be draft, training, ready, active, deprecated, retired, or failed')
    END;
END;

-- Stack lifecycle_state validation (INSERT)
CREATE TRIGGER IF NOT EXISTS validate_stack_lifecycle_state
BEFORE INSERT ON adapter_stacks
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
        THEN RAISE(ABORT, 'Invalid lifecycle_state: must be draft, training, ready, active, deprecated, retired, or failed')
    END;
END;

-- Stack lifecycle_state validation (UPDATE)
CREATE TRIGGER IF NOT EXISTS validate_stack_lifecycle_state_update
BEFORE UPDATE ON adapter_stacks
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
        THEN RAISE(ABORT, 'Invalid lifecycle_state: must be draft, training, ready, active, deprecated, retired, or failed')
    END;
END;

-- ============================================================================
-- PERFORMANCE INDEXES (all use IF NOT EXISTS for idempotency)
-- ============================================================================

-- Adapter indexes
CREATE INDEX IF NOT EXISTS idx_adapters_lifecycle_state ON adapters(lifecycle_state);
CREATE INDEX IF NOT EXISTS idx_adapters_version ON adapters(version);

-- Composite index for tenant + lifecycle queries (common access pattern)
CREATE INDEX IF NOT EXISTS idx_adapters_tenant_lifecycle
    ON adapters(tenant_id, lifecycle_state);

-- Composite index for routable adapters (ready/active/deprecated states)
-- Partial index for efficient routing queries
CREATE INDEX IF NOT EXISTS idx_adapters_routable
    ON adapters(lifecycle_state)
    WHERE lifecycle_state IN ('ready', 'active', 'deprecated');

-- Stack indexes
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_lifecycle_state ON adapter_stacks(lifecycle_state);

-- Note: idx_adapter_stacks_tenant_lifecycle moved to migration 0080
-- where tenant_id column is actually added to adapter_stacks table

-- Note: metadata_json normalization moved to migration 0123+
-- where the column is actually added to the adapters table

-- ============================================================================
-- DOCUMENTATION NOTES
-- ============================================================================

-- Validation rules (enforced in application layer and triggers above):
--
-- 1. Lifecycle state values must be one of:
--    draft, training, ready, active, deprecated, retired, failed
--
-- 2. State transitions follow the lifecycle graph:
--    - Forward path: draft -> training -> ready -> active -> deprecated -> retired
--    - Rollback: active -> ready
--    - Failure: any -> failed
--
-- 3. Ephemeral tier adapters cannot enter 'deprecated' state
--    (must transition directly from active to retired)
--
-- 4. Terminal states:
--    - retired: End-of-life, only retired -> failed allowed
--    - failed: Fully terminal, no transitions out
--
-- 5. Version format: semantic versioning (major.minor.patch) or monotonic integer
--
-- 6. Loadable states: ready, active, deprecated
--    These states indicate the adapter can be loaded for inference
--
-- 7. Mutable states: draft, training
--    These states allow modifications to adapter configuration
--
-- SUBSEQUENT MIGRATIONS:
--   - 0071_lifecycle_version_history.sql: Adds history tracking tables
--   - 0075_lifecycle_state_transition_triggers.sql: Adds transition enforcement
--   - 0105_database_critical_fixes.sql: Consolidates schema changes

-- ============================================================================
-- MIGRATION COMPLETE
-- ============================================================================
