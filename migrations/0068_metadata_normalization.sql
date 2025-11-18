-- PRD-02: Adapter & Stack Metadata Normalization
-- Adds version and lifecycle state fields to adapters and stacks

-- Add version field to adapters table
ALTER TABLE adapters ADD COLUMN version TEXT NOT NULL DEFAULT '1.0.0';

-- Add lifecycle state field to adapters table
-- States: draft, active, deprecated, retired
-- Note: This is distinct from current_state (which is runtime loading state)
-- and load_state (which is lifecycle tier)
ALTER TABLE adapters ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'active';

-- Add constraint to validate lifecycle_state values
CREATE TRIGGER IF NOT EXISTS validate_adapter_lifecycle_state
BEFORE INSERT ON adapters
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'active', 'deprecated', 'retired')
        THEN RAISE(ABORT, 'Invalid lifecycle_state: must be draft, active, deprecated, or retired')
    END;
END;

CREATE TRIGGER IF NOT EXISTS validate_adapter_lifecycle_state_update
BEFORE UPDATE ON adapters
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'active', 'deprecated', 'retired')
        THEN RAISE(ABORT, 'Invalid lifecycle_state: must be draft, active, deprecated, or retired')
    END;
END;

-- Add version field to adapter_stacks table
ALTER TABLE adapter_stacks ADD COLUMN version TEXT NOT NULL DEFAULT '1.0.0';

-- Add lifecycle state field to adapter_stacks table
ALTER TABLE adapter_stacks ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'active';

-- Add constraint to validate stack lifecycle_state values
CREATE TRIGGER IF NOT EXISTS validate_stack_lifecycle_state
BEFORE INSERT ON adapter_stacks
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'active', 'deprecated', 'retired')
        THEN RAISE(ABORT, 'Invalid lifecycle_state: must be draft, active, deprecated, or retired')
    END;
END;

CREATE TRIGGER IF NOT EXISTS validate_stack_lifecycle_state_update
BEFORE UPDATE ON adapter_stacks
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'active', 'deprecated', 'retired')
        THEN RAISE(ABORT, 'Invalid lifecycle_state: must be draft, active, deprecated, or retired')
    END;
END;

-- Add schema_version field for API/telemetry payload versioning
-- This is a metadata field, not stored in tables, but documented here for reference
-- schema_version will be added to API responses and telemetry bundles

-- Add indexes for common queries on new fields
CREATE INDEX IF NOT EXISTS idx_adapters_lifecycle_state ON adapters(lifecycle_state);
CREATE INDEX IF NOT EXISTS idx_adapters_version ON adapters(version);
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_lifecycle_state ON adapter_stacks(lifecycle_state);
CREATE INDEX IF NOT EXISTS idx_adapter_stacks_version ON adapter_stacks(version);

-- Validation rules documented (checked in application logic):
-- 1. ephemeral tier adapters cannot be deprecated (must be retired directly)
-- 2. version must follow semantic versioning (major.minor.patch) or be monotonic
-- 3. state transitions: draft -> active -> deprecated -> retired
-- 4. retired adapters cannot transition back to any other state
