-- Migration 0017: Process Debugging and Troubleshooting
-- Adds tables for process debugging, logs, and troubleshooting features
-- Citation: docs/runaway-prevention.md, docs/architecture.md

-- Process logs table for storing worker process logs
CREATE TABLE IF NOT EXISTS process_logs (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    level TEXT NOT NULL CHECK(level IN ('debug','info','warn','error','fatal')),
    message TEXT NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_process_logs_worker_id ON process_logs(worker_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_process_logs_level ON process_logs(level);
CREATE INDEX IF NOT EXISTS idx_process_logs_timestamp ON process_logs(timestamp DESC);

-- Process crash dumps table
CREATE TABLE IF NOT EXISTS process_crash_dumps (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    crash_type TEXT NOT NULL CHECK(crash_type IN ('panic','oom','timeout','signal','deadlock')),
    stack_trace TEXT,
    memory_snapshot_json TEXT,
    crash_timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    recovery_action TEXT,
    recovered_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_crash_dumps_worker_id ON process_crash_dumps(worker_id, crash_timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_crash_dumps_type ON process_crash_dumps(crash_type);

-- Process performance profiles table
CREATE TABLE IF NOT EXISTS process_performance_profiles (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    profile_type TEXT NOT NULL CHECK(profile_type IN ('cpu','memory','io','network','gpu')),
    profile_data_json TEXT NOT NULL,
    duration_seconds INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_performance_profiles_worker_id ON process_performance_profiles(worker_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_performance_profiles_type ON process_performance_profiles(profile_type);

-- Process debugging sessions table
CREATE TABLE IF NOT EXISTS process_debug_sessions (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    session_type TEXT NOT NULL CHECK(session_type IN ('live','replay','analysis')),
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','paused','completed','failed')),
    config_json TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at TEXT,
    results_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_debug_sessions_worker_id ON process_debug_sessions(worker_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_debug_sessions_status ON process_debug_sessions(status);

-- Process troubleshooting steps table
CREATE TABLE IF NOT EXISTS process_troubleshooting_steps (
    id TEXT PRIMARY KEY,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    step_name TEXT NOT NULL,
    step_type TEXT NOT NULL CHECK(step_type IN ('diagnostic','recovery','prevention')),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','running','completed','failed','skipped')),
    command TEXT,
    output TEXT,
    error_message TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_troubleshooting_steps_worker_id ON process_troubleshooting_steps(worker_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_troubleshooting_steps_status ON process_troubleshooting_steps(status);
