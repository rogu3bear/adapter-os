-- Add policy_packs table for storing signed policy pack definitions
-- Add policy_assignments table for tenant/stack/resource policy associations
-- Migration: 0090
-- Created: 2025-11-25
-- Purpose: PRD-GOV-01 - Policies, audit, compliance storage

-- Policy packs table: stores signed policy pack definitions
CREATE TABLE IF NOT EXISTS policy_packs (
    id TEXT PRIMARY KEY,                    -- policy pack ID (e.g., "cp-egress-001")
    version TEXT NOT NULL,                  -- version string (e.g., "1.0")
    policy_type TEXT NOT NULL,              -- policy type (e.g., "egress", "determinism", "naming")
    content_json TEXT NOT NULL,             -- policy configuration as JSON
    signature TEXT NOT NULL,                -- Ed25519 signature
    public_key TEXT NOT NULL,               -- Ed25519 public key
    hash_b3 TEXT NOT NULL,                  -- BLAKE3 hash of content
    status TEXT NOT NULL DEFAULT 'draft',   -- status: draft, active, deprecated, revoked
    description TEXT,                       -- policy description
    created_at TEXT NOT NULL,               -- creation timestamp (RFC3339)
    created_by TEXT NOT NULL,               -- user who created the policy
    activated_at TEXT,                      -- when policy was activated (RFC3339)
    deprecated_at TEXT,                     -- when policy was deprecated (RFC3339)
    metadata_json TEXT                      -- additional metadata as JSON
);

-- Index for querying active policies by type
CREATE INDEX IF NOT EXISTS idx_policy_packs_type_status ON policy_packs(policy_type, status);
CREATE INDEX IF NOT EXISTS idx_policy_packs_created_at ON policy_packs(created_at);
CREATE INDEX IF NOT EXISTS idx_policy_packs_status ON policy_packs(status);

-- Policy assignments table: associates policies with tenants, stacks, or resources
CREATE TABLE IF NOT EXISTS policy_assignments (
    id TEXT PRIMARY KEY,                    -- assignment ID (UUID)
    policy_pack_id TEXT NOT NULL,           -- references policy_packs(id)
    target_type TEXT NOT NULL,              -- target type: "system", "tenant", "stack", "adapter", "dataset"
    target_id TEXT,                         -- ID of the target (NULL for system-level policies)
    priority INTEGER NOT NULL DEFAULT 100,  -- priority for conflict resolution (higher = higher priority)
    enforced INTEGER NOT NULL DEFAULT 1,    -- whether policy is enforced (0 = audit-only, 1 = enforced)
    assigned_at TEXT NOT NULL,              -- assignment timestamp (RFC3339)
    assigned_by TEXT NOT NULL,              -- user who assigned the policy
    expires_at TEXT,                        -- optional expiration (RFC3339)
    metadata_json TEXT,                     -- additional assignment metadata
    FOREIGN KEY (policy_pack_id) REFERENCES policy_packs(id) ON DELETE CASCADE
);

