-- Migration: CAB (Change Advisory Board) Lineage and Rollback
-- Purpose: Track Control Plane lineage for safe rollback operations
-- Policy Compliance: Build & Release Ruleset (#15), Incident Ruleset (#17)

-- Control Plane lineage tracking
CREATE TABLE IF NOT EXISTS cp_lineage (
    cpid TEXT PRIMARY KEY,
    parent_cpid TEXT, -- Previous CP in the lineage
    promotion_timestamp INTEGER NOT NULL,
    promoted_by TEXT, -- Operator who performed promotion
    rollback_available INTEGER NOT NULL DEFAULT 1, -- Boolean: 0 = false, 1 = true
    determinism_replay_hash TEXT, -- BLAKE3 hash of replay output
    policy_hash TEXT, -- BLAKE3 hash of policy configuration
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (parent_cpid) REFERENCES cp_lineage(cpid)
);

-- Index for lineage traversal
CREATE INDEX IF NOT EXISTS idx_cp_lineage_parent 
    ON cp_lineage(parent_cpid);

-- Index for timestamp queries
CREATE INDEX IF NOT EXISTS idx_cp_lineage_promotion_ts 
    ON cp_lineage(promotion_timestamp DESC);

-- Rollback history
CREATE TABLE IF NOT EXISTS cp_rollbacks (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    from_cpid TEXT NOT NULL,
    to_cpid TEXT NOT NULL,
    reason TEXT NOT NULL,
    performed_by TEXT NOT NULL,
    dry_run INTEGER NOT NULL DEFAULT 0, -- Boolean: 0 = actual, 1 = dry run
    success INTEGER NOT NULL, -- Boolean: 0 = failed, 1 = succeeded
    error_message TEXT,
    performed_at INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (from_cpid) REFERENCES cp_lineage(cpid),
    FOREIGN KEY (to_cpid) REFERENCES cp_lineage(cpid)
);

-- Index for rollback history queries
CREATE INDEX IF NOT EXISTS idx_cp_rollbacks_from 
    ON cp_rollbacks(from_cpid, performed_at DESC);

-- Index for rollback history by operator
CREATE INDEX IF NOT EXISTS idx_cp_rollbacks_operator 
    ON cp_rollbacks(performed_by, performed_at DESC);

-- Differential verification results
CREATE TABLE IF NOT EXISTS cp_diff_verifications (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    cpid_a TEXT NOT NULL,
    cpid_b TEXT NOT NULL,
    input_corpus_hash TEXT NOT NULL, -- BLAKE3 hash of input test corpus
    identical INTEGER NOT NULL, -- Boolean: 0 = differences found, 1 = identical
    divergence_count INTEGER NOT NULL DEFAULT 0,
    divergence_details TEXT, -- JSON: list of divergence points
    verified_at INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (cpid_a) REFERENCES cp_lineage(cpid),
    FOREIGN KEY (cpid_b) REFERENCES cp_lineage(cpid)
);

-- Index for diff verification lookups
CREATE INDEX IF NOT EXISTS idx_cp_diff_cpids 
    ON cp_diff_verifications(cpid_a, cpid_b);

-- Index for corpus-based lookups
CREATE INDEX IF NOT EXISTS idx_cp_diff_corpus 
    ON cp_diff_verifications(input_corpus_hash, verified_at DESC);

