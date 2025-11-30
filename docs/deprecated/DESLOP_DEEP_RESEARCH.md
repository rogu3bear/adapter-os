# De-Slop Deep Research: System Architecture Analysis

**Purpose:** Comprehensive research into actual system architecture before proposing fixes

**Status:** Research Phase

**Last Updated:** 2025-01-27

---

## Research Questions

Before proposing fixes, we need to understand:

1. **Lifecycle Manager Integration:**
   - How is lifecycle manager initialized?
   - What methods does it expose for adapter loading/unloading?
   - Does it handle database updates internally or require external updates?
   - What is the correct pattern for state transitions?

2. **Database Layer:**
   - What methods exist in `Db` trait?
   - What operations should go through `Db` vs direct SQL?
   - How does lifecycle manager integrate with database?

3. **Deterministic Execution:**
   - Where is deterministic executor initialized?
   - When should `spawn_deterministic` be used vs `tokio::spawn`?
   - What contexts require deterministic execution?

4. **Architectural Patterns:**
   - What are the correct integration patterns?
   - What are examples of correct usage?
   - What are the actual violations?

---

## Lifecycle Manager Architecture

### Initialization Pattern

**Location:** `crates/adapteros-server-api/src/state.rs:228-231`

```rust
pub fn with_lifecycle(mut self, lifecycle_manager: Arc<Mutex<LifecycleManager>>) -> Self {
    self.lifecycle_manager = Some(lifecycle_manager);
    self
}
```

**Key Finding:** Lifecycle manager is optional (`Option<Arc<Mutex<LifecycleManager>>>`)

**Implication:** Handlers must check `if let Some(ref lifecycle) = state.lifecycle_manager` before use

### Lifecycle Manager Structure

**Location:** `crates/adapteros-lora-lifecycle/src/lib.rs:133-163`

```rust
pub struct LifecycleManager {
    states: Arc<RwLock<HashMap<u16, AdapterStateRecord>>>,
    policy: LifecyclePolicy,
    loader: Arc<RwLock<AdapterLoader>>,
    telemetry: Option<TelemetryWriter>,
    current_k: Arc<RwLock<usize>>,
    category_policies: CategoryPolicyManager,
    db: Option<Db>,  // ⚠️ Database is optional
    activation_tracker: Arc<RwLock<ActivationTracker>>,
    active_stack: Arc<RwLock<Option<(String, Vec<String>)>>>,
    k_reduction_coordinator: Arc<LifecycleKReductionCoordinator>,
    // ...
}
```

**Key Findings:**
1. Lifecycle manager has its own internal state (`states: HashMap<u16, AdapterStateRecord>`)
2. Database is optional (`db: Option<Db>`)
3. Has its own `AdapterLoader` instance
4. Manages activation tracking and K reduction

### Lifecycle Manager Methods

**From codebase search:**

1. **`record_router_decision(&self, adapter_indices: &[u16]) -> Result<()>`**
   - Records router decisions
   - Updates activation tracking
   - Auto-promotes adapters based on activation %
   - **Location:** `crates/adapteros-lora-lifecycle/src/lib.rs` (line ~2100+)

2. **`evict_adapter(&self, adapter_id: u16) -> Result<()>`**
   - Evicts adapter from memory
   - Updates internal state
   - Updates database if `db` is set
   - **Location:** `crates/adapteros-lora-lifecycle/src/lib.rs:1457`

3. **`promote_adapter(&self, adapter_id: u16) -> Result<()>`**
   - Manually promotes adapter state
   - Updates internal state
   - **Location:** Found in tests

4. **`get_or_reload(&self, adapter_id: &str) -> Result<()>`**
   - Gets adapter or reloads if needed
   - **Location:** `crates/adapteros-server-api/src/handlers/domain_adapters.rs:334`

**Critical Question:** Does lifecycle manager update database automatically, or do handlers need to do it?

### Database Integration in Lifecycle Manager

**Location:** `crates/adapteros-lora-lifecycle/src/lib.rs:1457-1494`

```rust
pub async fn evict_adapter(&self, adapter_id: u16) -> Result<()> {
    // Update internal state (lock released before async)
    let (adapter_id_str, old_state, category, memory_freed) = {
        let mut states = self.states.write();
        // ... update state ...
    }; // LOCK RELEASED HERE

    // Unload from loader
    {
        let mut loader = self.loader.write();
        loader.unload_adapter(adapter_id)?;
    } // LOADER LOCK RELEASED

    // Async operations happen WITHOUT any locks
    if let Some(ref db) = self.db {
        let db_clone = db.clone();
        // Update database
        db_clone.update_adapter_state(&adapter_id_str, "unloaded", "evicted").await?;
    }
    
    // Emit telemetry
    // ...
}
```

