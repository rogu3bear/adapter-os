-- Migration: Federation Consensus Ledger
-- Purpose: Track consensus decisions across federated hosts for determinism verification
-- Policy Compliance: Determinism Ruleset (#2), Federation Policy (#8)
-- Extends: 0038_federation.sql - Federation Infrastructure
-- Created: 2025-11-22

-- Federation consensus rounds - tracks voting rounds for adapter deployments
CREATE TABLE IF NOT EXISTS federation_consensus_rounds (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    round_id TEXT NOT NULL UNIQUE,
    consensus_type TEXT NOT NULL, -- 'adapter_deploy', 'adapter_retire', 'policy_update', 'golden_run_promote'

    -- Subject of consensus
    subject_id TEXT NOT NULL, -- adapter_id, policy_id, or golden_run_id
    subject_hash TEXT NOT NULL, -- BLAKE3 hash of the subject content

    -- Round configuration
    required_votes INTEGER NOT NULL DEFAULT 2, -- Minimum votes needed
    quorum_threshold REAL NOT NULL DEFAULT 0.67, -- Percentage of peers required

    -- Round state
    status TEXT NOT NULL DEFAULT 'open', -- 'open', 'achieved', 'failed', 'expired'
    votes_for INTEGER NOT NULL DEFAULT 0,
    votes_against INTEGER NOT NULL DEFAULT 0,
    total_votes INTEGER NOT NULL DEFAULT 0,

    -- Timing
    initiated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deadline_at TIMESTAMP NOT NULL,
    concluded_at TIMESTAMP,

    -- Initiator
    initiator_host_id TEXT NOT NULL,
    initiator_signature TEXT NOT NULL, -- Ed25519 signature

    FOREIGN KEY (initiator_host_id) REFERENCES federation_peers(host_id)
);

-- Indexes for consensus round queries
CREATE INDEX IF NOT EXISTS idx_consensus_rounds_status ON federation_consensus_rounds(status);
CREATE INDEX IF NOT EXISTS idx_consensus_rounds_type ON federation_consensus_rounds(consensus_type);
CREATE INDEX IF NOT EXISTS idx_consensus_rounds_subject ON federation_consensus_rounds(subject_id);
CREATE INDEX IF NOT EXISTS idx_consensus_rounds_deadline ON federation_consensus_rounds(deadline_at) WHERE status = 'open';
CREATE INDEX IF NOT EXISTS idx_consensus_rounds_initiated ON federation_consensus_rounds(initiated_at DESC);

-- Federation consensus votes - individual votes from peers
CREATE TABLE IF NOT EXISTS federation_consensus_votes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    round_id TEXT NOT NULL,
    host_id TEXT NOT NULL,

    -- Vote details
    vote TEXT NOT NULL, -- 'approve', 'reject', 'abstain'
    vote_reason TEXT,

    -- Verification data
    subject_hash TEXT NOT NULL, -- Must match round's subject_hash for valid vote
    determinism_verified INTEGER NOT NULL DEFAULT 0, -- Boolean: verified local determinism
    verification_output_hash TEXT, -- BLAKE3 hash of local verification output

    -- Cryptographic proof
    signature TEXT NOT NULL, -- Ed25519 signature of vote
    public_key TEXT NOT NULL, -- Ed25519 public key

    -- Timing
    voted_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (round_id) REFERENCES federation_consensus_rounds(round_id) ON DELETE CASCADE,
    FOREIGN KEY (host_id) REFERENCES federation_peers(host_id),
    UNIQUE(round_id, host_id) -- One vote per host per round
);

-- Indexes for vote queries
CREATE INDEX IF NOT EXISTS idx_consensus_votes_round ON federation_consensus_votes(round_id);
CREATE INDEX IF NOT EXISTS idx_consensus_votes_host ON federation_consensus_votes(host_id);
CREATE INDEX IF NOT EXISTS idx_consensus_votes_voted_at ON federation_consensus_votes(voted_at DESC);

-- Federation consensus outcomes - finalized consensus decisions
CREATE TABLE IF NOT EXISTS federation_consensus_outcomes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    round_id TEXT NOT NULL UNIQUE,
    outcome TEXT NOT NULL, -- 'approved', 'rejected', 'expired', 'no_quorum'

    -- Final counts
    final_votes_for INTEGER NOT NULL,
    final_votes_against INTEGER NOT NULL,
    final_total_votes INTEGER NOT NULL,
    quorum_met INTEGER NOT NULL, -- Boolean

    -- Determinism verification
    all_hashes_match INTEGER NOT NULL DEFAULT 0, -- Boolean: all verification_output_hash values match
    determinism_score REAL, -- Percentage of matching hashes

    -- Aggregate signature (threshold signature of all approving votes)
    aggregate_signature TEXT,
    signing_hosts_json TEXT, -- JSON array of host_ids that signed

    -- Timing
    concluded_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (round_id) REFERENCES federation_consensus_rounds(round_id) ON DELETE CASCADE
);

