# Hot-Swap API Contract Documentation

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Purpose:** API contract reference for adapter hot-swapping subsystem
**Last Updated:** 2025-01-18
**Maintained by:** James KC Auchterlonie

---

## Executive Summary

This document specifies the API contracts for AdapterOS's hot-swap subsystem, which enables live adapter replacement with zero downtime, automatic rollback, and cross-layer integrity verification. The contracts span three major components:

1. **AdapterTable** (Lifecycle Layer) - Double-buffered state machine for atomic swaps
2. **Router** (Selection Layer) - K-sparse adapter selection with stack filtering
3. **FusedKernels** (GPU Layer) - Metal kernel interface for GPU buffer management

**Critical Invariant:** All three layers must maintain consistency via content-addressed hashing (BLAKE3) and generation counters to prevent divergence.

---

## 1. AdapterTable API Contract

**Location:** `crates/adapteros-lora-worker/src/adapter_hotswap.rs:126-741`

### 1.1 Core Responsibilities

The `AdapterTable` implements RCU-style double-buffering for safe concurrent hot-swaps:

- **Preload Stage**: Load adapters into staging area without activating
- **Atomic Swap**: Mutex-guarded pointer flip with automatic rollback on failure
- **RCU Retirement**: Deferred unload when reference counts reach zero
- **Cross-Layer Hashing**: Combine metadata and GPU fingerprints for integrity

### 1.2 Public Functions

#### `preload(id: String, hash: B3Hash, vram_mb: u64) -> Result<()>`

**Lines:** 186-213
**Responsibility:** Stage adapter in preparation for swap

**Preconditions:**
- `id` must not already exist in `staged` map
- `vram_mb` must be > 0

**Postconditions:**
- Adapter added to `staged` HashMap
- Refcount entry initialized to 0
- Adapter state marked as `active: false`

**Errors:**
- `AosError::Worker` - Adapter already staged

**Invariant Maintained:** Staged adapters have refcount entries but are not counted in active generation.

---

#### `swap(add_ids: &[String], remove_ids: &[String]) -> Result<(i64, usize)>`

**Lines:** 215-304
**Responsibility:** Atomically swap adapter sets with automatic rollback

**Preconditions:**
- All `add_ids` must exist in `staged` map
- `remove_ids` entries may or may not be in active set (graceful handling)

**Postconditions:**
- **Success Path:**
  - Old state saved to `rollback_state`
  - Active adapters updated (`removed` items deleted, `added` items inserted)
  - Generation counter incremented (`old_gen + 1`)
  - Old stack moved to `retired_stacks` if generation changed
  - Returns `(vram_delta, added_count)`
- **Failure Path:**
  - Automatic rollback to `rollback_state` generation
  - Staged map cleared
  - Returns `Err(AosError::Worker)`

**Errors:**
- `AosError::Worker` - Missing staged adapter (triggers rollback)

**Critical Invariants:**
1. **Generation Monotonicity:** `new_gen = old_gen + 1` (strictly increasing, no duplicates)
2. **Atomicity:** Pointer flip via `AtomicUsize::swap(new_gen, Ordering::AcqRel)` is atomic
3. **Rollback Safety:** Rollback state captured BEFORE any mutations

**Concurrency Contract:**
- Swap operations are **serialized** (mutex held during critical section)
- Readers (inference) can proceed concurrently using old generation via refcounts
- No UAF: retired stacks remain valid until `refcount == 0`

---

#### `rollback() -> Result<()>`

**Lines:** 306-337
**Responsibility:** Revert to last verified state

**Preconditions:**
- `rollback_state` must be `Some(Arc<Stack>)` (not None)

**Postconditions:**
- `current_stack` restored to rollback generation
- Old stack moved to `retired_stacks`
- `rollback_state` cleared (set to None)
- Stack hash logged for verification

**Errors:**
- `AosError::Worker` - No rollback state available

**Invariant:** Rollback generation must be < current generation to avoid time travel.

---

#### `compute_stack_hash() -> B3Hash`

