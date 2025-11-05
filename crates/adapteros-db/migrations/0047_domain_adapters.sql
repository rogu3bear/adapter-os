-- Domain adapters table for storing domain-specific adapter metadata
-- Citation: crates/adapteros-api-types/src/domain_adapters.rs

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
    config JSONB NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'unloaded',
    epsilon_stats JSONB,
    last_execution TIMESTAMP WITH TIME ZONE,
    execution_count BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index for efficient lookups
CREATE INDEX idx_domain_adapters_domain_type ON domain_adapters(domain_type);
CREATE INDEX idx_domain_adapters_model ON domain_adapters(model);
CREATE INDEX idx_domain_adapters_status ON domain_adapters(status);

-- Domain adapter executions table for tracking execution history
CREATE TABLE domain_adapter_executions (
    execution_id TEXT PRIMARY KEY,
    adapter_id TEXT NOT NULL REFERENCES domain_adapters(id) ON DELETE CASCADE,
    input_hash TEXT NOT NULL,
    output_hash TEXT NOT NULL,
    epsilon DOUBLE PRECISION NOT NULL,
    execution_time_ms BIGINT NOT NULL,
    trace_events JSONB NOT NULL DEFAULT '[]',
    executed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index for execution history
CREATE INDEX idx_domain_adapter_executions_adapter_id ON domain_adapter_executions(adapter_id);
CREATE INDEX idx_domain_adapter_executions_executed_at ON domain_adapter_executions(executed_at);

-- Domain adapter tests table for tracking test results
CREATE TABLE domain_adapter_tests (
    test_id TEXT PRIMARY KEY,
    adapter_id TEXT NOT NULL REFERENCES domain_adapters(id) ON DELETE CASCADE,
    input_data TEXT NOT NULL,
    actual_output TEXT NOT NULL,
    expected_output TEXT,
    epsilon DOUBLE PRECISION,
    passed BOOLEAN NOT NULL,
    iterations INTEGER NOT NULL,
    execution_time_ms BIGINT NOT NULL,
    executed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index for test history
CREATE INDEX idx_domain_adapter_tests_adapter_id ON domain_adapter_tests(adapter_id);
CREATE INDEX idx_domain_adapter_tests_passed ON domain_adapter_tests(passed);
