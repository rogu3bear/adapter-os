# Metal ↔ Hot-Swap Integration & GPU Memory Management

**Purpose:** Detailed technical guide for Metal kernel ↔ hot-swap system integration

**Last Updated:** 2025-11-21

**Status:** Production-Ready (Alpha v0.01-1)

---

## Overview

The Metal hot-swap system enables live adapter replacement on macOS without service interruption. Metal kernels interact with hot-swap through:

1. **GPU Memory Management** - Memory pooling, allocation tracking, pressure handling
2. **Adapter ID Mapping** - Deterministic BLAKE3-based u16 indexing
3. **Cross-Layer Integrity** - Metadata + GPU fingerprint verification
4. **Atomic Swaps** - Two-phase loading (preload → swap) with rollback
5. **Recovery** - Panic handling and GPU device recovery

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                     Adapter Hot-Swap System                      │
├─────────────────────────────────────────────────────────────────┤
│
│  Client Request
│       │
│       ▼
│  ┌─────────────────────────────────────────────────────┐
│  │ Router (Inference Request Accepted)                 │
│  │ - Route inference to K-sparse adapters              │
│  │ - Emit routing_decision telemetry                   │
│  └─────────────────────────────────────────────────────┘
│       │
│       ├──► [Memory Pressure Check]
│       │    └─► Trigger eviction if utilization > 85%
│       │
│       ▼
│  ┌─────────────────────────────────────────────────────┐
│  │ Adapter Lifecycle Manager                           │
│  │ - Track activation % and state transitions          │
│  │ - Promote: Cold → Warm → Hot → Resident            │
│  │ - Demote: Hot → Warm → Cold → Unloaded             │
│  └─────────────────────────────────────────────────────┘
│       │
│       ├──► [Lifecycle State Manager]
│       │    └─► Update activation_% and emit telemetry
│       │
│       ├──► [Hot-Swap Coordinator]
│       │    ├─► Preload: stage adapter into VRAM
│       │    ├─► Swap: atomic pointer flip
│       │    └─► Verify: compute cross-layer hash
│       │
│       ▼
│  ┌─────────────────────────────────────────────────────┐
│  │ Metal GPU Layer                                     │
│  │ - FusedMlpKernel, FusedQkvKernel, etc.            │
│  │ - GpuMemoryPool (buffer pooling + reuse)           │
│  │ - RecoveryWrapper (panic handling)                 │
│  └─────────────────────────────────────────────────────┘
│       │
│       ├──► [Metal Device (MTLDevice)]
│       │    └─► Command Queue (MTLCommandQueue)
│       │
│       ├──► [GpuMemoryPool]
│       │    ├─► Allocate/deallocate Metal buffers
│       │    ├─► Memory pressure monitoring
│       │    └─► Telemetry (alloc_us, dealloc_us, reuse_count)
│       │
│       ├──► [GpuFingerprint (Buffer Verification)]
│       │    ├─► adapter_id (String)
│       │    ├─► buffer_bytes (u64)
│       │    └─► checkpoint_hash: BLAKE3(first/mid/last 4KB samples)
│       │
│       └──► [RecoveryWrapper]
│            ├─► safe_dispatch() catches panics
│            └─► Mark device degraded + emit panic telemetry
│
│  ┌─────────────────────────────────────────────────────┐
│  │ Adapter Table (Double-Buffered)                     │
│  │ ┌─────────────┐      ┌─────────────┐               │
│  │ │   Active    │◄─────►│   Staged    │               │
│  │ │  Adapters   │      │  Adapters   │               │
│  │ └─────────────┘      └─────────────┘               │
│  │ • Inference reads    • Pre-load zone                │
│  │ • RwLock protected   • No refcounts yet            │
│  │                                                     │
│  │ Atomic Pointer: current_stack (generation counter) │
│  │ Rollback State: last_verified StackCheckpoint      │
│  │ Checkpoint History: Vec<StackCheckpoint> (max 20)  │
│  └─────────────────────────────────────────────────────┘
│
└─────────────────────────────────────────────────────────────────┘
```

---

## GPU Memory Management During Hot-Swap

### Memory Pool Architecture

**File:** `crates/adapteros-lora-kernel-mtl/src/gpu_memory_pool.rs`

```rust
GpuMemoryPool {
    // Configuration
    max_pooled_memory: 512 MB (default)
    pressure_threshold: 0.85 (85% utilization)
    target_headroom: 0.15 (15% free after cleanup)
    idle_timeout_secs: 60 (cleanup stale buffers)

    // State
    pooled_buffers: HashMap<u64_size_key, VecDeque<PooledGpuBuffer>>
    total_pooled_bytes: AtomicU64
    allocation_id_counter: AtomicU64
}