**Key Finding:** Lifecycle manager DOES update database if `db` is set, but only for certain operations (eviction)

**Question:** Does it update database for loading/promotion operations?

---

## Database Layer Architecture

### Db Trait Methods

**From codebase search, `Db` has methods like:**

1. **`get_adapter(&self, adapter_id: &str) -> Result<Option<AdapterRecord>>`**
   - Gets adapter from database
   - **Location:** `crates/adapteros-db/src/adapters.rs`

2. **`update_adapter_state(&self, adapter_id: &str, state: &str, reason: &str) -> Result<()>`**
   - Updates adapter state in database
   - **Location:** `crates/adapteros-db/src/adapters.rs`

3. **`update_adapter_state_tx(&self, adapter_id: &str, state: &str, reason: &str) -> Result<()>`**
   - Transaction-protected state update
   - **Location:** `crates/adapteros-db/src/adapters.rs:763-799`

4. **`transition_adapter_lifecycle(&self, adapter_id: &str, new_state: &str, reason: &str, initiated_by: &str) -> Result<String>`**
   - Lifecycle state transition with version bumping
   - **Location:** `crates/adapteros-db/src/lifecycle.rs:49`

**Key Finding:** Database has methods for state updates, but lifecycle manager may also update database

**Question:** What's the division of responsibility between lifecycle manager and database?

---

## Current Handler Patterns (Analysis)

### Pattern 1: Direct DB Update Before Lifecycle Manager

**Location:** `crates/adapteros-server-api/src/handlers.rs:434-442`

```rust
let adapter_result = state.db.get_adapter(&adapter_id).await;

match adapter_result {
    Ok(Some(a)) => {
        // ❌ Updates DB state BEFORE lifecycle manager
        let _ = state
            .db
            .update_adapter_state(&adapter_id, "loading", "directory_upsert")
            .await;

        if let Some(ref lifecycle) = state.lifecycle_manager {
            // Creates new AdapterLoader instead of using lifecycle manager
            let mut loader = AdapterLoader::new(adapters_path, expected_hashes);
            // ...
        }
    }
}
```

**Issues:**
1. Updates database state before lifecycle manager is involved
2. Creates new `AdapterLoader` instead of using lifecycle manager's loader
3. Doesn't use lifecycle manager's state tracking

**Question:** What should this code do instead?

### Pattern 2: Lifecycle Manager Then DB Update

**Location:** `crates/adapteros-server-api/src/handlers/domain_adapters.rs:331-362`

```rust
// Use lifecycle manager if available
if let Some(ref lifecycle) = state.lifecycle_manager {
    let mut manager = lifecycle.lock().await;
    manager.get_or_reload(&adapter_id).map_err(|e| {
        // Error handling
    })?;
}

// Update adapter state to loaded in database
state
    .db
    .update_adapter_state_tx(&adapter_id, "warm", "Loaded via API")
    .await
    .map_err(|e| {
        // Error handling
    })?;
```

**Analysis:**
1. Uses lifecycle manager first (`get_or_reload`)
2. Then updates database state separately
3. **Question:** Does `get_or_reload` update database? If so, this might be redundant

### Pattern 3: Direct SQL Query

**Location:** `crates/adapteros-server-api/src/handlers.rs:5319-5336`

```rust
// Update adapter tier in database
sqlx::query(
    "UPDATE adapters SET tier = ?, updated_at = ? WHERE adapter_id = ?"
)
.bind(&new_tier)
.bind(&timestamp)
.bind(&adapter_id)
.execute(state.db.pool())  // Direct SQL, bypasses Db trait
.await
```

**Analysis:**
1. Bypasses `Db` trait abstraction
2. Direct SQL query
3. **Question:** Should there be a `Db::update_adapter_tier()` method?

---

## Deterministic Execution Architecture

### Initialization Pattern

**Location:** `crates/adapteros-server/src/main.rs:247-266`

```rust
// Derive executor seed using HKDF from manifest hash
let base_seed = manifest_hash.unwrap_or_else(|| B3Hash::hash(b"default-seed-non-production"));
let global_seed = derive_seed(&base_seed, "executor");

let executor_config = ExecutorConfig {
    global_seed,
    enable_event_logging: true,
    max_ticks_per_task: 10000,
    ..Default::default()
};
init_global_executor(executor_config)?;
```

**Key Finding:** Deterministic executor is initialized at server startup with manifest-derived seed

