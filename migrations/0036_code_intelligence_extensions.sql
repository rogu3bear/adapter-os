-- Code intelligence extensions for Cursor integration
-- Adds missing fields to repositories table and creates CodeGraph metadata and scan job tables

-- Add missing fields to repositories table
-- Note: SQLite doesn't support IF NOT EXISTS in ALTER TABLE ADD COLUMN
-- These may already exist from earlier migrations
ALTER TABLE repositories ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE repositories ADD COLUMN latest_scan_commit TEXT;
ALTER TABLE repositories ADD COLUMN latest_scan_at TEXT;
ALTER TABLE repositories ADD COLUMN latest_graph_hash TEXT;
ALTER TABLE repositories ADD COLUMN languages_json TEXT; -- Migrate from languages

-- Update repositories table to use languages_json
UPDATE repositories SET languages_json = languages WHERE languages_json IS NULL;

-- Create CodeGraph metadata table
CREATE TABLE IF NOT EXISTS code_graph_metadata (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    hash_b3 TEXT NOT NULL,
    file_count INTEGER NOT NULL,
    symbol_count INTEGER NOT NULL,
    test_count INTEGER NOT NULL,
    languages_json TEXT NOT NULL,
    frameworks_json TEXT,
    size_bytes INTEGER NOT NULL,
    symbol_index_hash TEXT,
    vector_index_hash TEXT,
    test_map_hash TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(repo_id, commit_sha),
    FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_code_graph_metadata_repo ON code_graph_metadata(repo_id);
CREATE INDEX IF NOT EXISTS idx_code_graph_metadata_commit ON code_graph_metadata(commit_sha);
CREATE INDEX IF NOT EXISTS idx_code_graph_metadata_hash ON code_graph_metadata(hash_b3);

-- Create scan jobs table
CREATE TABLE IF NOT EXISTS scan_jobs (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending', 'running', 'completed', 'failed')),
    current_stage TEXT,
    progress_pct INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT,
    FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_scan_jobs_repo ON scan_jobs(repo_id);
CREATE INDEX IF NOT EXISTS idx_scan_jobs_status ON scan_jobs(status);
CREATE INDEX IF NOT EXISTS idx_scan_jobs_started ON scan_jobs(started_at DESC);

