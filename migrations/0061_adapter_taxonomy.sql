-- Adapter Taxonomy & Semantic Naming
-- Implements canonical naming scheme and lineage tracking
-- See docs/ADAPTER_TAXONOMY.md for specification

-- Add semantic naming columns to adapters table
-- Note: SQLite doesn't support adding UNIQUE constraints via ALTER TABLE
-- We add the column first, then create a UNIQUE index
ALTER TABLE adapters ADD COLUMN adapter_name TEXT;
ALTER TABLE adapters ADD COLUMN tenant_namespace TEXT;
ALTER TABLE adapters ADD COLUMN domain TEXT;
ALTER TABLE adapters ADD COLUMN purpose TEXT;
ALTER TABLE adapters ADD COLUMN revision TEXT;

-- Create UNIQUE index for adapter_name
CREATE UNIQUE INDEX IF NOT EXISTS idx_adapters_adapter_name_unique ON adapters(adapter_name) WHERE adapter_name IS NOT NULL;

-- Add lineage tracking columns
ALTER TABLE adapters ADD COLUMN parent_id TEXT REFERENCES adapters(id);
ALTER TABLE adapters ADD COLUMN fork_type TEXT CHECK(fork_type IS NULL OR fork_type IN ('independent', 'extension'));
ALTER TABLE adapters ADD COLUMN fork_reason TEXT;

-- Create indices for semantic name lookups
CREATE INDEX IF NOT EXISTS idx_adapters_semantic_name ON adapters(tenant_namespace, domain, purpose, revision);
CREATE INDEX IF NOT EXISTS idx_adapters_parent_id ON adapters(parent_id);
CREATE INDEX IF NOT EXISTS idx_adapters_base_path ON adapters(tenant_namespace, domain, purpose);

-- Validation trigger: enforce adapter name format
CREATE TRIGGER IF NOT EXISTS validate_adapter_name_format
BEFORE INSERT ON adapters
FOR EACH ROW
WHEN NEW.adapter_name IS NOT NULL
BEGIN
    -- Validate format: {tenant}/{domain}/{purpose}/r{NNN}
    SELECT CASE
        WHEN NEW.adapter_name NOT GLOB '[a-z0-9]*[a-z0-9]/[a-z0-9]*[a-z0-9]/[a-z0-9]*[a-z0-9]/r[0-9][0-9][0-9]*'
        THEN RAISE(ABORT, 'Invalid adapter name format: must match {tenant}/{domain}/{purpose}/r{NNN}')
    END;

    -- Validate max length
    SELECT CASE
        WHEN length(NEW.adapter_name) > 200
        THEN RAISE(ABORT, 'Adapter name exceeds 200 character limit')
    END;

    -- Validate no consecutive hyphens
    SELECT CASE
        WHEN NEW.adapter_name LIKE '%---%'
        THEN RAISE(ABORT, 'Adapter name cannot contain consecutive hyphens')
    END;
END;

-- Validation trigger: enforce parent exists before child
CREATE TRIGGER IF NOT EXISTS validate_parent_exists
BEFORE INSERT ON adapters
FOR EACH ROW
WHEN NEW.parent_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN (SELECT COUNT(*) FROM adapters WHERE id = NEW.parent_id) = 0
        THEN RAISE(ABORT, 'Parent adapter does not exist')
    END;
END;

-- Validation trigger: prevent circular parent references
CREATE TRIGGER IF NOT EXISTS validate_no_circular_parent
BEFORE UPDATE OF parent_id ON adapters
FOR EACH ROW
WHEN NEW.parent_id IS NOT NULL
BEGIN
    -- Prevent self-reference
    SELECT CASE
        WHEN NEW.id = NEW.parent_id
        THEN RAISE(ABORT, 'Adapter cannot be its own parent')
    END;

    -- Note: Full circular dependency detection requires recursive CTE
    -- which is handled in application code
END;

-- Validation trigger: ensure fork_type is set if parent exists
CREATE TRIGGER IF NOT EXISTS validate_fork_type_with_parent
BEFORE INSERT ON adapters
FOR EACH ROW
WHEN NEW.parent_id IS NOT NULL AND NEW.fork_type IS NULL
BEGIN
    SELECT RAISE(ABORT, 'fork_type must be specified when parent_id is set');
END;

-- Update trigger: sync adapter_name with components on update
CREATE TRIGGER IF NOT EXISTS sync_adapter_name_on_update
BEFORE UPDATE ON adapters
FOR EACH ROW
WHEN NEW.tenant_namespace IS NOT NULL
    AND NEW.domain IS NOT NULL
    AND NEW.purpose IS NOT NULL
    AND NEW.revision IS NOT NULL
BEGIN
    SELECT CASE
        WHEN NEW.adapter_name != (NEW.tenant_namespace || '/' || NEW.domain || '/' || NEW.purpose || '/' || NEW.revision)
        THEN RAISE(ABORT, 'adapter_name must match {tenant_namespace}/{domain}/{purpose}/{revision}')
    END;
END;

-- Backfill existing adapters with default semantic names
-- This allows gradual migration without breaking existing data
-- Note: Simplified backfill for compatibility with minimal schemas
UPDATE adapters
SET
    tenant_namespace = 'global',
    domain = 'general',
    purpose = COALESCE(
        LOWER(REPLACE(adapters.name, ' ', '-')),
        'unnamed-' || substr(adapters.id, 1, 8)
    ),
    revision = 'r001'
WHERE adapter_name IS NULL;

-- Generate adapter_name from components for backfilled records
UPDATE adapters
SET adapter_name = tenant_namespace || '/' || domain || '/' || purpose || '/' || revision
WHERE adapter_name IS NULL
    AND tenant_namespace IS NOT NULL
    AND domain IS NOT NULL
    AND purpose IS NOT NULL
    AND revision IS NOT NULL;

-- Note: Stack naming validation trigger moved to migration 0064 (where adapter_stacks table is created)

-- Add comments for documentation
-- Note: SQLite doesn't support column comments, but we can add table-level comments

-- Semantic naming schema:
-- adapter_name:       Full semantic name (e.g., "shop-floor/hydraulics/troubleshooting/r042")
-- tenant_namespace:   Tenant component (e.g., "shop-floor")
-- domain:             Domain component (e.g., "hydraulics")
-- purpose:            Purpose component (e.g., "troubleshooting")
-- revision:           Revision component (e.g., "r042")
-- parent_id:          Reference to parent adapter for lineage tracking
-- fork_type:          Type of fork: 'independent' or 'extension'
-- fork_reason:        Human-readable reason for forking
