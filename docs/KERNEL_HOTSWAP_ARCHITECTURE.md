# Kernel Hot-Swap Architecture: Control Plane → GPU Execution

**Status:** CRITICAL GAP - Hot-swap returns OK but kernels use placeholder weights
**Last Updated:** 2025-01-16
**Priority:** P0 - "Single biggest risk in the whole system"

---

## Executive Summary

**The Problem:**
AdapterOS has a complete disconnect between the hot-swap control plane and GPU kernel execution. The system reports successful adapter loading, but **adapter weights are never uploaded to Metal GPU**. Kernels execute with placeholder/uninitialized LoRA weights, producing incorrect outputs while reporting success.

**Impact:**
- Silent failures: System reports OK, produces wrong results
- No determinism: Outputs are from placeholder computation
- Production blocker: Hot-swap is non-functional

**Root Cause:**
`AdapterTable` (control plane) only tracks metadata. `MetalKernels` (execution plane) never implements `load_adapter()`. No data path connects them.

---

## Current Architecture (BROKEN)

### Data Flow - As Implemented

```
User Request: "Load adapter_1"
    ↓
┌─────────────────────────────────────────────────────────┐
│ HotSwapManager::execute(Preload)                        │
│ [adapteros-lora-worker/src/adapter_hotswap.rs:217-229] │
│                                                          │
│  let vram_mb = 24;  // ← MOCK VALUE                     │
│  self.table.preload(adapter_id, hash, vram_mb)?;        │
│  return OK  // ← Returns success without loading!       │
└─────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────┐
│ AdapterTable::preload()                                  │
│ [adapteros-lora-worker/src/adapter_hotswap.rs:73-93]   │
│                                                          │
│  staged.insert(id, AdapterState {                        │
│      id, hash, vram_mb,  // ← Just metadata             │
│      loaded_at: Instant::now(),                          │
│      active: false,                                      │
│  });                                                     │
│  // NO WEIGHT LOADING!                                   │
└─────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────┐
│ AdapterTable::swap(add_ids, remove_ids)                 │
│ [adapteros-lora-worker/src/adapter_hotswap.rs:108-152] │
│                                                          │
│  // Move from staged → active (metadata only)           │
│  // NO GPU OPERATIONS!                                   │
└─────────────────────────────────────────────────────────┘
    ↓
    ↓ [GAP: No communication with MetalKernels]
    ↓
┌─────────────────────────────────────────────────────────┐
│ MetalKernels::run_step()                                │
│ [adapteros-lora-kernel-mtl/src/lib.rs:910-937]         │
│                                                          │
│  // RingBuffer has adapter IDs and gates                │
│  // NO WEIGHT BUFFERS!                                  │
└─────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────┐
│ RingBuffer::update(adapters)                            │
│ [adapteros-lora-kernel-mtl/src/ring_buffer.rs:64-86]   │
│                                                          │
│  self.adapter_indices[i] = adapter.id;  // Just ID      │
│  self.gates[i] = adapter.gate;          // Just gate    │
│  // NO WEIGHT POINTERS!                                 │
└─────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────┐
│ Metal Kernel Execution (mplora.metal)                   │
│ [adapteros-lora-kernel-mtl/src/kernels/mplora.metal]   │
│                                                          │
│  shared_output += input[i] * shared_A[idx];             │
│                               ^^^^^^^^                   │
│                        UNINITIALIZED BUFFER!             │
│                                                          │
│  // Garbage * gate = garbage output                     │
└─────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────┐
│ Placeholder Output Generation                           │
│ [adapteros-lora-kernel-mtl/src/lib.rs:717-723]         │
│                                                          │
│  for (i, logit) in io.output_logits.iter_mut() {        │
│      let adapter_influence: f32 =                        │
│          adapters.iter()                                 │
│              .map(|a| (a.id as f32) * 0.001)             │
│              .sum();                                     │
│      *logit = total_gate_weight * ...                    │
│          + adapter_influence;                            │
│  }                                                       │
│  // ← Computing from adapter IDs, NOT LoRA math!        │
└─────────────────────────────────────────────────────────┘
    ↓
    ✅ Returns success (with WRONG output)
```

