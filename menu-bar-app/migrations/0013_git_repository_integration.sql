-- Git repository integration migration
-- Evidence: migrations/0002_patch_proposals.sql:1-18
-- Pattern: Database schema for patch proposals

-- Git repositories table
CREATE TABLE IF NOT EXISTS git_repositories (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    path TEXT NOT NULL,
    branch TEXT NOT NULL,
    analysis_json TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    security_scan_json TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    created_by TEXT NOT NULL
);

-- Index for faster lookups
CREATE INDEX IF NOT EXISTS idx_git_repositories_repo ON git_repositories(repo_id);
CREATE INDEX IF NOT EXISTS idx_git_repositories_status ON git_repositories(status);
CREATE INDEX IF NOT EXISTS idx_git_repositories_created_by ON git_repositories(created_by);

-- Repository training jobs table
CREATE TABLE IF NOT EXISTS repository_training_jobs (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    training_config_json TEXT NOT NULL,
    status TEXT NOT NULL,
    progress_json TEXT NOT NULL,
    started_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMP,
    created_by TEXT NOT NULL,
    FOREIGN KEY (repo_id) REFERENCES git_repositories(repo_id)
);

-- Index for training jobs
CREATE INDEX IF NOT EXISTS idx_training_jobs_repo ON repository_training_jobs(repo_id);
CREATE INDEX IF NOT EXISTS idx_training_jobs_status ON repository_training_jobs(status);
CREATE INDEX IF NOT EXISTS idx_training_jobs_created_by ON repository_training_jobs(created_by);

-- Repository evidence spans table
CREATE TABLE IF NOT EXISTS repository_evidence_spans (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    span_id TEXT NOT NULL,
    evidence_type TEXT NOT NULL,
    file_path TEXT NOT NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL,
    relevance_score REAL NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (repo_id) REFERENCES git_repositories(repo_id)
);

-- Index for evidence spans
CREATE INDEX IF NOT EXISTS idx_evidence_spans_repo ON repository_evidence_spans(repo_id);
CREATE INDEX IF NOT EXISTS idx_evidence_spans_type ON repository_evidence_spans(evidence_type);
CREATE INDEX IF NOT EXISTS idx_evidence_spans_score ON repository_evidence_spans(relevance_score);

-- Repository security violations table
CREATE TABLE IF NOT EXISTS repository_security_violations (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    pattern TEXT NOT NULL,
    line_number INTEGER,
    severity TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (repo_id) REFERENCES git_repositories(repo_id)
);

-- Index for security violations
CREATE INDEX IF NOT EXISTS idx_security_violations_repo ON repository_security_violations(repo_id);
CREATE INDEX IF NOT EXISTS idx_security_violations_severity ON repository_security_violations(severity);

-- Repository analysis cache table
CREATE TABLE IF NOT EXISTS repository_analysis_cache (
    id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    analysis_type TEXT NOT NULL,
    analysis_data_json TEXT NOT NULL,
    cache_key TEXT NOT NULL,
    expires_at TIMESTAMP NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (repo_id) REFERENCES git_repositories(repo_id)
);

-- Index for analysis cache
CREATE INDEX IF NOT EXISTS idx_analysis_cache_repo ON repository_analysis_cache(repo_id);
CREATE INDEX IF NOT EXISTS idx_analysis_cache_type ON repository_analysis_cache(analysis_type);
CREATE INDEX IF NOT EXISTS idx_analysis_cache_expires ON repository_analysis_cache(expires_at);

-- Repository training metrics table
CREATE TABLE IF NOT EXISTS repository_training_metrics (
    id TEXT PRIMARY KEY,
    training_job_id TEXT NOT NULL,
    metric_name TEXT NOT NULL,
    metric_value REAL NOT NULL,
    metric_timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (training_job_id) REFERENCES repository_training_jobs(id)
);

-- Index for training metrics
CREATE INDEX IF NOT EXISTS idx_training_metrics_job ON repository_training_metrics(training_job_id);
CREATE INDEX IF NOT EXISTS idx_training_metrics_name ON repository_training_metrics(metric_name);
CREATE INDEX IF NOT EXISTS idx_training_metrics_timestamp ON repository_training_metrics(metric_timestamp);
