# AdapterOS Database Layer Patterns

## Overview

The `adapteros-db` crate provides the persistence layer for AdapterOS, supporting both SQLite (SQL) and ReDB (KV) backends with a dual-write architecture for gradual migration.

**Location:** `/Users/star/Dev/adapter-os/crates/adapteros-db/`

## 1. SQLite Connection Pooling

### Configuration
```rust
// From factory.rs - SQLite pool creation
SqliteConnectOptions::from_str(&format!("sqlite://{}", database_url))?
    .create_if_missing(true)
    .journal_mode(SqliteJournalMode::Wal)     // WAL mode for concurrency
    .synchronous(SqliteSynchronous::Normal)    // Balance safety/performance
    .busy_timeout(Duration::from_secs(30))     // 30s busy timeout
    .statement_cache_capacity(50)              // Reduced from 100 for memory
    .foreign_keys(true);                       // CRITICAL: FK enforcement

SqlitePoolOptions::new()
    .max_connections(pool_size)
    .acquire_timeout(acquire_timeout)          // Configurable via AOS_DB_ACQUIRE_TIMEOUT_SECS
    .connect_with(options)
```

### Key Settings
- **WAL Mode:** Better concurrent read/write performance
- **Statement Cache:** 50 prepared statements per connection (~5KB each)
- **Pool Warning:** >10MB estimated cache triggers warning
- **In-Memory DBs:** Limited to 1 connection for data consistency

### Environment Variables
- `DATABASE_URL` or `AOS_DATABASE_URL`: Database path
- `AOS_DB_ACQUIRE_TIMEOUT_SECS`: Pool acquire timeout
- `AOS_STORAGE_BACKEND`: Storage mode selection

## 2. Migration System

### Signature Verification (Ed25519)
```rust
// From migration_verify.rs
pub struct MigrationSignature {
    pub hash: String,           // BLAKE3 or SHA256
    pub signature: String,      // Ed25519 (base64)
    pub algorithm: String,      // "ed25519"
    pub hash_algorithm: String, // "blake3" or "sha256"
}
```

### Migration Flow
1. Load `migrations/signatures.json`
2. Verify each `.sql` file's hash and Ed25519 signature
3. Run migrations via SQLx with timeout (default: 30s release, 120s debug)
4. Verify database is at expected version

### Validation Utilities (migration_validation.rs)
- `verify_migration_checksum()`: BLAKE3 hash verification
- `validate_migration_order()`: Ensures strictly increasing versions
- `check_schema_version()`: Returns Compatible/NeedsMigration/DatabaseAhead

## 3. Atomic Dual-Write Patterns

### Storage Modes (`StorageMode` enum)
```
SqlOnly (default) -> DualWrite -> KvPrimary -> KvOnly (target)
     |                  |            |            |
     v                  v            v            v
 SQL only         Write both     Write both    KV only
                  Read SQL       Read KV
```

### AtomicDualWriteConfig
```rust
// From adapters/mod.rs
pub struct AtomicDualWriteConfig {
    pub require_kv_success: bool,  // Strict mode default
}

// Environment: AOS_ATOMIC_DUAL_WRITE_STRICT (default: true)
// Strict: KV failure -> rollback SQL, propagate error
// Best-effort: KV failure -> log warning, operation succeeds
```

### Dual-Write Pattern
```rust
// 1. Begin SQL transaction
// 2. Execute SQL write
// 3. Execute KV write
// 4. If KV fails AND strict mode:
//    - Rollback SQL transaction
//    - Return error
// 5. Otherwise:
//    - Commit SQL
//    - Log KV failure (if any)
```

### KV Coverage Guard
The system tracks which domains have KV support:
```rust
// Supported: adapters, adapter_stacks, tenants, users, auth_sessions,
//            plans, tenant_policy_bindings, rag, telemetry, replay,
//            plugin_configs, messages, runtime_sessions, repositories
```

## 4. Adapter Registry

### Key Types (adapter_repositories.rs)
```rust
pub struct AdapterRepository {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub base_model_id: Option<String>,
    pub default_branch: String,
    pub archived: i64,
    // ...
}

pub struct AdapterVersion {
    pub id: String,
    pub repo_id: String,
    pub version: String,
    pub branch: String,
    pub release_state: String,  // draft|training|ready|active|deprecated|retired|failed
    pub adapter_trust_state: String,
    // ...
}
```

### TOCTOU Prevention (Tier Promotion)
```rust
// ANCHOR: UPDATE includes WHERE clause for expected state
// AUDIT: Tracks TIER_PROMOTION_TOCTOU_DETECTED counter
// RECTIFY: Returns Conflict error on 0 rows affected

static TIER_PROMOTION_TOCTOU_DETECTED: AtomicU64 = AtomicU64::new(0);
pub fn tier_promotion_toctou_count() -> u64 { ... }
```

### Release State Transitions
```
draft -> training -> ready -> active -> deprecated -> retired
                  \-> failed (from training or any state)
active <-> deprecated (bidirectional for rollback)
active -> ready (rollback)
```

## 5. Session/Auth Storage

### AuthSession Operations (auth_sessions.rs)
```rust
impl Db {
    // Dual-write aware operations
    pub async fn create_auth_session(&self, jti, tenant_id, user_id, ip, ua, expires) -> Result<()>;
    pub async fn delete_auth_session(&self, jti: &str) -> Result<()>;
    pub async fn revoke_token(&self, token_hash, revoked_by, reason) -> Result<()>;
    pub async fn is_token_revoked(&self, token_hash: &str) -> Result<bool>;
    pub async fn get_user_sessions(&self, user_id: &str) -> Result<Vec<AuthSession>>;
    pub async fn cleanup_expired_sessions(&self) -> Result<u64>;
}
```

