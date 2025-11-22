# Determinism Guarantees in AdapterOS

**Document Version:** 1.0
**Last Updated:** 2025-11-21
**Author:** James KC Auchterlonie
**Status:** Production Documentation

---

## 1. Overview

AdapterOS provides comprehensive determinism guarantees to ensure reproducible inference results across runs, platforms, and distributed deployments. This document covers:

1. **HKDF Seed Derivation** - Cryptographic key hierarchy for all randomness
2. **Task Execution Ordering** - Serial FIFO execution model
3. **Reproducibility Verification** - Cross-run validation mechanisms
4. **Platform Independence** - Architecture-agnostic determinism

---

## 2. HKDF Hierarchy Diagram

The entire system derives randomness from a single root seed using HKDF-SHA256 (RFC 5869). This ensures complete reproducibility when the same manifest is used.

```
                              ┌─────────────────────────────────┐
                              │      Base Model Manifest        │
                              │  (manifest.json / BLAKE3 hash)  │
                              └───────────────┬─────────────────┘
                                              │
                                              ▼
                              ┌─────────────────────────────────┐
                              │      manifest_hash (32 bytes)   │
                              │  derive_seed(manifest_hash, _)  │
                              └───────────────┬─────────────────┘
                                              │
                    ┌─────────────────────────┼─────────────────────────┐
                    │                         │                         │
                    ▼                         ▼                         ▼
     ┌──────────────────────┐  ┌──────────────────────┐  ┌──────────────────────┐
     │      "executor"      │  │       "router"       │  │      "sampling"      │
     │                      │  │                      │  │                      │
     │  Global executor     │  │  Router gate         │  │  Token sampling      │
     │  seed for task       │  │  weight init and     │  │  temperature and     │
     │  scheduling          │  │  adapter selection   │  │  top-p/top-k         │
     └──────────┬───────────┘  └──────────┬───────────┘  └──────────┬───────────┘
                │                         │                         │
                ▼                         ▼                         ▼
     ┌──────────────────────┐  ┌──────────────────────┐  ┌──────────────────────┐
     │ worker_id:nonce:task │  │ adapter_id:layer:idx │  │ position:vocab_idx   │
     │ ──────────────────── │  │ ──────────────────── │  │ ──────────────────── │
     │ Per-task RNG seeds   │  │ Per-layer LoRA seeds │  │ Per-token RNG seeds  │
     └──────────────────────┘  └──────────────────────┘  └──────────────────────┘

                              Additional Domain Labels:
     ┌──────────────────────┐  ┌──────────────────────┐  ┌──────────────────────┐
     │      "dropout"       │  │     "adapter_N"      │  │      "training"      │
     │                      │  │                      │  │                      │
     │  Dropout masks for   │  │  Adapter-specific    │  │  Training data       │
     │  regularization      │  │  noise injection     │  │  shuffling and       │
     │                      │  │                      │  │  augmentation        │
     └──────────────────────┘  └──────────────────────┘  └──────────────────────┘
```

### 2.1 Domain Label Registry

| Label | Purpose | Entropy Sources |
|-------|---------|-----------------|
| `executor` | Task scheduling order | manifest_hash |
| `router` | Adapter selection weights | manifest_hash, query embedding |
| `sampling` | Token generation | manifest_hash, position, temperature |
| `dropout` | Regularization masks | manifest_hash, layer_idx, step |
| `adapter_N` | Per-adapter noise | manifest_hash, adapter_id, layer |
| `training` | Dataset shuffling | manifest_hash, epoch, batch_idx |

---

## 3. Seed Derivation Paths

### 3.1 Core Derivation Function

**Location:** `crates/adapteros-core/src/seed.rs`

```rust
/// Derive a deterministic seed from a global seed and label
///
/// Uses HKDF-SHA256 for key derivation. All RNG in the system
/// must derive from these seeds to ensure determinism.
pub fn derive_seed(global: &B3Hash, label: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::from_prk(global.as_bytes()).expect("valid PRK");
    let mut okm = [0u8; 32];
    hk.expand(label.as_bytes(), &mut okm)
        .expect("32 bytes is valid length");

    // Validate HKDF output is exactly 32 bytes
    assert_eq!(okm.len(), 32, "HKDF output must be exactly 32 bytes");

    // Compute checksum for audit
    let checksum = B3Hash::hash(&okm);
    tracing::debug!(
        label = label,
        checksum = %checksum.to_hex()[..16],
        "Derived seed with validation"
    );

    okm
}
```

