-- Lifecycle Rules Table
-- Migration: 0247
-- Renumbered: Was 0245, renumbered to 0247 when 0242_dataset_repo_slug.sql was inserted
-- Stores rule definitions for managing adapter and dataset lifecycle transitions,
-- TTL policies, retention rules, and automated state management.
--
-- Rules are evaluated in priority order (highest first) and can be scoped to:
-- - System: applies globally
-- - Tenant: applies to a specific tenant
-- - Category: applies to a specific adapter category
-- - Adapter: applies to a specific adapter

CREATE TABLE IF NOT EXISTS lifecycle_rules (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT,

    -- Scope determines what the rule applies to
    -- Values: 'system', 'tenant', 'category', 'adapter'
    scope TEXT NOT NULL DEFAULT 'system',

    -- Target for the scope (e.g., tenant_id for tenant scope, category name for category scope)
    -- NULL for system scope
    scope_target TEXT,

    -- Type of lifecycle rule
    -- Values: 'ttl', 'retention', 'demotion', 'promotion', 'archival', 'cleanup', 'state_transition'
    rule_type TEXT NOT NULL,

    -- JSON array of conditions that must be met for the rule to apply
    -- Each condition has: { field, operator, value }
    -- Operators: equals, not_equals, greater_than, greater_than_or_equal, less_than, less_than_or_equal, in, not_in, contains, not_contains
    conditions_json TEXT NOT NULL DEFAULT '[]',

    -- JSON array of actions to take when conditions are met
    -- Each action has: { action_type, parameters }
    -- Action types: evict, delete, transition_state, archive, notify
    actions_json TEXT NOT NULL DEFAULT '[]',

    -- Priority for rule evaluation (higher = evaluated first)
    priority INTEGER NOT NULL DEFAULT 0,

    -- Whether the rule is currently active
    enabled INTEGER NOT NULL DEFAULT 1,

    -- Audit fields
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    -- Additional metadata as JSON
    metadata_json TEXT
);

-- Indexes for efficient rule lookup
CREATE INDEX IF NOT EXISTS idx_lifecycle_rules_scope ON lifecycle_rules(scope);
CREATE INDEX IF NOT EXISTS idx_lifecycle_rules_scope_target ON lifecycle_rules(scope_target);
CREATE INDEX IF NOT EXISTS idx_lifecycle_rules_rule_type ON lifecycle_rules(rule_type);
CREATE INDEX IF NOT EXISTS idx_lifecycle_rules_enabled ON lifecycle_rules(enabled);
CREATE INDEX IF NOT EXISTS idx_lifecycle_rules_priority ON lifecycle_rules(priority DESC);

-- Compound index for typical query patterns
CREATE INDEX IF NOT EXISTS idx_lifecycle_rules_active_by_type ON lifecycle_rules(rule_type, enabled, priority DESC);