**Lines:** 339-354
**Responsibility:** Compute metadata-only integrity hash

**Algorithm:**
1. Collect `(adapter_id, .aos hash)` pairs from active adapters
2. Sort by `adapter_id` (deterministic ordering)
3. Delegate to `adapteros_core::compute_stack_hash(pairs)`

**Properties:**
- **Deterministic:** Same active set → same hash (order-independent via sort)
- **Collision-Resistant:** Uses BLAKE3
- **Metadata-Only:** Does NOT include GPU buffer state (see `compute_cross_layer_hash` for that)

---

#### `compute_cross_layer_hash(gpu_fingerprints: &[GpuFingerprint]) -> B3Hash`

**Lines:** 356-392
**Responsibility:** Combine metadata + GPU fingerprints for full integrity

**Algorithm:**
1. Sort active adapters by ID
2. Hash: `adapter_id || .aos_hash` for each adapter
3. Sort GPU fingerprints by `adapter_id`
4. Hash: `adapter_id || buffer_bytes || checkpoint_hash` for each fingerprint
5. Return BLAKE3 of concatenated hash

**Purpose:** Detect divergence between lifecycle state (metadata) and GPU state (buffers).

**Open Issue:** GPU fingerprints are optional - if empty, falls back to metadata-only hash. This creates a silent downgrade path that could mask integrity violations.

---

#### `create_checkpoint(gpu_fingerprints: Vec<GpuFingerprint>) -> StackCheckpoint`

**Lines:** 394-432
**Responsibility:** Create snapshot for replay verification

**Postconditions:**
- Checkpoint contains: `timestamp`, `metadata_hash`, `cross_layer_hash`, `gpu_fingerprints`, `adapter_ids`
- Checkpoint added to ring buffer (max 20 retained)
- Oldest checkpoint evicted if limit exceeded

**Use Case:** Crash recovery, determinism verification, audit trails

---

#### `process_retired_stacks<K>(kernels_opt: Option<Arc<Mutex<K>>>) -> Result<()>`

**Lines:** 611-720
**Responsibility:** RCU grace period enforcement and GPU unload

**Algorithm:**
1. Lock `retired_stacks`
2. For each retired stack:
   - Check if all adapters have `refcount == 0`
   - If yes:
     - Call `kernels.unload_adapter(id_u16)` for each adapter
     - Remove from `retired_stacks` on success
     - **Quarantine after 3 retries** (prevents infinite loops)
   - If no: Skip to next

**Retry Invariant (Lines 656-680):**
- Each retired stack gets ≤ 3 unload attempts
- After 3 failures: log error, emit telemetry, remove from queue (quarantine)
- Prevents infinite retry loops under persistent kernel failures

**Errors:**
- Logs warnings on unload failure (does not propagate error)
- Quarantined stacks require manual intervention

**Open Issue:** No alerting mechanism for quarantined stacks. Operators must monitor telemetry events.

---

### 1.3 Internal State Machine

```
┌─────────────────────────────────────────────────────────┐
│                    AdapterTable State                    │
├─────────────────────────────────────────────────────────┤
│ active:           RwLock<HashMap<String, AdapterState>> │ ← Readers
│ staged:           RwLock<HashMap<String, AdapterState>> │ ← Writers preload
│ rollback_state:   RwLock<Option<Arc<Stack>>>            │ ← Swap checkpoint
│ current_stack:    AtomicUsize (generation counter)      │ ← Atomic pointer
│ retired_stacks:   Mutex<Vec<Arc<Stack>>>                │ ← RCU queue
│ refcounts:        Mutex<HashMap<String, AtomicUsize>>   │ ← Pin tracking
└─────────────────────────────────────────────────────────┘
```

**State Transitions:**
1. **Staged → Active** via `swap()` (atomic pointer flip)
2. **Active → Retired** when generation advances
3. **Retired → Unloaded** when `refcount == 0` (via `process_retired_stacks`)

---

## 2. Router API Contract

