-- Migration: Pinned Adapters Table
-- Description: Creates separate table for adapter pinning with TTL support
-- Citation: Agent G Stability Reinforcement Plan - Patch 1.3
-- Priority: CRITICAL - Resolves architectural drift between column and table approach
-- Date: 2025-01-16
-- Version: 0060

-- Create pinned_adapters table for time-based pinning with audit trail
CREATE TABLE IF NOT EXISTS pinned_adapters (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    pinned_until TEXT,  -- NULL = pinned indefinitely, otherwise datetime
    reason TEXT,
    pinned_by TEXT NOT NULL,
    pinned_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, adapter_id),
    FOREIGN KEY (adapter_id) REFERENCES adapters(adapter_id) ON DELETE CASCADE
);

-- Index for performance on lookups
CREATE INDEX IF NOT EXISTS idx_pinned_adapters_tenant_adapter
    ON pinned_adapters(tenant_id, adapter_id);

CREATE INDEX IF NOT EXISTS idx_pinned_adapters_adapter
    ON pinned_adapters(adapter_id);

CREATE INDEX IF NOT EXISTS idx_pinned_adapters_pinned_until
    ON pinned_adapters(pinned_until);

-- Trigger to update updated_at on changes
CREATE TRIGGER IF NOT EXISTS update_pinned_adapters_timestamp
    AFTER UPDATE ON pinned_adapters
BEGIN
    UPDATE pinned_adapters
    SET updated_at = datetime('now')
    WHERE id = NEW.id;
END;

-- View to show currently active pins (excluding expired ones)
CREATE VIEW IF NOT EXISTS active_pinned_adapters AS
SELECT
    pa.id,
    pa.tenant_id,
    pa.adapter_id,
    pa.pinned_until,
    pa.reason,
    pa.pinned_by,
    pa.pinned_at,
    a.name as adapter_name,
    a.current_state
FROM pinned_adapters pa
INNER JOIN adapters a ON pa.adapter_id = a.adapter_id
WHERE pa.pinned_until IS NULL
   OR pa.pinned_until > datetime('now');

-- Migration completion
-- Note: The existing 'pinned' column on adapters table remains for backward compatibility
-- and quick lookups. Both mechanisms should be checked when determining if adapter is pinned.