### Evidence of Disconnect

**Control Plane (Metadata Only):**
```rust
// adapteros-lora-worker/src/adapter_hotswap.rs:217-229
AdapterCommand::Preload { adapter_id, hash } => {
    // Mock VRAM size for now - in production this would come from actual loading
    let vram_mb = 24; // Mock value  ← PLACEHOLDER!
    self.table.preload(adapter_id.clone(), hash, vram_mb)?;

    AdapterCommandResult {
        success: true,  // ← Returns true without loading!
        message: format!("Preloaded adapter: {}", adapter_id),
        vram_delta_mb: Some(vram_mb as i64),
        duration_ms: start.elapsed().as_millis() as u64,
        stack_hash: None,
    }
}
```

**Missing Implementation:**
```rust
// adapteros-kernel-api/src/lib.rs:71-78 (default trait implementation)
fn load_adapter(&mut self, _id: u16, _weights: &[u8]) -> Result<()> {
    Err(adapteros_core::AosError::Kernel(
        "Hot-swap not supported by this backend".to_string(),
    ))
}

// MetalKernels NEVER OVERRIDES THIS!
// Uses default stub that returns error
```

**Placeholder Execution:**
```rust
// adapteros-lora-kernel-mtl/src/lib.rs:717-723
// Generate deterministic output based on adapters
let total_gate_weight: f32 = adapters.iter().map(|a| (a.gate as f32) / 32768.0).sum();

for (i, logit) in io.output_logits.iter_mut().enumerate() {
    let adapter_influence: f32 = adapters.iter().map(|a| (a.id as f32) * 0.001).sum();
    *logit = total_gate_weight * ((i % 100) as f32) * 0.01 + adapter_influence;
    // ← Not actual LoRA computation: W_base + Σ gate_i * (B_i @ A_i @ x)
}
```

---

## Target Architecture (TO BE IMPLEMENTED)

### Data Flow - Fixed

```
User Request: "Load adapter_1"
    ↓
┌─────────────────────────────────────────────────────────┐
│ HotSwapManager::execute(Preload)                        │
│                                                          │
│  1. Load .aos file from disk                            │
│  2. Parse AOS2 format (manifest + safetensors)          │
│  3. Extract adapter weight bytes                        │
│  4. Call: kernels.load_adapter(id, weight_bytes)        │
│  5. Get actual VRAM usage from Metal                    │
│  6. Store in AdapterTable with real VRAM size           │
└─────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────┐
│ MetalKernels::load_adapter(id, weights) [NEW]           │
│                                                          │
│  1. Parse SafeTensors format                            │
│     - Extract LoRA A matrices: q_proj, k_proj, etc.     │
│     - Extract LoRA B matrices: q_proj, k_proj, etc.     │
│                                                          │
│  2. Create Metal buffers for each weight tensor         │
│     - device.new_buffer_with_data(lora_a_data)          │
│     - Upload to GPU VRAM                                │
│                                                          │
│  3. Store in adapter_weights HashMap                    │
│     - adapter_weights[id] = AdapterWeights {            │
│         lora_a_buffers: [qA, kA, vA, mlpA],             │
│         lora_b_buffers: [qB, kB, vB, mlpB],             │
│         rank, alpha                                      │
│       }                                                  │
│                                                          │
│  4. Return actual VRAM used (sum of buffer sizes)       │
└─────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────┐
│ AdapterTable::swap(add_ids, remove_ids)                 │
│                                                          │
│  For each remove_id:                                    │
│    - kernels.unload_adapter(remove_id)  // Free GPU     │
│    - Remove from active map                             │
│                                                          │
│  For each add_id:                                       │
│    - Move staged[add_id] → active[add_id]               │
│    - Weights already loaded during preload              │
└─────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────┐
│ MetalKernels::run_step(adapters, io_buffers)            │
│                                                          │
│  1. RingBuffer::update(adapters) - sets IDs + gates     │
│  2. Lookup weight buffers for active adapters:          │
│     - adapter_weights[adapter.id] → AdapterWeights      │
│  3. Bind weight buffers to Metal encoder                │
│  4. Execute kernels with real LoRA computation          │
└─────────────────────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────────────────────┐
│ Metal Kernel Execution (mplora.metal) [UPDATED]        │
│                                                          │
│  // Receive weight buffer arguments                     │
│  device const float* adapter_A_weights,                 │
│  device const float* adapter_B_weights,                 │
│                                                          │
│  // Compute: output = W_base @ x + Σ gate_i * ΔW_i @ x  │
│  float lora_delta = 0.0;                                │
│  for (uint aid = 0; aid < num_adapters; aid++) {        │
│      float gate = gates[aid] / 32767.0;                 │
│      float shared_output = 0.0;                         │
│      for (uint r = 0; r < rank; r++) {                  │
│          shared_output +=                               │
│              input[r] * adapter_A_weights[aid][r];      │
│      }                                                   │
│      lora_delta += gate * (adapter_B_weights[aid] *     │
│                            shared_output);               │
│  }                                                       │
│  output = base_output + lora_delta;                     │
└─────────────────────────────────────────────────────────┘
    ↓
    ✅ Returns success (with CORRECT output)
```

