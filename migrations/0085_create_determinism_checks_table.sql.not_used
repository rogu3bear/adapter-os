-- ============================================================================
-- Create Determinism Checks Table
-- ============================================================================
-- File: migrations/0084_create_determinism_checks_table.sql
-- Purpose: Store determinism check results for PRD G2 - Determinism Certification
-- Status: New migration for Guardrails & Insight MVP
-- Dependencies: None (standalone table)
-- Notes: Tracks determinism check runs from CLI command `aosctl determinism check`
-- ============================================================================

-- Create determinism_checks table
CREATE TABLE IF NOT EXISTS determinism_checks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    
    -- Check metadata
    last_run TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    result TEXT NOT NULL CHECK(result IN ('pass', 'fail')),
    runs INTEGER NOT NULL DEFAULT 1,
    divergences INTEGER NOT NULL DEFAULT 0,
    
    -- Test configuration
    stack_id TEXT,
    seed TEXT, -- 64 hex chars for deterministic seed
    
    -- Timestamps
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index for quick lookup of latest check
CREATE INDEX IF NOT EXISTS idx_determinism_checks_last_run 
    ON determinism_checks(last_run DESC);

-- Index for stack-specific checks
CREATE INDEX IF NOT EXISTS idx_determinism_checks_stack_id 
    ON determinism_checks(stack_id) 
    WHERE stack_id IS NOT NULL;

