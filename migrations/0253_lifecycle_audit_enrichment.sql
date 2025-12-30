-- Lifecycle Audit Trail Enrichment
--
-- Extends adapter_lifecycle_history with additional context for compliance,
-- debugging, and automated policy enforcement.
--
-- Evidence: Set 32 Point 3 - Enforce lifecycle state semantics in lifecycle.rs
-- Pattern: Audit trail extension with structured context
--
-- Use cases:
-- 1. Distinguish manual vs automated state transitions
-- 2. Link transitions to policy rules that triggered them
-- 3. Track precondition validation results for debugging
-- 4. Support compliance audits with rich context

-- Add automation source tracking
ALTER TABLE adapter_lifecycle_history
    ADD COLUMN automation_source TEXT DEFAULT 'manual';
    -- Values: 'manual', 'api', 'cli', 'ci', 'policy', 'scheduler'

-- Add policy rule linkage
ALTER TABLE adapter_lifecycle_history
    ADD COLUMN policy_rule_id TEXT;
    -- References lifecycle_rules(id) if transition was policy-triggered

-- Add precondition snapshot (JSON)
ALTER TABLE adapter_lifecycle_history
    ADD COLUMN preconditions_json TEXT;
    -- Records which preconditions were checked and their results

-- Add context for debugging
ALTER TABLE adapter_lifecycle_history
    ADD COLUMN context_json TEXT;
    -- Additional context: version changed, artifact uploaded, tier, etc.

-- Index for automation-based queries (compliance audits)
CREATE INDEX IF NOT EXISTS idx_alh_automation
    ON adapter_lifecycle_history(automation_source)
    WHERE automation_source IS NOT NULL;

-- Index for policy-triggered transitions
CREATE INDEX IF NOT EXISTS idx_alh_policy_rule
    ON adapter_lifecycle_history(policy_rule_id)
    WHERE policy_rule_id IS NOT NULL;
