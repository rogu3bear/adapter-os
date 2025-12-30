-- Migration 0075: SQL Trigger Enforcement for Lifecycle State Transitions
-- Critical Gap Fix: Database-Level State Machine Enforcement
--
-- Purpose: Enforce lifecycle state transition rules at the database level to prevent
--          invalid state transitions via direct SQL updates, scripts, or bugs.
--
-- References:
--   - docs/VERSION_GUARANTEES.md (state machine specification)
--   - adapteros-core/src/lifecycle.rs (application-layer validation)
--   - adapteros-db/src/lifecycle.rs (database transition methods)
--
-- Date: 2025-11-19
-- Updated: 2025-12-29 - Added training, ready, failed states; transition rules table

-- ============================================================================
-- LIFECYCLE TRANSITION RULES TABLE
-- ============================================================================

-- Reference table for valid lifecycle state transitions
-- This serves as both documentation and can be used for programmatic validation

CREATE TABLE IF NOT EXISTS lifecycle_transition_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_state TEXT NOT NULL,
    to_state TEXT NOT NULL,
    description TEXT,
    is_rollback INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(from_state, to_state)
);

-- Seed valid transitions
-- State machine: draft -> training -> ready -> active -> deprecated -> retired
-- Special paths: any non-terminal -> failed, active -> ready (rollback), active -> retired (ephemeral only)

INSERT OR IGNORE INTO lifecycle_transition_rules (from_state, to_state, description, is_rollback) VALUES
    ('draft', 'training', 'Start training job', 0),
    ('draft', 'failed', 'Training failed before start', 0),
    ('training', 'ready', 'Training completed successfully', 0),
    ('training', 'failed', 'Training job failed', 0),
    ('ready', 'active', 'Promote to production', 0),
    ('ready', 'failed', 'Validation failed', 0),
    ('active', 'deprecated', 'Mark for deprecation', 0),
    ('active', 'ready', 'Rollback from production', 1),
    ('active', 'retired', 'Direct retirement (ephemeral tier only)', 0),
    ('active', 'failed', 'Runtime failure detected', 0),
    ('deprecated', 'retired', 'Complete retirement', 0),
    ('deprecated', 'failed', 'Deprecation process failed', 0);

-- ============================================================================
-- ADAPTER LIFECYCLE STATE VALUE VALIDATION
-- ============================================================================

-- Drop old triggers that only validated 4 states (we now have 7)
DROP TRIGGER IF EXISTS validate_adapter_lifecycle_state;
DROP TRIGGER IF EXISTS validate_adapter_lifecycle_state_insert;
DROP TRIGGER IF EXISTS validate_adapter_lifecycle_state_update;

-- Create new triggers that validate all 7 lifecycle states
CREATE TRIGGER validate_adapter_lifecycle_state_insert
BEFORE INSERT ON adapters
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Invalid lifecycle_state: must be one of draft, training, ready, active, deprecated, retired, failed')
    END;
    SELECT CASE
        WHEN NEW.tier = 'ephemeral' AND NEW.lifecycle_state = 'deprecated'
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Ephemeral tier adapters cannot be deprecated')
    END;
END;

CREATE TRIGGER validate_adapter_lifecycle_state_update
BEFORE UPDATE OF lifecycle_state ON adapters
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Invalid lifecycle_state: must be one of draft, training, ready, active, deprecated, retired, failed')
    END;
END;

-- ============================================================================
-- ADAPTER LIFECYCLE STATE TRANSITION ENFORCEMENT
-- ============================================================================

-- Drop the old trigger with incomplete logic
DROP TRIGGER IF EXISTS enforce_adapter_lifecycle_transitions;

-- Enforce state machine rules:
--   draft -> training -> ready -> active -> deprecated -> retired
--   Special: active -> ready (rollback), any non-terminal -> failed
--
-- Terminal states: failed and retired (fully terminal)
-- Ephemeral tier: cannot enter deprecated state, must go active -> retired

CREATE TRIGGER enforce_adapter_lifecycle_transitions
BEFORE UPDATE OF lifecycle_state ON adapters
FOR EACH ROW
WHEN OLD.lifecycle_state != NEW.lifecycle_state
BEGIN
    -- Rule 1: Failed and retired states are fully terminal (no transitions out)
    SELECT CASE
        WHEN OLD.lifecycle_state = 'failed'
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Cannot transition from failed state (terminal state)')
        WHEN OLD.lifecycle_state = 'retired'
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Cannot transition from retired state (terminal state)')
    END;

    -- Rule 2: Ephemeral tier adapters cannot be deprecated (must go directly to retired or failed)
    SELECT CASE
        WHEN OLD.tier = 'ephemeral' AND NEW.lifecycle_state = 'deprecated'
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Ephemeral tier adapters cannot be deprecated; use retired or failed')
    END;

    -- Rule 3: Non-ephemeral adapters cannot skip deprecated (active -> retired is only for ephemeral)
    SELECT CASE
        WHEN OLD.tier != 'ephemeral'
         AND OLD.lifecycle_state = 'active'
         AND NEW.lifecycle_state = 'retired'
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Non-ephemeral adapters must go through deprecated before retired')
    END;