**Location:** `crates/adapteros-lora-router/src/lib.rs:144-1135`

### 2.1 Core Responsibilities

The `Router` selects top-K adapters based on weighted feature scores and enforces stack filtering:

- **Stack Filtering:** Restrict routing to adapters in the active stack
- **K-Sparse Selection:** Select ≤ K adapters with highest scores
- **Q15 Quantization:** Convert float gates to i16 for deterministic Metal execution
- **Entropy Floor:** Prevent gate collapse (min gate = eps / k)

### 2.2 Stack Management API

#### `set_active_stack(stack_name: Option<String>, adapter_ids: Option<Vec<String>>, stack_hash: Option<B3Hash>)`

**Lines:** 345-360
**Responsibility:** Configure router to filter by stack membership

**Preconditions:**
- If `adapter_ids` is Some, must contain valid adapter IDs

**Postconditions:**
- `active_stack_adapter_ids` updated (used by `filter_by_stack`)
- `active_stack_hash` cached for telemetry correlation
- Logs debug message with adapter count

**Use Case:** When HotSwapManager swaps to a new stack, it calls this to restrict routing.

---

#### `filter_by_stack(adapter_info: &[AdapterInfo]) -> Vec<usize>`

**Lines:** 373-393
**Responsibility:** Return indices of adapters in active stack

**Logic:**
- If `active_stack_adapter_ids` is None: return all indices (no filtering)
- If Some: return indices where `adapter_info[i].id` is in allowed set

**Invariant:** Filtered indices are always a subset of input adapter_info indices.

---

#### `route_with_code_features(code_features: &CodeFeatures, adapter_info: &[AdapterInfo]) -> Decision`

**Lines:** 936-998
**Responsibility:** Route with stack filtering + code-based priors

**Algorithm:**
1. Call `filter_by_stack(adapter_info)` → `allowed_indices`
2. Set `priors[i] = 0.0` for adapters NOT in allowed set
3. Boost priors for framework/language matches
4. Call `route(feature_vec, &priors)` with zeroed-out priors for excluded adapters

**Critical Invariant:** Excluded adapters get `prior = 0.0`, ensuring they score below threshold.

**Open Issue:** What happens if ALL adapters are excluded? This triggers k0 detection (lines 806-916), but returns empty Decision. Callers must handle empty Decision gracefully.

---

### 2.3 Decision Structure

```rust
pub struct Decision {
    pub indices: SmallVec<[u16; 8]>,      // Selected adapter indices (K max)
    pub gates_q15: SmallVec<[i16; 8]>,    // Q15 gates (int16 for Metal)
    pub entropy: f32,                      // Shannon entropy of gate distribution
    pub candidates: Vec<DecisionCandidate>,// Full candidate list with raw scores
}
```

**Conversion to RouterRing (Lines 1113-1120):**
- Decision → `adapteros_lora_kernel_api::RouterRing` via `to_router_ring()`
- Asserts 1:1 mapping between indices and gates
- Asserts K ≤ 8 (MAX_K limit)

---

## 3. FusedKernels API Contract

**Location:** `crates/adapteros-lora-kernel-api/src/lib.rs:94-212`

### 3.1 Core Responsibilities

The `FusedKernels` trait abstracts GPU kernel operations with determinism guarantees:

- **Adapter Loading:** `load_adapter()` - Load .aos weights into VRAM
- **GPU Verification:** `verify_adapter_buffers()` - Sample buffers for fingerprinting
- **Hot-Swap Support:** `unload_adapter()` - Remove adapter from VRAM
- **Determinism Attestation:** `attest_determinism()` - Prove backend is deterministic

### 3.2 Hot-Swap Methods

#### `load_adapter(id: u16, weights: &[u8]) -> Result<()>`

**Lines:** 114-121 (default), full impl in Metal backend
**Responsibility:** Load adapter weights into GPU buffers

**Preconditions:**
- `weights` must be valid SafeTensors payload (not .aos archive)
- `id` must be unique (no duplicate loads without unload)

