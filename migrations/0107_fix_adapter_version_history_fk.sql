-- Migration 0107: Fix adapter_version_history FK constraint
-- Problem: The FK references adapters(adapter_id) which is NOT the PK
-- Fix:
--   1. Rename column to adapter_pk for clarity
--   2. FK references adapters(id) (the actual PK)
--   3. Do NOT touch the adapters table (preserve all columns)

-- Step 1: Drop dependent views
DROP VIEW IF EXISTS recent_adapter_lifecycle_changes;
DROP VIEW IF EXISTS adapters_lifecycle_summary;

-- Step 2: Drop and recreate adapter_version_history with correct FK
DROP TABLE IF EXISTS adapter_version_history;

CREATE TABLE IF NOT EXISTS adapter_version_history (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    adapter_pk TEXT NOT NULL,  -- Renamed from adapter_id for clarity - stores adapters.id
    version TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    previous_lifecycle_state TEXT,
    reason TEXT,
    initiated_by TEXT NOT NULL,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    -- Correct FK: reference adapters.id (the actual PK)
    FOREIGN KEY (adapter_pk) REFERENCES adapters(id) ON DELETE CASCADE
);

-- Step 3: Recreate indexes
CREATE INDEX IF NOT EXISTS idx_adapter_version_history_adapter_pk
    ON adapter_version_history(adapter_pk);
CREATE INDEX IF NOT EXISTS idx_adapter_version_history_version
    ON adapter_version_history(adapter_pk, version);
CREATE INDEX IF NOT EXISTS idx_adapter_version_history_created_at
    ON adapter_version_history(created_at);
CREATE INDEX IF NOT EXISTS idx_adapter_version_history_lifecycle_state
    ON adapter_version_history(lifecycle_state);

-- Step 4: Recreate views with correct join (using adapter_pk -> adapters.id)

CREATE VIEW IF NOT EXISTS recent_adapter_lifecycle_changes AS
SELECT
    avh.id,
    avh.adapter_pk,
    a.adapter_id,  -- Include adapter_id for external reference
    a.name AS adapter_name,
    avh.version,
    avh.previous_lifecycle_state,
    avh.lifecycle_state,
    avh.reason,
    avh.initiated_by,
    avh.created_at
FROM adapter_version_history avh
LEFT JOIN adapters a ON avh.adapter_pk = a.id
WHERE avh.created_at >= datetime('now', '-30 days')
ORDER BY avh.created_at DESC;

CREATE VIEW IF NOT EXISTS adapters_lifecycle_summary AS
SELECT
    a.id AS adapter_pk,
    a.adapter_id,
    a.name,
    a.tenant_id,
    a.lifecycle_state,
    a.version,
    COUNT(avh.id) AS total_transitions,
    MAX(avh.created_at) AS last_transition_at
FROM adapters a
LEFT JOIN adapter_version_history avh ON a.id = avh.adapter_pk
GROUP BY a.id, a.adapter_id, a.name, a.tenant_id, a.lifecycle_state, a.version;

-- Step 5: Recreate validation trigger
CREATE TRIGGER IF NOT EXISTS validate_adapter_version_history_lifecycle_state
BEFORE INSERT ON adapter_version_history
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'active', 'deprecated', 'retired')
        THEN RAISE(ABORT, 'Invalid lifecycle_state in history: must be draft, active, deprecated, or retired')
    END;
    SELECT CASE
        WHEN NEW.previous_lifecycle_state IS NOT NULL
         AND NEW.previous_lifecycle_state NOT IN ('draft', 'active', 'deprecated', 'retired')
        THEN RAISE(ABORT, 'Invalid previous_lifecycle_state in history: must be draft, active, deprecated, or retired')
    END;
END;
