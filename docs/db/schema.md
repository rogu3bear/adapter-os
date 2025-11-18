# AdapterOS Canonical Database Schema

**Version:** 0070 (2025-11-18)
**Migration Chain:** 0001-0070 (70 migrations)
**Database:** SQLite with WAL mode
**Author:** JKCA

---

## Overview

This document defines the canonical database schema for AdapterOS after all migrations (0001-0070) are applied. It serves as the single source of truth for schema validation and migration consistency checks.

**Critical Tables:**
- `adapters` - Adapter metadata and lifecycle state
- `adapter_stacks` - Reusable adapter combinations
- `tenants` - Tenant isolation and resource limits
- `routing_decisions` - Router decision telemetry (PRD-04)
- `pinned_adapters` - Deletion protection for critical adapters
- `training_datasets`, `training_jobs` - Training pipeline
- `tick_ledger` - Deterministic execution log
- `audit_logs` - RBAC audit trail (PRD-06)

---

## Core Tables

### adapters

Primary adapter metadata table tracking lifecycle state, memory usage, and activation metrics.

```sql
CREATE TABLE adapters (
    -- Identity
    adapter_id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    adapter_name TEXT,  -- Semantic name (tenant/domain/purpose/revision)
    tenant_namespace TEXT,
    domain TEXT,
    purpose TEXT,
    revision TEXT,

    -- Content Addressing
    hash TEXT NOT NULL,
    aos_file_path TEXT,
    aos_file_hash TEXT,
    adapter_path TEXT,

    -- Configuration
    rank INTEGER NOT NULL,
    alpha REAL,
    backend TEXT,  -- 'secure_enclave' | 'file'
    quantization TEXT,  -- 'q15' | 'q8' | etc.

    -- Lifecycle State (runtime loading)
    load_state TEXT NOT NULL DEFAULT 'unloaded',  -- unloaded|cold|warm|hot|resident
    current_state TEXT DEFAULT 'unloaded',
    last_loaded_at TEXT,
    last_error TEXT,

    -- Lifecycle State (metadata version)
    lifecycle_state TEXT NOT NULL DEFAULT 'active',  -- draft|active|deprecated|retired
    version TEXT NOT NULL DEFAULT '1.0.0',

    -- Memory & Performance
    memory_mb INTEGER,
    max_memory_mb INTEGER,
    activation_pct REAL DEFAULT 0.0,

    -- Access Control
    acl TEXT,  -- Comma-separated tenant IDs

    -- Lineage & Provenance
    parent_id TEXT,
    fork_type TEXT,  -- 'specialization' | 'experiment' | etc.
    fork_reason TEXT,

    -- TTL & Expiration
    expires_at TEXT,

    -- Heartbeat (Phase 2 stability)
    last_heartbeat INTEGER,

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (parent_id) REFERENCES adapters(adapter_id) ON DELETE SET NULL
);

-- Indexes
CREATE INDEX idx_adapters_hash ON adapters(hash);
CREATE INDEX idx_adapters_name ON adapters(name);
CREATE INDEX idx_adapters_tier ON adapters(load_state);
CREATE INDEX idx_adapters_activation ON adapters(activation_pct);
CREATE INDEX idx_adapters_adapter_name ON adapters(adapter_name);
CREATE INDEX idx_adapters_lifecycle_state ON adapters(lifecycle_state);
CREATE INDEX idx_adapters_version ON adapters(version);
CREATE INDEX idx_adapters_heartbeat ON adapters(last_heartbeat) WHERE last_heartbeat IS NOT NULL;

-- Views
CREATE VIEW stale_adapters AS
SELECT * FROM adapters
WHERE last_heartbeat IS NOT NULL
  AND last_heartbeat < unixepoch('now') - 300;  -- 5-minute threshold
```

### adapter_stacks

Reusable adapter combinations with workflow execution strategies.

```sql
CREATE TABLE adapter_stacks (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    tenant_id TEXT NOT NULL DEFAULT 'default',

    -- Configuration
    adapter_ids_json TEXT NOT NULL,  -- JSON array of adapter IDs
    workflow_type TEXT NOT NULL,  -- 'sequential' | 'parallel' | 'upstream_downstream'

    -- Lifecycle State
    lifecycle_state TEXT NOT NULL DEFAULT 'active',  -- draft|active|deprecated|retired
    version TEXT NOT NULL DEFAULT '1.0.0',

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

-- Indexes
CREATE INDEX idx_adapter_stacks_tenant ON adapter_stacks(tenant_id);
CREATE INDEX idx_adapter_stacks_lifecycle_state ON adapter_stacks(lifecycle_state);
CREATE INDEX idx_adapter_stacks_version ON adapter_stacks(version);

-- Triggers
CREATE TRIGGER IF NOT EXISTS validate_stack_lifecycle_state
BEFORE INSERT ON adapter_stacks
FOR EACH ROW
BEGIN
    SELECT CASE
        WHEN NEW.lifecycle_state NOT IN ('draft', 'active', 'deprecated', 'retired')
        THEN RAISE(ABORT, 'Invalid lifecycle_state: must be draft, active, deprecated, or retired')
    END;
END;
```

