-- Adapter trust state + regression handling
-- Adds adapter_trust_state to adapter versions and auto rollback policy flag.

PRAGMA foreign_keys = ON;

-- Adapter trust surface for dataset lineage derived trust.
ALTER TABLE adapter_versions
ADD COLUMN adapter_trust_state TEXT NOT NULL DEFAULT 'unknown'
    CHECK (adapter_trust_state IN ('allowed', 'warn', 'blocked', 'unknown', 'blocked_regressed'));

CREATE INDEX IF NOT EXISTS idx_adapter_versions_trust_state
    ON adapter_versions(adapter_trust_state);

-- Repository-level policy to auto rollback on trust regressions.
ALTER TABLE adapter_repository_policies
ADD COLUMN auto_rollback_on_trust_regress INTEGER NOT NULL DEFAULT 0
    CHECK (auto_rollback_on_trust_regress IN (0, 1));

-- Normalize any NULLs from legacy rows to the default.
UPDATE adapter_repository_policies
SET auto_rollback_on_trust_regress = COALESCE(auto_rollback_on_trust_regress, 0);
