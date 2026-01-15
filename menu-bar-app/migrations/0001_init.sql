-- adapterOS Control Plane Database Schema
-- SQLite with WAL mode for concurrent reads

-- Users table: local authentication and RBAC
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    pw_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('admin','operator','sre','compliance','auditor','viewer')),
    disabled INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Tenants table: multi-tenant isolation boundaries
CREATE TABLE IF NOT EXISTS tenants (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    itar_flag INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Nodes table: worker hosts running aos-node agent
CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    hostname TEXT UNIQUE NOT NULL,
    agent_endpoint TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','active','offline','maintenance')),
    last_seen_at TEXT,
    labels_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Models table: base model artifacts
CREATE TABLE IF NOT EXISTS models (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    hash_b3 TEXT UNIQUE NOT NULL,
    license_hash_b3 TEXT,
    config_hash_b3 TEXT NOT NULL,
    tokenizer_hash_b3 TEXT NOT NULL,
    tokenizer_cfg_hash_b3 TEXT NOT NULL,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Adapters table: per-tenant LoRA adapters
CREATE TABLE IF NOT EXISTS adapters (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    tier TEXT NOT NULL CHECK(tier IN ('persistent','warm','ephemeral')),
    hash_b3 TEXT UNIQUE NOT NULL,
    rank INTEGER NOT NULL,
    alpha REAL NOT NULL,
    targets_json TEXT NOT NULL,
    acl_json TEXT,
    adapter_id TEXT,  -- External adapter ID for lookups
    languages_json TEXT,
    framework TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, name)
);

CREATE INDEX IF NOT EXISTS idx_adapters_adapter_id ON adapters(adapter_id);
CREATE INDEX IF NOT EXISTS idx_adapters_active ON adapters(active);

-- Manifests table: declarative configuration
CREATE TABLE IF NOT EXISTS manifests (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    hash_b3 TEXT UNIQUE NOT NULL,
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Plans table: compiled execution plans with kernel hashes
CREATE TABLE IF NOT EXISTS plans (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    plan_id_b3 TEXT UNIQUE NOT NULL,
    manifest_hash_b3 TEXT NOT NULL REFERENCES manifests(hash_b3),
    kernel_hashes_json TEXT NOT NULL,
    layout_hash_b3 TEXT NOT NULL,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- CP Pointers table: active plan pointers (e.g., "production", "staging")
CREATE TABLE IF NOT EXISTS cp_pointers (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    plan_id TEXT NOT NULL REFERENCES plans(id),
    active INTEGER NOT NULL DEFAULT 1,
    promoted_by TEXT REFERENCES users(id),
    promoted_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, name)
);

-- Policies table: policy packs per tenant
CREATE TABLE IF NOT EXISTS policies (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    hash_b3 TEXT NOT NULL,
    body_json TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, active) -- Only one active policy per tenant
);

-- Jobs table: async tasks (build, audit, replay)
CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('build_plan','audit','replay','node_command')),
    tenant_id TEXT REFERENCES tenants(id) ON DELETE SET NULL,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    payload_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','running','finished','failed','cancelled')),
    result_json TEXT,
    logs_path TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    finished_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_status_created_at ON jobs(status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_tenant_id ON jobs(tenant_id);

-- Telemetry Bundles table: NDJSON event bundles
CREATE TABLE IF NOT EXISTS telemetry_bundles (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    cpid TEXT NOT NULL,
    path TEXT UNIQUE NOT NULL,
    merkle_root_b3 TEXT NOT NULL,
    start_seq INTEGER NOT NULL,
    end_seq INTEGER NOT NULL,
    event_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_telemetry_bundles_cpid ON telemetry_bundles(cpid);
CREATE INDEX IF NOT EXISTS idx_telemetry_bundles_tenant ON telemetry_bundles(tenant_id, created_at DESC);

-- Audits table: hallucination metrics and compliance checks
CREATE TABLE IF NOT EXISTS audits (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    cpid TEXT NOT NULL,
    suite_name TEXT NOT NULL,
    bundle_id TEXT REFERENCES telemetry_bundles(id),
    arr REAL,          -- Answer Relevance Rate
    ecs5 REAL,         -- Evidence Coverage Score @5
    hlr REAL,          -- Hallucination Rate
    cr REAL,           -- Conflict Rate
    nar REAL,          -- Numeric Accuracy Rate
    par REAL,          -- Provenance Attribution Rate
    verdict TEXT NOT NULL CHECK(verdict IN ('pass','fail','warn')),
    details_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_audits_cpid ON audits(cpid, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audits_verdict ON audits(verdict);

-- Workers table: active worker processes
CREATE TABLE IF NOT EXISTS workers (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    plan_id TEXT NOT NULL REFERENCES plans(id),
    uds_path TEXT NOT NULL,
    pid INTEGER,
    status TEXT NOT NULL DEFAULT 'starting' CHECK(status IN ('starting','serving','draining','stopped','crashed')),
    memory_headroom_pct REAL,
    k_current INTEGER,
    adapters_loaded_json TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_heartbeat_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_workers_tenant ON workers(tenant_id);
CREATE INDEX IF NOT EXISTS idx_workers_node ON workers(node_id);
CREATE INDEX IF NOT EXISTS idx_workers_status ON workers(status);

-- Artifacts table: CAS registry with signatures
CREATE TABLE IF NOT EXISTS artifacts (
    hash_b3 TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('model','adapter','metallib','sbom','plan','bundle')),
    signature_b64 TEXT NOT NULL,
    sbom_hash_b3 TEXT,
    size_bytes INTEGER NOT NULL,
    imported_by TEXT REFERENCES users(id),
    imported_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Incidents table: security and policy violations
CREATE TABLE IF NOT EXISTS incidents (
    id TEXT PRIMARY KEY,
    tenant_id TEXT REFERENCES tenants(id),
    severity TEXT NOT NULL CHECK(severity IN ('critical','high','medium','low')),
    kind TEXT NOT NULL,
    description TEXT NOT NULL,
    worker_id TEXT REFERENCES workers(id),
    bundle_id TEXT REFERENCES telemetry_bundles(id),
    resolved INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_incidents_tenant ON incidents(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_incidents_resolved ON incidents(resolved);