**Postconditions (Metal Backend):**
- Weights uploaded to Metal buffers
- VRAM tracker updated with `(id, buffer_size)`
- GPU fingerprint computed and stored

**Errors:**
- Default impl: `AosError::Kernel("Hot-swap not supported")`
- Metal impl: GPU allocation errors, invalid SafeTensors

**Open Issue:** No explicit check for duplicate `id`. Overwriting an existing adapter could leak VRAM.

---

#### `unload_adapter(id: u16) -> Result<()>`

**Lines:** 123-130 (default), full impl in Metal backend
**Responsibility:** Release GPU buffers for adapter

**Preconditions:**
- Adapter `id` must exist in GPU (graceful no-op if missing)

**Postconditions:**
- Metal buffers deallocated
- VRAM tracker entry removed
- GPU fingerprint removed

**Errors:**
- Default impl: `AosError::Kernel("Hot-swap not supported")`

**Open Issue:** What if adapter is in-flight (refcount > 0)? No explicit check. Caller (AdapterTable) must enforce RCU protocol.

---

#### `verify_adapter_buffers(id: u16) -> Result<(u64, Vec<u8>, Vec<u8>, Vec<u8>)>`

**Lines:** 133-150
**Responsibility:** Sample GPU buffers for fingerprinting

**Returns:**
- `buffer_size` - Total Metal buffer bytes
- `first_sample` - First 4KB of buffer (or less if smaller)
- `last_sample` - Last 4KB of buffer
- `mid_sample` - Midpoint 4KB of buffer

**Algorithm (Conceptual):**
1. Read GPU buffer metadata (size, pointer)
2. Copy 3 checkpoints from VRAM to CPU (4KB each)
3. Return samples for BLAKE3 hashing

**Purpose:** Enables integrity verification WITHOUT full buffer readback (3×4KB vs. 100MB+).

**Open Issue:** If buffer < 12KB, samples may overlap. Hash will still be unique but may be less collision-resistant.

---

#### `store_gpu_fingerprint(id: u16, buffer_size: u64, checkpoint_hash_hex: &str)`

**Lines:** 152-165
**Responsibility:** Store baseline fingerprint after load

**Preconditions:**
- `checkpoint_hash_hex` must be valid BLAKE3 hex string (64 chars)

**Postconditions:**
- VramTracker stores `GpuBufferFingerprint` for `id`
- Adaptive baseline updated with `buffer_size` sample

---

#### `verify_gpu_fingerprint(id: u16, buffer_size: u64, checkpoint_hash_hex: &str) -> Result<bool>`

**Lines:** 171-189
**Responsibility:** Compare current fingerprint to baseline

**Returns:**
- `Ok(true)` - Fingerprint matches baseline
- `Ok(false)` - No baseline stored yet (first verification)
- `Err(msg)` - Fingerprint mismatch (integrity violation)

**Use Case:** Detect GPU buffer corruption/tampering after load.

---

### 3.3 Memory Footprint Anomaly Detection

#### `check_memory_footprint(id: u16, buffer_size: u64) -> (bool, f64, Option<(f64, f64, usize)>)`

**Lines:** 194-212
**Responsibility:** Detect anomalous VRAM usage via adaptive baseline

**Algorithm:**
1. Get baseline statistics (mean, stddev, sample count)
2. Compute z-score: `(buffer_size - mean) / stddev`
3. Check if z-score ≤ 2.0 (within 2σ tolerance)

**Returns:**
- `within_tolerance` - true if buffer_size is within 2σ
- `z_score` - Number of standard deviations from mean
- `baseline_stats` - (mean, stddev, sample_count) for telemetry

**Open Issue:** Baseline is adapter-specific (per-ID). If adapter is updated (new revision), should baseline reset? Currently NO - baseline persists across revisions.

---

## 4. Cross-Layer Consistency Protocol