### 3.2 Full Entropy Isolation

For complete isolation across different execution contexts:

```rust
/// Derive a deterministic seed with full entropy isolation
///
/// Incorporates: manifest_hash || adapter_dir || worker_id || nonce
pub fn derive_seed_full(
    global: &B3Hash,
    manifest_hash: &B3Hash,
    adapter_dir_hash: &B3Hash,
    worker_id: u32,
    label: &str,
    nonce: u64,
) -> [u8; 32] {
    let composite_label = format!(
        "{}:{}:{}:{}:{}",
        label,
        manifest_hash.to_hex(),
        adapter_dir_hash.to_hex(),
        worker_id,
        nonce
    );
    derive_seed(global, &composite_label)
}
```

### 3.3 Derivation Path Examples

```
┌─────────────────────────────────────────────────────────────────────────┐
│ Example: Router Seed for Worker 3, Nonce 42                             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│ manifest_hash: "a1b2c3d4e5f6..."                                       │
│ adapter_dir_hash: "f1e2d3c4b5a6..."                                    │
│ worker_id: 3                                                            │
│ nonce: 42                                                               │
│                                                                         │
│ composite_label = "router:a1b2c3d4e5f6...:f1e2d3c4b5a6...:3:42"       │
│                                                                         │
│ router_seed = HKDF-Expand(manifest_hash, composite_label, 32)          │
│                                                                         │
│ Result: [0x7a, 0x3b, 0x9c, ...] (32 bytes)                             │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│ Example: Per-Adapter Layer Seed                                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│ adapter_id: 5                                                           │
│ layer: 12                                                               │
│ nonce: 0                                                                │
│                                                                         │
│ label = "adapter_5:layer_12"                                            │
│                                                                         │
│ layer_seed = HKDF-Expand(global_seed, label, 32)                       │
│                                                                         │
│ Note: Reuse detection via SEED_REGISTRY prevents accidental reuse      │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.4 Seed Reuse Prevention

```rust
lazy_static::lazy_static! {
    /// Seed registry to prevent reuse
    static ref SEED_REGISTRY: Mutex<HashMap<(String, u64), bool>> = Mutex::new(HashMap::new());
}

/// Derive per-adapter seed with layer isolation and reuse prevention
pub fn derive_adapter_seed(
    global: &B3Hash,
    adapter_id: usize,
    layer: usize,
    nonce: u64,
) -> Result<[u8; 32], String> {
    let label = format!("adapter_{}:layer_{}", adapter_id, layer);

    // Check for reuse
    let key = (label.clone(), nonce);
    let mut registry = SEED_REGISTRY.lock().unwrap();
    if registry.contains_key(&key) {
        return Err(format!(
            "Seed reuse detected: {} with nonce {}",
            label, nonce
        ));
    }
    registry.insert(key, true);

    Ok(derive_seed(global, &label))
}

/// Clear seed registry (call at inference boundaries)
pub fn clear_seed_registry() {
    let mut registry = SEED_REGISTRY.lock().unwrap();
    registry.clear();
    tracing::debug!("Cleared seed registry");
}
```

---

## 4. Task Execution Ordering

### 4.1 Serial FIFO Execution Model

**Location:** `crates/adapteros-deterministic-exec/src/lib.rs`

The deterministic executor ensures all tasks execute in a fixed, reproducible order:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Deterministic Executor Model                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  Task Queue (FIFO)                                                      │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  Task 1  │  Task 2  │  Task 3  │  Task 4  │  ...  │  Task N     │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│       │                                                                 │
│       ▼                                                                 │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    Serial Execution                              │   │
│  │                                                                  │   │
│  │  poll(Task 1)                                                   │   │
│  │       │                                                          │   │
│  │       ├── Ready(()) → Complete, log TaskCompleted event         │   │
│  │       │                                                          │   │
│  │       └── Pending → Re-queue, advance tick counter              │   │
│  │                                                                  │   │
│  │  poll(Task 2)                                                   │   │
│  │       │                                                          │   │
│  │       └── ... (same logic)                                      │   │
│  │                                                                  │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  Invariant: Tasks execute in submission order, never concurrently      │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 4.2 Tick-Based Time Model

Wall-clock time is non-deterministic. The executor uses logical ticks instead:

```rust
/// Tick-based timeout guard
pub struct TickTimeout {
    task_id: TaskId,
    timeout_tick: u64,
    current_tick: Arc<AtomicU64>,
}