PooledGpuBuffer {
    buffer: Metal::Buffer
    size: u64
    last_accessed: Instant
    reuse_count: u32         // Metric: how many times reused
    allocation_id: u64       // Unique ID for tracking
}
```

### Memory Pressure Flow

```
┌─────────────────────────────────────────────────────────────┐
│ Step 1: Monitor Memory Utilization (Continuous)             │
│ - get_total_reserved_bytes() from Metal device              │
│ - Calculate pressure = current / total_available            │
│ - If pressure > threshold (85%), trigger cleanup            │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 2: Evict Stale Adapters (Lifecycle Manager)            │
│ - Find adapters with activation % < threshold               │
│ - Mark lowest activation_% as eviction candidate            │
│ - Emit adapter_eviction telemetry                           │
│ - Remove from lifecycle tracking                            │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 3: Deallocate Metal Buffers (GpuMemoryPool)            │
│ - Remove oldest buffers from pooled_buffers queues          │
│ - Release buffer.release() to Metal device                  │
│ - Decrement total_pooled_bytes                              │
│ - Emit memory_freed telemetry                               │
│ - Continue until pressure <= target (70%)                   │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 4: Verify Headroom (Post-Cleanup)                      │
│ - Confirm pressure now <= target_headroom (15%)             │
│ - If still high, escalate to operator alert                 │
│ - Log cleanup metrics for observability                      │
└─────────────────────────────────────────────────────────────┘
```

### Pre-Swap Memory Validation

Before `swap()` is called, the hot-swap coordinator:

1. **Check Available VRAM**
   ```
   total_needed_mb = sum(adapter.vram_mb for adapter in add_ids)
   available_mb = device.memory_size - current_usage
   if total_needed_mb > available_mb:
       return Err(AosError::Memory("Insufficient VRAM for swap"))
   ```

2. **Mark Adapters for Allocation**
   - Lock GPU memory pool
   - Reserve buffer space (atomic increment of refcount)
   - If reservation fails, release and return error

3. **Staged Load Check**
   - Verify staged adapters have GPU memory backing
   - Cross-reference Metal buffer allocation IDs
   - Ensure no orphaned/dangling pointers

---

## Adapter ID Mapping: BLAKE3 → u16

### Deterministic Indexing

**File:** `crates/adapteros-lora-worker/src/adapter_hotswap.rs`

```rust
/// Map adapter ID string to u16 using BLAKE3 hash (first 2 bytes)
pub fn adapter_id_to_u16(adapter_id: &str) -> u16 {
    let hash = B3Hash::hash(adapter_id.as_bytes());
    let bytes = hash.to_bytes();
    u16::from_le_bytes([bytes[0], bytes[1]])
}
```

### Properties & Guarantees

| Property | Value | Justification |
|----------|-------|---------------|
| **Determinism** | ✓ Guaranteed | BLAKE3 is stable across runs/platforms |
| **Collision Rate (16-bit)** | ~65k unique IDs | Birthday paradox: 2^16 = 65,536 addresses |
| **Hash Algorithm** | BLAKE3 | Cryptographic + fast (200MB/s) |
| **Endianness** | Little-endian (LE) | Matches Metal buffer layout conventions |

### Collision Handling

```rust
// In hot-swap swap() function
for id in add_ids {
    let adapter_idx = adapter_id_to_u16(&id);

    // Check for collisions in current active set
    if let Some(existing) = active.get(&id) {
        if existing.hash != new_hash {
            error!(
                adapter_id = %id,
                adapter_idx = adapter_idx,
                expected_hash = %new_hash,
                actual_hash = %existing.hash,
                "Adapter hash mismatch detected"
            );
            return Err(AosError::Worker(
                "Adapter ID collision or hash mismatch".into()
            ));
        }
    }
}
```

### Use Cases

1. **Router K-Sparse Selection** - Map adapter ID → gate weight index
2. **GPU Buffer Fingerprinting** - Map adapter ID → buffer checkpoint location
3. **Telemetry Correlation** - Map adapter ID → numeric ID in structured events
4. **Deterministic Seeding** - Use u16 as domain separator in HKDF

---

## Cross-Layer Integrity Verification

### Stack Hash Types

**Metadata Hash** (Lightweight - Fast Path)
```
metadata_hash = BLAKE3(adapter_id_1 || adapter_id_2 || ... || adapter_id_n)
- Used for quick cache invalidation
- Computed at adapter load time
- Deterministic, no GPU state needed
```

**Cross-Layer Hash** (Full - Slow Path)
```
cross_layer_hash = BLAKE3(
    metadata_hash ||
    GPU_fingerprint_1.checkpoint_hash ||
    GPU_fingerprint_2.checkpoint_hash ||
    ...
)
- Includes GPU buffer state verification
- Computed post-swap (verify phase)
- Detects memory corruption or buffer overwrites
```

### Verification Sequence

```rust
// Step 1: Compute metadata hash (quick)
let metadata_hash = compute_stack_hash(&adapter_ids)?;