### 4.1 Hash Correlation Chain

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 1: Router Decision                                     │
│   • stack_hash: Option<B3Hash>   (from active_stack_hash)   │
│   • Cached in Decision struct                                │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ↓
┌─────────────────────────────────────────────────────────────┐
│ Layer 2: AdapterTable State                                  │
│   • metadata_hash = compute_stack_hash()                     │
│   • cross_layer_hash = compute_cross_layer_hash(gpu_fps)    │
│   • Checkpoint contains both hashes                          │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ↓
┌─────────────────────────────────────────────────────────────┐
│ Layer 3: GPU Fingerprints (VramTracker)                      │
│   • GpuBufferFingerprint per adapter                         │
│   • checkpoint_hash = BLAKE3(first||last||mid samples)       │
│   • Included in cross_layer_hash computation                 │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 Verification Workflow

**Trigger:** After swap completion (lines 997-1032 in `adapter_hotswap.rs`)

1. **Metadata Verification:**
   - Compute `metadata_hash` via `compute_stack_hash()`
   - Compare to expected hash from manifest

2. **GPU Verification:**
   - For each active adapter:
     - Call `kernels.verify_adapter_buffers(id)` → samples
     - Create `GpuFingerprint` from samples
     - Store fingerprint via `kernels.store_gpu_fingerprint()`

3. **Cross-Layer Verification:**
   - Compute `cross_layer_hash` combining metadata + GPU fingerprints
   - Store in `StackCheckpoint`
   - Log to telemetry for audit

**Open Issue:** If GPU verification fails, there's no automatic rollback. The swap commits and logs a warning. Should failure trigger rollback?

---

## 5. Error Handling Contracts

### 5.1 AdapterTable Error Modes

| Error Condition | Error Type | Behavior | Recovery |
|----------------|-----------|----------|----------|
| Adapter already staged | `AosError::Worker` | Return error, no state change | Caller must check before preload |
| Missing staged adapter | `AosError::Worker` | **Automatic rollback** | Previous state restored |
| No rollback state | `AosError::Worker` | Return error | Must have performed ≥1 swap |
| RCU unload fails 3× | N/A (logged) | Quarantine stack | Manual intervention |

### 5.2 Router Error Modes

| Error Condition | Error Type | Behavior | Recovery |
|----------------|-----------|----------|----------|
| K > MAX_K (8) | `AosError::Config` | Construction fails | Use K ≤ 8 |
| All adapters excluded | N/A (silent) | Return empty Decision | Caller handles k0 |
| Invalid feature vector | N/A (silent) | Fallback to uniform priors | Graceful degradation |

### 5.3 FusedKernels Error Modes

| Error Condition | Error Type | Behavior | Recovery |
|----------------|-----------|----------|----------|
| Hot-swap not supported | `AosError::Kernel` | Return error (default impl) | Use Metal backend |
| GPU OOM | `AosError::Kernel` | Load fails | Evict adapters, retry |
| Fingerprint mismatch | `Err(String)` | Return error message | Log, continue |
| Invalid SafeTensors | `AosError::Parse` | Load fails | Fix .aos file |

---

## 6. Threading and Concurrency

### 6.1 Lock Ordering Protocol

**AdapterTable locks (must acquire in this order to prevent deadlock):**

1. `staged` (RwLock) - Short-lived, read/write preload
2. `active` (RwLock) - Short-lived, read during hash, write during swap
3. `rollback_state` (RwLock) - Short-lived, write during swap
4. `refcounts` (Mutex) - Very short-lived, inc/dec operations
5. `retired_stacks` (Mutex) - Short-lived, append during swap, iterate during RCU

**Open Issue:** No explicit deadlock detection. Loom tests (lines 1287-1339) verify no deadlock in 5000+ interleavings.

### 6.2 RCU Protocol

**Reader (Inference) Path:**
```rust
let stack = table.current_stack.load(Ordering::Acquire);  // Snapshot generation
table.inc_ref(&adapter_id);                               // Pin adapter
// ... use adapter weights ...
table.dec_ref(&adapter_id);                               // Unpin adapter
```

