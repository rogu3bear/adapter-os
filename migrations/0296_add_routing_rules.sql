-- Migration 0296: Add routing_rules table
-- Purpose: Support identity-based routing rules
-- Date: 2026-01-24
CREATE TABLE IF NOT EXISTS routing_rules (
    id TEXT PRIMARY KEY,
    identity_dataset_id TEXT,
    condition_logic TEXT NOT NULL,
    target_adapter_id TEXT NOT NULL,
    priority INTEGER NOT NULL,
    created_at TEXT DEFAULT (datetime('now')),
    created_by TEXT,
    FOREIGN KEY (identity_dataset_id) REFERENCES training_dataset_versions(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_routing_rules_identity ON routing_rules(identity_dataset_id);