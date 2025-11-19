-- Migration 0075: SQL Trigger Enforcement for Lifecycle State Transitions
-- PRD-02 Critical Gap Fix: Database-Level State Machine Enforcement
--
-- Purpose: Enforce lifecycle state transition rules at the database level to prevent
--          invalid state transitions via direct SQL updates, scripts, or bugs.
--
-- References:
--   - docs/VERSION_GUARANTEES.md (state machine specification)
--   - adapteros-core/src/lifecycle.rs (application-layer validation)
--   - adapteros-db/src/metadata.rs (validate_state_transition function)
--
-- Author: PRD-02 Verification Agent
-- Date: 2025-11-19

-- ============================================================================
-- ADAPTER LIFECYCLE STATE TRANSITION ENFORCEMENT
-- ============================================================================

-- Enforce state machine rules: draft → active → deprecated → retired
-- Special rules:
--   1. retired is terminal (no transitions out)
--   2. ephemeral tier adapters cannot enter deprecated state
--   3. no backward transitions (e.g., active → draft is forbidden)

CREATE TRIGGER IF NOT EXISTS enforce_adapter_lifecycle_transitions
BEFORE UPDATE OF lifecycle_state ON adapters
FOR EACH ROW
WHEN OLD.lifecycle_state != NEW.lifecycle_state
BEGIN
    -- Rule 1: Retired is a terminal state (cannot transition to any other state)
    SELECT CASE
        WHEN OLD.lifecycle_state = 'retired'
        THEN RAISE(ABORT, 'Cannot transition from retired state (terminal state)')
    END;

    -- Rule 2: Ephemeral tier adapters cannot be deprecated (must go directly to retired)
    SELECT CASE
        WHEN NEW.tier = 'ephemeral' AND NEW.lifecycle_state = 'deprecated'
        THEN RAISE(ABORT, 'Ephemeral tier adapters cannot be deprecated; must transition directly to retired')
    END;

    -- Rule 3: No backward transitions (state machine is forward-only)
    SELECT CASE
        -- Cannot regress from active to draft
        WHEN OLD.lifecycle_state = 'active' AND NEW.lifecycle_state = 'draft'
        THEN RAISE(ABORT, 'Invalid backward transition: active cannot regress to draft')

        -- Cannot regress from deprecated to active or draft
        WHEN OLD.lifecycle_state = 'deprecated' AND NEW.lifecycle_state IN ('draft', 'active')
        THEN RAISE(ABORT, 'Invalid backward transition: deprecated cannot regress to active or draft')

        -- Cannot regress from retired (redundant with Rule 1, but explicit)
        WHEN OLD.lifecycle_state = 'retired' AND NEW.lifecycle_state IN ('draft', 'active', 'deprecated')
        THEN RAISE(ABORT, 'Invalid backward transition: retired is terminal')
    END;
END;

-- ============================================================================
-- ADAPTER STACK LIFECYCLE STATE TRANSITION ENFORCEMENT
-- ============================================================================

-- Stacks follow the same state machine rules as individual adapters

CREATE TRIGGER IF NOT EXISTS enforce_stack_lifecycle_transitions
BEFORE UPDATE OF lifecycle_state ON adapter_stacks
FOR EACH ROW
WHEN OLD.lifecycle_state != NEW.lifecycle_state
BEGIN
    -- Rule 1: Retired is terminal
    SELECT CASE
        WHEN OLD.lifecycle_state = 'retired'
        THEN RAISE(ABORT, 'Cannot transition stack from retired state (terminal state)')
    END;

    -- Rule 2: No backward transitions for stacks
    SELECT CASE
        WHEN OLD.lifecycle_state = 'active' AND NEW.lifecycle_state = 'draft'
        THEN RAISE(ABORT, 'Invalid backward transition: active stack cannot regress to draft')

        WHEN OLD.lifecycle_state = 'deprecated' AND NEW.lifecycle_state IN ('draft', 'active')
        THEN RAISE(ABORT, 'Invalid backward transition: deprecated stack cannot regress to active or draft')

        WHEN OLD.lifecycle_state = 'retired' AND NEW.lifecycle_state IN ('draft', 'active', 'deprecated')
        THEN RAISE(ABORT, 'Invalid backward transition: retired stack is terminal')
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

-- ============================================================================
-- VALIDATION NOTES
-- ============================================================================

-- Valid state transitions (enforced by triggers above):
--   draft → active → deprecated → retired (standard path)
--   draft → retired (skip intermediate states, allowed)
--   active → retired (skip deprecated, allowed)
--   [ephemeral tier]: draft → active → retired (skip deprecated, enforced)
--
-- Invalid transitions (blocked by triggers):
--   retired → * (any transition from retired)
--   active → draft (backward transition)
--   deprecated → active (backward transition)
--   deprecated → draft (backward transition)
--   ephemeral + deprecated (tier-specific rule)
--
-- Same-state updates (no-op):
--   Allowed (e.g., UPDATE to same lifecycle_state for metadata changes)
--
-- Testing:
--   See crates/adapteros-db/tests/lifecycle_trigger_tests.rs

-- ============================================================================
-- VERSION FORMAT VALIDATION (Future Enhancement)
-- ============================================================================

-- Note: Version format validation (semver or monotonic) is currently enforced
--       in the application layer only (adapteros-db/src/metadata.rs:303-318).
--
-- Adding database-level validation would require CHECK constraints:
--   ALTER TABLE adapters ADD CONSTRAINT check_version_format
--     CHECK (version GLOB '[0-9]*.[0-9]*.[0-9]*' OR CAST(version AS INTEGER) IS NOT NULL);
--
-- However, SQLite CHECK constraints have limitations:
--   - Cannot reference other tables
--   - Limited pattern matching (GLOB is less flexible than regex)
--   - May not cover all semver edge cases (pre-release, metadata)
--
-- Recommendation: Keep version format validation in application layer for now.
--                 Consider adding in future migration if needed.

-- ============================================================================
-- MIGRATION COMPLETE
-- ============================================================================

-- This migration adds critical database-level enforcement of lifecycle state
-- transition rules, closing the PRD-02 gap identified in verification report.
--
-- Database layer is now production-ready with integrity guarantees.
