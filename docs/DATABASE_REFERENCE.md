# Database Schema Reference

**Purpose:** Comprehensive database schema documentation for AdapterOS

**Last Updated:** 2025-01-19

---

## Core Tables

### adapters

Adapter metadata and lifecycle state

| Column | Type | Description |
|--------|------|-------------|
| `adapter_id` | TEXT PRIMARY KEY | Unique adapter identifier |
| `adapter_name` | TEXT | Semantic name (tenant/domain/purpose/revision) |
| `hash` | TEXT NOT NULL | BLAKE3 content hash |
| `tier` | TEXT | Memory tier (tier_1, tier_2, tier_3) |
| `rank` | INTEGER | LoRA rank |
| `acl` | TEXT | JSON array of allowed tenant IDs |
| `activation_pct` | REAL | Router selection percentage |
| `current_state` | TEXT | Lifecycle state (unloaded, cold, warm, hot, resident) |
| `memory_usage_mb` | INTEGER | Current VRAM footprint |
| `max_memory_mb` | INTEGER | Peak VRAM usage |
| `expires_at` | TEXT | TTL timestamp (SQLite datetime format) |
| `last_heartbeat` | INTEGER | Unix timestamp of last heartbeat |
| `created_at` | TEXT | Creation timestamp |
| `updated_at` | TEXT | Last update timestamp |

**Indexes:**
- `idx_adapters_name` on `adapter_name`
- `idx_adapters_state` on `current_state`
- `idx_adapters_expires_at` on `expires_at` (WHERE `expires_at IS NOT NULL`)
- `idx_adapters_heartbeat` on `last_heartbeat` (WHERE `last_heartbeat IS NOT NULL`)

### tenants

Tenant isolation and resource limits

| Column | Type | Description |
|--------|------|-------------|
| `tenant_id` | TEXT PRIMARY KEY | Unique tenant identifier |
| `uid` | INTEGER | Unix user ID for isolation |
| `gid` | INTEGER | Unix group ID for isolation |
| `isolation_metadata` | TEXT | JSON metadata for isolation policies |
| `resource_limits` | TEXT | JSON resource quotas |
| `created_at` | TEXT | Creation timestamp |

### adapter_stacks

Reusable adapter combinations

| Column | Type | Description |
|--------|------|-------------|
| `id` | INTEGER PRIMARY KEY | Auto-incrementing ID |
| `name` | TEXT UNIQUE | Stack name |
| `adapter_ids_json` | TEXT | JSON array of adapter IDs |
| `workflow_type` | TEXT | Sequential, Parallel, UpstreamDownstream |
| `tenant_id` | TEXT | Owning tenant |
| `created_at` | TEXT | Creation timestamp |

### training_datasets

Dataset metadata

| Column | Type | Description |
|--------|------|-------------|
| `id` | INTEGER PRIMARY KEY | Auto-incrementing ID |
| `dataset_id` | TEXT UNIQUE | Dataset identifier |
| `hash_b3` | TEXT | BLAKE3 content hash |
| `validation_status` | TEXT | valid, invalid, pending |
| `num_examples` | INTEGER | Total examples count |
| `total_tokens` | INTEGER | Total token count |
| `created_at` | TEXT | Creation timestamp |

### training_jobs

Job tracking and progress

| Column | Type | Description |
|--------|------|-------------|
| `id` | INTEGER PRIMARY KEY | Auto-incrementing ID |
| `job_id` | TEXT UNIQUE | Job identifier |
| `dataset_id` | INTEGER | Foreign key to `training_datasets` |
| `status` | TEXT | pending, running, completed, failed, cancelled |
| `progress_pct` | REAL | Completion percentage (0-100) |
| `loss` | REAL | Current training loss |
| `tokens_per_sec` | REAL | Throughput metric |
| `last_heartbeat` | INTEGER | Unix timestamp of last update |
| `created_at` | TEXT | Creation timestamp |
| `updated_at` | TEXT | Last update timestamp |

