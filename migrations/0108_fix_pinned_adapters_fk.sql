-- Migration 0108: Fix pinned_adapters FK constraint
-- Problem: The FK references adapters(adapter_id) which is NOT the PK
-- Fix:
--   1. Rename column to adapter_pk for clarity
--   2. FK references adapters(id) (the actual PK)
--   3. Recreate view with correct join

-- Step 1: Drop dependent view
DROP VIEW IF EXISTS active_pinned_adapters;

-- Step 2: Drop and recreate pinned_adapters with correct FK
DROP TABLE IF EXISTS pinned_adapters;

CREATE TABLE IF NOT EXISTS pinned_adapters (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    adapter_pk TEXT NOT NULL,  -- Renamed from adapter_id for clarity - stores adapters.id
    pinned_until TEXT,  -- NULL = pinned indefinitely, otherwise datetime
    reason TEXT,
    pinned_by TEXT NOT NULL,
    pinned_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, adapter_pk),

    -- Correct FK: reference adapters.id (the actual PK)
    FOREIGN KEY (adapter_pk) REFERENCES adapters(id) ON DELETE CASCADE
);

-- Step 3: Recreate indexes
CREATE INDEX IF NOT EXISTS idx_pinned_adapters_tenant_adapter
    ON pinned_adapters(tenant_id, adapter_pk);

CREATE INDEX IF NOT EXISTS idx_pinned_adapters_adapter
    ON pinned_adapters(adapter_pk);

CREATE INDEX IF NOT EXISTS idx_pinned_adapters_pinned_until
    ON pinned_adapters(pinned_until);

-- Step 4: Recreate trigger
CREATE TRIGGER IF NOT EXISTS update_pinned_adapters_timestamp
    AFTER UPDATE ON pinned_adapters
BEGIN
    UPDATE pinned_adapters
    SET updated_at = datetime('now')
    WHERE id = NEW.id;
END;

-- Step 5: Recreate view with correct join (using adapter_pk -> adapters.id)
CREATE VIEW IF NOT EXISTS active_pinned_adapters AS
SELECT
    pa.id,
    pa.tenant_id,
    pa.adapter_pk,
    a.adapter_id,  -- Include adapter_id for external reference
    pa.pinned_until,
    pa.reason,
    pa.pinned_by,
    pa.pinned_at,
    a.name as adapter_name,
    a.current_state
FROM pinned_adapters pa
INNER JOIN adapters a ON pa.adapter_pk = a.id
WHERE pa.pinned_until IS NULL
   OR pa.pinned_until > datetime('now');
