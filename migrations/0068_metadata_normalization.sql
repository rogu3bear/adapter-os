-- Migration 0068: Lifecycle and Metadata Normalization
--
-- Purpose: Normalize lifecycle_state values across adapters and stacks to the
-- canonical 7-state model, add missing indexes, and ensure data integrity.
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

-- ============================================================================
-- ADAPTERS TABLE: Add version and lifecycle_state columns
-- ============================================================================

-- Add version field to adapters table (semantic versioning)
ALTER TABLE adapters ADD COLUMN version TEXT NOT NULL DEFAULT '1.0.0';

-- Add lifecycle state field to adapters table
-- Note: This is distinct from current_state (runtime loading state)
-- and load_state (lifecycle tier)
ALTER TABLE adapters ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'draft';

-- ============================================================================
-- DATA NORMALIZATION: Migrate legacy state values
-- ============================================================================

-- Normalize any case variations to lowercase (e.g., "Active" -> "active")
UPDATE adapters SET lifecycle_state = LOWER(lifecycle_state)
WHERE lifecycle_state != LOWER(lifecycle_state);

-- Map legacy/unknown states to canonical values
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

-- ============================================================================
-- ADAPTER STACKS TABLE: Add lifecycle_state column
-- ============================================================================

-- Note: version field for adapter_stacks already added in migration 0066
-- This migration only adds lifecycle state field to adapter_stacks table

-- Add lifecycle state field to adapter_stacks table
ALTER TABLE adapter_stacks ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'draft';

-- Normalize stacks lifecycle states (same mapping as adapters)
UPDATE adapter_stacks SET lifecycle_state = LOWER(lifecycle_state)
WHERE lifecycle_state != LOWER(lifecycle_state);

UPDATE adapter_stacks SET lifecycle_state = 'draft'
WHERE lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed');

-- ============================================================================
-- VALIDATION TRIGGERS: Enforce valid lifecycle_state values
-- ============================================================================

-- Adapter lifecycle_state validation (INSERT)
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
-- PERFORMANCE INDEXES
-- ============================================================================

-- Adapter indexes
CREATE INDEX IF NOT EXISTS idx_adapters_lifecycle_state ON adapters(lifecycle_state);
CREATE INDEX IF NOT EXISTS idx_adapters_version ON adapters(version);

-- Composite index for tenant + lifecycle queries (common access pattern)
CREATE INDEX IF NOT EXISTS idx_adapters_tenant_lifecycle
    ON adapters(tenant_id, lifecycle_state);

-- Composite index for routable adapters (ready/active/deprecated states)
CREATE INDEX IF NOT EXISTS idx_adapters_routable
    ON adapters(lifecycle_state)
    WHERE lifecycle_state IN ('ready', 'active', 'deprecated');

-- Stack indexes
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_lifecycle_state ON adapter_stacks(lifecycle_state);

-- Composite index for tenant + lifecycle queries on stacks
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_tenant_lifecycle
    ON adapter_stacks(tenant_id, lifecycle_state);

-- ============================================================================
-- METADATA NORMALIZATION: Ensure consistent metadata_json format
-- ============================================================================

-- Ensure metadata_json is valid JSON or NULL (not empty string)
UPDATE adapters SET metadata_json = NULL
WHERE metadata_json = '' OR metadata_json = '{}';

UPDATE adapter_stacks SET metadata_json = NULL
WHERE metadata_json = '' OR metadata_json = '{}';

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
-- 4. Terminal states: retired, failed (no transitions out)
--
-- 5. Version format: semantic versioning (major.minor.patch) or monotonic integer
--
-- 6. Loadable states: ready, active, deprecated
--    These states indicate the adapter can be loaded for inference
--
-- 7. Mutable states: draft, training
--    These states allow modifications to adapter configuration

-- ============================================================================
-- MIGRATION COMPLETE
-- ============================================================================
