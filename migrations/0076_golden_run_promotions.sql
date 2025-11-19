-- Migration: Golden Run Promotion Workflow
-- Purpose: Add promotion workflow tables for golden run validation and approval
-- Policy Compliance: Build & Release Ruleset (#15) - Promotion gates and rollback
-- Created: 2025-11-19

-- Golden run promotion requests table - tracks all promotion requests
CREATE TABLE IF NOT EXISTS golden_run_promotion_requests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL UNIQUE,
    golden_run_id TEXT NOT NULL,
    target_stage TEXT NOT NULL, -- 'staging', 'production'
    status TEXT NOT NULL DEFAULT 'pending', -- 'pending', 'approved', 'rejected', 'promoted', 'rolled_back'
    requester_id TEXT NOT NULL,
    requester_email TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    notes TEXT
);

CREATE INDEX IF NOT EXISTS idx_promotion_requests_golden_run_id ON golden_run_promotion_requests(golden_run_id);
CREATE INDEX IF NOT EXISTS idx_promotion_requests_status ON golden_run_promotion_requests(status);
CREATE INDEX IF NOT EXISTS idx_promotion_requests_created_at ON golden_run_promotion_requests(created_at DESC);

-- Golden run promotion approvals - tracks approval workflow
CREATE TABLE IF NOT EXISTS golden_run_promotion_approvals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL,
    approver_id TEXT NOT NULL,
    approver_email TEXT NOT NULL,
    action TEXT NOT NULL, -- 'approve', 'reject'
    approval_message TEXT NOT NULL,
    signature TEXT NOT NULL, -- Ed25519 signature (hex)
    public_key TEXT NOT NULL, -- Ed25519 public key (hex)
    approved_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (request_id) REFERENCES golden_run_promotion_requests(request_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_promotion_approvals_request_id ON golden_run_promotion_approvals(request_id);
CREATE INDEX IF NOT EXISTS idx_promotion_approvals_approved_at ON golden_run_promotion_approvals(approved_at DESC);

-- Golden run promotion gates - tracks gate validation status
CREATE TABLE IF NOT EXISTS golden_run_promotion_gates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL,
    gate_name TEXT NOT NULL, -- 'hash_validation', 'policy_check', 'test_results', 'determinism_check'
    status TEXT NOT NULL, -- 'pending', 'passed', 'failed', 'skipped'
    passed BOOLEAN NOT NULL DEFAULT 0,
    details TEXT, -- JSON with gate-specific data
    error_message TEXT,
    checked_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (request_id) REFERENCES golden_run_promotion_requests(request_id) ON DELETE CASCADE,
    UNIQUE(request_id, gate_name)
);

CREATE INDEX IF NOT EXISTS idx_promotion_gates_request_id ON golden_run_promotion_gates(request_id);
CREATE INDEX IF NOT EXISTS idx_promotion_gates_status ON golden_run_promotion_gates(status);

-- Golden run promotion history - audit trail of all promotions and rollbacks
CREATE TABLE IF NOT EXISTS golden_run_promotion_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL,
    golden_run_id TEXT NOT NULL,
    action TEXT NOT NULL, -- 'promoted', 'rolled_back'
    target_stage TEXT NOT NULL,
    previous_golden_run_id TEXT, -- For rollback tracking
    promoted_by TEXT NOT NULL,
    approval_signature TEXT NOT NULL,
    metadata TEXT, -- JSON with additional context
    promoted_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (request_id) REFERENCES golden_run_promotion_requests(request_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_promotion_history_golden_run_id ON golden_run_promotion_history(golden_run_id);
CREATE INDEX IF NOT EXISTS idx_promotion_history_target_stage ON golden_run_promotion_history(target_stage);
CREATE INDEX IF NOT EXISTS idx_promotion_history_promoted_at ON golden_run_promotion_history(promoted_at DESC);

-- Golden run stages - tracks current golden run per stage
CREATE TABLE IF NOT EXISTS golden_run_stages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    stage_name TEXT NOT NULL UNIQUE, -- 'staging', 'production'
    active_golden_run_id TEXT NOT NULL,
    previous_golden_run_id TEXT,
    promoted_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    promoted_by TEXT NOT NULL
);

-- Initialize default stages
INSERT OR IGNORE INTO golden_run_stages (stage_name, active_golden_run_id, promoted_by)
VALUES
    ('staging', 'none', 'system'),
    ('production', 'none', 'system');

CREATE INDEX IF NOT EXISTS idx_golden_run_stages_stage_name ON golden_run_stages(stage_name);