impl TickTimeout {
    pub fn new(task_id: TaskId, timeout_ticks: u64, current_tick: Arc<AtomicU64>) -> Self {
        let timeout_tick = current_tick.load(Ordering::Relaxed) + timeout_ticks;
        Self { task_id, timeout_tick, current_tick }
    }

    pub fn is_timeout(&self) -> bool {
        self.current_tick.load(Ordering::Relaxed) >= self.timeout_tick
    }
}

/// Tick-based delay future (replaces tokio::time::sleep)
pub struct TickDelay {
    target_tick: u64,
    current_tick: Arc<AtomicU64>,
}

impl Future for TickDelay {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.current_tick.load(Ordering::Relaxed) >= self.target_tick {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
```

### 4.3 Event Logging for Replay

Every executor action is logged for replay verification:

```rust
pub enum ExecutorEvent {
    /// Task spawned with ID and description
    TaskSpawned {
        task_id: TaskId,
        description: String,
        tick: u64,
        agent_id: Option<String>,
        hash: [u8; 32],  // BLAKE3 hash for integrity
    },

    /// Task completed successfully
    TaskCompleted {
        task_id: TaskId,
        tick: u64,
        duration_ticks: u64,
        agent_id: Option<String>,
        hash: [u8; 32],
    },

    /// Task failed with error
    TaskFailed {
        task_id: TaskId,
        error: String,
        tick: u64,
        duration_ticks: u64,
        agent_id: Option<String>,
        hash: [u8; 32],
    },

    /// Task timed out
    TaskTimeout {
        task_id: TaskId,
        timeout_ticks: u64,
        tick: u64,
        agent_id: Option<String>,
        hash: [u8; 32],
    },

    /// Tick counter advanced
    TickAdvanced {
        from_tick: u64,
        to_tick: u64,
        agent_id: Option<String>,
        hash: [u8; 32],
    },
}
```

### 4.4 Deterministic Task ID Generation

```rust
/// Global sequence counter for deterministic task ID generation
static GLOBAL_TASK_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Deterministic task ID type using BLAKE3 hash
pub struct TaskId([u8; 32]);

impl TaskId {
    /// Generate deterministic task ID from global seed and sequence number
    pub fn from_seed_and_seq(global_seed: &[u8; 32], seq: u64) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(global_seed);
        hasher.update(&seq.to_le_bytes());
        let hash = hasher.finalize();
        Self(*hash.as_bytes())
    }
}
```

---

## 5. Reproducibility Verification

### 5.1 Cross-Run Verification Flow

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Reproducibility Verification                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  Run 1 (Reference)                 Run 2 (Verification)                 │
│  ──────────────────                ─────────────────────                │
│                                                                         │
│  manifest.json ─────────────────────────────────────► manifest.json    │
│       │                                                    │            │
│       ▼                                                    ▼            │
│  manifest_hash_1 ════════════════════════════════ manifest_hash_2      │
│       │                            (must match)            │            │
│       ▼                                                    ▼            │
│  global_seed_1 ═══════════════════════════════════ global_seed_2       │
│       │                            (must match)            │            │
│       ▼                                                    ▼            │
│  ┌─────────────┐                              ┌─────────────┐          │
│  │ Executor    │                              │ Executor    │          │
│  │ Run 1       │                              │ Run 2       │          │
│  └─────────────┘                              └─────────────┘          │
│       │                                                    │            │
│       ▼                                                    ▼            │
│  event_log_1                                      event_log_2          │
│       │                                                    │            │
│       └──────────────────┬─────────────────────────────────┘            │
│                          │                                              │
│                          ▼                                              │
│               ┌─────────────────────┐                                   │
│               │   Hash Comparison   │                                   │
│               │   ─────────────────  │                                  │
│               │   hash(event_log_1)  │                                  │
│               │        ==            │                                  │
│               │   hash(event_log_2)  │                                  │
│               └─────────────────────┘                                   │
│                          │                                              │
│              ┌───────────┴───────────┐                                  │
│              │                       │                                  │
│              ▼                       ▼                                  │
│         [MATCH]                 [MISMATCH]                              │
│    Determinism                 Investigate:                             │
│    verified                    - Platform diff                          │
│                                - Seed drift                             │
│                                - Non-deterministic code                 │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Backend Attestation

Each backend provides determinism attestation:

```rust
pub trait FusedKernels {
    /// Attest to determinism guarantees
    fn attest_determinism(&self) -> Result<DeterminismReport>;
}

pub struct DeterminismReport {
    /// Backend type (Metal, CoreML, MLX)
    pub backend_type: BackendType,

