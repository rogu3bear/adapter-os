# AdapterOS Database Schema Documentation

## Table of Contents
- [Overview](#overview)
- [Architecture](#architecture)
- [Entity Relationship Diagram](#entity-relationship-diagram)
- [Core Tables](#core-tables)
- [Adapter Management Tables](#adapter-management-tables)
- [Routing and Telemetry Tables](#routing-and-telemetry-tables)
- [Training and Dataset Tables](#training-and-dataset-tables)
- [Federation and Determinism Tables](#federation-and-determinism-tables)
- [Process Management Tables](#process-management-tables)
- [Security and Compliance Tables](#security-and-compliance-tables)
- [Workspace and Collaboration Tables](#workspace-and-collaboration-tables)
- [Migration History](#migration-history)
- [Database Backend Support](#database-backend-support)
- [Common Queries](#common-queries)
- [Troubleshooting](#troubleshooting)

---

## Overview

AdapterOS uses a hybrid database architecture supporting both SQLite and PostgreSQL backends. The database schema is designed to support:

- **Multi-tenant isolation** with strict tenant boundaries
- **Adapter lifecycle management** with versioning and state tracking
- **Deterministic execution** with cross-host verification
- **Federated deployment** with peer-to-peer synchronization
- **Security and compliance** tracking with comprehensive audit trails
- **Training and inference** telemetry with evidence-based validation

**Key Design Principles:**
- Content-addressable storage using BLAKE3 hashing
- Cryptographic signatures for artifact verification
- Event sourcing patterns for audit trails
- Cross-host determinism verification
- Multi-backend abstraction (SQLite/PostgreSQL)

---

## Architecture

### Database Backend Abstraction

AdapterOS abstracts database operations through the `DatabaseBackend` trait, allowing seamless switching between SQLite and PostgreSQL:

```rust
pub trait DatabaseBackend: Send + Sync {
    async fn insert_stack(&self, req: &CreateStackRequest) -> Result<String>;
    async fn get_stack(&self, tenant_id: &str, id: &str) -> Result<Option<StackRecord>>;
    async fn list_stacks(&self) -> Result<Vec<StackRecord>>;
    async fn run_migrations(&self) -> Result<()>;
    fn database_type(&self) -> &str;
}
```

**Supported Backends:**
- **SQLite**: Default backend with WAL mode for concurrent reads
- **PostgreSQL**: Production backend with advanced features (requires `postgres` feature flag)

### Migration System

Migrations are managed using the Refinery migration framework and stored in `/crates/adapteros-db/migrations/`. Each migration is numbered sequentially (0001-0071+) and includes both schema changes and data transformations.

---

## Entity Relationship Diagram

```mermaid
erDiagram
    tenants ||--o{ adapters : owns
    tenants ||--o{ adapter_stacks : owns
    tenants ||--o{ workers : runs
    tenants ||--o{ plans : manages
    tenants ||--o{ telemetry_bundles : generates

    users ||--o{ adapters : creates
    users ||--o{ cp_pointers : promotes
    users ||--o{ jobs : initiates

    nodes ||--o{ workers : hosts

    models ||--o{ adapters : "based on"

    adapters }o--|| adapter_stacks : "grouped in"
    adapters ||--o{ adapter_version_history : tracks
    adapters ||--o{ pinned_adapters : pinned

    adapter_stacks ||--o{ stack_version_history : tracks
    adapter_stacks ||--o{ routing_decisions : routes

    workers ||--o{ process_security_audit_logs : generates

    plans ||--|| cp_pointers : points_to
    plans ||--|| manifests : compiled_from

    telemetry_bundles ||--o{ audits : audited_by

    training_datasets ||--o{ dataset_files : contains
    training_datasets ||--|| dataset_statistics : statistics

    federation_peers ||--o{ federation_output_hashes : verifies

    workspaces ||--o{ workspace_members : members
    workspaces ||--o{ workspace_resources : resources
    workspaces ||--o{ messages : messages
    workspaces ||--o{ activity_events : activity

    routing_decisions }o--|| adapter_stacks : uses
```

---

## Core Tables

### `tenants`

**Purpose:** Multi-tenant isolation boundaries. Each tenant has separate namespace for adapters, workers, and policies.

**Schema:**
```sql
CREATE TABLE tenants (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    itar_flag INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Columns:**
- `id`: Unique tenant identifier (UUID format)
- `name`: Human-readable tenant name (unique)
- `itar_flag`: ITAR compliance flag (0=non-ITAR, 1=ITAR-compliant)
- `created_at`: Tenant creation timestamp

**Indexes:**
- Primary key on `id`
- Unique constraint on `name`

**Example Query:**
```sql
-- Get all ITAR-compliant tenants
SELECT * FROM tenants WHERE itar_flag = 1;

-- Count adapters per tenant
SELECT t.name, COUNT(a.id) as adapter_count
FROM tenants t
LEFT JOIN adapters a ON t.id = a.tenant_id
GROUP BY t.id, t.name;
```

---

### `users`

**Purpose:** Local authentication and role-based access control (RBAC).

**Schema:**
```sql
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    email TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    pw_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('admin','operator','sre','compliance','auditor','viewer')),
    disabled INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Columns:**
- `id`: Unique user identifier
- `email`: User email address (unique, used for login)
- `display_name`: User's display name
- `pw_hash`: Argon2id password hash
- `role`: RBAC role (admin, operator, sre, compliance, auditor, viewer)
- `disabled`: Account disabled flag
- `created_at`: Account creation timestamp

**Roles:**
- `admin`: Full system access
- `operator`: Adapter and worker management
- `sre`: Infrastructure and monitoring
- `compliance`: Audit and policy management
- `auditor`: Read-only audit access
- `viewer`: Read-only general access

**Example Query:**
```sql
-- Find active admins
SELECT * FROM users WHERE role = 'admin' AND disabled = 0;

-- Audit user activity
SELECT u.email, COUNT(a.id) as adapters_created
FROM users u
LEFT JOIN adapters a ON u.id = a.created_by
GROUP BY u.id, u.email;
```

---

### `nodes`

**Purpose:** Worker hosts running the aos-node agent.

**Schema:**
```sql
CREATE TABLE nodes (
    id TEXT PRIMARY KEY,
    hostname TEXT UNIQUE NOT NULL,
    agent_endpoint TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','active','offline','maintenance')),
    last_seen_at TEXT,
    labels_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Columns:**
- `id`: Unique node identifier
- `hostname`: Node hostname (unique)
- `agent_endpoint`: gRPC or HTTP endpoint for node agent
- `status`: Node status (pending, active, offline, maintenance)
- `last_seen_at`: Last heartbeat timestamp
- `labels_json`: JSON object with node labels (e.g., `{"gpu": "A100", "region": "us-west"}`)
- `created_at`: Node registration timestamp

**Indexes:**
- `idx_nodes_status` on `status`

**Example Query:**
```sql
-- Find active nodes with GPUs
SELECT * FROM nodes
WHERE status = 'active'
  AND labels_json LIKE '%gpu%';

-- Nodes offline for >1 hour
SELECT * FROM nodes
WHERE status = 'active'
  AND datetime(last_seen_at) < datetime('now', '-1 hour');
```

---

### `models`

**Purpose:** Base model artifacts (e.g., Qwen2.5, Llama3.1).

**Schema:**
```sql
CREATE TABLE models (
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
```

**Columns:**
- `id`: Unique model identifier
- `name`: Model name (e.g., "Qwen2.5-7B-Instruct")
- `hash_b3`: BLAKE3 hash of model weights
- `license_hash_b3`: BLAKE3 hash of license file
- `config_hash_b3`: BLAKE3 hash of model config
- `tokenizer_hash_b3`: BLAKE3 hash of tokenizer model
- `tokenizer_cfg_hash_b3`: BLAKE3 hash of tokenizer config
- `metadata_json`: Additional metadata (JSON)

**Indexes:**
- Unique constraint on `name`
- Unique constraint on `hash_b3`

**Example Query:**
```sql
-- List all models with their adapter counts
SELECT m.name, COUNT(a.id) as adapter_count
FROM models m
LEFT JOIN adapters a ON m.id = a.model_id
GROUP BY m.id, m.name;
```

---

## Adapter Management Tables

### `adapters`

**Purpose:** Per-tenant LoRA adapters with lifecycle management.

**Schema:**
```sql
CREATE TABLE adapters (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    tier TEXT NOT NULL CHECK(tier IN ('persistent','warm','ephemeral')),
    hash_b3 TEXT UNIQUE NOT NULL,
    rank INTEGER NOT NULL,
    alpha REAL NOT NULL,
    targets_json TEXT NOT NULL,
    acl_json TEXT,
    adapter_id TEXT,
    languages_json TEXT,
    framework TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    category TEXT,
    scope TEXT,
    framework_id TEXT,
    framework_version TEXT,
    repo_id TEXT,
    commit_sha TEXT,
    intent TEXT,
    current_state TEXT,
    pinned INTEGER,
    memory_bytes INTEGER,
    last_activated TEXT,
    activation_count INTEGER,
    expires_at TEXT,
    load_state TEXT,
    last_loaded_at TEXT,
    adapter_name TEXT,
    tenant_namespace TEXT,
    domain TEXT,
    purpose TEXT,
    revision TEXT,
    parent_id TEXT,
    fork_type TEXT,
    fork_reason TEXT,
    version TEXT,
    lifecycle_state TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, name)
);
```

**Key Columns:**
- `id`: Internal database ID
- `adapter_id`: External adapter identifier (for API lookups)
- `tenant_id`: Owning tenant
- `name`: Adapter name (unique within tenant)
- `tier`: Memory tier (persistent, warm, ephemeral)
- `hash_b3`: BLAKE3 content hash of adapter weights
- `rank`: LoRA rank
- `alpha`: LoRA alpha scaling factor
- `targets_json`: JSON array of target modules (e.g., `["q_proj", "v_proj"]`)
- `lifecycle_state`: Lifecycle state (draft, active, deprecated, retired)
- `version`: Semantic version string
- `load_state`: Current load state (unloaded, loading, loaded, error)
- `pinned`: Pin status for eviction protection

**Indexes:**
- `idx_adapters_adapter_id` on `adapter_id`
- `idx_adapters_active` on `active`
- `idx_adapters_tenant_id` on `tenant_id`

**Example Query:**
```sql
-- Find all active adapters for a tenant
SELECT * FROM adapters
WHERE tenant_id = 'tenant-123'
  AND active = 1
  AND lifecycle_state = 'active';

-- Adapters nearing expiration
SELECT name, expires_at FROM adapters
WHERE expires_at IS NOT NULL
  AND datetime(expires_at) < datetime('now', '+7 days');

-- Most frequently activated adapters
SELECT name, activation_count
FROM adapters
ORDER BY activation_count DESC
LIMIT 10;
```

---

### `adapter_stacks`

**Purpose:** Named adapter stacks for workflow selection. Enables grouping multiple adapters into reusable stacks.

**Schema:**
```sql
CREATE TABLE adapter_stacks (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tenant_id TEXT NOT NULL,
    name TEXT UNIQUE NOT NULL,
    description TEXT,
    adapter_ids_json TEXT NOT NULL,
    workflow_type TEXT CHECK(workflow_type IN ('Parallel', 'UpstreamDownstream', 'Sequential')),
    lifecycle_state TEXT,
    version INTEGER,
    created_by TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);
```

**Columns:**
- `id`: Unique stack identifier
- `tenant_id`: Owning tenant
- `name`: Stack name (must match pattern `stack.{namespace}[.{identifier}]`)
- `adapter_ids_json`: JSON array of adapter IDs
- `workflow_type`: Execution workflow (Parallel, UpstreamDownstream, Sequential)
- `lifecycle_state`: Lifecycle state (draft, active, deprecated, retired)
- `version`: Auto-incremented version number

**Naming Rules:**
- Format: `stack.{namespace}[.{identifier}]`
- Max length: 100 characters
- No consecutive hyphens
- Reserved names: `stack.safe-default`, `stack.system`

**Indexes:**
- `idx_adapter_stacks_name` on `name`
- `idx_adapter_stacks_created_at` on `created_at`

**Example Query:**
```sql
-- Get stack with adapter details
SELECT
    s.name,
    s.workflow_type,
    a.name as adapter_name,
    a.lifecycle_state
FROM adapter_stacks s
CROSS JOIN json_each(s.adapter_ids_json) as adapter_id
LEFT JOIN adapters a ON json_extract(adapter_id.value, '$') = a.adapter_id
WHERE s.id = 'stack-id-123';
```

---

### `adapter_version_history`

**Purpose:** Track all lifecycle transitions and version bumps for adapters (audit trail).

**Schema:**
```sql
CREATE TABLE adapter_version_history (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    adapter_id TEXT NOT NULL,
    version TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL,
    previous_lifecycle_state TEXT,
    reason TEXT,
    initiated_by TEXT NOT NULL,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (adapter_id) REFERENCES adapters(adapter_id) ON DELETE CASCADE
);
```

**Indexes:**
- `idx_adapter_version_history_adapter_id` on `adapter_id`
- `idx_adapter_version_history_version` on `(adapter_id, version)`
- `idx_adapter_version_history_lifecycle_state` on `lifecycle_state`

**Example Query:**
```sql
-- Get version history for an adapter
SELECT version, lifecycle_state, previous_lifecycle_state, reason, created_at
FROM adapter_version_history
WHERE adapter_id = 'adapter-123'
ORDER BY created_at DESC;
```

---

### `pinned_adapters`

**Purpose:** Time-based adapter pinning with TTL support and audit trail.

**Schema:**
```sql
CREATE TABLE pinned_adapters (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    pinned_until TEXT,  -- NULL = indefinite
    reason TEXT,
    pinned_by TEXT NOT NULL,
    pinned_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, adapter_id),
    FOREIGN KEY (adapter_id) REFERENCES adapters(adapter_id) ON DELETE CASCADE
);
```

**Columns:**
- `pinned_until`: Expiration timestamp (NULL = pinned indefinitely)
- `reason`: Human-readable pin reason
- `pinned_by`: User who created the pin

**View:**
```sql
-- View: active_pinned_adapters (only non-expired pins)
CREATE VIEW active_pinned_adapters AS
SELECT pa.*, a.name as adapter_name, a.current_state
FROM pinned_adapters pa
INNER JOIN adapters a ON pa.adapter_id = a.adapter_id
WHERE pa.pinned_until IS NULL OR pa.pinned_until > datetime('now');
```

**Example Query:**
```sql
-- Find adapters pinned for more than 30 days
SELECT a.name, pa.reason, pa.pinned_at
FROM pinned_adapters pa
JOIN adapters a ON pa.adapter_id = a.adapter_id
WHERE datetime(pa.pinned_at) < datetime('now', '-30 days');
```

---

## Routing and Telemetry Tables

### `routing_decisions`

**Purpose:** Store router decision events with timing metrics, candidate sets, and stack relationships (PRD-04).

**Schema:**
```sql
CREATE TABLE routing_decisions (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    request_id TEXT,

    -- Router Decision Context
    step INTEGER NOT NULL,
    input_token_id INTEGER,
    stack_id TEXT,
    stack_hash TEXT,

    -- Routing Parameters
    entropy REAL NOT NULL,
    tau REAL NOT NULL,
    entropy_floor REAL NOT NULL,
    k_value INTEGER,

    -- Candidate Adapters (JSON array)
    candidate_adapters TEXT NOT NULL,
    selected_adapter_ids TEXT,

    -- Timing Metrics
    router_latency_us INTEGER,
    total_inference_latency_us INTEGER,
    overhead_pct REAL,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (stack_id) REFERENCES adapter_stacks(id) ON DELETE SET NULL
);
```

**Key Columns:**
- `step`: Token generation step in inference
- `entropy`: Shannon entropy of gate distribution
- `tau`: Temperature parameter
- `k_value`: Number of adapters selected
- `candidate_adapters`: JSON array of `{adapter_idx, raw_score, gate_q15}`
- `router_latency_us`: Router execution time in microseconds
- `overhead_pct`: Router overhead as percentage of total inference time

**Indexes:**
- `idx_routing_decisions_tenant_timestamp` on `(tenant_id, timestamp DESC)`
- `idx_routing_decisions_stack_id` on `stack_id`
- `idx_routing_decisions_request_id` on `request_id`

**Views:**
```sql
-- High overhead decisions (>8% budget)
CREATE VIEW routing_decisions_high_overhead AS
SELECT * FROM routing_decisions
WHERE overhead_pct > 8.0
ORDER BY timestamp DESC;

-- Low entropy decisions (potential routing issues)
CREATE VIEW routing_decisions_low_entropy AS
SELECT * FROM routing_decisions
WHERE entropy < 0.5
ORDER BY timestamp DESC;
```

**Example Query:**
```sql
-- Average router overhead by stack
SELECT
    s.name,
    AVG(rd.overhead_pct) as avg_overhead,
    AVG(rd.router_latency_us) as avg_latency_us
FROM routing_decisions rd
JOIN adapter_stacks s ON rd.stack_id = s.id
GROUP BY s.id, s.name
ORDER BY avg_overhead DESC;
```

---

### `telemetry_bundles`

**Purpose:** NDJSON event bundles with Merkle root verification.

**Schema:**
```sql
CREATE TABLE telemetry_bundles (
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
```

**Columns:**
- `cpid`: Control plane ID (plan identifier)
- `path`: File system path to NDJSON bundle
- `merkle_root_b3`: BLAKE3 Merkle root of events
- `start_seq`, `end_seq`: Event sequence range
- `event_count`: Total events in bundle

**Indexes:**
- `idx_telemetry_bundles_cpid` on `cpid`
- `idx_telemetry_bundles_tenant` on `(tenant_id, created_at DESC)`

**Example Query:**
```sql
-- Find large telemetry bundles
SELECT path, event_count, created_at
FROM telemetry_bundles
WHERE event_count > 10000
ORDER BY event_count DESC;
```

---

### `audits`

**Purpose:** Hallucination metrics and compliance checks.

**Schema:**
```sql
CREATE TABLE audits (
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
```

**Metrics:**
- `arr`: Answer Relevance Rate (0.0-1.0)
- `ecs5`: Evidence Coverage Score at top-5 (0.0-1.0)
- `hlr`: Hallucination Rate (0.0-1.0, lower is better)
- `cr`: Conflict Rate (0.0-1.0, lower is better)
- `nar`: Numeric Accuracy Rate (0.0-1.0)
- `par`: Provenance Attribution Rate (0.0-1.0)

**Indexes:**
- `idx_audits_cpid` on `(cpid, created_at DESC)`
- `idx_audits_verdict` on `verdict`

**Example Query:**
```sql
-- Find failed audits with high hallucination rate
SELECT cpid, suite_name, hlr, created_at
FROM audits
WHERE verdict = 'fail' AND hlr > 0.1
ORDER BY hlr DESC;
```

---

## Training and Dataset Tables

### `training_datasets`

**Purpose:** Store uploaded training data for adapter fine-tuning.

**Schema:**
```sql
CREATE TABLE training_datasets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    file_count INTEGER NOT NULL DEFAULT 0,
    total_size_bytes INTEGER NOT NULL DEFAULT 0,
    format TEXT NOT NULL,  -- 'patches', 'jsonl', 'txt', 'custom'
    hash_b3 TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    validation_status TEXT NOT NULL DEFAULT 'pending',
    validation_errors TEXT,
    metadata_json TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL
);
```

**Columns:**
- `format`: Dataset format (patches, jsonl, txt, custom)
- `validation_status`: Validation status (pending, valid, invalid)
- `storage_path`: File system path to dataset

**Indexes:**
- `idx_training_datasets_created_at` on `created_at DESC`
- `idx_training_datasets_format` on `format`

---

### `dataset_files`

**Purpose:** Track individual files within a training dataset.

**Schema:**
```sql
CREATE TABLE dataset_files (
    id TEXT PRIMARY KEY,
    dataset_id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    hash_b3 TEXT NOT NULL,
    mime_type TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (dataset_id) REFERENCES training_datasets(id) ON DELETE CASCADE
);
```

---

### `dataset_statistics`

**Purpose:** Cache computed statistics for datasets.

**Schema:**
```sql
CREATE TABLE dataset_statistics (
    dataset_id TEXT PRIMARY KEY,
    num_examples INTEGER NOT NULL DEFAULT 0,
    avg_input_length REAL NOT NULL DEFAULT 0.0,
    avg_target_length REAL NOT NULL DEFAULT 0.0,
    language_distribution TEXT,  -- JSON
    file_type_distribution TEXT,  -- JSON
    total_tokens INTEGER NOT NULL DEFAULT 0,
    computed_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (dataset_id) REFERENCES training_datasets(id) ON DELETE CASCADE
);
```

**Example Query:**
```sql
-- Dataset summary with statistics
SELECT
    td.name,
    td.file_count,
    td.total_size_bytes / 1024 / 1024 as size_mb,
    ds.num_examples,
    ds.total_tokens
FROM training_datasets td
LEFT JOIN dataset_statistics ds ON td.id = ds.dataset_id
WHERE td.validation_status = 'valid';
```

---

## Federation and Determinism Tables

### `federation_peers`

**Purpose:** Federated host registry for cross-host verification.

**Schema:**
```sql
CREATE TABLE federation_peers (
    host_id TEXT PRIMARY KEY,
    pubkey TEXT NOT NULL,  -- Ed25519 public key (hex)
    hostname TEXT,
    registered_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT,
    attestation_metadata TEXT,  -- JSON
    active INTEGER NOT NULL DEFAULT 1,
    UNIQUE(pubkey)
);
```

**Columns:**
- `pubkey`: Ed25519 public key for signature verification
- `attestation_metadata`: Hardware attestation data (JSON)

**Indexes:**
- `idx_federation_peers_active` on `(active, last_seen_at DESC)`

---

### `federation_output_hashes`

**Purpose:** Store inference output hashes for cross-host determinism verification.

**Schema:**
```sql
CREATE TABLE federation_output_hashes (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    session_id TEXT NOT NULL,
    host_id TEXT NOT NULL,
    output_hash TEXT NOT NULL,  -- BLAKE3 hash
    input_hash TEXT NOT NULL,   -- BLAKE3 hash
    computed_at TEXT NOT NULL DEFAULT (datetime('now')),
    deterministic INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (host_id) REFERENCES federation_peers(host_id)
);
```

**Indexes:**
- `idx_federation_output_session` on `(session_id, host_id)`
- `idx_federation_output_input_hash` on `(input_hash, host_id)`

**Example Query:**
```sql
-- Find non-deterministic outputs (different hashes for same input)
SELECT input_hash, COUNT(DISTINCT output_hash) as hash_count
FROM federation_output_hashes
GROUP BY input_hash
HAVING hash_count > 1;
```

---

### `tick_ledger_entries`

**Purpose:** Global tick ledger for deterministic execution tracking.

**Schema:**
```sql
CREATE TABLE tick_ledger_entries (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    tick INTEGER NOT NULL,
    tenant_id TEXT NOT NULL,
    host_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_hash TEXT NOT NULL,  -- BLAKE3 hash
    timestamp_us INTEGER NOT NULL,
    prev_entry_hash TEXT,  -- Merkle chain
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Columns:**
- `tick`: Global tick counter
- `event_type`: TaskSpawned, TaskCompleted, TaskFailed, TaskTimeout, TickAdvanced
- `event_hash`: BLAKE3 hash of event data
- `prev_entry_hash`: Previous entry hash (Merkle chain)

**Indexes:**
- `idx_tick_ledger_tick` on `tick DESC`
- `idx_tick_ledger_tenant` on `(tenant_id, tick DESC)`
- `idx_tick_ledger_tenant_host` on `(tenant_id, host_id, tick DESC)`

**Example Query:**
```sql
-- Verify tick ledger chain integrity
SELECT
    id,
    tick,
    event_hash,
    prev_entry_hash,
    (SELECT event_hash FROM tick_ledger_entries t2
     WHERE t2.tick = t1.tick - 1 LIMIT 1) as expected_prev
FROM tick_ledger_entries t1
WHERE prev_entry_hash != (SELECT event_hash FROM tick_ledger_entries t2
                          WHERE t2.tick = t1.tick - 1 LIMIT 1);
```

---

## Process Management Tables

### `workers`

**Purpose:** Active worker processes with resource tracking.

**Schema:**
```sql
CREATE TABLE workers (
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
```

**Columns:**
- `uds_path`: Unix domain socket path for IPC
- `pid`: OS process ID
- `memory_headroom_pct`: Available memory percentage
- `k_current`: Current number of adapters loaded
- `adapters_loaded_json`: JSON array of loaded adapter IDs

**Indexes:**
- `idx_workers_tenant` on `tenant_id`
- `idx_workers_node` on `node_id`
- `idx_workers_status` on `status`

**Example Query:**
```sql
-- Find workers with low memory headroom
SELECT w.id, n.hostname, w.memory_headroom_pct
FROM workers w
JOIN nodes n ON w.node_id = n.id
WHERE w.status = 'serving' AND w.memory_headroom_pct < 10.0;
```

---

### `plans`

**Purpose:** Compiled execution plans with kernel hashes.

**Schema:**
```sql
CREATE TABLE plans (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    plan_id_b3 TEXT UNIQUE NOT NULL,
    manifest_hash_b3 TEXT NOT NULL REFERENCES manifests(hash_b3),
    kernel_hashes_json TEXT NOT NULL,
    layout_hash_b3 TEXT NOT NULL,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Columns:**
- `plan_id_b3`: BLAKE3 hash of plan content
- `manifest_hash_b3`: Reference to source manifest
- `kernel_hashes_json`: JSON array of Metal kernel hashes
- `layout_hash_b3`: Memory layout hash

---

### `cp_pointers`

**Purpose:** Active plan pointers (e.g., "production", "staging").

**Schema:**
```sql
CREATE TABLE cp_pointers (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    plan_id TEXT NOT NULL REFERENCES plans(id),
    active INTEGER NOT NULL DEFAULT 1,
    promoted_by TEXT REFERENCES users(id),
    promoted_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tenant_id, name)
);
```

**Example Query:**
```sql
-- Get current production plan for tenant
SELECT p.*, cp.promoted_at, u.email as promoted_by_email
FROM cp_pointers cp
JOIN plans p ON cp.plan_id = p.id
LEFT JOIN users u ON cp.promoted_by = u.id
WHERE cp.tenant_id = 'tenant-123'
  AND cp.name = 'production'
  AND cp.active = 1;
```

---

## Security and Compliance Tables

### `process_access_controls`

**Purpose:** Resource-level access control with role-based permissions.

**Schema:**
```sql
CREATE TABLE process_access_controls (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    resource_type TEXT NOT NULL CHECK(resource_type IN ('worker','configuration','template','deployment','log','metric')),
    resource_id TEXT NOT NULL,
    user_id TEXT REFERENCES users(id),
    role TEXT REFERENCES users(role),
    permission TEXT NOT NULL CHECK(permission IN ('read','write','execute','admin','none')),
    granted_by TEXT REFERENCES users(id),
    granted_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    conditions_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Indexes:**
- `idx_access_controls_tenant_id` on `tenant_id`
- `idx_access_controls_resource` on `(resource_type, resource_id)`
- `idx_access_controls_user_id` on `user_id`

**Example Query:**
```sql
-- Check user permissions for a resource
SELECT permission FROM process_access_controls
WHERE resource_type = 'worker'
  AND resource_id = 'worker-123'
  AND user_id = 'user-456'
  AND is_active = 1
  AND (expires_at IS NULL OR datetime(expires_at) > datetime('now'));
```

---

### `process_security_audit_logs`

**Purpose:** Security event audit trail.

**Schema:**
```sql
CREATE TABLE process_security_audit_logs (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    worker_id TEXT REFERENCES workers(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL CHECK(event_type IN ('access_granted','access_denied','policy_violation','configuration_change','authentication','authorization','data_access','network_access')),
    event_category TEXT NOT NULL CHECK(event_category IN ('security','compliance','access','data','network','system')),
    severity TEXT NOT NULL CHECK(severity IN ('info','warning','error','critical')),
    event_description TEXT NOT NULL,
    user_id TEXT REFERENCES users(id),
    source_ip TEXT,
    user_agent TEXT,
    request_id TEXT,
    event_data_json TEXT,
    policy_id TEXT REFERENCES process_security_policies(id),
    compliance_standard_id TEXT REFERENCES process_compliance_standards(id),
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Indexes:**
- `idx_security_audit_tenant_id` on `(tenant_id, timestamp DESC)`
- `idx_security_audit_severity` on `severity`

**Example Query:**
```sql
-- Find recent critical security events
SELECT event_type, event_description, user_id, timestamp
FROM process_security_audit_logs
WHERE severity = 'critical'
  AND timestamp > datetime('now', '-24 hours')
ORDER BY timestamp DESC;
```

---

### `incidents`

**Purpose:** Security and policy violations.

**Schema:**
```sql
CREATE TABLE incidents (
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
```

**Indexes:**
- `idx_incidents_tenant` on `(tenant_id, created_at DESC)`
- `idx_incidents_resolved` on `resolved`

**Example Query:**
```sql
-- Unresolved critical incidents
SELECT id, kind, description, created_at
FROM incidents
WHERE severity = 'critical' AND resolved = 0
ORDER BY created_at DESC;
```

---

## Workspace and Collaboration Tables

### `workspaces`

**Purpose:** Resource containers for cross-tenant collaboration.

**Schema:**
```sql
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL,
    description TEXT,
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE RESTRICT
);
```

---

### `workspace_members`

**Purpose:** Tenant/user membership in workspaces.

**Schema:**
```sql
CREATE TABLE workspace_members (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    workspace_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    user_id TEXT,  -- NULL = entire tenant
    role TEXT NOT NULL CHECK(role IN ('owner', 'member', 'viewer')),
    permissions_json TEXT,
    added_by TEXT NOT NULL,
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE,
    UNIQUE(workspace_id, tenant_id, user_id)
);
```

---

### `activity_events`

**Purpose:** User actions and collaboration events for unified activity feed.

**Schema:**
```sql
CREATE TABLE activity_events (
    id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
    workspace_id TEXT,
    user_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    event_type TEXT NOT NULL CHECK(event_type IN (
        'adapter_created', 'adapter_updated', 'adapter_deleted',
        'adapter_shared', 'adapter_unshared',
        'resource_shared', 'resource_unshared',
        'message_sent', 'message_edited',
        'user_mentioned', 'user_joined_workspace', 'user_left_workspace',
        'workspace_created', 'workspace_updated',
        'member_added', 'member_removed', 'member_role_changed'
    )),
    target_type TEXT,
    target_id TEXT,
    metadata_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);
```

**Indexes:**
- `idx_activity_events_workspace` on `workspace_id`
- `idx_activity_events_type` on `event_type`
- `idx_activity_events_created_at` on `created_at DESC`

**Example Query:**
```sql
-- Recent activity in a workspace
SELECT
    event_type,
    target_type,
    target_id,
    u.display_name as user_name,
    created_at
FROM activity_events ae
JOIN users u ON ae.user_id = u.id
WHERE workspace_id = 'workspace-123'
ORDER BY created_at DESC
LIMIT 50;
```

---

## Migration History

### Complete Migration List (0001-0071)

| Migration | Description | Key Tables |
|-----------|-------------|------------|
| 0001_init.sql | Initial schema | users, tenants, nodes, models, adapters, plans, workers, telemetry_bundles, audits |
| 0002_patch_proposals.sql | Patch proposal system | patch_proposals |
| 0003_ephemeral_adapters.sql | Ephemeral adapter tier | (alters adapters) |
| 0004_signing_keys.sql | Cryptographic signing | signing_keys |
| 0005_code_intelligence.sql | Code analysis | code_intelligence_* |
| 0006_production_safety.sql | Production safeguards | (schema constraints) |
| 0007_adapter_provenance.sql | Adapter lineage tracking | adapter_provenance |
| 0008_enclave_audit.sql | Enclave audit logs | enclave_audit_logs |
| 0009_placeholder.sql | Reserved slot | - |
| 0010_placeholder.sql | Reserved slot | - |
| 0011_system_metrics.sql | System metrics storage | system_metrics |
| 0012_enhanced_adapter_schema.sql | Extended adapter metadata | (alters adapters) |
| 0013_git_repository_integration.sql | Git repository tracking | git_repositories, repository_* |
| 0014_contacts_and_streams.sql | Contact management | contacts, streams |
| 0015_git_sessions.sql | Git session tracking | git_sessions |
| 0016_replay_sessions.sql | Replay session support | replay_sessions |
| 0017_process_debugging.sql | Process debugging tools | process_debug_* |
| 0018_advanced_process_control.sql | Process control | process_control_* |
| 0019_real_time_process_insights.sql | Real-time monitoring | process_insights_* |
| 0020_process_configuration_management.sql | Config management | process_configs |
| 0021_process_security_compliance.sql | Security & compliance | process_security_policies, process_access_controls |
| 0022_process_automation_orchestration.sql | Automation workflows | process_automation_* |
| 0023_process_analytics_reporting.sql | Analytics & reporting | process_analytics_* |
| 0024_process_integration_apis.sql | API integrations | process_api_* |
| 0025_advanced_process_monitoring.sql | Advanced monitoring | process_monitoring_* |
| 0026_evidence_indices.sql | Evidence indexing | (indices) |
| 0027_fix_process_monitoring_schema.sql | Schema fixes | (fixes) |
| 0028_base_model_status.sql | Base model status | base_model_status |
| 0029_pgvector_rag.sql | Vector embeddings (PostgreSQL) | embeddings |
| 0030_cab_promotion_workflow.sql | CAB promotion workflow | cab_* |
| 0031_adapter_load_state.sql | Adapter loading state | (alters adapters) |
| 0032_tick_ledger.sql | Tick ledger for determinism | tick_ledger_entries, tick_ledger_consistency_reports |
| 0033_cab_lineage.sql | CAB lineage tracking | cab_lineage |
| 0034_policy_quarantine.sql | Policy quarantine | policy_quarantine |
| 0035_tick_ledger_federation.sql | Federated tick ledger | (alters tick_ledger) |
| 0036_code_intelligence_extensions.sql | Extended code analysis | (alters code_intelligence) |
| 0037_policy_hashes.sql | Policy hash tracking | policy_hashes |
| 0038_federation.sql | Federation infrastructure | federation_peers, federation_output_hashes |
| 0039_federation_bundle_signatures.sql | Bundle signature quorum | federation_bundle_quorum |
| 0040_align_production_schema.sql | Production alignment | (schema updates) |
| 0041_training_datasets.sql | Training dataset management | training_datasets, dataset_files, dataset_statistics |
| 0042_base_model_ui_support.sql | UI support for base models | (alters base_model_status) |
| 0043_patch_system.sql | Patch application system | patch_applications, patch_signatures |
| 0044_add_adapter_ttl.sql | Adapter TTL support | (alters adapters) |
| 0045_aos_adapters.sql | AOS adapter format | (alters adapters) |
| 0046_policy_evidence.sql | Policy evidence tracking | policy_evidence |
| 0047_domain_adapters.sql | Domain-specific adapters | domain_adapters, domain_adapter_executions, domain_adapter_tests |
| 0048_workspaces_and_messaging.sql | Workspaces & collaboration | workspaces, workspace_members, messages, notifications, activity_events |
| 0049_tutorial_statuses.sql | Tutorial progress tracking | tutorial_statuses |
| 0050_training_jobs_extensions.sql | Extended training job metadata | (alters training jobs) |
| 0051_model_operations_audit.sql | Model operations audit | model_operations_audit |
| 0052_progress_events.sql | Progress event tracking | progress_events |
| 0053_add_model_metadata.sql | Extended model metadata | (alters models) |
| 0054_add_git_repository_last_scan.sql | Git scan timestamps | (alters git_repositories) |
| 0055_add_model_loading_fields.sql | Model loading state | (alters models) |
| 0056_extend_security_audit_logs.sql | Extended audit logs | (alters process_security_audit_logs) |
| 0057_fix_domain_adapters_sqlite_compatibility.sql | SQLite compatibility fixes | (fixes domain_adapters) |
| 0058_cleanup_unused_tables.sql | Remove unused tables | (drops tables) |
| 0059_remove_unused_adapter_columns.sql | Remove unused columns | (alters adapters) |
| 0060_create_pinned_adapters_table.sql | Pinned adapters table | pinned_adapters |
| 0061-0065 | (Reserved/merged into later migrations) | - |
| 0066_tenant_snapshots.sql | Tenant state snapshots | tenant_snapshots |
| 0067_index_hashes.sql | Index hash tracking | index_hashes |
| 0068_legacy_index_migration.sql | Index migration cleanup | (alters indices) |
| 0069_plugin_tenant_enables.sql | Plugin enablement per tenant | plugin_tenant_enables |
| 0070_routing_decisions.sql | Routing decision tracking | routing_decisions |
| 0071_lifecycle_version_history.sql | Lifecycle & version history | adapter_version_history, stack_version_history |

### Migration Consolidation Notes

Several migration numbers (0061-0065) were consolidated into migration 0064 and later migrations as documented in `docs/PRD-05_MIGRATION_CONSOLIDATION.md`.

---

## Database Backend Support

### SQLite Configuration

**Connection String:**
```
sqlite://adapteros.db?mode=rwc
```

**Features:**
- WAL mode for concurrent reads
- Foreign key enforcement enabled
- `datetime('now')` for timestamps
- `randomblob(16)` for UUIDs
- JSON support via `json_extract()` functions

**Triggers:**
SQLite uses triggers for:
- Auto-updating `updated_at` timestamps
- Validation constraints (e.g., stack naming rules)
- Cascade operations

---

### PostgreSQL Configuration

**Connection String:**
```
postgresql://user:pass@localhost:5432/adapteros
```

**Features:**
- Native UUID generation with `gen_random_uuid()`
- `NOW()` for timestamps
- JSONB column type for efficient JSON operations
- `TIMESTAMP WITH TIME ZONE` for timestamps
- Advanced indexing (GIN, GiST for JSONB)
- pgvector extension for embeddings (migration 0029)

**Compatibility Notes:**
- Migration files use conditional SQL for backend-specific syntax
- Some features (like pgvector) are PostgreSQL-only
- The `DatabaseBackend` trait abstracts differences

---

## Common Queries

### Adapter Lifecycle Queries

```sql
-- Count adapters by lifecycle state
SELECT lifecycle_state, COUNT(*) as count
FROM adapters
GROUP BY lifecycle_state;

-- Find adapters ready for retirement (deprecated >90 days)
SELECT name, lifecycle_state, updated_at
FROM adapters
WHERE lifecycle_state = 'deprecated'
  AND datetime(updated_at) < datetime('now', '-90 days');

-- Adapter version timeline
SELECT
    a.name,
    avh.version,
    avh.lifecycle_state,
    avh.created_at
FROM adapter_version_history avh
JOIN adapters a ON avh.adapter_id = a.adapter_id
WHERE a.name = 'my-adapter'
ORDER BY avh.created_at DESC;
```

### Stack Management Queries

```sql
-- Get stack with all adapter details
SELECT
    s.name as stack_name,
    s.workflow_type,
    json_extract(value, '$') as adapter_id,
    a.name as adapter_name,
    a.lifecycle_state
FROM adapter_stacks s,
     json_each(s.adapter_ids_json) as value
LEFT JOIN adapters a ON json_extract(value, '$') = a.adapter_id
WHERE s.id = ?;

-- Find stacks using deprecated adapters
SELECT DISTINCT s.name
FROM adapter_stacks s,
     json_each(s.adapter_ids_json) as value
JOIN adapters a ON json_extract(value, '$') = a.adapter_id
WHERE a.lifecycle_state = 'deprecated';
```

### Performance and Resource Queries

```sql
-- Worker resource utilization
SELECT
    n.hostname,
    COUNT(w.id) as worker_count,
    AVG(w.memory_headroom_pct) as avg_headroom
FROM nodes n
LEFT JOIN workers w ON n.id = w.node_id AND w.status = 'serving'
GROUP BY n.id, n.hostname;

-- Router overhead analysis
SELECT
    DATE(timestamp) as date,
    AVG(overhead_pct) as avg_overhead,
    MAX(overhead_pct) as max_overhead,
    COUNT(*) as decision_count
FROM routing_decisions
WHERE timestamp > datetime('now', '-7 days')
GROUP BY DATE(timestamp)
ORDER BY date DESC;
```

### Audit and Compliance Queries

```sql
-- Recent security events by severity
SELECT
    severity,
    event_type,
    COUNT(*) as event_count
FROM process_security_audit_logs
WHERE timestamp > datetime('now', '-24 hours')
GROUP BY severity, event_type
ORDER BY severity DESC, event_count DESC;

-- Compliance assessment summary
SELECT
    cs.standard_name,
    COUNT(ca.id) as assessment_count,
    AVG(ca.compliance_score) as avg_score
FROM process_compliance_standards cs
LEFT JOIN process_compliance_assessments ca ON cs.id = ca.standard_id
GROUP BY cs.id, cs.standard_name;
```

### Federation and Determinism Queries

```sql
-- Check cross-host determinism
SELECT
    session_id,
    input_hash,
    COUNT(DISTINCT output_hash) as unique_outputs,
    COUNT(DISTINCT host_id) as host_count
FROM federation_output_hashes
GROUP BY session_id, input_hash
HAVING COUNT(DISTINCT output_hash) > 1;

-- Tick ledger consistency
SELECT
    tenant_id,
    COUNT(*) as inconsistency_count,
    SUM(divergence_count) as total_divergences
FROM tick_ledger_consistency_reports
WHERE consistent = 0
GROUP BY tenant_id;
```

---

## Troubleshooting

### Common Issues

#### 1. Migration Failures

**Symptom:** Migration fails with foreign key constraint error.

**Cause:** SQLite foreign key enforcement not enabled.

**Solution:**
```sql
PRAGMA foreign_keys = ON;
```

**Prevention:** The database initialization code should automatically enable this.

---

#### 2. Slow Queries on Large Tables

**Symptom:** Queries on `routing_decisions` or `telemetry_bundles` are slow.

**Cause:** Missing or unused indexes.

**Diagnosis:**
```sql
-- Check if indexes are being used (SQLite)
EXPLAIN QUERY PLAN
SELECT * FROM routing_decisions
WHERE tenant_id = 'tenant-123'
ORDER BY timestamp DESC;
```

**Solution:** Ensure indexes exist and are used:
```sql
-- Verify index exists
SELECT name FROM sqlite_master
WHERE type = 'index'
  AND tbl_name = 'routing_decisions';

-- Add missing index if needed
CREATE INDEX IF NOT EXISTS idx_routing_decisions_tenant_timestamp
    ON routing_decisions(tenant_id, timestamp DESC);
```

---

#### 3. JSON Query Performance

**Symptom:** Queries using `json_extract()` are slow.

**Cause:** JSON columns are not indexed efficiently.

**Solution (SQLite):**
- Use expression indexes on frequently queried JSON paths
- Consider denormalizing hot paths into separate columns

**Solution (PostgreSQL):**
- Use JSONB instead of JSON
- Create GIN indexes on JSONB columns:
```sql
CREATE INDEX idx_adapter_metadata_gin ON adapters USING GIN (metadata_json);
```

---

#### 4. Database Lock Errors

**Symptom:** `database is locked` errors in SQLite.

**Cause:** Long-running transactions blocking writes.

**Solution:**
- Enable WAL mode: `PRAGMA journal_mode=WAL;`
- Reduce transaction duration
- Use connection pooling with appropriate timeout settings

---

#### 5. Orphaned Records

**Symptom:** Adapters reference non-existent stacks or models.

**Cause:** Foreign key constraints not enforced or cascade delete not configured.

**Diagnosis:**
```sql
-- Find orphaned adapters (no matching tenant)
SELECT a.id, a.name, a.tenant_id
FROM adapters a
LEFT JOIN tenants t ON a.tenant_id = t.id
WHERE t.id IS NULL;

-- Find orphaned routing decisions (no matching stack)
SELECT rd.id, rd.stack_id
FROM routing_decisions rd
LEFT JOIN adapter_stacks s ON rd.stack_id = s.id
WHERE rd.stack_id IS NOT NULL AND s.id IS NULL;
```

**Solution:**
```sql
-- Clean up orphaned records
DELETE FROM adapters WHERE tenant_id NOT IN (SELECT id FROM tenants);

-- Ensure foreign keys are enforced (SQLite)
PRAGMA foreign_keys = ON;
```

---

#### 6. Version History Bloat

**Symptom:** `adapter_version_history` table grows very large.

**Cause:** Frequent lifecycle transitions creating many history records.

**Solution:**
```sql
-- Archive old history (>1 year)
CREATE TABLE adapter_version_history_archive AS
SELECT * FROM adapter_version_history
WHERE created_at < datetime('now', '-1 year');

DELETE FROM adapter_version_history
WHERE created_at < datetime('now', '-1 year');

-- Vacuum to reclaim space (SQLite)
VACUUM;
```

---

#### 7. Stack Naming Validation Errors

**Symptom:** `INSERT` fails with "Invalid stack name format".

**Cause:** Stack name doesn't match required pattern `stack.{namespace}[.{identifier}]`.

**Valid Examples:**
- `stack.my-project`
- `stack.my-project.v2`
- `stack.code-assistant`

**Invalid Examples:**
- `my-stack` (missing `stack.` prefix)
- `stack.` (empty namespace)
- `stack.my project` (spaces not allowed)
- `stack.my---project` (consecutive hyphens)

**Solution:** Ensure stack names follow the pattern and avoid reserved names.

---

#### 8. Index Hash Mismatches

**Symptom:** `index_hashes` table shows outdated hashes after schema changes.

**Cause:** Index hashes not updated after migrations.

**Solution:**
```sql
-- Regenerate index hashes for a tenant
UPDATE index_hashes
SET hash = '<new-hash>', updated_at = datetime('now')
WHERE tenant_id = 'tenant-123' AND index_type = 'adapters';
```

---

### Performance Tuning

#### SQLite Optimizations

```sql
-- Enable WAL mode for better concurrency
PRAGMA journal_mode = WAL;

-- Increase cache size (default is often too small)
PRAGMA cache_size = -64000;  -- 64MB

-- Synchronous mode for better performance (less crash-safe)
PRAGMA synchronous = NORMAL;

-- Memory-mapped I/O for large databases
PRAGMA mmap_size = 268435456;  -- 256MB

-- Analyze tables for better query planning
ANALYZE;
```

#### PostgreSQL Optimizations

```sql
-- Update table statistics
ANALYZE routing_decisions;

-- Reindex if queries are slow despite indexes
REINDEX TABLE routing_decisions;

-- Check for bloat in frequently updated tables
SELECT
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;
```

---

### Maintenance Tasks

#### Daily
- Check worker heartbeats and mark stale workers as offline
- Clean up expired pinned adapters
- Rotate telemetry bundles

#### Weekly
- Vacuum database (SQLite) or analyze tables (PostgreSQL)
- Review audit logs for security incidents
- Check tick ledger consistency across federation peers

#### Monthly
- Archive old version history (>90 days)
- Review and clean up orphaned records
- Update table statistics for query optimization

---

### Backup and Recovery

#### SQLite Backup

```bash
# Online backup (while database is running)
sqlite3 adapteros.db ".backup adapteros-backup.db"

# Copy WAL files too
cp adapteros.db-wal adapteros-backup.db-wal
cp adapteros.db-shm adapteros-backup.db-shm
```

#### PostgreSQL Backup

```bash
# Dump entire database
pg_dump adapteros > adapteros-backup.sql

# Dump schema only
pg_dump --schema-only adapteros > schema-backup.sql

# Dump data only
pg_dump --data-only adapteros > data-backup.sql
```

#### Recovery Testing

Regularly test recovery procedures:

1. Restore backup to test environment
2. Run migrations to ensure schema is current
3. Verify data integrity with checksums
4. Test application connectivity

---

## Additional Resources

- **Migration Documentation:** `/docs/PRD-05_MIGRATION_CONSOLIDATION.md`
- **Database Abstraction Code:** `/crates/adapteros-db/src/traits.rs`
- **SQLite Backend:** `/crates/adapteros-db/src/sqlite_backend.rs`
- **PostgreSQL Backend:** `/crates/adapteros-db/src/postgres_backend.rs`
- **Schema Migrations:** `/crates/adapteros-db/migrations/`

---

## See Also

- [CLAUDE.md](../CLAUDE.md) - Developer quick reference (source of truth)
- [DATABASE_REFERENCE.md](DATABASE_REFERENCE.md) - Core tables quick reference with simplified ERD
- [database-schema/README.md](database-schema/README.md) - Extended schema documentation
- [LIFECYCLE.md](LIFECYCLE.md) - Adapter lifecycle states (Unloaded, Cold, Warm, Hot, Resident)
- [PINNING_TTL.md](PINNING_TTL.md) - Pinning and TTL enforcement details
- [TRAINING_PIPELINE.md](TRAINING_PIPELINE.md) - Training dataset and job workflows
- [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) - Full architecture overview

---

**Last Updated:** 2025-11-19
**Schema Version:** 0071 (71 migrations applied)
**Maintained by:** AdapterOS Engineering Team
