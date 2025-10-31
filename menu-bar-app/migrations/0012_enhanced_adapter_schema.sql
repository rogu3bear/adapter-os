-- Migration: Enhanced Adapter Schema for Code Intelligence
-- Description: Adds code intelligence fields and lifecycle state management to adapters table
-- Date: 2024-01-01
-- Version: 0012

-- Add new columns to adapters table for code intelligence
ALTER TABLE adapters ADD COLUMN category TEXT DEFAULT 'code' NOT NULL;
ALTER TABLE adapters ADD COLUMN scope TEXT DEFAULT 'global' NOT NULL;
ALTER TABLE adapters ADD COLUMN framework_id TEXT;
ALTER TABLE adapters ADD COLUMN framework_version TEXT;
ALTER TABLE adapters ADD COLUMN repo_id TEXT;
ALTER TABLE adapters ADD COLUMN commit_sha TEXT;
ALTER TABLE adapters ADD COLUMN intent TEXT;

-- Add lifecycle state management columns
ALTER TABLE adapters ADD COLUMN current_state TEXT DEFAULT 'unloaded' NOT NULL;
ALTER TABLE adapters ADD COLUMN pinned INTEGER DEFAULT 0 NOT NULL;
ALTER TABLE adapters ADD COLUMN memory_bytes INTEGER DEFAULT 0 NOT NULL;
ALTER TABLE adapters ADD COLUMN last_activated TEXT;
ALTER TABLE adapters ADD COLUMN activation_count INTEGER DEFAULT 0 NOT NULL;

-- Add indexes for performance
CREATE INDEX IF NOT EXISTS idx_adapters_category ON adapters(category);
CREATE INDEX IF NOT EXISTS idx_adapters_scope ON adapters(scope);
CREATE INDEX IF NOT EXISTS idx_adapters_framework ON adapters(framework_id);
CREATE INDEX IF NOT EXISTS idx_adapters_repo ON adapters(repo_id);
CREATE INDEX IF NOT EXISTS idx_adapters_state ON adapters(current_state);
CREATE INDEX IF NOT EXISTS idx_adapters_pinned ON adapters(pinned);
CREATE INDEX IF NOT EXISTS idx_adapters_last_activated ON adapters(last_activated);

-- Add constraints for data integrity
-- Category must be one of: code, framework, codebase, ephemeral
CREATE TABLE IF NOT EXISTS adapter_categories (
    name TEXT PRIMARY KEY
);

INSERT OR IGNORE INTO adapter_categories (name) VALUES 
    ('code'),
    ('framework'), 
    ('codebase'),
    ('ephemeral');

-- Scope must be one of: global, tenant, repo, commit
CREATE TABLE IF NOT EXISTS adapter_scopes (
    name TEXT PRIMARY KEY
);

INSERT OR IGNORE INTO adapter_scopes (name) VALUES 
    ('global'),
    ('tenant'),
    ('repo'),
    ('commit');

-- State must be one of: unloaded, cold, warm, hot, resident
CREATE TABLE IF NOT EXISTS adapter_states (
    name TEXT PRIMARY KEY
);

INSERT OR IGNORE INTO adapter_states (name) VALUES 
    ('unloaded'),
    ('cold'),
    ('warm'),
    ('hot'),
    ('resident');

-- Add trigger to validate category
CREATE TRIGGER IF NOT EXISTS validate_adapter_category
    AFTER INSERT ON adapters
    WHEN NEW.category NOT IN (SELECT name FROM adapter_categories)
BEGIN
    SELECT RAISE(ABORT, 'Invalid adapter category');
END;

-- Add trigger to validate scope
CREATE TRIGGER IF NOT EXISTS validate_adapter_scope
    AFTER INSERT ON adapters
    WHEN NEW.scope NOT IN (SELECT name FROM adapter_scopes)
BEGIN
    SELECT RAISE(ABORT, 'Invalid adapter scope');
END;

-- Add trigger to validate state
CREATE TRIGGER IF NOT EXISTS validate_adapter_state
    AFTER INSERT ON adapters
    WHEN NEW.current_state NOT IN (SELECT name FROM adapter_states)
BEGIN
    SELECT RAISE(ABORT, 'Invalid adapter state');
END;

-- Add trigger to update updated_at on state changes
CREATE TRIGGER IF NOT EXISTS update_adapter_state_timestamp
    AFTER UPDATE OF current_state ON adapters
BEGIN
    UPDATE adapters 
    SET updated_at = datetime('now') 
    WHERE id = NEW.id;
END;

-- Add trigger to update activation count
-- TODO: Re-enable when adapter_activations table is created
-- CREATE TRIGGER IF NOT EXISTS update_adapter_activation_count
--     AFTER INSERT ON adapter_activations
--     WHEN NEW.selected = 1
-- BEGIN
--     UPDATE adapters 
--     SET activation_count = activation_count + 1,
--         last_activated = datetime('now'),
--         updated_at = datetime('now')
--     WHERE adapter_id = NEW.adapter_id;
-- END;

-- Create view for adapter state summary
CREATE VIEW IF NOT EXISTS adapter_state_summary AS
SELECT 
    category,
    scope,
    current_state,
    COUNT(*) as count,
    SUM(memory_bytes) as total_memory_bytes,
    AVG(activation_count) as avg_activations,
    MAX(last_activated) as most_recent_activation
FROM adapters 
WHERE active = 1
GROUP BY category, scope, current_state
ORDER BY category, scope, current_state;

-- Create view for framework adapters
CREATE VIEW IF NOT EXISTS framework_adapters AS
SELECT 
    adapter_id,
    name,
    framework_id,
    framework_version,
    current_state,
    memory_bytes,
    activation_count,
    last_activated
FROM adapters 
WHERE category = 'framework' AND active = 1
ORDER BY framework_id, framework_version, activation_count DESC;

-- Create view for codebase adapters by tenant
CREATE VIEW IF NOT EXISTS codebase_adapters_by_tenant AS
SELECT 
    repo_id,
    COUNT(*) as adapter_count,
    SUM(memory_bytes) as total_memory_bytes,
    MAX(last_activated) as most_recent_activation,
    AVG(activation_count) as avg_activations
FROM adapters 
WHERE category = 'codebase' AND active = 1
GROUP BY repo_id
ORDER BY total_memory_bytes DESC;

-- Update existing adapters to have proper defaults
UPDATE adapters 
SET 
    category = CASE 
        WHEN framework IS NOT NULL THEN 'framework'
        ELSE 'code'
    END,
    scope = 'global',
    current_state = 'unloaded',
    pinned = 0,
    memory_bytes = 0,
    activation_count = 0
WHERE category IS NULL OR scope IS NULL OR current_state IS NULL;

-- Migration completion logged by sqlx automatically