---

## Weight Layout Design

### Per-Adapter GPU Memory Layout

Each adapter has LoRA weights for multiple target modules:

```
Adapter ID: 1
├─ Q Projection LoRA
│  ├─ A matrix: [rank × hidden_size]  → Metal Buffer
│  └─ B matrix: [hidden_size × rank]  → Metal Buffer
├─ K Projection LoRA
│  ├─ A matrix: [rank × hidden_size]  → Metal Buffer
│  └─ B matrix: [hidden_size × rank]  → Metal Buffer
├─ V Projection LoRA
│  ├─ A matrix: [rank × hidden_size]  → Metal Buffer
│  └─ B matrix: [hidden_size × rank]  → Metal Buffer
└─ MLP LoRA
   ├─ A matrix: [rank × mlp_hidden]   → Metal Buffer
   └─ B matrix: [mlp_hidden × rank]   → Metal Buffer

Total VRAM per adapter ≈ 2 * rank * (3 * hidden_size + mlp_hidden) * sizeof(f32)
For rank=16, hidden=4096, mlp=11008: ~1.1 MB per adapter
```

### AdapterWeights Struct (NEW)

```rust
/// GPU-resident adapter weights
pub struct AdapterWeights {
    /// LoRA A matrices per target module [rank × in_dim]
    pub lora_a_buffers: Vec<Buffer>,  // [q_proj_A, k_proj_A, v_proj_A, mlp_A]

    /// LoRA B matrices per target module [out_dim × rank]
    pub lora_b_buffers: Vec<Buffer>,  // [q_proj_B, k_proj_B, v_proj_B, mlp_B]

    /// LoRA rank (typically 4-64)
    pub rank: usize,

    /// LoRA alpha scaling factor
    pub alpha: f32,

    /// Total VRAM used (bytes)
    pub vram_bytes: u64,

    /// Content hash for integrity verification
    pub hash_b3: B3Hash,
}

impl MetalKernels {
    /// Adapter weights indexed by adapter_id
    adapter_weights: HashMap<u16, AdapterWeights>,
}
```

### Ring Buffer Extension

**Current (Broken):**
```rust
pub struct RingBuffer {
    adapter_indices: [u16; MAX_ADAPTERS],  // Just IDs
    gates: [i16; MAX_ADAPTERS],            // Just Q15 gates
    metal_buffer: Buffer,                   // Only uploads IDs + gates
}
```

