-- Migration: Worker + Model lifecycle unification
-- Purpose:
--   1) Add per-worker model lifecycle state and transition history.
--   2) Keep base_model_status as compatibility projection while moving to canonical statuses.
--   3) Preserve backward compatibility by tolerating legacy status strings in DB constraints.

PRAGMA foreign_keys=off;

-- ---------------------------------------------------------------------------
-- 1) Per-worker model lifecycle state
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS worker_model_state (
    worker_id TEXT PRIMARY KEY REFERENCES workers(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    active_model_id TEXT,
    active_model_hash_b3 TEXT,
    desired_model_id TEXT,
    status TEXT NOT NULL DEFAULT 'no-model'
        CHECK(status IN (
            'no-model', 'loading', 'ready', 'unloading', 'error',
            -- legacy compatibility
            'unloaded', 'loaded', 'checking'
        )),
    generation INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    memory_usage_mb INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_worker_model_state_tenant_status
    ON worker_model_state(tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_worker_model_state_active_model
    ON worker_model_state(active_model_id);
CREATE INDEX IF NOT EXISTS idx_worker_model_state_active_hash
    ON worker_model_state(active_model_hash_b3);

CREATE TRIGGER IF NOT EXISTS update_worker_model_state_updated_at
AFTER UPDATE ON worker_model_state
FOR EACH ROW
BEGIN
    UPDATE worker_model_state
       SET updated_at = datetime('now')
     WHERE worker_id = NEW.worker_id;
END;

-- ---------------------------------------------------------------------------
-- 2) Per-worker model lifecycle transition history
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS worker_model_status_history (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL,
    model_id TEXT,
    from_status TEXT,
    to_status TEXT NOT NULL
        CHECK(to_status IN (
            'no-model', 'loading', 'ready', 'unloading', 'error',
            -- legacy compatibility
            'unloaded', 'loaded', 'checking'
        )),
    reason TEXT NOT NULL,
    actor TEXT,
    valid_transition INTEGER NOT NULL DEFAULT 1,
    generation INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_worker_model_status_history_worker
    ON worker_model_status_history(worker_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_worker_model_status_history_tenant
    ON worker_model_status_history(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_worker_model_status_history_valid
    ON worker_model_status_history(valid_transition, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_worker_model_status_history_temporal
    ON worker_model_status_history(worker_id, created_at DESC);

CREATE TRIGGER IF NOT EXISTS enforce_worker_model_status_temporal_ordering
BEFORE INSERT ON worker_model_status_history
FOR EACH ROW
WHEN EXISTS (
    SELECT 1
      FROM worker_model_status_history
     WHERE worker_id = NEW.worker_id
       AND created_at > NEW.created_at
)
BEGIN
    SELECT RAISE(ABORT, 'Worker model status history must be temporally ordered');
END;

-- ---------------------------------------------------------------------------
-- 3) Backfill worker_model_state from current worker rows and model hash mapping
-- ---------------------------------------------------------------------------
INSERT OR IGNORE INTO worker_model_state (
    worker_id,
    tenant_id,
    active_model_id,
    active_model_hash_b3,
    desired_model_id,
    status,
    generation,
    last_error,
    memory_usage_mb,
    created_at,
    updated_at
)
SELECT
    w.id,
    w.tenant_id,
    m.id AS active_model_id,
    w.model_hash_b3,
    m.id AS desired_model_id,
    CASE
        WHEN w.model_hash_b3 IS NOT NULL AND w.model_hash_b3 != '' THEN 'ready'
        WHEN lower(w.status) IN ('error', 'crashed', 'failed') THEN 'error'
        ELSE 'no-model'
    END AS status,
    0,
    CASE WHEN lower(w.status) IN ('error', 'crashed', 'failed') THEN 'worker in terminal error state' ELSE NULL END,
    NULL,
    datetime('now'),
    datetime('now')
FROM workers w
LEFT JOIN models m ON m.hash_b3 = w.model_hash_b3;

-- ---------------------------------------------------------------------------
-- 4) Rebuild base_model_status with canonical + legacy-compatible constraint
-- ---------------------------------------------------------------------------
DROP TRIGGER IF EXISTS update_base_model_status_updated_at;

CREATE TABLE base_model_status_new (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN (
        -- canonical
        'no-model', 'loading', 'ready', 'unloading', 'error',
        -- legacy compatibility
        'unloaded', 'loaded', 'checking'
    )),
    loaded_at TEXT,
    unloaded_at TEXT,
    error_message TEXT,
    memory_usage_mb INTEGER,
    import_id TEXT,
    last_patch_applied TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE CASCADE
);

INSERT INTO base_model_status_new (
    id,
    tenant_id,
    model_id,
    status,
    loaded_at,
    unloaded_at,
    error_message,
    memory_usage_mb,
    import_id,
    last_patch_applied,
    created_at,
    updated_at
)
SELECT
    id,
    tenant_id,
    model_id,
    CASE
        WHEN status = 'loaded' THEN 'ready'
        WHEN status = 'unloaded' THEN 'no-model'
        ELSE status
    END,
    loaded_at,
    unloaded_at,
    error_message,
    memory_usage_mb,
    import_id,
    last_patch_applied,
    created_at,
    updated_at
FROM base_model_status;

DROP TABLE base_model_status;
ALTER TABLE base_model_status_new RENAME TO base_model_status;

CREATE INDEX IF NOT EXISTS idx_base_model_status_tenant_id ON base_model_status(tenant_id);
CREATE INDEX IF NOT EXISTS idx_base_model_status_status ON base_model_status(status);
CREATE INDEX IF NOT EXISTS idx_base_model_status_model_id ON base_model_status(model_id);
CREATE INDEX IF NOT EXISTS idx_base_model_status_tenant_model_status_updated
    ON base_model_status(tenant_id, model_id, status, updated_at DESC);

CREATE TRIGGER update_base_model_status_updated_at
AFTER UPDATE ON base_model_status
FOR EACH ROW
BEGIN
    UPDATE base_model_status
    SET updated_at = datetime('now')
    WHERE id = NEW.id;
END;

PRAGMA foreign_keys=on;
