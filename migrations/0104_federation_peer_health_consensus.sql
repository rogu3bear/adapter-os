-- Migration: Federation Peer Health Tracking and Consensus Infrastructure
-- Purpose: Add peer health monitoring, consensus voting, and partition tracking
-- Related: Federation split-brain prevention, quarantine consensus

-- Extend federation_peers with health tracking columns
-- Note: Using ALTER TABLE to add columns to existing table from 0038_federation.sql
ALTER TABLE federation_peers ADD COLUMN health_status TEXT NOT NULL DEFAULT 'healthy';
ALTER TABLE federation_peers ADD COLUMN discovery_status TEXT NOT NULL DEFAULT 'registered';
ALTER TABLE federation_peers ADD COLUMN failed_heartbeats INTEGER NOT NULL DEFAULT 0;
ALTER TABLE federation_peers ADD COLUMN last_heartbeat_at TEXT;

-- Peer health check history
CREATE TABLE IF NOT EXISTS peer_health_checks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    host_id TEXT NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    status TEXT NOT NULL, -- 'healthy', 'degraded', 'unhealthy', 'isolated'
    response_time_ms INTEGER NOT NULL,
    error_message TEXT,
    FOREIGN KEY (host_id) REFERENCES federation_peers(host_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_peer_health_checks_host ON peer_health_checks(host_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_peer_health_checks_timestamp ON peer_health_checks(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_peer_health_checks_status ON peer_health_checks(status);

-- Consensus decisions for peer state changes (simpler table for peer.rs compatibility)
CREATE TABLE IF NOT EXISTS consensus_decisions (
    id TEXT PRIMARY KEY,
    peer_id TEXT NOT NULL,
    action TEXT NOT NULL, -- 'isolate_peer', 'release_quarantine', 'evict_peer', etc.
    participating_hosts_json TEXT NOT NULL, -- JSON array of host_ids
    required_votes INTEGER NOT NULL,
    collected_votes INTEGER NOT NULL DEFAULT 0,
    approved INTEGER NOT NULL DEFAULT 0, -- Boolean
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT, -- Optional deadline
    FOREIGN KEY (peer_id) REFERENCES federation_peers(host_id)
);

CREATE INDEX IF NOT EXISTS idx_consensus_decisions_peer ON consensus_decisions(peer_id);
CREATE INDEX IF NOT EXISTS idx_consensus_decisions_action ON consensus_decisions(action);
CREATE INDEX IF NOT EXISTS idx_consensus_decisions_approved ON consensus_decisions(approved, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_consensus_decisions_expires ON consensus_decisions(expires_at) WHERE expires_at IS NOT NULL;

-- Partition event tracking
CREATE TABLE IF NOT EXISTS partition_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    partition_id TEXT NOT NULL UNIQUE,
    detected_at TEXT NOT NULL DEFAULT (datetime('now')),
    isolated_peers_json TEXT NOT NULL, -- JSON array of isolated host_ids
    reachable_peers_json TEXT NOT NULL, -- JSON array of reachable host_ids
    quorum_leader TEXT, -- Host ID that leads the quorum
    resolved INTEGER NOT NULL DEFAULT 0, -- Boolean
    resolved_at TEXT, -- Timestamp when partition was resolved
    FOREIGN KEY (quorum_leader) REFERENCES federation_peers(host_id)
);

CREATE INDEX IF NOT EXISTS idx_partition_events_partition_id ON partition_events(partition_id);
CREATE INDEX IF NOT EXISTS idx_partition_events_resolved ON partition_events(resolved, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_partition_events_detected ON partition_events(detected_at DESC);

-- Quarantine release tracking (for cooldown and consensus)
CREATE TABLE IF NOT EXISTS quarantine_release_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    quarantine_id TEXT NOT NULL,
    requested_by TEXT NOT NULL, -- User ID or host ID
    requested_at TEXT NOT NULL DEFAULT (datetime('now')),
    consensus_decision_id TEXT, -- Links to consensus_decisions if consensus required
    approved INTEGER NOT NULL DEFAULT 0, -- Boolean
    executed INTEGER NOT NULL DEFAULT 0, -- Boolean
    executed_at TEXT,
    rejection_reason TEXT,
    FOREIGN KEY (quarantine_id) REFERENCES policy_quarantine(id),
    FOREIGN KEY (consensus_decision_id) REFERENCES consensus_decisions(id)
);

CREATE INDEX IF NOT EXISTS idx_quarantine_release_quarantine ON quarantine_release_attempts(quarantine_id, requested_at DESC);
CREATE INDEX IF NOT EXISTS idx_quarantine_release_approved ON quarantine_release_attempts(approved);
CREATE INDEX IF NOT EXISTS idx_quarantine_release_executed ON quarantine_release_attempts(executed);

-- Add cooldown tracking to policy_quarantine
ALTER TABLE policy_quarantine ADD COLUMN last_release_attempt_at TEXT;
ALTER TABLE policy_quarantine ADD COLUMN release_cooldown_minutes INTEGER NOT NULL DEFAULT 5;
