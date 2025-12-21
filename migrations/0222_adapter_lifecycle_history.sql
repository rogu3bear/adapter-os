-- Legacy adapter lifecycle history tracking
--
-- Migration 0186 repurposed adapter_version_history for the repo-based
-- adapter workflow (adapter_versions table). This creates a separate table
-- for tracking lifecycle transitions on the legacy adapters table.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS adapter_lifecycle_history (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    adapter_pk TEXT NOT NULL,  -- References adapters.id (internal PK)
    tenant_id TEXT NOT NULL,
    version TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    previous_lifecycle_state TEXT,
    reason TEXT,
    initiated_by TEXT NOT NULL,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (adapter_pk) REFERENCES adapters(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    CHECK (lifecycle_state IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')),
    CHECK (
        previous_lifecycle_state IS NULL OR
        previous_lifecycle_state IN ('draft', 'training', 'ready', 'active', 'deprecated', 'retired', 'failed')
    )
);

CREATE INDEX IF NOT EXISTS idx_adapter_lifecycle_history_adapter_pk
    ON adapter_lifecycle_history(adapter_pk);
CREATE INDEX IF NOT EXISTS idx_adapter_lifecycle_history_tenant
    ON adapter_lifecycle_history(tenant_id);
CREATE INDEX IF NOT EXISTS idx_adapter_lifecycle_history_created_at
    ON adapter_lifecycle_history(created_at DESC);

-- Tenant guard: history tenant must match adapter tenant
CREATE TRIGGER IF NOT EXISTS trg_adapter_lifecycle_history_tenant_match
BEFORE INSERT ON adapter_lifecycle_history
FOR EACH ROW
WHEN (
    SELECT tenant_id FROM adapters WHERE id = NEW.adapter_pk
) != NEW.tenant_id
BEGIN
    SELECT RAISE(ABORT, 'adapter_lifecycle_history.tenant_id must match adapters.tenant_id');
END;
