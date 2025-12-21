-- Model acquisition tracking for HF Hub downloads
-- Tracks download state, progress, and enables crash recovery

CREATE TABLE IF NOT EXISTS model_acquisitions (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    revision TEXT NOT NULL DEFAULT 'main',
    acquisition_state TEXT NOT NULL DEFAULT 'not_cached'
        CHECK(acquisition_state IN ('not_cached', 'queued', 'downloading', 'verifying', 'available', 'failed')),
    download_progress_pct INTEGER DEFAULT 0,
    expected_hash_b3 TEXT,
    actual_hash_b3 TEXT,
    local_path TEXT,
    symlink_path TEXT,
    size_bytes INTEGER,
    download_started_at TEXT,
    download_completed_at TEXT,
    failure_reason TEXT,
    retry_count INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(repo_id, revision)
);

CREATE INDEX idx_model_acquisitions_state ON model_acquisitions(acquisition_state);
CREATE INDEX idx_model_acquisitions_repo ON model_acquisitions(repo_id);

-- Health check results for model readiness verification
CREATE TABLE IF NOT EXISTS model_health_checks (
    id TEXT PRIMARY KEY,
    model_id TEXT NOT NULL,
    passed INTEGER NOT NULL,
    latency_ms INTEGER NOT NULL,
    tokens_generated INTEGER,
    tokens_per_second REAL,
    test_prompt TEXT,
    failure_reason TEXT,
    checked_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (model_id) REFERENCES model_acquisitions(id) ON DELETE CASCADE
);

CREATE INDEX idx_model_health_checks_model ON model_health_checks(model_id);
CREATE INDEX idx_model_health_checks_passed ON model_health_checks(passed);
