-- Chat sessions for workspace experience
-- Migration: 0085
-- Created: 2025-01-25
-- Purpose: Enable persistent chat sessions with stack context and trace linkage

CREATE TABLE IF NOT EXISTS chat_sessions (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    user_id TEXT,
    stack_id TEXT,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_activity_at TEXT NOT NULL DEFAULT (datetime('now')),
    metadata_json TEXT, -- Additional metadata (e.g., tags, description)
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (stack_id) REFERENCES adapter_stacks(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_chat_sessions_tenant ON chat_sessions(tenant_id, last_activity_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_user ON chat_sessions(user_id, last_activity_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_stack ON chat_sessions(stack_id);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_activity ON chat_sessions(last_activity_at DESC);

-- Chat session trace linkage: maps sessions to router decisions, adapters, jobs
CREATE TABLE IF NOT EXISTS chat_session_traces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    trace_type TEXT NOT NULL, -- 'router_decision', 'adapter', 'training_job', 'audit_event'
    trace_id TEXT NOT NULL, -- ID of the traced entity
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_chat_session_traces_session ON chat_session_traces(session_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_session_traces_type ON chat_session_traces(trace_type, trace_id);
CREATE INDEX IF NOT EXISTS idx_chat_session_traces_created ON chat_session_traces(created_at DESC);

-- Chat messages (persistent storage for chat history)
CREATE TABLE IF NOT EXISTS chat_messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    role TEXT NOT NULL, -- 'user', 'assistant', 'system'
    content TEXT NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    metadata_json TEXT, -- Router decisions, evidence, etc.
    FOREIGN KEY (session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_chat_messages_session ON chat_messages(session_id, timestamp ASC);
CREATE INDEX IF NOT EXISTS idx_chat_messages_timestamp ON chat_messages(timestamp DESC);
