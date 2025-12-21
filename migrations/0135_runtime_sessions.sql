-- Migration: 0135_runtime_sessions
-- Description: Create runtime_sessions table for tracking server lifecycle and configuration drift
-- Date: 2025-12-02

-- Create runtime_sessions table
CREATE TABLE IF NOT EXISTS runtime_sessions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    config_hash TEXT NOT NULL,
    binary_version TEXT NOT NULL,
    binary_commit TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    end_reason TEXT CHECK (end_reason IS NULL OR end_reason IN ('graceful', 'crash', 'terminated')),
    hostname TEXT NOT NULL,
    runtime_mode TEXT NOT NULL CHECK (runtime_mode IN ('development', 'production')),
    config_snapshot TEXT NOT NULL,
    drift_detected INTEGER DEFAULT 0 CHECK (drift_detected IN (0, 1)),
    drift_summary TEXT,
    previous_session_id TEXT,
    model_path TEXT,
    adapters_root TEXT,
    database_path TEXT,
    var_dir TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (previous_session_id) REFERENCES runtime_sessions(id) ON DELETE SET NULL
);

-- Create indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_runtime_sessions_started_at
    ON runtime_sessions(started_at DESC);

CREATE INDEX IF NOT EXISTS idx_runtime_sessions_hostname
    ON runtime_sessions(hostname);

CREATE INDEX IF NOT EXISTS idx_runtime_sessions_config_hash
    ON runtime_sessions(config_hash);

CREATE INDEX IF NOT EXISTS idx_runtime_sessions_ended_at
    ON runtime_sessions(ended_at);

-- Create view for active (running) sessions
CREATE VIEW IF NOT EXISTS active_sessions AS
SELECT
    id,
    session_id,
    config_hash,
    binary_version,
    binary_commit,
    started_at,
    hostname,
    runtime_mode,
    model_path,
    adapters_root,
    database_path,
    var_dir,
    julianday('now') - julianday(started_at) AS uptime_days
FROM runtime_sessions
WHERE ended_at IS NULL
ORDER BY started_at DESC;

-- Create view for configuration drift history
CREATE VIEW IF NOT EXISTS config_drift_history AS
SELECT
    rs.id,
    rs.session_id,
    rs.config_hash,
    rs.binary_version,
    rs.started_at,
    rs.ended_at,
    rs.hostname,
    rs.runtime_mode,
    rs.drift_summary,
    rs.previous_session_id,
    prev.config_hash AS previous_config_hash,
    prev.session_id AS previous_session_id_ref
FROM runtime_sessions rs
LEFT JOIN runtime_sessions prev ON rs.previous_session_id = prev.id
WHERE rs.drift_detected = 1
ORDER BY rs.started_at DESC;

-- Create trigger to update updated_at timestamp
CREATE TRIGGER IF NOT EXISTS update_runtime_sessions_timestamp
AFTER UPDATE ON runtime_sessions
FOR EACH ROW
BEGIN
    UPDATE runtime_sessions
    SET updated_at = datetime('now')
    WHERE id = NEW.id;
END;