**Fixed (With Weight References):**
```rust
pub struct RingBuffer {
    adapter_indices: [u16; MAX_ADAPTERS],   // Adapter IDs
    gates: [i16; MAX_ADAPTERS],             // Q15 gates

    // NEW: Weight buffer indices (index into MetalKernels.adapter_weights)
    weight_buffer_indices: [u16; MAX_ADAPTERS],

    metal_buffer: Buffer,                    // Uploads IDs, gates, weight indices
}
```

Or simpler: Pass `adapter_weights` HashMap directly to kernel encoder as buffer argument.

### Kernel Computation Formula

**Mathematical Definition:**

For input `x` and active adapters `{A₁, A₂, ..., Aₖ}` with gates `{g₁, g₂, ..., gₖ}`:

```
output = W_base @ x + Σᵢ₌₁ᵏ (gᵢ / 32767) * (alpha / rank) * (Bᵢ @ (Aᵢ @ x))
```

Where:
- `W_base`: Base model weights (pre-loaded)
- `Aᵢ`: LoRA A matrix for adapter i [rank × hidden_size]
- `Bᵢ`: LoRA B matrix for adapter i [hidden_size × rank]
- `gᵢ`: Q15 gate value in [-32768, 32767]
- `alpha`, `rank`: LoRA hyperparameters

**Pseudo-code (Metal Kernel):**

```metal
kernel void fused_mplora_qkv(
    device const float* input,           // [seq_len, hidden_size]
    device const float* base_qkv_weight, // [3 * hidden_size, hidden_size]
    device const float* adapter_A_all,   // [num_adapters, rank, hidden_size]
    device const float* adapter_B_all,   // [num_adapters, hidden_size, rank]
    device const int16_t* gates,         // [num_adapters] Q15 gates
    constant uint& num_adapters,
    constant uint& rank,
    constant float& alpha,
    device float* output,                // [seq_len, 3 * hidden_size]
    uint2 gid [[thread_position_in_grid]]
) {
    uint seq_idx = gid.x;
    uint out_idx = gid.y;

    // 1. Base model computation
    float base_output = 0.0;
    for (uint h = 0; h < hidden_size; h++) {
        base_output += input[seq_idx * hidden_size + h] *
                       base_qkv_weight[out_idx * hidden_size + h];
    }

    // 2. LoRA adapter contributions
    float lora_delta = 0.0;
    for (uint aid = 0; aid < num_adapters; aid++) {
        float gate = float(gates[aid]) / 32767.0;
        float scaling = (alpha / float(rank)) * gate;

        // Downsample: A @ x  [rank]
        float shared_output[rank];
        for (uint r = 0; r < rank; r++) {
            shared_output[r] = 0.0;
            for (uint h = 0; h < hidden_size; h++) {
                shared_output[r] += input[seq_idx * hidden_size + h] *
                                    adapter_A_all[aid * rank * hidden_size +
                                                  r * hidden_size + h];
            }
        }

        // Upsample: B @ (A @ x)
        float adapter_output = 0.0;
        for (uint r = 0; r < rank; r++) {
            adapter_output += shared_output[r] *
                             adapter_B_all[aid * hidden_size * rank +
                                          out_idx * rank + r];
        }

        lora_delta += scaling * adapter_output;
    }

    // 3. Combine
    output[seq_idx * (3 * hidden_size) + out_idx] = base_output + lora_delta;
}
```

---

## Memory Layout

### GPU VRAM Organization

