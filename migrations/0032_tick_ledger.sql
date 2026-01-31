-- Migration: Global Tick Ledger
-- Purpose: Cross-tenant and cross-host deterministic execution tracking
-- Policy Compliance: Determinism Ruleset (#2), Isolation Ruleset (#8)

-- Tick ledger entries table - stores all deterministic executor events
CREATE TABLE IF NOT EXISTS tick_ledger_entries (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tick INTEGER NOT NULL,
    tenant_id TEXT NOT NULL,
    host_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    event_type TEXT NOT NULL, -- TaskSpawned, TaskCompleted, TaskFailed, TaskTimeout, InferenceStarted, TickAdvanced
    event_hash TEXT NOT NULL, -- BLAKE3 hash of event data (hex)
    timestamp_us INTEGER NOT NULL,
    prev_entry_hash TEXT, -- Previous entry hash for Merkle chain
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Index for tick-based queries
CREATE INDEX IF NOT EXISTS idx_tick_ledger_tick 
    ON tick_ledger_entries(tick DESC);

-- Index for tenant-based queries
CREATE INDEX IF NOT EXISTS idx_tick_ledger_tenant 
    ON tick_ledger_entries(tenant_id, tick DESC);

-- Index for host-based queries
CREATE INDEX IF NOT EXISTS idx_tick_ledger_host 
    ON tick_ledger_entries(host_id, tick DESC);

-- Composite index for cross-host verification
CREATE INDEX IF NOT EXISTS idx_tick_ledger_tenant_host 
    ON tick_ledger_entries(tenant_id, host_id, tick DESC);

-- Index for task tracking
CREATE INDEX IF NOT EXISTS idx_tick_ledger_task 
    ON tick_ledger_entries(task_id, tick DESC);

-- Index for Merkle chain navigation
CREATE INDEX IF NOT EXISTS idx_tick_ledger_prev_hash 
    ON tick_ledger_entries(prev_entry_hash) WHERE prev_entry_hash IS NOT NULL;

-- Tick ledger consistency reports table - stores cross-host comparison results
CREATE TABLE IF NOT EXISTS tick_ledger_consistency_reports (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    host_a TEXT NOT NULL,
    host_b TEXT NOT NULL,
    tick_range_start INTEGER NOT NULL,
    tick_range_end INTEGER NOT NULL,
    consistent INTEGER NOT NULL, -- Boolean: 0 = inconsistent, 1 = consistent
    divergence_count INTEGER NOT NULL DEFAULT 0,
    divergence_details TEXT, -- JSON: list of divergence points
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Index for consistency report lookups
CREATE INDEX IF NOT EXISTS idx_consistency_tenant 
    ON tick_ledger_consistency_reports(tenant_id, created_at DESC);

-- Index for host-pair lookups
CREATE INDEX IF NOT EXISTS idx_consistency_hosts 
    ON tick_ledger_consistency_reports(host_a, host_b, created_at DESC);
