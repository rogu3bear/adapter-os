-- Replay session storage with full system state snapshots
-- Enables deterministic replay of inference sessions with cryptographic verification

CREATE TABLE IF NOT EXISTS replay_sessions (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    cpid TEXT NOT NULL,
    plan_id TEXT NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
    snapshot_at TEXT NOT NULL, -- ISO-8601 UTC timestamp
    seed_global_b3 TEXT NOT NULL,
    manifest_hash_b3 TEXT NOT NULL,
    policy_hash_b3 TEXT NOT NULL,
    kernel_hash_b3 TEXT,
    telemetry_bundle_ids_json TEXT NOT NULL, -- Array of bundle IDs
    adapter_state_json TEXT NOT NULL, -- Full adapter registry snapshot
    routing_decisions_json TEXT NOT NULL, -- Router state
    inference_traces_json TEXT, -- Trace records
    rng_state_json TEXT NOT NULL, -- RNG state for deterministic replay (Ruleset #2)
    signature TEXT NOT NULL, -- Ed25519 signature over snapshot
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_replay_sessions_tenant ON replay_sessions(tenant_id, snapshot_at DESC);
CREATE INDEX IF NOT EXISTS idx_replay_sessions_cpid ON replay_sessions(cpid);
CREATE INDEX IF NOT EXISTS idx_replay_sessions_snapshot ON replay_sessions(snapshot_at DESC);

