-- PRD-04: Lifecycle & Versioning Engine - History Tracking
-- Adds version history tables to track all lifecycle transitions and version changes
-- for adapters and stacks, enabling audit trails and replay capabilities.

-- Adapter version history table
-- Tracks all lifecycle transitions and version bumps for adapters
CREATE TABLE IF NOT EXISTS adapter_version_history (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    adapter_id TEXT NOT NULL,
    version TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    previous_lifecycle_state TEXT,
    reason TEXT,
    initiated_by TEXT NOT NULL,
    metadata_json TEXT,  -- JSON object for additional context
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (adapter_id) REFERENCES adapters(adapter_id) ON DELETE CASCADE
);

-- Stack version history table
-- Tracks all lifecycle transitions and version bumps for stacks
CREATE TABLE IF NOT EXISTS stack_version_history (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    stack_id TEXT NOT NULL,
    version TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    previous_lifecycle_state TEXT,
    adapter_ids_json TEXT NOT NULL,  -- Snapshot of adapter composition at this version
    reason TEXT,
    initiated_by TEXT NOT NULL,
    metadata_json TEXT,  -- JSON object for additional context
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (stack_id) REFERENCES adapter_stacks(id) ON DELETE CASCADE
);

-- Indexes for efficient history queries
CREATE INDEX IF NOT EXISTS idx_adapter_version_history_adapter_id
    ON adapter_version_history(adapter_id);
CREATE INDEX IF NOT EXISTS idx_adapter_version_history_version
    ON adapter_version_history(adapter_id, version);
CREATE INDEX IF NOT EXISTS idx_adapter_version_history_created_at
    ON adapter_version_history(created_at);
CREATE INDEX IF NOT EXISTS idx_adapter_version_history_lifecycle_state
    ON adapter_version_history(lifecycle_state);

CREATE INDEX IF NOT EXISTS idx_stack_version_history_stack_id
    ON stack_version_history(stack_id);
CREATE INDEX IF NOT EXISTS idx_stack_version_history_version
    ON stack_version_history(stack_id, version);
CREATE INDEX IF NOT EXISTS idx_stack_version_history_created_at
    ON stack_version_history(created_at);
CREATE INDEX IF NOT EXISTS idx_stack_version_history_lifecycle_state
    ON stack_version_history(lifecycle_state);

-- View: Recent adapter lifecycle changes (last 30 days)
CREATE VIEW IF NOT EXISTS recent_adapter_lifecycle_changes AS
SELECT
    avh.id,
    avh.adapter_id,
    a.name AS adapter_name,
    avh.version,
    avh.previous_lifecycle_state,
    avh.lifecycle_state,
    avh.reason,
    avh.initiated_by,
    avh.created_at
FROM adapter_version_history avh
LEFT JOIN adapters a ON avh.adapter_id = a.adapter_id
WHERE avh.created_at >= datetime('now', '-30 days')
ORDER BY avh.created_at DESC;

-- View: Recent stack lifecycle changes (last 30 days)
CREATE VIEW IF NOT EXISTS recent_stack_lifecycle_changes AS
SELECT
    svh.id,
    svh.stack_id,
    s.name AS stack_name,
    svh.version,
    svh.previous_lifecycle_state,
    svh.lifecycle_state,
    svh.reason,
    svh.initiated_by,
    svh.created_at
FROM stack_version_history svh
LEFT JOIN adapter_stacks s ON svh.stack_id = s.id
WHERE svh.created_at >= datetime('now', '-30 days')
ORDER BY svh.created_at DESC;

-- View: Adapters by lifecycle state with version info
CREATE VIEW IF NOT EXISTS adapters_lifecycle_summary AS
SELECT
    a.adapter_id,
    a.name,
    a.tenant_id,
    a.lifecycle_state,
    a.version,
    COUNT(avh.id) AS total_transitions,
    MAX(avh.created_at) AS last_transition_at
FROM adapters a
LEFT JOIN adapter_version_history avh ON a.adapter_id = avh.adapter_id
GROUP BY a.adapter_id, a.name, a.tenant_id, a.lifecycle_state, a.version;

-- View: Stacks by lifecycle state with version info
CREATE VIEW IF NOT EXISTS stacks_lifecycle_summary AS
SELECT
    s.id AS stack_id,
    s.name,
    s.tenant_id,
    s.lifecycle_state,
    s.version,
    COUNT(svh.id) AS total_transitions,
    MAX(svh.created_at) AS last_transition_at
FROM adapter_stacks s
LEFT JOIN stack_version_history svh ON s.id = svh.stack_id
GROUP BY s.id, s.name, s.tenant_id, s.lifecycle_state, s.version;

-- Validation: Ensure lifecycle_state values are valid in history tables
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

CREATE TRIGGER IF NOT EXISTS validate_stack_version_history_lifecycle_state
BEFORE INSERT ON stack_version_history
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
