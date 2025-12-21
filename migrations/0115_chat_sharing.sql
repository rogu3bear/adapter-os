-- Migration 0115: Session Sharing

-- Workspace shares
CREATE TABLE IF NOT EXISTS chat_session_shares (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    session_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    permission TEXT NOT NULL DEFAULT 'view' CHECK(permission IN ('view', 'comment', 'collaborate')),
    shared_by TEXT NOT NULL,
    shared_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT,
    revoked_at TEXT,
    FOREIGN KEY (session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    UNIQUE(session_id, workspace_id)
);

CREATE INDEX idx_session_shares_session ON chat_session_shares(session_id);
CREATE INDEX idx_session_shares_workspace ON chat_session_shares(workspace_id);

-- Direct user shares
CREATE TABLE IF NOT EXISTS chat_session_user_shares (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    session_id TEXT NOT NULL,
    shared_with_user_id TEXT NOT NULL,
    shared_with_tenant_id TEXT NOT NULL,
    permission TEXT NOT NULL DEFAULT 'view' CHECK(permission IN ('view', 'comment', 'collaborate')),
    shared_by TEXT NOT NULL,
    shared_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT,
    revoked_at TEXT,
    FOREIGN KEY (session_id) REFERENCES chat_sessions(id) ON DELETE CASCADE,
    UNIQUE(session_id, shared_with_user_id)
);

CREATE INDEX idx_user_shares_session ON chat_session_user_shares(session_id);
CREATE INDEX idx_user_shares_user ON chat_session_user_shares(shared_with_user_id);

-- Session sharing flags
ALTER TABLE chat_sessions ADD COLUMN is_shared INTEGER NOT NULL DEFAULT 0;
CREATE INDEX idx_chat_sessions_shared ON chat_sessions(tenant_id, is_shared) WHERE is_shared = 1;