    /// Metallib hash (Metal backend only)
    pub metallib_hash: Option<B3Hash>,

    /// Kernel manifest with build info
    pub manifest: Option<KernelManifest>,

    /// How randomness is seeded
    pub rng_seed_method: RngSeedingMethod,

    /// Floating point mode
    pub floating_point_mode: FloatingPointMode,

    /// Compiler flags used
    pub compiler_flags: Vec<String>,

    /// Overall determinism guarantee
    pub deterministic: bool,
}

pub enum RngSeedingMethod {
    /// All randomness derived from HKDF (fully deterministic)
    HkdfSeeded,
    /// Uses system entropy (non-deterministic)
    SystemEntropy,
    /// Fixed seed (deterministic but not derived)
    FixedSeed([u8; 32]),
}

pub enum FloatingPointMode {
    /// Strict IEEE 754 compliance (slower, deterministic)
    Deterministic,
    /// Fast math enabled (faster, may vary)
    FastMath,
    /// Mixed precision (may have platform variance)
    MixedPrecision,
}
```

### 5.3 Snapshot and Restore

```rust
/// Snapshot of executor state for crash recovery
pub struct ExecutorSnapshot {
    /// Current tick
    pub tick: u64,
    /// RNG state (serialized seed)
    pub rng_seed: [u8; 32],
    /// Pending tasks (without futures)
    pub pending_tasks: Vec<TaskSnapshot>,
    /// Event log
    pub event_log: Vec<ExecutorEvent>,
    /// Global sequence counter
    pub global_sequence: u64,
    /// Agent ID
    pub agent_id: Option<String>,
}

impl DeterministicExecutor {
    /// Create a snapshot of the executor state
    pub fn snapshot(&self) -> Result<ExecutorSnapshot> { ... }