```
┌─────────────────────────────────────────────────────────┐
│ Base Model Weights (Pre-loaded, Never Evicted)         │
│ - Embedding: [vocab_size × hidden_size]                │
│ - Transformer layers: [num_layers × ...]               │
│ - LM Head: [vocab_size × hidden_size]                  │
│ Total: ~2-8 GB depending on model size                 │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│ Adapter Weight Ring Buffer (Hot-Swappable)             │
│                                                          │
│ Adapter 1 (ID=1):                                       │
│   - q_proj_A: Buffer @0x1000 (256 KB)                  │
│   - q_proj_B: Buffer @0x1100 (256 KB)                  │
│   - k_proj_A: Buffer @0x1200 (256 KB)                  │
│   - k_proj_B: Buffer @0x1300 (256 KB)                  │
│   - v_proj_A: Buffer @0x1400 (256 KB)                  │
│   - v_proj_B: Buffer @0x1500 (256 KB)                  │
│   - mlp_A: Buffer @0x1600 (704 KB)                     │
│   - mlp_B: Buffer @0x1700 (704 KB)                     │
│   Total: ~2.8 MB                                        │
│                                                          │
│ Adapter 2 (ID=2):                                       │
│   - ... (same structure)                                │
│   Total: ~2.8 MB                                        │
│                                                          │
│ [Up to MAX_ADAPTERS = 32 adapters]                     │
│ Total: ~90 MB for 32 adapters @ rank=16                │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│ RingBuffer Metadata (Uploaded per inference)           │
│ - adapter_indices: [u16; 32]  (64 bytes)               │
│ - gates: [i16; 32]             (64 bytes)               │
│ Total: 128 bytes                                        │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│ I/O Buffers (Per-inference allocation)                 │
│ - input_tokens: [batch_size × seq_len]                 │
│ - output_logits: [batch_size × vocab_size]             │
│ Total: ~50-200 MB depending on batch size               │
└─────────────────────────────────────────────────────────┘
```

### Hot-Swap Operations

**Preload (Add adapter to VRAM):**
```
1. Allocate Metal buffers for LoRA A/B matrices
2. Upload weight data to GPU
3. Insert into adapter_weights HashMap
4. Update VRAM usage counter
5. Mark as staged (not active yet)
```

**Swap (Atomic activation):**
```
1. Remove old adapters:
   - Free Metal buffers
   - Remove from adapter_weights HashMap
   - Update VRAM counter (decrement)

2. Activate new adapters:
   - Move from staged → active
   - Already in VRAM from preload step
   - Update RingBuffer with new adapter IDs
```

**Unload (Free adapter from VRAM):**
```
1. Remove from adapter_weights HashMap
2. Call buffer.release() on all Metal buffers
3. Update VRAM counter (decrement)
```

---

## Implementation Checklist

### Phase 2: Weight Loading Infrastructure

- [ ] Add `AdapterWeights` struct to `adapteros-lora-kernel-mtl/src/lib.rs`
- [ ] Add `adapter_weights: HashMap<u16, AdapterWeights>` field to `MetalKernels`
- [ ] Implement `MetalKernels::load_adapter(id: u16, weights: &[u8]) -> Result<u64>`
  - [ ] Parse SafeTensors format (use `safetensors` crate)
  - [ ] Extract LoRA A/B matrices for each target module
  - [ ] Create Metal buffers via `device.new_buffer_with_data()`
  - [ ] Store in `adapter_weights` HashMap
  - [ ] Return actual VRAM bytes used
- [ ] Implement `MetalKernels::unload_adapter(id: u16) -> Result<u64>`
  - [ ] Remove from `adapter_weights` HashMap
  - [ ] Release Metal buffers
  - [ ] Return VRAM bytes freed

### Phase 3: Wire Hot-Swap to Weight Loading

- [ ] Modify `HotSwapManager::execute(Preload)` in `adapteros-lora-worker/src/adapter_hotswap.rs`
  - [ ] Load `.aos` file from disk (via `AOS2Loader`)
  - [ ] Parse SafeTensors payload
  - [ ] Call `kernels.load_adapter(id, weight_bytes)`
  - [ ] Use returned VRAM size (replace mock value)
- [ ] Modify `HotSwapManager::execute(Swap)` remove operations
  - [ ] Call `kernels.unload_adapter(id)` for each removed adapter
  - [ ] Track VRAM delta from actual unload

