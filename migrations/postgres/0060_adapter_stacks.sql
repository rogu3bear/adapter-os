-- Named adapter stacks for workflow selection (PostgreSQL version)
-- Enables grouping multiple adapters into reusable stacks

CREATE TABLE IF NOT EXISTS adapter_stacks (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    name TEXT UNIQUE NOT NULL,
    description TEXT,
    adapter_ids_json TEXT NOT NULL, -- JSON array of adapter IDs
    workflow_type TEXT, -- NULL, 'Parallel', 'UpstreamDownstream', 'Sequential'
    created_by TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT valid_workflow_type CHECK (
        workflow_type IS NULL OR
        workflow_type IN ('Parallel', 'UpstreamDownstream', 'Sequential')
    )
);

-- Index for faster lookups
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_name ON adapter_stacks(name);
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_created_at ON adapter_stacks(created_at DESC);

-- Trigger to automatically update the updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_adapter_stacks_updated_at
    BEFORE UPDATE ON adapter_stacks
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();