-- Rule 4: Validate transition is in the allowed transition rules
-- Valid forward transitions: draft->training, training->ready, ready->active, active->deprecated, deprecated->retired
-- Valid rollback: active->ready
-- Valid ephemeral path: active->retired (ephemeral tier only, enforced by Rule 3)
-- Valid failure paths are enumerated in lifecycle_transition_rules
    SELECT CASE
        -- Check if transition is explicitly allowed
        WHEN NOT EXISTS (
            SELECT 1 FROM lifecycle_transition_rules
            WHERE from_state = OLD.lifecycle_state AND to_state = NEW.lifecycle_state
        )
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Invalid state transition. Check lifecycle_transition_rules for valid paths.')
    END;
END;

-- ============================================================================
-- ADAPTER STACK LIFECYCLE STATE VALUE VALIDATION
-- ============================================================================

-- Drop old triggers that only validated 4 states
DROP TRIGGER IF EXISTS validate_stack_lifecycle_state;
DROP TRIGGER IF EXISTS validate_stack_lifecycle_state_insert;
DROP TRIGGER IF EXISTS validate_stack_lifecycle_state_update;

-- Create new triggers that validate all 7 lifecycle states
CREATE TRIGGER validate_stack_lifecycle_state_insert
BEFORE INSERT ON adapter_stacks
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Invalid lifecycle_state: must be one of draft, training, ready, active, deprecated, retired, failed')
    END;
END;

CREATE TRIGGER validate_stack_lifecycle_state_update
BEFORE UPDATE OF lifecycle_state ON adapter_stacks
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Invalid lifecycle_state: must be one of draft, training, ready, active, deprecated, retired, failed')
    END;
END;

-- ============================================================================
-- ADAPTER STACK LIFECYCLE STATE TRANSITION ENFORCEMENT
-- ============================================================================

-- Drop the old trigger with incomplete logic
DROP TRIGGER IF EXISTS enforce_stack_lifecycle_transitions;

-- Stacks follow the standard state machine rules:
-- draft -> training -> ready -> active -> deprecated -> retired
-- Stacks do NOT have tier-based exceptions (no active -> retired shortcut)

