-- Determinism check results persisted by `aosctl diag run`
-- Read by GET /v1/diagnostics/determinism (diagnostics.rs handler)

CREATE TABLE IF NOT EXISTS determinism_checks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    last_run TIMESTAMP NOT NULL,
    result TEXT NOT NULL CHECK (result IN ('pass', 'fail')),
    runs INTEGER NOT NULL,
    divergences INTEGER NOT NULL DEFAULT 0,
    stack_id TEXT,
    seed TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_determinism_checks_last_run
    ON determinism_checks(last_run DESC);
