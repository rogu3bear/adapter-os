-- Corrective migration to fix PostgreSQL-specific types in migration 0047
-- Converts JSONB → TEXT, DOUBLE PRECISION → REAL, BIGINT → INTEGER, TIMESTAMP WITH TIME ZONE → TEXT
-- Citation: Multi-agent schema audit - Agent B findings
-- Priority: CRITICAL - Migration 0047 will fail on SQLite without these corrections

-- ============================================================================
-- Table 1: domain_adapters
-- ============================================================================

-- Step 1: Rename old table
ALTER TABLE domain_adapters RENAME TO domain_adapters_old;

-- Step 2: Create new table with SQLite-compatible types
CREATE TABLE domain_adapters (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    description TEXT NOT NULL,
    domain_type TEXT NOT NULL,
    model TEXT NOT NULL,
    hash TEXT NOT NULL,
    input_format TEXT NOT NULL,
    output_format TEXT NOT NULL,
    config TEXT NOT NULL DEFAULT '{}',              -- Changed from JSONB
    status TEXT NOT NULL DEFAULT 'unloaded',
    epsilon_stats TEXT,                             -- Changed from JSONB
    last_execution TEXT,                            -- Changed from TIMESTAMP WITH TIME ZONE
    execution_count INTEGER NOT NULL DEFAULT 0,     -- Changed from BIGINT
    created_at TEXT NOT NULL DEFAULT (datetime('now')),  -- Changed from TIMESTAMP WITH TIME ZONE
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))   -- Changed from TIMESTAMP WITH TIME ZONE
);

-- Step 3: Copy data from old table
INSERT INTO domain_adapters (
    id, name, version, description, domain_type, model, hash,
    input_format, output_format, config, status, epsilon_stats,
    last_execution, execution_count, created_at, updated_at
)
SELECT
    id, name, version, description, domain_type, model, hash,
    input_format, output_format, config, status, epsilon_stats,
    last_execution, execution_count, created_at, updated_at
FROM domain_adapters_old;

-- Step 4: Drop old table
DROP TABLE domain_adapters_old;

-- Step 5: Recreate indexes
CREATE INDEX idx_domain_adapters_domain_type ON domain_adapters(domain_type);
CREATE INDEX idx_domain_adapters_model ON domain_adapters(model);
CREATE INDEX idx_domain_adapters_status ON domain_adapters(status);

-- ============================================================================
-- Table 2: domain_adapter_executions
-- ============================================================================

-- Step 1: Rename old table
ALTER TABLE domain_adapter_executions RENAME TO domain_adapter_executions_old;

-- Step 2: Create new table with SQLite-compatible types
CREATE TABLE domain_adapter_executions (
    execution_id TEXT PRIMARY KEY,
    adapter_id TEXT NOT NULL REFERENCES domain_adapters(id) ON DELETE CASCADE,
    input_hash TEXT NOT NULL,
    output_hash TEXT NOT NULL,
    epsilon REAL NOT NULL,                          -- Changed from DOUBLE PRECISION
    execution_time_ms INTEGER NOT NULL,             -- Changed from BIGINT
    trace_events TEXT NOT NULL DEFAULT '[]',        -- Changed from JSONB
    executed_at TEXT NOT NULL DEFAULT (datetime('now'))  -- Changed from TIMESTAMP WITH TIME ZONE
);

-- Step 3: Copy data from old table
INSERT INTO domain_adapter_executions (
    execution_id, adapter_id, input_hash, output_hash, epsilon,
    execution_time_ms, trace_events, executed_at
)
SELECT
    execution_id, adapter_id, input_hash, output_hash, epsilon,
    execution_time_ms, trace_events, executed_at
FROM domain_adapter_executions_old;

-- Step 4: Drop old table
DROP TABLE domain_adapter_executions_old;

-- Step 5: Recreate indexes
CREATE INDEX idx_domain_adapter_executions_adapter_id ON domain_adapter_executions(adapter_id);
CREATE INDEX idx_domain_adapter_executions_executed_at ON domain_adapter_executions(executed_at);

-- ============================================================================
-- Table 3: domain_adapter_tests
-- ============================================================================

-- Step 1: Rename old table
ALTER TABLE domain_adapter_tests RENAME TO domain_adapter_tests_old;

-- Step 2: Create new table with SQLite-compatible types
CREATE TABLE domain_adapter_tests (
    test_id TEXT PRIMARY KEY,
    adapter_id TEXT NOT NULL REFERENCES domain_adapters(id) ON DELETE CASCADE,
    input_data TEXT NOT NULL,
    actual_output TEXT NOT NULL,
    expected_output TEXT,
    epsilon REAL,                                   -- Changed from DOUBLE PRECISION
    passed BOOLEAN NOT NULL,
    iterations INTEGER NOT NULL,
    execution_time_ms INTEGER NOT NULL,             -- Changed from BIGINT
    executed_at TEXT NOT NULL DEFAULT (datetime('now'))  -- Changed from TIMESTAMP WITH TIME ZONE
);

-- Step 3: Copy data from old table
INSERT INTO domain_adapter_tests (
    test_id, adapter_id, input_data, actual_output, expected_output,
    epsilon, passed, iterations, execution_time_ms, executed_at
)
SELECT
    test_id, adapter_id, input_data, actual_output, expected_output,
    epsilon, passed, iterations, execution_time_ms, executed_at
FROM domain_adapter_tests_old;

-- Step 4: Drop old table
DROP TABLE domain_adapter_tests_old;

-- Step 5: Recreate indexes
CREATE INDEX idx_domain_adapter_tests_adapter_id ON domain_adapter_tests(adapter_id);
CREATE INDEX idx_domain_adapter_tests_passed ON domain_adapter_tests(passed);

-- ============================================================================
-- Migration complete
-- ============================================================================
-- All three tables now use SQLite-compatible types:
-- - JSONB → TEXT (with JSON string content)
-- - DOUBLE PRECISION → REAL
-- - BIGINT → INTEGER
-- - TIMESTAMP WITH TIME ZONE → TEXT (with datetime() default)