**Writer (Swap) Path:**
```rust
table.swap(&add_ids, &remove_ids)?;                       // Advance generation
// Old stack moved to retired_stacks
// RCU background task unloads when refcount == 0
```

**Guarantee:** No UAF (Use-After-Free) because:
1. Readers pin adapters via refcount before use
2. Writers defer unload until refcount == 0
3. `Arc<Stack>` keeps metadata alive until all refs drop

---

## 7. Open Issues and Fragile Assumptions

### 7.1 Critical Issues

**Issue 1: No duplicate load detection (FusedKernels::load_adapter)**
- **Impact:** VRAM leak if same `id` loaded twice
- **Location:** `adapteros-lora-kernel-api/src/lib.rs:114-121`
- **Mitigation:** Caller must check before load (currently NOT enforced)

**Issue 2: GPU verification failure does NOT trigger rollback**
- **Impact:** Corrupted GPU state accepted silently
- **Location:** `adapter_hotswap.rs:997-1032`
- **Mitigation:** Currently logs warning only (should rollback?)

**Issue 3: Quarantined stacks have no alerting**
- **Impact:** Silent VRAM leak until manual inspection
- **Location:** `adapter_hotswap.rs:656-680`
- **Mitigation:** Monitor `rcu_unload_failed` telemetry events

### 7.2 Design Assumptions

**Assumption 1: Adapter IDs fit in u16**
- **Rationale:** BLAKE3(adapter_id)[0..2] → u16
- **Collision Risk:** ~1/65536 per pair
- **Mitigation:** Use full adapter_id strings in AdapterTable, u16 only at kernel boundary

**Assumption 2: Checkpoint samples (3×4KB) are sufficient**
- **Rationale:** Tampering likely affects first/last/mid regions
- **Weakness:** Targeted tampering in non-sampled regions goes undetected
- **Mitigation:** Increase sample count or use Merkle tree

**Assumption 3: Generation counter never overflows**
- **Type:** `usize` (64-bit on typical platforms)
- **Max Swaps:** 2^64 swaps = ~5 billion years at 100 swaps/sec
- **Mitigation:** None needed (effectively infinite)

### 7.3 Unspecified Behaviors

**Behavior 1: What if GPU fingerprints are empty?**
- **Current:** Falls back to metadata-only hash (lines 1016-1032)
- **Should:** Fail loudly if GPU verification is required

**Behavior 2: What if Router filters all adapters?**
- **Current:** Returns empty Decision (k0 detection logs warning)
- **Should:** Return error instead of empty Decision?

**Behavior 3: What if adapter unload fails during inference?**
- **Current:** Logs warning, continues inference with stale adapters
- **Should:** Surface error to caller for circuit-breaker logic

---

## 8. Testing Coverage

### 8.1 Unit Tests

**AdapterTable Tests (adapter_hotswap.rs:1202-1428):**
- ✅ Preload and swap basic flow (line 1205)
- ✅ Rollback on partial failure (line 1233)
- ✅ Stack hash determinism (line 1264)
- ✅ RCU refcount protocol (line 1265)
- ✅ Loom concurrency model (1287, 5000+ interleavings)
- ✅ Stress test: 100 concurrent infers + 50 swaps (1341)

**Router Tests (lib.rs:1137-1313):**
- ✅ Top-K selection (line 1142)
- ✅ Entropy floor enforcement (line 1160)
- ✅ Weighted scoring (line 1215)
- ✅ Stack filtering (implicitly tested in route_with_code_features)

### 8.2 Integration Tests

**Hot-Swap Integration (tests/adapter_hotswap.rs:1-570):**
- ✅ Preload and swap basic (line 32)
- ✅ 100 swap cycles (line 57)
- ✅ Rollback on missing adapter (line 121)
- ✅ Hash determinism (line 149)
- ✅ VRAM delta tracking (line 216)
- ✅ Mmap adapter load (line 329)
- ✅ Atomic swap timing < 10ms (line 364)
- ✅ Rollback on missing file (line 408)
- ✅ Concurrent swaps thread safety (line 494)
- ✅ Telemetry logging (line 540)

