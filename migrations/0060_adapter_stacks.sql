-- Named adapter stacks for workflow selection
-- Enables grouping multiple adapters into reusable stacks
CREATE TABLE IF NOT EXISTS adapter_stacks (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT UNIQUE NOT NULL,
    description TEXT,
    adapter_ids_json TEXT NOT NULL, -- JSON array of adapter IDs
    workflow_type TEXT, -- NULL, 'Parallel', 'UpstreamDownstream', 'Sequential'
    created_by TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    CONSTRAINT valid_workflow_type CHECK (
        workflow_type IS NULL OR
        workflow_type IN ('Parallel', 'UpstreamDownstream', 'Sequential')
    )
);

-- Index for faster lookups
CREATE INDEX idx_adapter_stacks_name ON adapter_stacks(name);
CREATE INDEX idx_adapter_stacks_created_at ON adapter_stacks(created_at DESC);