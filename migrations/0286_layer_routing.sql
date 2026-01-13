-- Patent 3535886.0002 Compliance: Per-Layer Adapter MoE Routing (Claim 7)
--
-- Stores per-layer routing decisions for adapter-layer mixture-of-experts.
-- Each layer can activate different adapters with different gate values.

-- Per-layer routing decisions table
CREATE TABLE IF NOT EXISTS layer_routing_decisions (
    id TEXT PRIMARY KEY,
    -- Link to inference trace
    trace_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    -- Token/step context
    step_idx INTEGER NOT NULL,
    token_id INTEGER NOT NULL DEFAULT 0,
    -- Layer context
    layer_idx INTEGER NOT NULL,
    layer_type TEXT NOT NULL, -- 'attention', 'ffn', 'combined', etc.
    -- Routing decision (JSON arrays for flexibility)
    adapter_ids TEXT NOT NULL, -- JSON array of adapter IDs
    adapter_indices TEXT NOT NULL, -- JSON array of u16 indices
    gates_q15 TEXT NOT NULL, -- JSON array of i16 Q15 values
    entropy REAL NOT NULL DEFAULT 0.0,
    -- Layer features (optional, for debugging/analysis)
    attention_entropy REAL,
    hidden_state_norm REAL,
    activation_mean REAL,
    activation_variance REAL,
    -- Cryptographic binding
    decision_hash_b3 BYTEA NOT NULL,
    -- Timestamps
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_layer_routing_trace 
    ON layer_routing_decisions(trace_id);
CREATE INDEX IF NOT EXISTS idx_layer_routing_trace_step 
    ON layer_routing_decisions(trace_id, step_idx);
CREATE INDEX IF NOT EXISTS idx_layer_routing_trace_layer 
    ON layer_routing_decisions(trace_id, layer_idx);
CREATE INDEX IF NOT EXISTS idx_layer_routing_tenant 
    ON layer_routing_decisions(tenant_id);

-- Layer routing chain summary table (per-step aggregation)
CREATE TABLE IF NOT EXISTS layer_routing_chains (
    id TEXT PRIMARY KEY,
    trace_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    -- Step context
    step_idx INTEGER NOT NULL,
    token_id INTEGER NOT NULL DEFAULT 0,
    -- Summary statistics
    num_layers INTEGER NOT NULL,
    active_layers INTEGER NOT NULL,
    total_adapters INTEGER NOT NULL,
    avg_entropy REAL NOT NULL,
    -- Cryptographic binding (hash of all layer decisions)
    chain_hash_b3 BYTEA NOT NULL,
    -- Timestamps
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    
    UNIQUE(trace_id, step_idx)
);

-- Index for chain queries
CREATE INDEX IF NOT EXISTS idx_layer_chains_trace 
    ON layer_routing_chains(trace_id);
CREATE INDEX IF NOT EXISTS idx_layer_chains_tenant 
    ON layer_routing_chains(tenant_id);