CREATE TRIGGER enforce_stack_lifecycle_transitions
BEFORE UPDATE OF lifecycle_state ON adapter_stacks
FOR EACH ROW
WHEN OLD.lifecycle_state != NEW.lifecycle_state
BEGIN
    -- Rule 1: Failed and retired states are fully terminal (no transitions out)
    SELECT CASE
        WHEN OLD.lifecycle_state = 'failed'
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Cannot transition stack from failed state (terminal state)')
        WHEN OLD.lifecycle_state = 'retired'
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Cannot transition stack from retired state (terminal state)')
    END;

    -- Rule 2: Stacks must go through deprecated (no tier-based exceptions)
    -- active -> retired is NOT allowed for stacks (that's ephemeral adapter-only)
    SELECT CASE
        WHEN OLD.lifecycle_state = 'active' AND NEW.lifecycle_state = 'retired'
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Stacks must go through deprecated before retired')
    END;

-- Rule 3: Validate transition is in the allowed transition rules
    SELECT CASE
        WHEN NOT EXISTS (
            SELECT 1 FROM lifecycle_transition_rules
            WHERE from_state = OLD.lifecycle_state AND to_state = NEW.lifecycle_state
        )
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Invalid stack state transition. Check lifecycle_transition_rules for valid paths.')
    END;
END;

-- ============================================================================
-- HISTORY TABLE VALIDATION TRIGGERS
-- ============================================================================

-- Ensure lifecycle history table exists for legacy adapters
CREATE TABLE IF NOT EXISTS adapter_lifecycle_history (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    adapter_pk TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    version TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    previous_lifecycle_state TEXT,
    reason TEXT,
    initiated_by TEXT NOT NULL,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (adapter_pk) REFERENCES adapters(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    CHECK (lifecycle_state IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')),
    CHECK (
        previous_lifecycle_state IS NULL OR
        previous_lifecycle_state IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
    )
);

CREATE INDEX IF NOT EXISTS idx_adapter_lifecycle_history_adapter_pk
    ON adapter_lifecycle_history(adapter_pk);
CREATE INDEX IF NOT EXISTS idx_adapter_lifecycle_history_tenant
    ON adapter_lifecycle_history(tenant_id);
CREATE INDEX IF NOT EXISTS idx_adapter_lifecycle_history_created_at
    ON adapter_lifecycle_history(created_at DESC);

CREATE TRIGGER IF NOT EXISTS trg_adapter_lifecycle_history_tenant_match
BEFORE INSERT ON adapter_lifecycle_history
FOR EACH ROW
WHEN (
    SELECT tenant_id FROM adapters WHERE id = NEW.adapter_pk
) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'adapter_lifecycle_history.tenant_id must match adapters.tenant_id');
END;

-- Update the history table triggers to recognize all 7 states
-- Note: These may have been created in migration 0071, so we drop and recreate

DROP TRIGGER IF EXISTS validate_adapter_version_history_lifecycle_state;
DROP TRIGGER IF EXISTS validate_adapter_lifecycle_history_lifecycle_state;
DROP TRIGGER IF EXISTS validate_stack_version_history_lifecycle_state;

CREATE TRIGGER validate_adapter_lifecycle_history_lifecycle_state
BEFORE INSERT ON adapter_lifecycle_history
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Invalid lifecycle_state in history: must be one of draft, training, ready, active, deprecated, retired, failed')
    END;
    SELECT CASE
        WHEN NEW.previous_lifecycle_state IS NOT NULL
         AND NEW.previous_lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Invalid previous_lifecycle_state in history: must be one of draft, training, ready, active, deprecated, retired, failed')
    END;
END;

CREATE TRIGGER validate_stack_version_history_lifecycle_state
BEFORE INSERT ON stack_version_history
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Invalid lifecycle_state in history: must be one of draft, training, ready, active, deprecated, retired, failed')
    END;
    SELECT CASE
        WHEN NEW.previous_lifecycle_state IS NOT NULL
         AND NEW.previous_lifecycle_state NOT IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
        THEN RAISE(ABORT, 'LIFECYCLE_VIOLATION: Invalid previous_lifecycle_state in history: must be one of draft, training, ready, active, deprecated, retired, failed')
    END;
END;

-- ============================================================================
-- PERFORMANCE INDEXES
-- ============================================================================

-- Index lifecycle_state for efficient queries filtering by state
CREATE INDEX IF NOT EXISTS idx_adapters_lifecycle_state
    ON adapters(lifecycle_state);

CREATE INDEX IF NOT EXISTS idx_adapter_stacks_lifecycle_state
    ON adapter_stacks(lifecycle_state);

-- Composite index for tier + lifecycle_state queries (for ephemeral adapters)
CREATE INDEX IF NOT EXISTS idx_adapters_tier_lifecycle
    ON adapters(tier, lifecycle_state);

-- Index for transition rules lookup
CREATE INDEX IF NOT EXISTS idx_lifecycle_transition_rules_from_to
    ON lifecycle_transition_rules(from_state, to_state);

-- ============================================================================
-- VALIDATION NOTES
-- ============================================================================

-- Valid state transitions (enforced by triggers above):
--   draft -> training -> ready -> active -> deprecated -> retired (standard path)
--   active -> ready (rollback for production issues)
--   active -> retired (ephemeral tier only - skip deprecated)
--   any non-terminal state -> failed (failure path from draft/training/ready/active/deprecated)
--
-- Terminal states:
--   retired - End-of-life, no further transitions of any kind
--   failed - Fully terminal, no further transitions of any kind
--
-- Invalid transitions (blocked by triggers):
--   failed -> * (any transition from failed - fully terminal)
--   retired -> * (any transition from retired - fully terminal)
--   Skipping states: draft -> ready, draft -> active, training -> active, etc.
--   Backward transitions: ready -> training, deprecated -> active, etc.
--   (except active -> ready which is allowed as rollback)
--
-- Tier-specific rules:
--   [ephemeral tier]: Cannot enter deprecated state (must go active -> retired or active -> failed)
--   [persistent/warm tier]: Must go through deprecated (active -> deprecated -> retired)
--
-- Same-state updates (no-op):
--   Allowed (e.g., UPDATE to same lifecycle_state for metadata changes)
--
-- Testing:
--   See crates/adapteros-db/tests/lifecycle_trigger_tests.rs

-- ============================================================================
-- MIGRATION COMPLETE
-- ============================================================================

-- This migration adds critical database-level enforcement of lifecycle state
-- transition rules, closing the gap identified in verification report.
--
-- Key additions:
--   1. lifecycle_transition_rules table for reference and validation
--   2. Support for all 7 states: draft, training, ready, active, deprecated, retired, failed
--   3. Terminal state enforcement (failed and retired are fully terminal)
--   4. Ephemeral tier restriction (cannot be deprecated, must skip to retired)
--   5. Non-ephemeral tier restriction (must go through deprecated before retired)
--   6. Rollback support (active -> ready)
--   7. Consistent error codes (LIFECYCLE_VIOLATION prefix)
--
-- Database layer is now production-ready with integrity guarantees.