**Missing Test Coverage:**
- ❌ GPU fingerprint mismatch → rollback
- ❌ Quarantine after 3 RCU retries
- ❌ Router k0 scenario (all adapters excluded)
- ❌ Duplicate adapter load (VRAM leak)

---

## 9. Performance Characteristics

### 9.1 Latency Targets

| Operation | Target | Measured | Notes |
|-----------|--------|----------|-------|
| Preload (to staging) | < 500ms | Varies | Disk I/O bound |
| Swap (pointer flip) | < 10ms | < 5ms | Mutex-guarded atomic op |
| Rollback | < 10ms | < 5ms | Restore pointer only |
| Compute stack hash | < 1ms | < 0.5ms | BLAKE3 on metadata |
| Cross-layer hash | < 10ms | Varies | Depends on GPU sample count |
| RCU unload | < 100ms | < 50ms | Metal buffer dealloc |

### 9.2 Memory Overhead

| Component | Per Adapter | Notes |
|-----------|-------------|-------|
| AdapterState struct | ~80 bytes | id, hash, vram_mb, loaded_at |
| Refcount entry | 16 bytes | AtomicUsize + HashMap overhead |
| GpuFingerprint | ~120 bytes | buffer_bytes, timestamp, B3Hash |
| Checkpoint | ~200 bytes + N×120 | Timestamp, 2 hashes, N fingerprints |

**Total Overhead for 100 Adapters:** ~30KB (negligible compared to VRAM usage)

---

## 10. Recommended Practices

### 10.1 Safe Swap Protocol

```rust
// 1. Preload new adapters
for (id, hash, vram_mb) in new_adapters {
    table.preload(id, hash, vram_mb)?;
}

// 2. Perform swap with error handling
match table.swap(&add_ids, &remove_ids) {
    Ok((vram_delta, count)) => {
        // 3. Verify integrity
        let metadata_hash = table.compute_stack_hash();
        let gpu_fps = collect_gpu_fingerprints(&kernels).await?;
        let cross_layer_hash = table.compute_cross_layer_hash(&gpu_fps);

        // 4. Create checkpoint for crash recovery
        let checkpoint = table.create_checkpoint(gpu_fps);
        table.save_checkpoints(&checkpoint_path)?;

        // 5. Update router stack filter
        router.set_active_stack(Some(stack_name), Some(add_ids), Some(cross_layer_hash));
    }
    Err(e) => {
        // Automatic rollback already performed
        error!("Swap failed: {}", e);
        return Err(e);
    }
}
```

### 10.2 Monitoring and Alerting

**Critical Telemetry Events:**
- `rcu_unload_failed` - Adapter quarantined after 3 retries (lines 664-678)
- `barrier.timeout` - Multi-agent coordination failure (if using deterministic exec)
- `gpu_fingerprint_mismatch` - Integrity violation detected

**Recommended Metrics:**
- `hotswap_latency_ms` - P50/P99 swap duration
- `quarantined_adapters_count` - Track stuck RCU stacks
- `active_stack_generation` - Monotonically increasing counter

---

## 11. Version History

| Version | Date | Changes | Author |
|---------|------|---------|--------|
| 1.0 | 2025-01-18 | Initial contract documentation | James KC Auchterlonie |

---

## 12. References

- **CLAUDE.md** - System architecture and patterns
- `crates/adapteros-lora-worker/src/adapter_hotswap.rs` - AdapterTable implementation
- `crates/adapteros-lora-router/src/lib.rs` - Router stack filtering
- `crates/adapteros-lora-kernel-api/src/lib.rs` - FusedKernels trait
- `crates/adapteros-lora-kernel-mtl/src/vram.rs` - GPU fingerprinting
- `tests/adapter_hotswap.rs` - Integration test suite

---

**Rule:** When in doubt about hot-swap contracts, consult this document and verify against source code line ranges. All documentation and code signed by **James KC Auchterlonie**.