### pinned_adapters

Adapter pinning enforcement

| Column | Type | Description |
|--------|------|-------------|
| `tenant_id` | TEXT | Tenant identifier |
| `adapter_id` | TEXT | Adapter identifier |
| `pinned_at` | TEXT | Pin timestamp |
| `pinned_until` | TEXT | Expiration timestamp (NULL = permanent) |
| `reason` | TEXT | Pin justification |
| `pinned_by` | TEXT | User/system who pinned |

**Primary Key:** `(tenant_id, adapter_id)`

**View:** `active_pinned_adapters`
```sql
CREATE VIEW active_pinned_adapters AS
SELECT * FROM pinned_adapters
WHERE pinned_until IS NULL OR pinned_until > datetime('now');
```

### audit_logs

Immutable audit trail

| Column | Type | Description |
|--------|------|-------------|
| `id` | INTEGER PRIMARY KEY | Auto-incrementing ID |
| `user_id` | TEXT | User identifier |
| `action` | TEXT | Action performed (e.g., adapter.register) |
| `resource` | TEXT | Resource type (e.g., adapter) |
| `resource_id` | TEXT | Resource identifier |
| `status` | TEXT | success, failure |
| `error_message` | TEXT | Error details (if failed) |
| `timestamp` | TEXT | Action timestamp |

**Indexes:**
- `idx_audit_logs_action` on `action`
- `idx_audit_logs_user` on `user_id`
- `idx_audit_logs_timestamp` on `timestamp`

---

## Migration Management

### Directory Structure

**Canonical Location:** `/migrations/` (root)

**Migration Count:** 80 migrations (0001-0080, complete sequence)

**Signing:** All migrations signed with Ed25519 (`migrations/signatures.json`)

### Key Migrations

| Migration | Description | Tables Affected |
|-----------|-------------|-----------------|
| **0035** | Tick ledger federation columns | `global_tick_ledger` |
| **0045** | .aos file support | `adapters` (add `aos_file_path`, `aos_file_hash`) |
| **0055** | Model backend fields | `adapters` (add `adapter_path`, `backend`, `quantization`, `last_error`) |
| **0060** | Pinned adapters table | `pinned_adapters` (new table), `active_pinned_adapters` (view) |
| **0061** | Semantic naming taxonomy | `adapters` (add `tenant_namespace`, `domain`, `purpose`, `revision`) |
| **0062** | RBAC audit logs | `audit_logs` (new table) |
| **0063** | Dashboard configuration | `dashboard_configs` (new table) |
| **0064** | Adapter stacks | `adapter_stacks` (new table) |
| **0065** | Heartbeat mechanism | `adapters`, `training_jobs` (add `last_heartbeat`), views |
| **0066** | Stack versioning (telemetry correlation) | `adapter_stacks` |
| **0067** | Multi-tenancy for adapter stacks | `adapter_stacks` |
| **0068** | Metadata normalization | `adapters` (version, lifecycle_state) |
| **0069** | Plugin tenant enables | `plugin_tenant_enables` |
| **0070** | Routing decisions telemetry | `routing_decisions` |
| **0071** | Lifecycle version history | `lifecycle_version_history` (adapter/stack audit trail) |
| **0072** | Tenant snapshots | `tenant_snapshots` |
| **0073** | Index hash tracking | `index_hashes` |
| **0074** | Legacy index migration | Index cleanup |
| **0075** | Lifecycle state transition triggers | Triggers |
| **0076** | Golden run promotions | `golden_run_promotions` |
| **0077** | Adapter performance tracking | `adapter_performance` |
| **0078** | Federation consensus ledger | `federation_consensus` |
| **0079** | Stack versioning extensions | `adapter_stacks` |
| **0080** | Tenant adapter stack isolation | Multi-tenant stack isolation |

### Creating New Migrations