-- Indexes for policy assignment queries
CREATE INDEX IF NOT EXISTS idx_policy_assignments_target ON policy_assignments(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_policy_assignments_policy_pack ON policy_assignments(policy_pack_id);
CREATE INDEX IF NOT EXISTS idx_policy_assignments_assigned_at ON policy_assignments(assigned_at);

-- Policy violations table: tracks policy violations for compliance reporting
CREATE TABLE IF NOT EXISTS policy_violations (
    id TEXT PRIMARY KEY,                    -- violation ID (UUID)
    policy_pack_id TEXT NOT NULL,           -- references policy_packs(id)
    policy_assignment_id TEXT,              -- references policy_assignments(id) if applicable
    violation_type TEXT NOT NULL,           -- violation type (e.g., "naming", "egress", "determinism")
    severity TEXT NOT NULL,                 -- severity: low, medium, high, critical
    resource_type TEXT NOT NULL,            -- resource type (e.g., "adapter", "tenant", "stack")
    resource_id TEXT,                       -- resource ID
    tenant_id TEXT NOT NULL,                -- tenant context
    violation_message TEXT NOT NULL,        -- human-readable violation message
    violation_details_json TEXT,            -- detailed violation data as JSON
    detected_at TEXT NOT NULL,              -- when violation was detected (RFC3339)
    resolved_at TEXT,                       -- when violation was resolved (RFC3339)
    resolved_by TEXT,                       -- user who resolved the violation
    resolution_notes TEXT,                  -- resolution notes
    FOREIGN KEY (policy_pack_id) REFERENCES policy_packs(id) ON DELETE CASCADE,
    FOREIGN KEY (policy_assignment_id) REFERENCES policy_assignments(id) ON DELETE SET NULL
);

-- Indexes for violation queries
CREATE INDEX IF NOT EXISTS idx_policy_violations_policy_pack ON policy_violations(policy_pack_id);
CREATE INDEX IF NOT EXISTS idx_policy_violations_resource ON policy_violations(resource_type, resource_id);
CREATE INDEX IF NOT EXISTS idx_policy_violations_tenant ON policy_violations(tenant_id);
CREATE INDEX IF NOT EXISTS idx_policy_violations_detected_at ON policy_violations(detected_at);
CREATE INDEX IF NOT EXISTS idx_policy_violations_severity ON policy_violations(severity);
CREATE INDEX IF NOT EXISTS idx_policy_violations_resolved ON policy_violations(resolved_at);

-- Composite index for compliance queries (tenant violations by severity and time)
CREATE INDEX IF NOT EXISTS idx_policy_violations_tenant_severity_time ON policy_violations(tenant_id, severity, detected_at DESC);

-- Compliance scores table: aggregated compliance scores for reporting
CREATE TABLE IF NOT EXISTS compliance_scores (
    id TEXT PRIMARY KEY,                    -- score ID (UUID)
    target_type TEXT NOT NULL,              -- target type: "system", "tenant", "stack"
    target_id TEXT,                         -- target ID (NULL for system-level)
    policy_pack_id TEXT,                    -- specific policy pack (NULL for overall score)
    score REAL NOT NULL,                    -- compliance score (0.0 - 1.0)
    total_checks INTEGER NOT NULL,          -- total checks performed
    passed_checks INTEGER NOT NULL,         -- checks that passed
    failed_checks INTEGER NOT NULL,         -- checks that failed
    critical_violations INTEGER NOT NULL DEFAULT 0,  -- count of critical violations
    high_violations INTEGER NOT NULL DEFAULT 0,      -- count of high violations
    medium_violations INTEGER NOT NULL DEFAULT 0,    -- count of medium violations
    low_violations INTEGER NOT NULL DEFAULT 0,       -- count of low violations
    calculated_at TEXT NOT NULL,            -- calculation timestamp (RFC3339)
    period_start TEXT,                      -- reporting period start (RFC3339)
    period_end TEXT,                        -- reporting period end (RFC3339)
    metadata_json TEXT,                     -- additional metadata
    FOREIGN KEY (policy_pack_id) REFERENCES policy_packs(id) ON DELETE CASCADE
);

-- Indexes for compliance score queries
CREATE INDEX IF NOT EXISTS idx_compliance_scores_target ON compliance_scores(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_compliance_scores_policy_pack ON compliance_scores(policy_pack_id);
CREATE INDEX IF NOT EXISTS idx_compliance_scores_calculated_at ON compliance_scores(calculated_at DESC);
CREATE INDEX IF NOT EXISTS idx_compliance_scores_score ON compliance_scores(score);

-- Audit log chain verification: add signature fields to audit_logs
-- This enables tamper detection and chain-of-custody for audit trails
-- Note: Modifying existing table with ALTER TABLE for backward compatibility
ALTER TABLE audit_logs ADD COLUMN signature TEXT;
ALTER TABLE audit_logs ADD COLUMN previous_hash TEXT;
ALTER TABLE audit_logs ADD COLUMN chain_sequence INTEGER;

-- Index for audit log chain verification
CREATE INDEX IF NOT EXISTS idx_audit_logs_chain_sequence ON audit_logs(chain_sequence);
