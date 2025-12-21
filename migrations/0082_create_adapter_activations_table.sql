-- ============================================================================
-- Adapter Activations Table Creation
-- ============================================================================
-- File: migrations/0082_create_adapter_activations_table.sql
-- Purpose: Create the adapter_activations table for tracking router decisions
-- Status: New migration to align schema with code references
-- Dependencies: adapters table (migration 0001)
-- ============================================================================

-- Create adapter_activations table for tracking router decisions and gate values
CREATE TABLE IF NOT EXISTS adapter_activations (
    id TEXT PRIMARY KEY,
    adapter_id TEXT NOT NULL,
    request_id TEXT,
    gate_value REAL NOT NULL,
    selected INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (adapter_id) REFERENCES adapters(id) ON DELETE CASCADE
);

-- Create indexes for performance
CREATE INDEX IF NOT EXISTS idx_adapter_activations_adapter_id
    ON adapter_activations(adapter_id);
CREATE INDEX IF NOT EXISTS idx_adapter_activations_request_id
    ON adapter_activations(request_id);
CREATE INDEX IF NOT EXISTS idx_adapter_activations_created_at
    ON adapter_activations(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_adapter_activations_selected
    ON adapter_activations(selected);
