-- Migration: CAB Promotion Workflow
-- Purpose: Add tables for Change Advisory Board promotion process
-- Policy Compliance: Build & Release Ruleset (#15) - Promotion gates and rollback

-- CAB approvals table - stores signed approval records
CREATE TABLE IF NOT EXISTS cab_approvals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cpid TEXT NOT NULL,
    approver TEXT NOT NULL,
    approval_message TEXT NOT NULL,
    signature TEXT NOT NULL, -- Ed25519 signature (hex)
    public_key TEXT NOT NULL, -- Ed25519 public key (hex)
    approved_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    UNIQUE(cpid, approver, approved_at)
);

CREATE INDEX IF NOT EXISTS idx_cab_approvals_cpid ON cab_approvals(cpid);
CREATE INDEX IF NOT EXISTS idx_cab_approvals_approved_at ON cab_approvals(approved_at DESC);

-- Replay test bundles - determinism verification test suites
CREATE TABLE IF NOT EXISTS replay_test_bundles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    test_bundle_id TEXT NOT NULL UNIQUE,
    cpid TEXT NOT NULL,
    test_name TEXT NOT NULL,
    input_prompt TEXT NOT NULL,
    expected_output TEXT NOT NULL,
    expected_hash TEXT NOT NULL, -- BLAKE3 hash of expected output
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    UNIQUE(cpid, test_name)
);

CREATE INDEX IF NOT EXISTS idx_replay_test_bundles_cpid ON replay_test_bundles(cpid);

-- Promotion history - audit trail of all promotions and rollbacks
CREATE TABLE IF NOT EXISTS promotion_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cpid TEXT NOT NULL,
    status TEXT NOT NULL, -- 'production', 'rollback', 'failed'
    approval_signature TEXT NOT NULL,
    before_cpid TEXT, -- Previous CPID for rollback tracking
    promoted_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_promotion_history_cpid ON promotion_history(cpid);
CREATE INDEX IF NOT EXISTS idx_promotion_history_promoted_at ON promotion_history(promoted_at DESC);

<<<<<<< HEAD
-- Note: cp_pointers, plans, and artifacts table extensions now handled by migration 0040
-- This migration focuses on new tables specific to CAB promotion workflow
=======
-- CP pointers - named pointers to active CPIDs
CREATE TABLE IF NOT EXISTS cp_pointers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE, -- 'production', 'staging', 'canary'
    active_cpid TEXT,
    before_cpid TEXT, -- Track previous CPID for instant rollback
    approval_signature TEXT,
    promoted_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Initialize production pointer
INSERT OR IGNORE INTO cp_pointers (name) VALUES ('production');
INSERT OR IGNORE INTO cp_pointers (name) VALUES ('staging');
INSERT OR IGNORE INTO cp_pointers (name) VALUES ('canary');

-- Add cpid column to plans table (already exists from migration 0001)
-- Note: SQLite doesn't support adding NOT NULL columns with ALTER TABLE directly
-- We add it as nullable first, then we can enforce NOT NULL constraint via CHECK
ALTER TABLE plans ADD COLUMN cpid TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_plans_cpid_unique ON plans(cpid) WHERE cpid IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_plans_cpid ON plans(cpid);

-- Artifacts table (if not exists) - stores SBOMs and signatures
CREATE TABLE IF NOT EXISTS artifacts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    artifact_id TEXT NOT NULL UNIQUE,
    cpid TEXT NOT NULL,
    artifact_type TEXT NOT NULL, -- 'sbom', 'signature', 'metallib'
    content_hash TEXT NOT NULL, -- BLAKE3 hash
    content BLOB NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    UNIQUE(cpid, artifact_type)
);

CREATE INDEX IF NOT EXISTS idx_artifacts_cpid ON artifacts(cpid);
CREATE INDEX IF NOT EXISTS idx_artifacts_type ON artifacts(artifact_type);
>>>>>>> d313374 (WIP: Fix migration conflicts - needs schema alignment)

-- Quality gate results - store hallucination metrics per CPID
CREATE TABLE IF NOT EXISTS quality_gate_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cpid TEXT NOT NULL,
    test_suite TEXT NOT NULL, -- 'hallucination_metrics', 'performance_benchmarks'
    arr_score REAL, -- Answer Relevance Rate
    ecs5_score REAL, -- Evidence Coverage Score @5
    hlr_score REAL, -- Hallucination Rate
    cr_score REAL, -- Contradiction Rate
    passed BOOLEAN NOT NULL,
    run_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    UNIQUE(cpid, test_suite, run_at)
);

CREATE INDEX IF NOT EXISTS idx_quality_gate_cpid ON quality_gate_results(cpid);
CREATE INDEX IF NOT EXISTS idx_quality_gate_run_at ON quality_gate_results(run_at DESC);


