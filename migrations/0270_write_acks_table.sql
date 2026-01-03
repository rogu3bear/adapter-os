-- Migration 0270: Write Acknowledgments Table for Dual-Write Consistency
-- Purpose: Track dual-write (SQL + KV) outcomes for consistency verification and repair
-- Related: Issue #4 - Dual-write consistency
-- Author: JKCA
-- Date: 2025-01-02

-- Write acknowledgment records track the outcome of dual-write operations.
-- This enables:
-- 1. Strict mode: Fail-fast when any store fails
-- 2. Relaxed mode: Continue with degraded state, queue for repair
-- 3. Audit trail: Full history of write operations and their outcomes

CREATE TABLE IF NOT EXISTS write_acks (
    -- Operation identity
    operation_id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,       -- e.g., "adapter", "trace", "session"
    entity_id TEXT NOT NULL,         -- ID of the entity written

    -- Write outcomes (enum as text: ok, failed, pending, unavailable)
    sql_status TEXT NOT NULL DEFAULT 'pending',
    sql_error TEXT,                  -- Error message if sql_status = 'failed'
    kv_status TEXT NOT NULL DEFAULT 'pending',
    kv_error TEXT,                   -- Error message if kv_status = 'failed'

    -- Degraded state tracking
    degraded INTEGER NOT NULL DEFAULT 0,    -- Boolean: 1 if stores disagree
    degraded_reason TEXT,                   -- Human-readable explanation

    -- Integrity verification
    content_hash TEXT,               -- BLAKE3 hash of content for cross-check

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,               -- Set when operation finishes

    -- Constraints
    CHECK (sql_status IN ('ok', 'failed', 'pending', 'unavailable')),
    CHECK (kv_status IN ('ok', 'failed', 'pending', 'unavailable')),
    CHECK (degraded IN (0, 1))
);

-- Index for repair queue: find degraded operations needing attention
CREATE INDEX IF NOT EXISTS idx_write_acks_degraded
    ON write_acks(degraded, created_at DESC)
    WHERE degraded = 1;

-- Index for entity lookup: find write history for an entity
CREATE INDEX IF NOT EXISTS idx_write_acks_entity
    ON write_acks(entity_type, entity_id, created_at DESC);

-- Index for pending operations: find incomplete writes
CREATE INDEX IF NOT EXISTS idx_write_acks_pending
    ON write_acks(sql_status, kv_status)
    WHERE sql_status = 'pending' OR kv_status = 'pending';

-- Index for failed operations: audit trail
CREATE INDEX IF NOT EXISTS idx_write_acks_failed
    ON write_acks(created_at DESC)
    WHERE sql_status = 'failed' OR kv_status = 'failed';

-- View for degraded writes requiring repair
CREATE VIEW IF NOT EXISTS write_acks_repair_queue AS
SELECT
    operation_id,
    entity_type,
    entity_id,
    sql_status,
    kv_status,
    degraded_reason,
    content_hash,
    created_at,
    CAST((julianday('now') - julianday(created_at)) * 24 * 60 AS INTEGER) AS age_minutes
FROM write_acks
WHERE degraded = 1
ORDER BY created_at ASC;

-- View for write operation summary by entity type
CREATE VIEW IF NOT EXISTS write_acks_summary AS
SELECT
    entity_type,
    COUNT(*) AS total_ops,
    SUM(CASE WHEN sql_status = 'ok' AND kv_status = 'ok' THEN 1 ELSE 0 END) AS fully_ok,
    SUM(CASE WHEN degraded = 1 THEN 1 ELSE 0 END) AS degraded_count,
    SUM(CASE WHEN sql_status = 'failed' OR kv_status = 'failed' THEN 1 ELSE 0 END) AS failed_count
FROM write_acks
GROUP BY entity_type;