-- Indexes for outcome queries
CREATE INDEX IF NOT EXISTS idx_consensus_outcomes_outcome ON federation_consensus_outcomes(outcome);
CREATE INDEX IF NOT EXISTS idx_consensus_outcomes_concluded ON federation_consensus_outcomes(concluded_at DESC);

-- Federation ledger entries - immutable audit trail of all consensus actions
CREATE TABLE IF NOT EXISTS federation_consensus_ledger (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_id TEXT NOT NULL UNIQUE, -- UUIDv7
    sequence_number INTEGER NOT NULL, -- Monotonic sequence for ordering

    -- Entry type
    entry_type TEXT NOT NULL, -- 'round_initiated', 'vote_cast', 'consensus_achieved', 'consensus_failed'

    -- References
    round_id TEXT,
    host_id TEXT,

    -- Entry data
    entry_hash TEXT NOT NULL, -- BLAKE3 hash of entry content
    previous_hash TEXT, -- Hash of previous entry (chain linkage)
    entry_data TEXT NOT NULL, -- JSON payload

    -- Cryptographic proof
    signature TEXT NOT NULL, -- Ed25519 signature of entry
    signer_host_id TEXT NOT NULL,

    -- Timing
    recorded_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (round_id) REFERENCES federation_consensus_rounds(round_id),
    FOREIGN KEY (host_id) REFERENCES federation_peers(host_id)
);

-- Indexes for ledger queries
CREATE INDEX IF NOT EXISTS idx_consensus_ledger_sequence ON federation_consensus_ledger(sequence_number DESC);
CREATE INDEX IF NOT EXISTS idx_consensus_ledger_entry_type ON federation_consensus_ledger(entry_type);
CREATE INDEX IF NOT EXISTS idx_consensus_ledger_round ON federation_consensus_ledger(round_id) WHERE round_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_consensus_ledger_recorded ON federation_consensus_ledger(recorded_at DESC);
CREATE INDEX IF NOT EXISTS idx_consensus_ledger_hash ON federation_consensus_ledger(entry_hash);

-- View for open consensus rounds requiring votes
CREATE VIEW IF NOT EXISTS open_consensus_rounds AS
SELECT
    r.round_id,
    r.consensus_type,
    r.subject_id,
    r.subject_hash,
    r.required_votes,
    r.quorum_threshold,
    r.votes_for,
    r.votes_against,
    r.total_votes,
    r.initiated_at,
    r.deadline_at,
    r.initiator_host_id,
    (r.required_votes - r.total_votes) as votes_remaining,
    (julianday(r.deadline_at) - julianday('now')) * 24 * 60 as minutes_remaining
FROM federation_consensus_rounds r
WHERE r.status = 'open'
  AND r.deadline_at > datetime('now')
ORDER BY r.deadline_at ASC;

-- View for recent consensus activity
CREATE VIEW IF NOT EXISTS recent_consensus_activity AS
SELECT
    l.entry_id,
    l.sequence_number,
    l.entry_type,
    l.round_id,
    r.consensus_type,
    r.subject_id,
    l.host_id,
    p.hostname as host_name,
    l.recorded_at
FROM federation_consensus_ledger l
LEFT JOIN federation_consensus_rounds r ON l.round_id = r.round_id
LEFT JOIN federation_peers p ON l.host_id = p.host_id
WHERE l.recorded_at > datetime('now', '-24 hours')
ORDER BY l.sequence_number DESC
LIMIT 100;

-- View for consensus health metrics
CREATE VIEW IF NOT EXISTS consensus_health_metrics AS
SELECT
    COUNT(CASE WHEN status = 'achieved' THEN 1 END) as successful_rounds,
    COUNT(CASE WHEN status = 'failed' THEN 1 END) as failed_rounds,
    COUNT(CASE WHEN status = 'expired' THEN 1 END) as expired_rounds,
    COUNT(CASE WHEN status = 'open' THEN 1 END) as open_rounds,
    AVG(CASE WHEN status = 'achieved' THEN total_votes END) as avg_votes_for_success,
    (SELECT COUNT(*) FROM federation_peers WHERE active = 1) as active_peers
FROM federation_consensus_rounds
WHERE initiated_at > datetime('now', '-7 days');
