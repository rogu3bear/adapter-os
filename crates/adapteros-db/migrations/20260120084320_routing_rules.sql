-- Routing Rules for Identity Sets
--
-- Enables conditional routing of inference requests based on Identity Sets.
--
-- Table: routing_rules
-- id: UUID primary key
-- identity_dataset_id: The Identity Set this rule applies to
-- condition_logic: JSON specifying the logic (e.g. { "field": "sentiment", "op": "eq", "value": "negative" })
-- target_adapter_id: The adapter to route to if condition is met
-- priority: integer for rule ordering (higher wins)
-- created_at: Timestamp
CREATE TABLE IF NOT EXISTS routing_rules (
    id TEXT PRIMARY KEY NOT NULL,
    identity_dataset_id TEXT NOT NULL,
    condition_logic TEXT NOT NULL,
    -- JSON
    target_adapter_id TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    created_by TEXT,
    FOREIGN KEY(identity_dataset_id) REFERENCES training_datasets(id) ON DELETE CASCADE -- Note: target_adapter_id is weak reference as adapter might be ephemeral or on another node
);
CREATE INDEX IF NOT EXISTS idx_routing_rules_identity ON routing_rules(identity_dataset_id);