### Pattern: Storage Mode Awareness
```rust
// Reads check mode first
if self.storage_mode().read_from_kv() {
    if let Some(repo) = self.get_auth_kv_repo() {
        return repo.method().await;
    }
    if !self.storage_mode().sql_fallback_enabled() {
        return Ok(default);
    }
}
// Fallback to SQL...

// Writes go to both when needed
if self.storage_mode().write_to_sql() { /* SQL write */ }
if self.storage_mode().write_to_kv() { /* KV write */ }
```

## 6. Key Queries and Repository Patterns

### Naming Conventions
| Prefix | Usage | Example |
|--------|-------|---------|
| `get_*` | Single entity by ID | `get_adapter()` |
| `list_*` | Multiple entities | `list_adapters_for_tenant()` |
| `create_*` | New entity (async) | `create_training_dataset()` |
| `register_*` | Create with validation | `register_adapter()` |
| `update_*` | Modify fields | `update_adapter_state()` |
| `delete_*` | Remove entity | `delete_adapter()` |
| `*_kv` | KV backend variant | `get_adapter_kv()` |
| `*_for_tenant` | Tenant-scoped | `list_adapters_for_tenant()` |

### Protected Write Access
```rust
// Read-only (always available)
let adapters = db.list_adapters_for_tenant(tenant_id).await?;

// Write access (requires lifecycle token)
let protected = ProtectedDb::from_db(db, lifecycle_token);
protected.create_adapter(params).await?;

// Or using write guard
let writable = protected.write_guard();
writable.register_adapter(params).await?;
```

### KV Repository Pattern (adapters_kv.rs)
```rust
#[async_trait]
pub trait AdapterKvOps {
    async fn register_adapter_kv(&self, params: AdapterRegistrationParams) -> Result<String>;
    async fn get_adapter_kv(&self, adapter_id: &str) -> Result<Option<Adapter>>;
    async fn list_adapters_for_tenant_kv(&self, tenant_id: &str, limit, offset) -> Result<Vec<Adapter>>;
    async fn delete_adapter_kv(&self, id: &str) -> Result<()>;
    async fn update_adapter_state_kv(&self, adapter_id, state, reason) -> Result<()>;
    // ... more operations
}
```

## 7. Transaction Handling

### Begin Write Transaction
```rust
impl Db {
    pub async fn begin_write_tx(&self) -> Result<Transaction<'_, Sqlite>> {
        let pool = self.pool();
        let tx = pool.begin().await?;
        Ok(tx)
    }
}
```

### Transaction with History (adapter_repositories.rs)
```rust
// Example: Creating version with history entry
let mut tx = self.begin_write_tx().await?;

// 1. Insert main record
sqlx::query("INSERT INTO adapter_versions ...")
    .execute(&mut *tx).await?;

// 2. Insert history entry
self.insert_version_history(&mut tx, VersionHistoryEntry { ... }).await?;

// 3. Commit
tx.commit().await?;
```

## 8. Test Utilities

### Creating Test Databases
```rust
// In-memory with migrations
let db = Db::new_in_memory().await?;

// Test DB with KV
let test_db = TestDb::new().await;
let db = create_test_db_with_kv().await?;

// Cleanup
test_db.cleanup().await;
```

### Test Factories (tests/common/factories.rs)
```rust
let adapter = TestAdapterFactory::default()
    .tenant_id("test-tenant")
    .build();

let tenant = TestTenantFactory::default()
    .name("Test Tenant")
    .build();
```

## 9. KV Backend (kv_backend.rs)

### KvDb Wrapper
```rust
pub struct KvDb {
    backend: Arc<dyn KvBackend>,      // ReDB backend
    index_manager: Arc<IndexManager>, // Secondary indexes
    increment_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl KvDb {
    pub fn init_redb(path: &Path) -> Result<Self>;
    pub fn init_in_memory() -> Result<Self>;
    
    // Operations with metrics
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>>;
    pub async fn set(&self, key: &str, value: Vec<u8>) -> Result<()>;
    pub async fn delete(&self, key: &str) -> Result<bool>;
    pub async fn scan_prefix(&self, prefix: &str) -> Result<Vec<String>>;
    
    // Index operations
    pub async fn query_by_index(&self, index_name, index_value) -> Result<Vec<String>>;
    pub async fn add_to_index(&self, index_name, index_value, entity_id) -> Result<()>;
}
```

## Key Files Reference

| File | Purpose |
|------|---------|
| `lib.rs` | Main Db struct, StorageMode, connect methods |
| `factory.rs` | DbFactory, pool creation |
| `adapter_repositories.rs` | AdapterRepository, AdapterVersion CRUD |
| `adapters/mod.rs` | Adapter dual-write operations |
| `adapters_kv.rs` | KV-only adapter operations |
| `auth_sessions.rs` | Session management |
| `protected_db.rs` | LifecycleToken, WriteCapableDb |
| `migration_verify.rs` | Ed25519 signature verification |
| `migration_validation.rs` | Checksum and ordering validation |
| `kv_backend.rs` | KvDb wrapper with indexes |
| `promotions.rs` | Golden run promotion workflow |