### Phase 4: Update Kernel Execution

- [ ] Remove placeholder output generation in `MetalKernels::run_transformer_layers()`
  - [ ] Delete lines 717-723 (placeholder computation)
- [ ] Update `fused_mlp_kernel.execute()` to bind adapter weight buffers
  - [ ] Add buffer arguments for LoRA A/B weights
  - [ ] Pass `adapter_weights` data to Metal encoder
- [ ] Update Metal shader signatures in `mplora.metal`
  - [ ] Add `device const float* adapter_A_all` parameter
  - [ ] Add `device const float* adapter_B_all` parameter
  - [ ] Implement real LoRA math: `W_base @ x + Σ gate_i * (B_i @ A_i @ x)`

### Phase 5: Instrumentation

- [ ] Add debug logging in `load_adapter()`:
  - [ ] Log adapter ID, weight hash, VRAM usage
  - [ ] Log number of active adapters
  - [ ] Log total VRAM used by all adapters
- [ ] Add verification after weight upload:
  - [ ] Verify buffer contents non-zero (sample first 10 values)
  - [ ] Compute and log BLAKE3 hash of loaded weights

### Phase 6: Testing

- [ ] Add `tests/hotswap_determinism.rs` with:
  - [ ] `test_hotswap_determinism_with_real_weights()` - load A → gen → swap B → gen → restart with B → compare
  - [ ] `test_adapter_weights_actually_loaded()` - verify output != base model, matches expected LoRA transform
- [ ] Update `tests/kernel_workflow_integration.rs`:
  - [ ] Remove `#[ignore]` attributes
  - [ ] Switch from `MockAdapterBackend` to real `MetalKernels`

---

## Testing Strategy

### Unit Tests

**Test: Weight Loading Correctness**
```rust
#[test]
fn test_load_adapter_creates_metal_buffers() {
    let mut kernels = MetalKernels::new(config)?;
    let adapter_weights = create_test_adapter_weights(); // Known SafeTensors

    let vram_used = kernels.load_adapter(1, &adapter_weights)?;

    // Verify buffer created
    assert!(kernels.adapter_weights.contains_key(&1));

    // Verify VRAM calculation
    assert!(vram_used > 0);
    assert_eq!(vram_used, kernels.adapter_weights[&1].vram_bytes);
}
```

### Integration Tests

**Test: Hot-Swap Determinism**
```rust
#[test]
fn test_hotswap_determinism_with_real_weights() {
    let mut worker = LoraWorker::new(config)?;
    let seed = B3Hash::hash(b"test_seed");

    // Load adapter A
    worker.load_adapter("adapter_a", adapter_a_weights)?;
    let output_a1 = worker.generate(prompt, seed, max_tokens)?;

    // Hot-swap to adapter B
    worker.swap_adapters(&["adapter_b"], &["adapter_a"])?;
    let output_b1 = worker.generate(prompt, seed, max_tokens)?;

    // Restart with B pre-loaded
    let mut worker2 = LoraWorker::new(config)?;
    worker2.load_adapter("adapter_b", adapter_b_weights)?;
    let output_b2 = worker2.generate(prompt, seed, max_tokens)?;

    // Verify determinism
    assert_ne!(output_a1, output_b1, "Different adapters should produce different outputs");
    assert_eq!(output_b1, output_b2, "Same adapter should produce identical outputs");
}
```

**Test: Weights Actually Used**
```rust
#[test]
fn test_adapter_weights_actually_loaded() {
    let mut kernels = MetalKernels::new(config)?;

    // Load adapter with known LoRA weights
    let adapter_weights = create_test_adapter_with_known_weights()?;
    kernels.load_adapter(1, &adapter_weights)?;

    // Run inference
    let input_tokens = vec![100, 200, 300];
    let adapters = vec![ActiveAdapter { id: 1, gate: 32767 }];
    let output = kernels.run_step(&adapters, input_tokens)?;

    // Compute expected output with known LoRA transformation
    let expected_output = compute_expected_lora_output(&input_tokens, &adapter_weights);

    // Verify output matches (within numerical tolerance)
    assert_approx_eq!(output, expected_output, epsilon = 1e-4);
}
```

