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

-- Stack naming validation trigger (moved from migration 0061)
CREATE TRIGGER IF NOT EXISTS validate_stack_name_format
BEFORE INSERT ON adapter_stacks
FOR EACH ROW
BEGIN
    -- Validate format: stack.{namespace}[.{identifier}]
    SELECT CASE
        WHEN NEW.name NOT GLOB 'stack.[a-z0-9]*[a-z0-9]'
            AND NEW.name NOT GLOB 'stack.[a-z0-9]*[a-z0-9].[a-z0-9]*[a-z0-9]'
        THEN RAISE(ABORT, 'Invalid stack name format: must match stack.{namespace}[.{identifier}]')
    END;

    -- Validate max length
    SELECT CASE
        WHEN length(NEW.name) > 100
        THEN RAISE(ABORT, 'Stack name exceeds 100 character limit')
    END;

    -- Validate no consecutive hyphens
    SELECT CASE
        WHEN NEW.name LIKE '%---%'
        THEN RAISE(ABORT, 'Stack name cannot contain consecutive hyphens')
    END;

    -- Reject reserved stack names
    SELECT CASE
        WHEN NEW.name IN ('stack.safe-default', 'stack.system')
        THEN RAISE(ABORT, 'Stack name is reserved')
    END;
END;