// Step 2: Collect GPU fingerprints from Metal buffers
let gpu_fingerprints = vec![];
for (adapter_id, adapter_state) in &new_active {
    let fingerprint = GpuFingerprint {
        adapter_id: adapter_id.clone(),
        buffer_bytes: adapter_state.vram_mb * 1024 * 1024,
        checkpoint_hash: sample_gpu_buffer_hash(&adapter_id)?,
    };
    gpu_fingerprints.push(fingerprint);
}

// Step 3: Compute cross-layer hash
let cross_layer_hash = compute_cross_layer_hash(
    &metadata_hash,
    &gpu_fingerprints
)?;

// Step 4: Store checkpoint
let checkpoint = StackCheckpoint {
    timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    metadata_hash,
    cross_layer_hash: Some(cross_layer_hash),
    gpu_fingerprints,
    adapter_ids: adapter_ids.clone(),
};

// Step 5: Compare to previous for anomaly detection
if let Some(prev_checkpoint) = checkpoints.last() {
    if prev_checkpoint.cross_layer_hash != Some(cross_layer_hash) {
        warn!(
            "Cross-layer hash changed between checkpoints",
            previous = %prev_checkpoint.cross_layer_hash.unwrap(),
            current = %cross_layer_hash,
        );
        // Emit determinism_violation telemetry
    }
}

checkpoints.push(checkpoint);
```

### GPU Fingerprint Sampling

```rust
/// Sample GPU buffer at three points: first, middle, last 4KB chunks
fn sample_gpu_buffer_hash(adapter_id: &str) -> Result<B3Hash> {
    let buffer = get_gpu_buffer(adapter_id)?;
    let size = buffer.length();

    let mut samples = Vec::new();

    // First 4KB
    samples.extend_from_slice(&buffer.contents()[0..4096].to_owned());

    // Middle 4KB
    let mid = (size / 2) as usize;
    samples.extend_from_slice(&buffer.contents()[mid..mid+4096].to_owned());

    // Last 4KB
    samples.extend_from_slice(&buffer.contents()[size-4096..size].to_owned());

    Ok(B3Hash::hash(&samples))
}
```

---

## Hot-Swap Sequence Diagrams

### Normal Two-Phase Swap

```
Timeline:

t=0   Inference Request
      ├─ Route → select K=2 adapters: "adapter-a", "adapter-b"
      ├─ Check lifecycle states
      └─ If either unloaded → preload
              │
              ▼
t=1   Preload Phase (Async)
      ├─ Read adapter weights from disk
      ├─ Allocate Metal buffers (GpuMemoryPool)
      ├─ Upload to GPU VRAM
      ├─ Move to staged.write()
      └─ Emit adapter_loaded telemetry
              │
              ▼
t=2   Swap Phase (Atomic)
      ├─ Save current stack → rollback_state
      ├─ Lock active.write()
      ├─ Remove specified adapters
      ├─ Move staged adapters → active
      ├─ Increment current_stack generation
      ├─ Release active lock
      └─ Vram_delta = +200MB (added) - 0MB (removed)
              │
              ▼