    /// Restore executor state from a snapshot
    pub fn restore(&self, snapshot: ExecutorSnapshot) -> Result<()> { ... }
}
```

---

## 6. Thread-Local Seed Management

**Location:** `crates/adapteros-deterministic-exec/src/seed.rs`

### 6.1 Thread Seed Propagation

```rust
/// Thread-local seed with collision detection
pub struct ThreadSeed {
    seed: [u8; 32],
    thread_id: ThreadId,
    generation: u64,
    parent_seed: Option<[u8; 32]>,
}

impl ThreadSeed {
    /// Create a child seed derived from parent
    pub fn derive_child(&self, label: &str) -> Self {
        use hkdf::Hkdf;
        use sha2::Sha256;

        let hk = Hkdf::<Sha256>::new(Some(label.as_bytes()), &self.seed);
        let mut derived = [0u8; 32];
        hk.expand(&[], &mut derived).expect("HKDF expansion failed");

        Self {
            seed: derived,
            thread_id: std::thread::current().id(),
            generation: self.generation + 1,
            parent_seed: Some(self.seed),
        }
    }
}
```

### 6.2 Collision Detection

```rust
/// Seed registry for managing thread-local seeds
pub struct SeedRegistry {
    registry: Arc<Mutex<HashMap<ThreadId, [u8; 32]>>>,
}

impl SeedRegistry {
    /// Register a seed for the current thread
    pub fn register_seed(&self, seed: [u8; 32]) -> Result<(), SeedError> {
        let thread_id = std::thread::current().id();

        let mut registry = self.registry.lock();
        if let Some(existing_seed) = registry.get(&thread_id) {
            if *existing_seed != seed {
                SEED_COLLISION_COUNT.fetch_add(1, Ordering::Relaxed);
                warn!(thread_id = ?thread_id, "Seed collision detected");
                return Err(SeedError::CollisionDetected);
            }
        }

        registry.insert(thread_id, seed);
        Ok(())
    }
}
```

### 6.3 Async Seed Propagation

```rust
/// Propagate current thread seed to a new async task
pub fn propagate_seed_to_task<F, Fut>(f: F) -> impl Future<Output = Fut::Output>
where
    F: FnOnce() -> Fut,
    Fut: Future,
{
    let current_seed = get_thread_seed();

    async move {
        if let Some(seed) = current_seed {
            if let Err(e) = set_thread_seed(*seed.as_bytes()) {
                SEED_PROPAGATION_FAILURES.fetch_add(1, Ordering::Relaxed);
                error!("Failed to propagate thread seed to task: {}", e);
            }
        }
        f().await
    }
}
```

---

## 7. Multi-Agent Coordination

**Location:** `crates/adapteros-deterministic-exec/src/multi_agent.rs`

### 7.1 Global Tick Ledger

For distributed deployments, the global tick ledger ensures ordering across hosts:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Multi-Agent Tick Coordination                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  Agent A (Host 1)             Agent B (Host 2)             Ledger      │
│  ────────────────             ────────────────             ──────      │
│                                                                         │
│  spawn(task_1) ─────────────────────────────────────────► tick=1       │
│                               spawn(task_2) ────────────► tick=2       │
│  spawn(task_3) ─────────────────────────────────────────► tick=3       │
│                               complete(task_2) ─────────► tick=4       │
│  complete(task_1) ──────────────────────────────────────► tick=5       │
│  complete(task_3) ──────────────────────────────────────► tick=6       │
│                                                                         │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  Global Tick Ledger (Persistent)                                │   │
│  │  ──────────────────────────────────                             │   │
│  │  tick=1: TaskSpawned(task_1, agent_a, hash_1)                  │   │
│  │  tick=2: TaskSpawned(task_2, agent_b, hash_2)                  │   │
│  │  tick=3: TaskSpawned(task_3, agent_a, hash_3)                  │   │
│  │  tick=4: TaskCompleted(task_2, agent_b, hash_4)                │   │
│  │  tick=5: TaskCompleted(task_1, agent_a, hash_5)                │   │
│  │  tick=6: TaskCompleted(task_3, agent_a, hash_6)                │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                                                         │
│  Replay: Any agent can replay the ledger to verify determinism         │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 7.2 Ledger Implementation

```rust
pub struct GlobalTickLedger {
    /// Current global tick (monotonically increasing)
    global_tick: AtomicU64,
    /// Agent ID
    agent_id: String,
    /// Event storage (in-memory + persistence)
    events: RwLock<Vec<(u64, TaskId, ExecutorEvent)>>,
    /// Database connection for persistence
    db: Option<Arc<Db>>,
}

impl GlobalTickLedger {
    /// Record an event with atomically assigned tick
    pub async fn record_tick(
        &self,
        task_id: TaskId,
        event: &ExecutorEvent,
    ) -> Result<u64> {
        // Atomically get and increment global tick
        let tick = self.global_tick.fetch_add(1, Ordering::SeqCst);

        // Store event
        self.events.write().unwrap().push((tick, task_id, event.clone()));

        // Persist to database if available
        if let Some(ref db) = self.db {
            self.persist_event(db, tick, &task_id, event).await?;
        }

        Ok(tick)
    }
}
```

---

## 8. Platform Independence Guarantees

### 8.1 Cross-Platform Consistency

| Component | Guarantee | Implementation |
|-----------|-----------|----------------|
| **BLAKE3 Hashing** | Identical output | Pure Rust implementation |
| **HKDF-SHA256** | RFC 5869 compliant | `hkdf` + `sha2` crates |
| **ChaCha20 RNG** | Deterministic | `rand_chacha` crate |
| **Float Operations** | IEEE 754 strict | `-fno-fast-math` in Metal |
| **Byte Order** | Little-endian | Explicit `from_le_bytes` |

### 8.2 Metal Backend Determinism

```rust
impl FusedKernels for MetalKernels {
    fn attest_determinism(&self) -> Result<DeterminismReport> {
        let metallib_hash = B3Hash::from_hex(METALLIB_HASH.trim())?;

        Ok(DeterminismReport {
            backend_type: BackendType::Metal,
            metallib_hash: Some(metallib_hash),
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            compiler_flags: vec!["-O2".to_string(), "-std=metal3.1".to_string()],
            deterministic: true,
        })
    }
}
```

### 8.3 MLX Backend Seeding

```rust
// In adapteros-lora-mlx-ffi/src/backend.rs
impl MlxBackend {
    pub fn set_seed(&mut self, seed: u64) -> Result<()> {
        // HKDF-derived seed for MLX RNG
        let hkdf_seed = derive_seed(&self.manifest_hash, "mlx_rng");
        let seed_u64 = u64::from_le_bytes([
            hkdf_seed[0], hkdf_seed[1], hkdf_seed[2], hkdf_seed[3],
            hkdf_seed[4], hkdf_seed[5], hkdf_seed[6], hkdf_seed[7],
        ]);

        mlx_sys::set_random_seed(seed_u64);
        Ok(())
    }
}
```

---

## 9. Verification Checklist

Before deploying any inference workload, verify:

### 9.1 Seed Derivation
- [ ] Manifest hash is computed from canonical JSON (sorted keys)
- [ ] Global seed is derived: `derive_seed(&manifest_hash, "executor")`
- [ ] All component seeds derive from global seed with unique labels
- [ ] Seed registry is cleared between inference batches

### 9.2 Task Execution
- [ ] All tasks spawn through `DeterministicExecutor`
- [ ] No use of `tokio::time::sleep` (use `TickDelay` instead)
- [ ] No use of `rand::thread_rng()` (use HKDF-seeded ChaCha20)
- [ ] Event logging is enabled for audit trail

### 9.3 GPU Operations
- [ ] Metal kernels compiled with deterministic flags
- [ ] GPU fingerprints verified after adapter loads
- [ ] Cross-layer hashes computed and stored in checkpoints
- [ ] No fast-math optimizations enabled

### 9.4 Multi-Agent
- [ ] Global tick ledger connected to all agents
- [ ] Agent IDs are unique and stable
- [ ] Ledger replay produces identical event hashes

---

## 10. Troubleshooting

### 10.1 Non-Deterministic Output Detected

**Symptoms:**
- Different output for same input
- Event log hash mismatch between runs

**Debugging Steps:**
1. Verify manifest hash is identical between runs
2. Check for unseeded `rand::thread_rng()` usage
3. Search for `std::time::SystemTime` usage
4. Verify GPU backend attestation

```bash
# Check for non-deterministic code patterns
grep -r "thread_rng" crates/
grep -r "SystemTime::now" crates/
grep -r "Instant::now" crates/
```

### 10.2 Seed Collision Detected

**Symptoms:**
- `SeedError::CollisionDetected` error
- `SEED_COLLISION_COUNT` metric increasing

**Resolution:**
1. Ensure `clear_seed_registry()` called at inference boundaries
2. Check for duplicate worker_id assignments
3. Verify nonce is incrementing correctly

### 10.3 Tick Ledger Divergence

**Symptoms:**
- Global tick mismatch between agents
- Event ordering inconsistency

**Resolution:**
1. Check network latency between agents
2. Verify database persistence is working
3. Ensure atomic tick assignment (`fetch_add`)

---

## 11. Related Documentation

- [DETERMINISTIC_EXECUTION.md](DETERMINISTIC_EXECUTION.md) - Detailed execution model
- [METAL_HOTSWAP_INTEGRATION.md](METAL_HOTSWAP_INTEGRATION.md) - GPU integration
- [ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - Overall architecture
- [TELEMETRY_EVENTS.md](TELEMETRY_EVENTS.md) - Event catalog

---

**Copyright 2025 JKCA / James KC Auchterlonie. All rights reserved.**