**Question:** Is it initialized in all code paths? What about tests?

### Usage Pattern

**Location:** `crates/adapteros-server/src/main.rs:458`

```rust
let sighup_handle = spawn_deterministic("SIGHUP handler".to_string(), async move {
    // Signal handling logic
});
```

**Key Finding:** `spawn_deterministic!` macro is used for background tasks

**Question:** When should `tokio::spawn` be used vs `spawn_deterministic`?

### Current Violations

**Found:** 14 instances of `tokio::spawn`/`thread::spawn`

**Locations:**
1. `crates/adapteros-orchestrator/src/training.rs:183, 440` - Training operations
2. `crates/adapteros-server-api/src/handlers/datasets.rs:1698` - Dataset operations
3. `crates/adapteros-lora-worker/src/lib.rs:467` - Worker operations

**Question:** Do these contexts require deterministic execution?

---

## Research Findings Summary

### Lifecycle Manager

**What we know:**
- Lifecycle manager is optional in `AppState`
- Has internal state tracking (`HashMap<u16, AdapterStateRecord>`)
- Database integration is optional (`db: Option<Db>`)
- Updates database for eviction operations
- Has methods: `record_router_decision`, `evict_adapter`, `get_or_reload`, `promote_adapter`

**What we learned:**

1. **`promote_adapter()` (line 854):**
   - Updates internal state only
   - Does NOT update database
   - Only logs telemetry
   - **Implication:** Handlers must update database separately if using this method

2. **`update_adapter_state()` (line 1193):**
   - Updates both internal state AND database (if `db` is set)
   - Spawns deterministic task to update database asynchronously
   - **Implication:** This is the method that handles database updates

3. **`get_or_reload()` (line 1137):**
   - Loads adapter into memory
   - Updates internal state to `Cold`
   - Does NOT update database
   - **Implication:** Handlers must update database separately

4. **`evict_adapter()` (line 1457):**
   - Updates internal state
   - Unloads from loader
   - Updates database if `db` is set
   - **Implication:** This method handles database updates

5. **`record_router_decision()` (line 666):**
   - Updates activation percentages in database (if `db` is set)
   - Can trigger auto-eviction
   - Uses deterministic spawn for DB updates

**Architectural Pattern:**
- Lifecycle manager maintains in-memory state (`HashMap<u16, AdapterStateRecord>`)
- Database is source of truth for persistence
- Some methods update database automatically (`evict_adapter`, `update_adapter_state`, `record_router_decision`)
- Some methods don't (`promote_adapter`, `get_or_reload`)
- Division of responsibility is inconsistent

**What we still don't know:**
- What's the correct pattern for loading adapters via API?
- Should handlers call `update_adapter_state()` after `get_or_reload()`?
- Why do some handlers update database directly instead of using lifecycle manager methods?

### Database Layer

**What we know:**
- `Db` trait has methods: `get_adapter`, `update_adapter_state`, `update_adapter_state_tx`, `transition_adapter_lifecycle`
- Some handlers use direct SQL queries
- Database and lifecycle manager both update adapter state

**What we don't know:**
- What operations should go through `Db` trait vs direct SQL?
- What's the division of responsibility between `Db` and lifecycle manager?

### Deterministic Execution

**What we know:**
- Deterministic executor initialized at server startup
- Uses `spawn_deterministic!` macro
- Seed derived from manifest hash via HKDF

**What we don't know:**
- When is deterministic execution required vs optional?
- Should training operations be deterministic?
- Should background tasks be deterministic?

---

## Next Steps for Research

1. **Trace correct usage patterns:**
   - Find examples of correct lifecycle manager usage
   - Find examples of correct database usage
   - Find examples of correct deterministic execution usage

2. **Understand integration points:**
   - How does lifecycle manager integrate with database?
   - How does lifecycle manager integrate with worker?
   - What are the actual data flows?

3. **Identify architectural constraints:**
   - What operations must go through lifecycle manager?
   - What operations can bypass lifecycle manager?
   - What are the actual requirements vs preferences?

4. **Document correct patterns:**
   - Create examples of correct usage
   - Document integration patterns
   - Clarify responsibilities

---

## References

- [Lifecycle Manager Implementation](../../crates/adapteros-lora-lifecycle/src/lib.rs)
- [Database Layer Implementation](../../crates/adapteros-db/src/lib.rs)
- [Deterministic Executor Implementation](../../crates/adapteros-deterministic-exec/src/lib.rs)
- [Server API State Management](../../crates/adapteros-server-api/src/state.rs)
- [Handler Patterns](../../crates/adapteros-server-api/src/handlers.rs)