t=3   Verify Phase
      ├─ Compute metadata_hash(adapter_ids)
      ├─ Sample GPU fingerprints (3 * 4KB per adapter)
      ├─ Compute cross_layer_hash
      ├─ Store StackCheckpoint
      ├─ Emit swap_completed telemetry
      └─ Return (vram_delta, duration)
              │
              ▼
t=4   Inference Execution
      ├─ Read from active.read() (RwLock)
      ├─ Access Metal buffers via adapter_id → u16 mapping
      ├─ Run FusedMlpKernel, FusedQkvKernel
      ├─ RecoveryWrapper catches panics
      └─ Write output to result buffer
              │
              ▼
t=5   RCU Cleanup (Background)
      ├─ Decrement refcount for removed adapters
      ├─ When refcount → 0, retire stack
      ├─ Release Metal buffers back to pool
      ├─ Check memory pressure
      └─ Emit cleanup_complete telemetry
```

### Failure & Rollback Sequence

```
Timeline:

t=0   Swap Initiated
      ├─ Intent: add "adapter-new", remove "adapter-old"
      ├─ VRAM available: 400MB, needed: 350MB ✓
      └─ Preload successful
              │
              ▼
t=1   Swap Execution
      ├─ Save current stack → rollback_state
      ├─ Begin pointer update
      ├─ GPU fingerprint sampling starts
      │   └─ First 4KB sampled ✓
      │   └─ Middle 4KB sampled ✓
      │   └─ Last 4KB sample FAILS (Buffer corrupted? Adapter unloaded?)
      │
      ├─ Emit gpu_integrity_violation telemetry
      └─ Return Err(AosError::Worker("GPU fingerprint mismatch"))
              │
              ▼
t=2   Rollback Triggered
      ├─ Restore active from rollback_state
      ├─ Revert current_stack generation
      ├─ Clear staged adapters
      ├─ Release "adapter-new" Metal buffers
      └─ Emit swap_rollback telemetry
              │
              ▼
t=3   Recovery Actions
      ├─ Log error in crash journal
      ├─ Alert operator (if automated monitoring enabled)
      ├─ Continue inference with previous adapter stack
      ├─ Mark adapter_new as "requires_reinspection"
      └─ Retry swap with same adapters (exponential backoff)