### tenants

Tenant isolation with UID/GID mappings and isolation metadata.

```sql
CREATE TABLE tenants (
    id TEXT PRIMARY KEY NOT NULL,
    uid INTEGER NOT NULL,
    gid INTEGER NOT NULL,
    isolation_metadata TEXT,  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX idx_tenants_uid ON tenants(uid);
```

### routing_decisions

Router decision telemetry with timing metrics and candidate sets (PRD-04).

```sql
CREATE TABLE routing_decisions (
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
    k_value INTEGER,  -- Number of adapters selected

    -- Candidate Adapters (JSON array of {adapter_idx, raw_score, gate_q15})
    candidate_adapters TEXT NOT NULL,  -- JSON array of RouterCandidate objects

    -- Selected Adapter Names (for easy filtering)
    selected_adapter_ids TEXT,  -- Comma-separated list

    -- Timing Metrics
    router_latency_us INTEGER,  -- Router execution time in microseconds
    total_inference_latency_us INTEGER,  -- Total inference time
    overhead_pct REAL,  -- Router overhead as percentage

    -- Metadata
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (stack_id) REFERENCES adapter_stacks(id) ON DELETE SET NULL
);

-- Indexes
CREATE INDEX idx_routing_decisions_tenant_timestamp
    ON routing_decisions(tenant_id, timestamp DESC);
CREATE INDEX idx_routing_decisions_stack_id
    ON routing_decisions(stack_id) WHERE stack_id IS NOT NULL;
CREATE INDEX idx_routing_decisions_request_id
    ON routing_decisions(request_id) WHERE request_id IS NOT NULL;
CREATE INDEX idx_routing_decisions_timestamp
    ON routing_decisions(timestamp DESC);

-- Views
CREATE VIEW routing_decisions_enriched AS
SELECT
    rd.*,
    s.name AS stack_name,
    s.workflow_type,
    COUNT(DISTINCT json_extract(value, '$.adapter_idx')) AS num_candidates
FROM routing_decisions rd
LEFT JOIN adapter_stacks s ON rd.stack_id = s.id,
     json_each(rd.candidate_adapters) AS candidate
GROUP BY rd.id;

CREATE VIEW routing_decisions_high_overhead AS
SELECT * FROM routing_decisions
WHERE overhead_pct > 8.0
ORDER BY timestamp DESC;

CREATE VIEW routing_decisions_low_entropy AS
SELECT * FROM routing_decisions
WHERE entropy < 0.5
ORDER BY timestamp DESC;
```

### pinned_adapters

Deletion protection for critical adapters with TTL support.

```sql
CREATE TABLE pinned_adapters (
    tenant_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    pinned_at TEXT NOT NULL DEFAULT (datetime('now')),
    pinned_until TEXT,  -- NULL = permanent pin
    reason TEXT,
    pinned_by TEXT,
    PRIMARY KEY (tenant_id, adapter_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (adapter_id) REFERENCES adapters(adapter_id) ON DELETE CASCADE
);

CREATE INDEX idx_pinned_adapters_expiry ON pinned_adapters(pinned_until)
WHERE pinned_until IS NOT NULL;

-- View respects TTL automatically
CREATE VIEW active_pinned_adapters AS
SELECT * FROM pinned_adapters
WHERE pinned_until IS NULL OR pinned_until > datetime('now');
```

### training_datasets

Training dataset metadata with content-addressable file storage.

```sql
CREATE TABLE training_datasets (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    hash_b3 TEXT NOT NULL,
    validation_status TEXT NOT NULL DEFAULT 'pending',  -- pending|valid|invalid
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE dataset_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    dataset_id TEXT NOT NULL,
    path TEXT NOT NULL,
    size INTEGER NOT NULL,
    hash TEXT NOT NULL,
    ingestion_metadata TEXT,  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (dataset_id) REFERENCES training_datasets(id) ON DELETE CASCADE
);

CREATE TABLE dataset_statistics (
    dataset_id TEXT PRIMARY KEY NOT NULL,
    num_examples INTEGER NOT NULL,
    total_tokens INTEGER NOT NULL,
    distributions TEXT,  -- JSON
    computed_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (dataset_id) REFERENCES training_datasets(id) ON DELETE CASCADE
);
```