---

## File Citations

### Current Implementation (Broken)

| File | Lines | Description |
|------|-------|-------------|
| `crates/adapteros-lora-worker/src/adapter_hotswap.rs` | 73-93 | `AdapterTable::preload()` - metadata only |
| `crates/adapteros-lora-worker/src/adapter_hotswap.rs` | 108-152 | `AdapterTable::swap()` - no GPU ops |
| `crates/adapteros-lora-worker/src/adapter_hotswap.rs` | 217-229 | `HotSwapManager::execute(Preload)` - mock VRAM |
| `crates/adapteros-kernel-api/src/lib.rs` | 71-78 | `load_adapter()` default stub (returns error) |
| `crates/adapteros-lora-kernel-mtl/src/lib.rs` | 910-937 | `MetalKernels::run_step()` - no weight loading |
| `crates/adapteros-lora-kernel-mtl/src/lib.rs` | 717-723 | Placeholder output generation |
| `crates/adapteros-lora-kernel-mtl/src/ring_buffer.rs` | 64-86 | `RingBuffer::update()` - IDs + gates only |
| `crates/adapteros-lora-kernel-mtl/src/kernels/mplora.metal` | 23-57 | Metal kernel using uninitialized buffers |

### Implementation Targets

| File | Action | Description |
|------|--------|-------------|
| `crates/adapteros-lora-kernel-mtl/src/lib.rs` | Add struct | `AdapterWeights` with Metal buffers |
| `crates/adapteros-lora-kernel-mtl/src/lib.rs` | Add field | `adapter_weights: HashMap<u16, AdapterWeights>` |
| `crates/adapteros-lora-kernel-mtl/src/lib.rs` | Implement | `load_adapter()` - parse SafeTensors, upload to GPU |
| `crates/adapteros-lora-kernel-mtl/src/lib.rs` | Implement | `unload_adapter()` - free Metal buffers |
| `crates/adapteros-lora-kernel-mtl/src/lib.rs` | Remove | Lines 717-723 (placeholder computation) |
| `crates/adapteros-lora-worker/src/adapter_hotswap.rs` | Modify | `execute(Preload)` - call `kernels.load_adapter()` |
| `crates/adapteros-lora-worker/src/adapter_hotswap.rs` | Modify | `execute(Swap)` - call `kernels.unload_adapter()` |
| `crates/adapteros-lora-kernel-mtl/src/kernels/mplora.metal` | Update | Add weight buffer parameters, implement LoRA math |
| `tests/hotswap_determinism.rs` | Create | Determinism tests with real weights |
| `tests/kernel_workflow_integration.rs` | Modify | Enable tests, use real Metal backend |

---

## Risk Mitigation

**Before Fix:**
- ❌ Silent failures (reports OK, produces wrong output)
- ❌ No test coverage for actual weight usage
- ❌ Placeholder computation in production code
- ❌ Control plane disconnected from execution plane

**After Fix:**
- ✅ Weight loading verified with non-zero buffer checks
- ✅ Hash-based integrity verification
- ✅ Determinism tests with real LoRA computation
- ✅ End-to-end integration tests enabled
- ✅ Debug instrumentation for VRAM tracking

**Success Criteria:**
1. `load_adapter()` creates Metal buffers and uploads weights to GPU
2. `unload_adapter()` frees Metal buffers and updates VRAM counter
3. Kernels execute with real LoRA math: `W_base @ x + Σ gate_i * (B_i @ A_i @ x)`
4. Tests verify: different adapters → different outputs, same adapter → identical outputs
5. No placeholder computation in production code paths

---

**Next Steps:** Proceed with Phase 2 implementation (weight loading infrastructure).