```bash
# 1. Create migration file
touch migrations/NNNN_description.sql

# 2. Write SQL (use SQLite-compatible types)
# Prefer: TEXT, INTEGER, REAL, BOOLEAN
# Avoid: JSONB, BIGINT, DOUBLE PRECISION, TIMESTAMP WITH TIME ZONE

# 3. Sign all migrations
./scripts/sign_migrations.sh

# 4. Test schema consistency
cargo test -p adapteros-db schema_consistency_tests
```

### Schema Consistency Requirements

- All migrations in `/migrations/` must be signed
- Adapter struct fields must match database columns
- INSERT statements must include all new schema columns
- SELECT queries must reference valid columns
- See `/crates/adapteros-db/tests/schema_consistency_tests.rs` for validation

### Migration Verification

```rust
use adapteros_db::migration_verify::MigrationVerifier;

let verifier = MigrationVerifier::new("migrations")?;
verifier.verify_all()?; // Checks Ed25519 signatures
```

---

## Views

### stale_adapters

Adapters with heartbeat timeout (5-minute threshold)

```sql
CREATE VIEW stale_adapters AS
SELECT * FROM adapters
WHERE last_heartbeat IS NOT NULL
  AND last_heartbeat < unixepoch('now') - 300;
```

### stale_training_jobs

Training jobs with heartbeat timeout

```sql
CREATE VIEW stale_training_jobs AS
SELECT * FROM training_jobs
WHERE last_heartbeat IS NOT NULL
  AND last_heartbeat < unixepoch('now') - 300;
```

---

## Transactional Updates

### State Update Concurrency

**Design:** Optimistic concurrency via SQLite transactions (no explicit locking required)

**Implementation:** `crates/adapteros-db/src/adapters.rs`

#### update_adapter_state_tx()

Lines 752-789

```rust
pub async fn update_adapter_state_tx(&self, adapter_id: &str, state: &str, reason: &str) -> Result<()> {
    // Begin transaction - SQLite acquires lock
    let mut tx = self.pool().begin().await?;

    // Verify row exists (transaction holds lock)
    let exists = sqlx::query_as::<_, (String,)>(
        "SELECT adapter_id FROM adapters WHERE adapter_id = ?"
    ).bind(adapter_id).fetch_optional(&mut *tx).await?;

    if exists.is_none() {
        return Err(anyhow::anyhow!("Adapter not found"));
    }

    // Perform update (serialized by transaction)
    sqlx::query("UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE adapter_id = ?")
        .bind(state).bind(adapter_id)
        .execute(&mut *tx).await?;

    // Commit - releases lock
    tx.commit().await?;
    Ok(())
}
```

#### update_adapter_memory_tx()

Lines 796-826

**Concurrency Guarantees:**
- SQLite's default isolation level provides serialization
- Transaction locks prevent lost updates in concurrent scenarios
- No need for explicit mutexes or row-level locks in application code
- Tested: `tests/stability_reinforcement_tests.rs::test_concurrent_state_update_race_condition`

---

## Common Queries

### Find Expired Adapters

```sql
SELECT * FROM adapters
WHERE expires_at IS NOT NULL
  AND expires_at < datetime('now');
```

### Find Adapters by Tier

```sql
SELECT * FROM adapters
WHERE tier = 'tier_1'
  AND current_state IN ('hot', 'resident');
```

### Adapter Activation Leaderboard

```sql
SELECT adapter_id, adapter_name, activation_pct, current_state
FROM adapters
ORDER BY activation_pct DESC
LIMIT 10;
```

### Active Pins by Tenant

```sql
SELECT * FROM active_pinned_adapters
WHERE tenant_id = 'tenant-a'
ORDER BY pinned_at DESC;
```

---

## See Also

- [CLAUDE.md](../CLAUDE.md) - Developer quick reference
- [PINNING_TTL.md](PINNING_TTL.md) - Pinning and TTL enforcement details
- [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) - Full architecture overview