### training_jobs

Training job tracking with progress monitoring.

```sql
CREATE TABLE training_jobs (
    id TEXT PRIMARY KEY NOT NULL,
    dataset_id TEXT NOT NULL,
    adapter_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending|running|completed|failed|cancelled
    progress_pct REAL DEFAULT 0.0,
    loss REAL,
    tokens_per_sec REAL,
    last_heartbeat INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (dataset_id) REFERENCES training_datasets(id) ON DELETE CASCADE
);

CREATE INDEX idx_training_jobs_status ON training_jobs(status);
CREATE INDEX idx_training_jobs_heartbeat ON training_jobs(last_heartbeat)
WHERE last_heartbeat IS NOT NULL;

CREATE VIEW stale_training_jobs AS
SELECT * FROM training_jobs
WHERE last_heartbeat IS NOT NULL
  AND last_heartbeat < unixepoch('now') - 300;
```

### tick_ledger

Deterministic execution log with Merkle chain integrity.

```sql
CREATE TABLE tick_ledger (
    tick INTEGER PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    event_hash TEXT NOT NULL,
    prev_hash TEXT NOT NULL,
    cumulative_hash TEXT NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_tick_ledger_task ON tick_ledger(task_id);
CREATE INDEX idx_tick_ledger_timestamp ON tick_ledger(timestamp DESC);
```

### audit_logs

RBAC audit trail for security compliance (PRD-06).

```sql
CREATE TABLE audit_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    resource_id TEXT,
    status TEXT NOT NULL,  -- 'success' | 'failure'
    details TEXT,  -- JSON
    ip_address TEXT,
    user_agent TEXT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_audit_logs_user ON audit_logs(user_id);
CREATE INDEX idx_audit_logs_action ON audit_logs(action);
CREATE INDEX idx_audit_logs_timestamp ON audit_logs(timestamp DESC);
CREATE INDEX idx_audit_logs_status ON audit_logs(status);
```

---

## Plugin & Configuration Tables

### plugin_tenant_enables

Per-tenant plugin enablement.

```sql
CREATE TABLE plugin_tenant_enables (
    tenant_id TEXT NOT NULL,
    plugin_name TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    updated_at DATETIME NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (tenant_id, plugin_name),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_plugin_tenant_enables_plugin ON plugin_tenant_enables(plugin_name);
```

### dashboard_configs

Per-user dashboard widget customization.

```sql
CREATE TABLE dashboard_configs (
    user_id TEXT PRIMARY KEY NOT NULL,
    config_json TEXT NOT NULL,  -- JSON widget layout
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

---

## Migration Metadata

### _sqlx_migrations

SQLx migration tracking (auto-managed).

```sql
CREATE TABLE _sqlx_migrations (
    version BIGINT PRIMARY KEY NOT NULL,
    description TEXT NOT NULL,
    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    success BOOLEAN NOT NULL,
    checksum BLOB NOT NULL,
    execution_time BIGINT NOT NULL
);
```

---

## Schema Version Enforcement

**Expected Version:** 70 (0070_routing_decisions.sql)

**Startup Check (in `Db::verify_migration_version`):**
1. Query `_sqlx_migrations` for latest version
2. Compare against expected version (70)
3. If mismatch: **FAIL FAST** with clear error message
4. Server refuses to start if schema is outdated

**Reset Command:**
```bash
aosctl db reset  # Development only - recreates database
```

---

## Consistency Requirements

Per PRD-05, the following must be true after all migrations:

1. ✅ All 70 migrations applied sequentially (0001-0070)
2. ✅ No duplicate migration numbers
3. ✅ All critical tables exist: `adapters`, `adapter_stacks`, `routing_decisions`, `pinned_adapters`
4. ✅ Foreign key constraints validated
5. ✅ Indexes created for performance-critical queries
6. ✅ Views created for common aggregations
7. ✅ Triggers enforce data integrity (e.g., lifecycle_state validation)

**Validation Command:**
```bash
cargo test -p adapteros-db schema_consistency_tests
```

---

## References

- Migration files: `/migrations/0001_init.sql` through `/migrations/0070_routing_decisions.sql`
- Migration signatures: `/migrations/signatures.json`
- Database module: `/crates/adapteros-db/src/lib.rs`
- Schema tests: `/crates/adapteros-db/tests/schema_consistency_tests.rs`

---

**Last Updated:** 2025-11-18
**Maintained by:** James KC Auchterlonie (JKCA)
