-- Policy Quarantine Table
-- 
-- Tracks system quarantine events triggered by policy violations,
-- particularly federation verification failures and policy hash mismatches.
--
-- Per Determinism Ruleset #2 and Incident Ruleset #17

CREATE TABLE IF NOT EXISTS policy_quarantine (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    reason TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    released BOOLEAN NOT NULL DEFAULT FALSE,
    released_at TIMESTAMP,
    released_by TEXT,
    cpid TEXT,
    violation_type TEXT,
    metadata TEXT -- JSON metadata about the violation
);

CREATE INDEX idx_policy_quarantine_created ON policy_quarantine(created_at DESC);
CREATE INDEX idx_policy_quarantine_released ON policy_quarantine(released, created_at DESC);
CREATE INDEX idx_policy_quarantine_cpid ON policy_quarantine(cpid) WHERE cpid IS NOT NULL;
CREATE INDEX idx_policy_quarantine_type ON policy_quarantine(violation_type) WHERE violation_type IS NOT NULL;

-- View for active quarantine events
CREATE VIEW IF NOT EXISTS active_quarantine AS
SELECT 
    id,
    reason,
    created_at,
    violation_type,
    cpid,
    metadata
FROM policy_quarantine
WHERE released = FALSE
ORDER BY created_at DESC;

