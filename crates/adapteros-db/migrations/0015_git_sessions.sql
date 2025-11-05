-- Git session and file change tracking migration
-- Enables adapter branch lifecycle and file-change streaming for Cursor integration

-- Track adapter Git sessions
CREATE TABLE IF NOT EXISTS adapter_git_sessions (
    id TEXT PRIMARY KEY,
    adapter_id TEXT NOT NULL,
    repo_id TEXT NOT NULL,
    branch_name TEXT NOT NULL,
    base_commit_sha TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at TEXT,
    status TEXT NOT NULL DEFAULT 'active', -- active, merged, abandoned
    merge_commit_sha TEXT,
    FOREIGN KEY (adapter_id) REFERENCES adapters(id) ON DELETE CASCADE,
    FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_adapter_git_sessions_adapter ON adapter_git_sessions(adapter_id);
CREATE INDEX IF NOT EXISTS idx_adapter_git_sessions_repo ON adapter_git_sessions(repo_id);
CREATE INDEX IF NOT EXISTS idx_adapter_git_sessions_status ON adapter_git_sessions(status);
CREATE INDEX IF NOT EXISTS idx_adapter_git_sessions_started ON adapter_git_sessions(started_at);

-- Track file change events for SSE streaming to Cursor
CREATE TABLE IF NOT EXISTS file_change_events (
    id TEXT PRIMARY KEY,
    adapter_id TEXT,
    repo_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    change_type TEXT NOT NULL, -- create, modify, delete
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    broadcasted INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (adapter_id) REFERENCES adapters(id) ON DELETE SET NULL,
    FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_file_change_events_timestamp ON file_change_events(timestamp);
CREATE INDEX IF NOT EXISTS idx_file_change_events_adapter ON file_change_events(adapter_id);
CREATE INDEX IF NOT EXISTS idx_file_change_events_repo ON file_change_events(repo_id);
CREATE INDEX IF NOT EXISTS idx_file_change_events_broadcasted ON file_change_events(broadcasted);

-- Git commit tracking (extends existing commits table)
CREATE TABLE IF NOT EXISTS git_adapter_commits (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    message TEXT NOT NULL,
    files_changed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES adapter_git_sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_git_adapter_commits_session ON git_adapter_commits(session_id);
CREATE INDEX IF NOT EXISTS idx_git_adapter_commits_sha ON git_adapter_commits(commit_sha);
CREATE INDEX IF NOT EXISTS idx_git_adapter_commits_created ON git_adapter_commits(created_at);



