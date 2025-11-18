-- Migration 0070: Routing Decisions Table for Router & Stack Visibility (PRD-04)
-- Purpose: Store router decision events with timing metrics, candidate sets, and stack relationships
-- Author: JKCA
-- Date: 2025-11-17

CREATE TABLE IF NOT EXISTS routing_decisions (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    request_id TEXT,  -- Correlation with inference requests

    -- Router Decision Context
    step INTEGER NOT NULL,  -- Token generation step
    input_token_id INTEGER,  -- Token ID guiding decision
    stack_id TEXT,  -- Reference to adapter_stacks.id
    stack_hash TEXT,  -- Hash of active adapter stack

    -- Routing Parameters
    entropy REAL NOT NULL,  -- Shannon entropy of gate distribution
    tau REAL NOT NULL,  -- Temperature parameter
    entropy_floor REAL NOT NULL,  -- Epsilon enforcement threshold
    k_value INTEGER,  -- Number of adapters selected (derived from candidates)

    -- Candidate Adapters (JSON array of {adapter_idx, raw_score, gate_q15})
    candidate_adapters TEXT NOT NULL,  -- JSON array of RouterCandidate objects

    -- Selected Adapter Names (for easy filtering without JSON parsing)
    selected_adapter_ids TEXT,  -- Comma-separated list of adapter IDs

    -- Timing Metrics
    router_latency_us INTEGER,  -- Router execution time in microseconds
    total_inference_latency_us INTEGER,  -- Total inference time
    overhead_pct REAL,  -- Router overhead as percentage

    -- Metadata
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (stack_id) REFERENCES adapter_stacks(id) ON DELETE SET NULL
);

-- Index for common query patterns
CREATE INDEX IF NOT EXISTS idx_routing_decisions_tenant_timestamp
    ON routing_decisions(tenant_id, timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_routing_decisions_stack_id
    ON routing_decisions(stack_id) WHERE stack_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_routing_decisions_request_id
    ON routing_decisions(request_id) WHERE request_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_routing_decisions_timestamp
    ON routing_decisions(timestamp DESC);

-- View for recent routing decisions with enriched metadata
CREATE VIEW IF NOT EXISTS routing_decisions_enriched AS
SELECT
    rd.*,
    s.name AS stack_name,
    s.workflow_type,
    COUNT(DISTINCT json_extract(value, '$.adapter_idx')) AS num_candidates
FROM routing_decisions rd
LEFT JOIN adapter_stacks s ON rd.stack_id = s.id,
     json_each(rd.candidate_adapters) AS candidate
GROUP BY rd.id;

-- View for high-overhead routing decisions (>8% budget)
CREATE VIEW IF NOT EXISTS routing_decisions_high_overhead AS
SELECT *
FROM routing_decisions
WHERE overhead_pct > 8.0
ORDER BY timestamp DESC;

-- View for low-entropy decisions (potential routing issues)
CREATE VIEW IF NOT EXISTS routing_decisions_low_entropy AS
SELECT *
FROM routing_decisions
WHERE entropy < 0.5
ORDER BY timestamp DESC;