```

### Concurrent Inferences During Swap

```
Thread A (Inference #1)        Thread B (Hot-Swap)         Thread C (Inference #2)
     │                               │                            │
     ├─ Lock active.read() ──┐       │                            │
     │  (acquire snapshot)   │       │                            │
     │                       │       ├─ Wait for active.write() ◄──┤ Spinning on read()
     │                       │       │                            │
     ├─ Execute kernels ────┤       │                            │
     │ (10ms)               │       │                            │
     │                      │       ├─ Get write lock ◄──────────┤ [queue]
     │                      │       │  add/remove ops           │
     │                      │       │                            │
     │                      │       ├─ Unlock write lock ──────────► read() acquires lock
     │                      │       │  (generation++)           │  (new snapshot)
     │                      │       │                           │
     ├─ Unlock read() ◄─────┘       │                           │
     │                              │                           │
     └─ Results ready               │                           ├─ Execute on NEW adapters
                                    │                           │
                                    └─ Verify complete         │
                                                                │
                                                                └─ Results ready
```

**Key Guarantee:** RwLock ensures readers never see a partial swap. Each inference sees a consistent adapter stack.

---

## Error Recovery Flows

### Panic Recovery (Metal Kernel Crash)

**File:** `crates/adapteros-lora-kernel-mtl/src/recovery.rs`

```rust
pub struct RecoveryWrapper {
    degraded: bool,           // Device requires recovery
    panic_count: usize,       // Total panics seen
    recovery_count: usize,    // Successful recoveries
    last_recovery_timestamp: Option<Instant>,
}

impl RecoveryWrapper {
    pub fn safe_dispatch<F, T>(&mut self, f: F) -> Result<T>
    where F: FnOnce() -> Result<T> + UnwindSafe
    {
        match catch_unwind(AssertUnwindSafe(f)) {
            Ok(result) => result,
            Err(panic_err) => {
                self.degraded = true;
                self.panic_count += 1;
                error!(
                    panic_count = self.panic_count,
                    "Metal kernel panic caught"
                );
                Err(AosError::Kernel("GPU kernel panic".into()))
            }
        }
    }

    pub fn recover(&mut self, device: &Device) -> Result<RecoveryResult> {
        if !self.degraded {
            return Err(AosError::Kernel("Device not degraded".into()));
        }

        // 1. Release old command queue resources
        // 2. Clear held buffer references

        // 3. Create fresh command queue
        let new_queue = device.new_command_queue();

        // 4. Verify with test dispatch
        let test_buffer = device.new_buffer(1024, MTLResourceOptions::Managed);
        let cmd = new_queue.new_command_buffer();
        cmd.commit();
        cmd.wait_until_completed();

        self.degraded = false;
        self.recovery_count += 1;
        self.last_recovery_timestamp = Some(Instant::now());

        Ok(RecoveryResult {
            command_queue: new_queue,
            test_dispatch_us: 0,
        })
    }
}
```

**Recovery Timeline:**
```
[Panic Caught]
      │
      ├─ Emit kernel_panic telemetry
      ├─ Set degraded = true
      ├─ Increment panic_count
      │
      ▼
[Degraded State - No Inference]
      │
      ├─ Operator monitors degraded alert
      ├─ Issues: POST /v1/workers/{worker_id}/recover
      │
      ▼
[Recovery Initiated]
      │
      ├─ Release MTLCommandQueue
      ├─ Clear buffer refcounts
      ├─ Create new queue from device
      ├─ Test dispatch (1KB buffer write + read)
      │
      ▼
[Recovery Success]
      │
      ├─ Set degraded = false
      ├─ Increment recovery_count
      ├─ Emit recovery_complete telemetry
      ├─ Resume inference
      │
      └─ Monitor for repeat panics (circuit breaker)
```

### Buffer Overflow / Corruption Detection

```rust
/// Monitor GPU buffer integrity during inference
fn verify_buffer_integrity(
    buffer: &Metal::Buffer,
    expected_hash: &B3Hash,
    adapter_id: &str
) -> Result<()> {
    // Sample buffer at checkpoints
    let actual_hash = sample_gpu_buffer_hash_quick(buffer)?;

    if actual_hash != expected_hash {
        error!(
            adapter_id = %adapter_id,
            expected = %expected_hash,
            actual = %actual_hash,
            "GPU buffer integrity violation"
        );

        // Emit determinism_violation telemetry
        emit_event(DeterminismViolationEvent {
            violation_type: "gpu_buffer_corruption".into(),
            adapter_id: adapter_id.to_string(),
            details: format!("Hash mismatch: {} vs {}", expected_hash, actual_hash),
        });

        return Err(AosError::DeterminismViolation(
            "GPU buffer corrupted".into()
        ));
    }

    Ok(())
}
```

---

## Integration Points

### 1. Lifecycle Manager → Hot-Swap

```rust
// crates/adapteros-lora-lifecycle/src/lib.rs
impl LifecycleManager {
    pub async fn record_router_decision(&self, selected_ids: &[String]) -> Result<()> {
        for id in selected_ids {
            // Increment activation counter
            self.activation_tracker.increment(id).await?;

            // Check if Cold → preload needed
            let state = self.get_state(id).await?;
            if state == LifecycleState::Cold {
                // Signal hot-swap coordinator: preload this adapter
                self.hotswap_tx.send(AdapterCommand::Preload {
                    adapter_id: id.clone(),
                    hash: self.get_hash(id).await?,
                }).await?;
            }
        }
        Ok(())
    }

    pub async fn check_memory_pressure(&self, total_usage: u64, threshold: f32) -> Result<()> {
        let pressure = total_usage as f32 / self.total_vram as f32;

        if pressure > threshold {
            // Find lowest activation_% adapter
            let eviction_candidate = self.activation_tracker.find_lowest().await?;

            // Signal hot-swap: remove this adapter
            self.hotswap_tx.send(AdapterCommand::Swap {
                add_ids: vec![],
                remove_ids: vec![eviction_candidate],
            }).await?;
        }
        Ok(())
    }
}
```

### 2. Hot-Swap → Router

```rust
// crates/adapteros-lora-router/src/lib.rs
pub struct Router {
    active_adapters: Arc<RwLock<HashMap<String, u16>>>, // ID → u16 index
    adapter_table: Arc<AdapterTable>,  // Hot-swap backing store
}

impl Router {
    pub async fn select_adapters(&self, k: usize, query_emb: &[f32]) -> Result<Vec<String>> {
        // Ensure adapters are loaded (trigger hot-swap if needed)
        let active = self.adapter_table.get_active().await?;

        // Compute Q15 gates for each active adapter
        let mut scores = vec![];
        for (adapter_id, _state) in active.iter() {
            let gate_value = self.compute_gate(adapter_id, query_emb).await?;
            scores.push((adapter_id.clone(), gate_value));
        }

        // Select top-K
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        Ok(scores.iter().take(k).map(|(id, _)| id.clone()).collect())
    }
}
```

### 3. Inference → Metal Kernels

```rust
// crates/adapteros-lora-worker/src/backend_factory.rs
pub async fn forward_with_hotswap(
    router_selected: Vec<String>,
    input: &[f32],
) -> Result<Vec<f32>> {
    // Get current active adapters (snapshot)
    let adapters = adapter_table.get_active().await?;

    // Map to u16 indices
    let adapter_indices: Vec<u16> = router_selected
        .iter()
        .map(|id| adapter_id_to_u16(id))
        .collect();

    // Get Metal buffers for selected adapters
    let buffers = vec![];
    for (idx, adapter_id) in router_selected.iter().enumerate() {
        let adapter = adapters.get(adapter_id)
            .ok_or(AosError::NotFound("Adapter removed during inference".into()))?;
        buffers.push(get_metal_buffer(adapter_id)?);
    }

    // Run kernels with panic recovery
    let result = recovery_wrapper.safe_dispatch(|| {
        let kernel = FusedMlpKernel::new(&buffers)?;
        kernel.forward(input)
    })?;

    // Verify GPU buffer integrity post-inference
    for (buffer, adapter_id) in buffers.iter().zip(&router_selected) {
        verify_buffer_integrity(buffer, &adapters[adapter_id].hash, adapter_id)?;
    }

    Ok(result)
}
```

---

## Production Monitoring & Alerts

### Key Metrics to Track

| Metric | Type | Alert Threshold | Runbook |
|--------|------|-----------------|---------|
| `metal_kernel_execution_time_us` | Histogram (p50, p95, p99) | p99 > 50ms | [Kernel Performance](#) |
| `hotswap_latency_ms` | Gauge | > 100ms | [Hot-Swap Latency](#) |
| `gpu_memory_pressure` | Gauge (0-1) | > 0.85 | [Memory Pressure](#) |
| `adapter_id_collisions_total` | Counter | > 0 | [ID Collision](#) |
| `gpu_panic_count_total` | Counter | > 3 in 5min | [GPU Panic](#) |
| `determinism_violations_total` | Counter | > 0 | [Determinism](#) |
| `buffer_corruption_detections` | Counter | > 0 | [Corruption](#) |
| `swap_rollback_count` | Counter | > 5% of swaps | [Swap Failures](#) |

### Telemetry Events

**Hot-Swap Events:**
```json
{
  "event_type": "adapter_swap_completed",
  "adapter_ids": ["adapter-a", "adapter-b"],
  "vram_delta_mb": 200,
  "duration_ms": 45,
  "cross_layer_hash": "blake3_hash_hex",
  "timestamp": 1700000000000
}

{
  "event_type": "swap_rollback",
  "reason": "gpu_fingerprint_mismatch",
  "adapter_ids": ["adapter-new"],
  "rollback_to_generation": 42,
  "timestamp": 1700000000000
}
```

**GPU Events:**
```json
{
  "event_type": "gpu_panic_caught",
  "adapter_id": "adapter-a",
  "adapter_idx": 12345,
  "panic_count": 3,
  "recovery_count": 2,
  "last_recovery_ms": 25,
  "timestamp": 1700000000000
}

{
  "event_type": "gpu_buffer_integrity_violation",
  "adapter_id": "adapter-b",
  "buffer_bytes": 104857600,
  "expected_hash": "blake3_hash_1",
  "actual_hash": "blake3_hash_2",
  "timestamp": 1700000000000
}
```

---

## Configuration Reference

### Hot-Swap Configuration (config.toml)

```toml
[hotswap]
# Maximum adapters in active set
max_active_adapters = 16

# Preload timeout (seconds)
preload_timeout_secs = 30

# Checkpoint history limit
max_checkpoints = 20

# RCU retirement check interval (milliseconds)
rcu_check_interval_ms = 100

# Enable cross-layer integrity verification
verify_cross_layer = true

[gpu_memory]
# Pool configuration
max_pooled_memory_mb = 512
max_buffers_per_bucket = 16
idle_timeout_secs = 60

# Pressure handling
pressure_threshold = 0.85
target_headroom = 0.15

# Recovery
panic_recovery_enabled = true
max_panics_before_offline = 5
recovery_timeout_secs = 10
```

---

## Troubleshooting Guide

### Scenario: Hot-Swap Latency > 100ms

**Check:**
1. GPU memory pool fragmentation
   ```rust
   let stats = memory_pool.get_stats().await?;
   println!("Fragmentation ratio: {}", stats.fragmentation_ratio);
   ```

2. RCU retirement backlog
   ```rust
   let retired = adapter_table.get_retired_stacks_count().await?;
   if retired > 10 {
       warn!("RCU backlog building up: {} stacks", retired);
   }
   ```

3. Preload disk I/O
   - Check disk throughput: `iostat -d 1`
   - Profile: `cargo flamegraph --bin worker -- --profile hotswap`

### Scenario: GPU Memory Pressure > 85%

**Immediate Actions:**
1. Trigger adapter eviction via lifecycle manager
2. Check for pinned adapters blocking eviction
3. Verify GpuMemoryPool cleanup is running

**Diagnostic:**
```bash
aosctl metrics get gpu_memory_pressure
aosctl metrics get adapter_memory_breakdown
aosctl db query "SELECT adapter_id, vram_mb FROM adapters WHERE tenant_id = ?"
```

### Scenario: Determinism Violation Detected

**Check:**
1. Cross-layer hash mismatch after swap
   ```
   expected_cross_layer_hash: abc123def456...
   actual_cross_layer_hash: xyz789mno456...
   ```

2. GPU buffer corruption signs
   - Bit flips in fingerprint samples (corruption detector)
   - Memory read errors in Metal command queue

3. Concurrent access issues
   - Verify RwLock is protecting adapter_table
   - Check for manual unsafe code in kernel dispatch

**Recovery:**
```bash
aosctl worker recover --worker-id <id>
aosctl adapter reload --adapter-id <id>
```

---

## Best Practices

1. **Always use `adapter_id_to_u16()` for consistent mapping**
   - Never use Rust's DefaultHasher (platform-dependent)
   - Never hardcode u16 values

2. **Verify GPU fingerprints post-swap**
   - Sample at 3 locations (first, mid, last)
   - Use BLAKE3 for fast 256-bit hash
   - Store in StackCheckpoint for audit trail

3. **Monitor RCU retirement**
   - Set alerts on retired_stacks.len() > 50
   - Check memory pressure after large evictions
   - Log all manual retire operations

4. **Enable panic recovery in production**
   - `panic_recovery_enabled = true`
   - Set max_panics_before_offline to circuit-break
   - Monitor recovery_count in metrics

5. **Plan for memory fragmentation**
   - Pre-size GpuMemoryPool: `max_pooled_memory_mb = 0.7 * total_vram_mb`
   - Set aggressive idle timeout: `idle_timeout_secs = 30`
   - Monitor pool fragmentation ratio

---

## References

- [docs/ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - Overall patterns
- [docs/LIFECYCLE.md](LIFECYCLE.md) - Lifecycle state machine
- [crates/adapteros-lora-kernel-mtl/src/lib.rs](../crates/adapteros-lora-kernel-mtl/src/lib.rs) - Metal kernel impl
- [crates/adapteros-lora-worker/src/adapter_hotswap.rs](../crates/adapteros-lora-worker/src/adapter_hotswap.rs) - Hot-swap impl
- [docs/OBJECTIVE_CPP_FFI_PATTERNS.md](OBJECTIVE_CPP_FFI_PATTERNS.md) - Metal FFI patterns

---

**Last Reviewed:** 2025-11-21
**Maintained by:** James KC Auchterlonie
