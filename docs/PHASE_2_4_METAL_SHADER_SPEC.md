# Phase 2.4: Metal Shader LoRA Math Implementation Specification

**Date:** 2025-01-16 (Updated)
**Status:** BLOCKED - Architecture Mismatch Identified
**Priority:** CRITICAL
**Estimated Effort:** 2-6 hours (depending on approach)

---

## ⚠️ BLOCKER UPDATE

**Previous Understanding:** "Metal shader source files not present"
**Reality:** Metal shader sources **exist** but implement a **different architecture**

**Metal Shader Locations:**
- `/Users/star/Dev/adapter-os/metal/src/kernels/mplora.metal` - MPLoRA implementation
- `/Users/star/Dev/adapter-os/metal/src/kernels/attention.metal`
- `/Users/star/Dev/adapter-os/metal/fused_mlp.metal`
- `/Users/star/Dev/adapter-os/metal/fused_attention.metal`

**Architecture Mismatch:**
- **Rust Code Expects:** Standard LoRA (separate A/B matrices per adapter, K=3 independent adapters)
- **Metal Shaders Implement:** MPLoRA (Multi-Path LoRA with shared downsample, orthogonal constraints)

**Decision Required:**
1. **Option A:** Implement standard LoRA shaders (2-3 hours) - Match Rust expectations, fastest path
2. **Option B:** Adapt Rust code to MPLoRA architecture (4-6 hours) - Use advanced architecture
3. **Option C:** Hybrid approach (support both) - Future enhancement

**Recommendation:** Option A (implement standard LoRA) for immediate functionality, add MPLoRA as v2 enhancement.

---

## Overview

The Rust-side infrastructure now successfully:
- ✅ Loads adapter weights into GPU VRAM (`AdapterWeights` with Metal buffers)
- ✅ Passes weight references to kernel execute() methods
- ✅ Validates adapter presence before execution

However, the Metal shaders themselves implement MPLoRA architecture while Rust code expects standard LoRA. This phase resolves the architectural mismatch.

---

## Required Metal Shader Changes (Standard LoRA Implementation)

### File Locations

**New files to create:**
- `crates/adapteros-lora-kernel-mtl/shaders/standard_lora_qkv.metal`
- `crates/adapteros-lora-kernel-mtl/shaders/standard_lora_mlp.metal`

**Compiled output:**
- `crates/adapteros-lora-kernel-mtl/shaders/mplora_kernels.metallib` (replace or augment)

**Functions to implement:**
1. `fused_qkv_gqa_standard_lora` - QKV projection with standard LoRA
2. `fused_mlp_standard_lora` - MLP with standard LoRA

---

## Current Shader Signatures (Broken)

### `fused_qkv_gqa` (Current)

```metal
kernel void fused_qkv_gqa(
    device const float* input [[buffer(0)]],
    device const float* q_weight [[buffer(1)]],
    device const float* k_weight [[buffer(2)]],
    device const float* v_weight [[buffer(3)]],
    device float* q_output [[buffer(4)]],
    device float* k_output [[buffer(5)]],
    device float* v_output [[buffer(6)]],
    device const ActiveAdapter* ring_buffer [[buffer(7)]],
    device const char* lora_config_json [[buffer(8)]],  // ← WRONG: JSON config
    device const char* gqa_config_json [[buffer(9)]],
    uint2 gid [[thread_position_in_grid]]
) {
    // Currently: Uses base weights only, ignores adapters
    // output = W_base @ input
}
```

### `fused_mlp` (Current)

```metal
kernel void fused_mlp(
    device const float* input [[buffer(0)]],
    device const float* gate_weight [[buffer(1)]],
    device const float* up_weight [[buffer(2)]],
    device const float* down_weight [[buffer(3)]],
    device float* output [[buffer(4)]],
    device const ActiveAdapter* ring_buffer [[buffer(5)]],
    device const char* lora_config_json [[buffer(6)]],  // ← WRONG: JSON config
    uint2 gid [[thread_position_in_grid]]
) {
    // Currently: Uses base weights only, ignores adapters
    // output = SwiGLU(gate_weight @ input, up_weight @ input) @ down_weight
}
```

---

## Target Shader Signatures (Fixed)

### `fused_qkv_gqa` (Target)

```metal
kernel void fused_qkv_gqa(
    device const float* input [[buffer(0)]],
    device const float* q_weight [[buffer(1)]],
    device const float* k_weight [[buffer(2)]],
    device const float* v_weight [[buffer(3)]],
    device float* q_output [[buffer(4)]],
    device float* k_output [[buffer(5)]],
    device float* v_output [[buffer(6)]],
    device const ActiveAdapter* ring_buffer [[buffer(7)]],

    // NEW: Adapter weight buffers (3 adapters × 3 projections × 2 matrices)
    device const float* adapter_0_q_A [[buffer(8)]],
    device const float* adapter_0_q_B [[buffer(9)]],
    device const float* adapter_0_k_A [[buffer(10)]],
    device const float* adapter_0_k_B [[buffer(11)]],
    device const float* adapter_0_v_A [[buffer(12)]],
    device const float* adapter_0_v_B [[buffer(13)]],

    device const float* adapter_1_q_A [[buffer(14)]],
    device const float* adapter_1_q_B [[buffer(15)]],
    device const float* adapter_1_k_A [[buffer(16)]],
    device const float* adapter_1_k_B [[buffer(17)]],
    device const float* adapter_1_v_A [[buffer(18)]],
    device const float* adapter_1_v_B [[buffer(19)]],

    device const float* adapter_2_q_A [[buffer(20)]],
    device const float* adapter_2_q_B [[buffer(21)]],
    device const float* adapter_2_k_A [[buffer(22)]],
    device const float* adapter_2_k_B [[buffer(23)]],
    device const float* adapter_2_v_A [[buffer(24)]],
    device const float* adapter_2_v_B [[buffer(25)]],

    // Adapter metadata (rank, alpha) - could be passed as constants
    constant uint* adapter_ranks [[buffer(26)]],     // [rank_0, rank_1, rank_2]
    constant float* adapter_alphas [[buffer(27)]],   // [alpha_0, alpha_1, alpha_2]
    device const char* gqa_config_json [[buffer(28)]],

    uint2 gid [[thread_position_in_grid]]
) {
    // Implementation below
}
```

### `fused_mlp` (Target)

```metal
kernel void fused_mlp(
    device const float* input [[buffer(0)]],
    device const float* gate_weight [[buffer(1)]],
    device const float* up_weight [[buffer(2)]],
    device const float* down_weight [[buffer(3)]],
    device float* output [[buffer(4)]],
    device const ActiveAdapter* ring_buffer [[buffer(5)]],

    // NEW: Adapter weight buffers (3 adapters × 2 MLP layers × 2 matrices)
    device const float* adapter_0_down_A [[buffer(6)]],
    device const float* adapter_0_down_B [[buffer(7)]],
    device const float* adapter_0_up_A [[buffer(8)]],
    device const float* adapter_0_up_B [[buffer(9)]],

    device const float* adapter_1_down_A [[buffer(10)]],
    device const float* adapter_1_down_B [[buffer(11)]],
    device const float* adapter_1_up_A [[buffer(12)]],
    device const float* adapter_1_up_B [[buffer(13)]],

    device const float* adapter_2_down_A [[buffer(14)]],
    device const float* adapter_2_down_B [[buffer(15)]],
    device const float* adapter_2_up_A [[buffer(16)]],
    device const float* adapter_2_up_B [[buffer(17)]],

    // Adapter metadata
    constant uint* adapter_ranks [[buffer(18)]],
    constant float* adapter_alphas [[buffer(19)]],

    uint2 gid [[thread_position_in_grid]]
) {
    // Implementation below
}
```

---

## LoRA Math Implementation

### Formula

For each projection (Q, K, V, MLP down/up):

```
output = W_base @ x + Σᵢ₌₀ᴷ⁻¹ (gateᵢ / 32767) * (alphaᵢ / rankᵢ) * (Bᵢ @ (Aᵢ @ x))
```

Where:
- `W_base` = base model weight matrix (e.g., `q_weight`)
- `x` = input activations
- `K` = number of active adapters (typically 3)
- `gateᵢ` = Q15 gate value from ring buffer (-32768 to 32767)
- `alphaᵢ` = LoRA alpha scaling factor (e.g., 32.0)
- `rankᵢ` = LoRA rank (e.g., 16)
- `Aᵢ` = LoRA down-projection matrix [rank × in_dim]
- `Bᵢ` = LoRA up-projection matrix [out_dim × rank]

### Pseudocode

```metal
// Read adapter metadata from ring buffer
ActiveAdapter adapters[3];
for (int i = 0; i < 3; i++) {
    adapters[i] = ring_buffer[i];
}

// Compute base output
float base_output = 0.0;
for (int j = 0; j < in_dim; j++) {
    base_output += q_weight[gid.x * in_dim + j] * input[j];
}

// Accumulate LoRA deltas
float lora_delta = 0.0;
for (int i = 0; i < 3; i++) {
    if (adapters[i].gate == 0) continue;  // Skip inactive adapters

    // Get adapter metadata
    uint rank = adapter_ranks[i];
    float alpha = adapter_alphas[i];
    float gate_normalized = float(adapters[i].gate) / 32767.0;
    float scaling = gate_normalized * (alpha / float(rank));

    // Compute A @ x (down-projection)
    float a_out[MAX_RANK];  // MAX_RANK = 64
    for (int r = 0; r < rank; r++) {
        a_out[r] = 0.0;
        for (int j = 0; j < in_dim; j++) {
            a_out[r] += adapter_i_q_A[r * in_dim + j] * input[j];
        }
    }

    // Compute B @ (A @ x) (up-projection)
    float b_out = 0.0;
    for (int r = 0; r < rank; r++) {
        b_out += adapter_i_q_B[gid.x * rank + r] * a_out[r];
    }

    lora_delta += scaling * b_out;
}

// Write final output
q_output[gid.x] = base_output + lora_delta;
```

---

## Rust-Side Changes Required

### Update Buffer Bindings in `fused_qkv.rs`

**Location:** `crates/adapteros-lora-kernel-mtl/src/fused_qkv.rs:131-181`

Replace the current LoraConfig buffer with actual weight buffers:

```rust
// Remove this:
let lora_config = if adapter_weights.is_empty() { ... };
let lora_config_bytes = serde_json::to_vec(&lora_config)?;
encoder.set_buffer(8, Some(&lora_config_buffer), 0);

// Add this:
// Bind adapter weight buffers (up to 3 adapters, K=3)
let mut buffer_index = 8;
for (i, weights) in adapter_weights.iter().take(3).enumerate() {
    // Q projection
    encoder.set_buffer(buffer_index, Some(&weights.lora_a_buffers[0]), 0);
    buffer_index += 1;
    encoder.set_buffer(buffer_index, Some(&weights.lora_b_buffers[0]), 0);
    buffer_index += 1;

    // K projection
    encoder.set_buffer(buffer_index, Some(&weights.lora_a_buffers[1]), 0);
    buffer_index += 1;
    encoder.set_buffer(buffer_index, Some(&weights.lora_b_buffers[1]), 0);
    buffer_index += 1;

    // V projection
    encoder.set_buffer(buffer_index, Some(&weights.lora_a_buffers[2]), 0);
    buffer_index += 1;
    encoder.set_buffer(buffer_index, Some(&weights.lora_b_buffers[2]), 0);
    buffer_index += 1;
}

// Pad with null buffers if fewer than 3 adapters
while adapter_weights.len() < 3 {
    for _ in 0..6 {  // 3 projections × 2 matrices
        encoder.set_buffer(buffer_index, None, 0);
        buffer_index += 1;
    }
}

// Bind adapter metadata
let ranks: Vec<u32> = adapter_weights.iter().map(|w| w.rank as u32).collect();
let alphas: Vec<f32> = adapter_weights.iter().map(|w| w.alpha).collect();

let ranks_buffer = self.device.new_buffer_with_data(
    ranks.as_ptr() as *const std::ffi::c_void,
    (ranks.len() * std::mem::size_of::<u32>()) as u64,
    MTLResourceOptions::StorageModeShared,
);
encoder.set_buffer(26, Some(&ranks_buffer), 0);

let alphas_buffer = self.device.new_buffer_with_data(
    alphas.as_ptr() as *const std::ffi::c_void,
    (alphas.len() * std::mem::size_of::<f32>()) as u64,
    MTLResourceOptions::StorageModeShared,
);
encoder.set_buffer(27, Some(&alphas_buffer), 0);

// GQA config moves to buffer 28
encoder.set_buffer(28, Some(&gqa_config_buffer), 0);
```

### Update Buffer Bindings in `fused_mlp.rs`

**Location:** `crates/adapteros-lora-kernel-mtl/src/fused_mlp.rs:118-142`

Similar pattern:

```rust
// Bind adapter weight buffers (up to 3 adapters, K=3)
let mut buffer_index = 6;
for (i, weights) in adapter_weights.iter().take(3).enumerate() {
    // MLP down projection (index 3 in lora_a/b_buffers)
    encoder.set_buffer(buffer_index, Some(&weights.lora_a_buffers[3]), 0);
    buffer_index += 1;
    encoder.set_buffer(buffer_index, Some(&weights.lora_b_buffers[3]), 0);
    buffer_index += 1;

    // MLP up projection (index 4 in lora_a/b_buffers)
    encoder.set_buffer(buffer_index, Some(&weights.lora_a_buffers[4]), 0);
    buffer_index += 1;
    encoder.set_buffer(buffer_index, Some(&weights.lora_b_buffers[4]), 0);
    buffer_index += 1;
}

// Pad with null buffers if fewer than 3 adapters
while adapter_weights.len() < 3 {
    for _ in 0..4 {  // 2 projections × 2 matrices
        encoder.set_buffer(buffer_index, None, 0);
        buffer_index += 1;
    }
}

// Bind metadata
encoder.set_buffer(18, Some(&ranks_buffer), 0);
encoder.set_buffer(19, Some(&alphas_buffer), 0);
```

---

## Compilation & Testing

### Metal Shader Compilation

```bash
# Compile Metal shaders (from .metal source to .metallib)
xcrun -sdk macosx metal -c -o mplora_kernels.air mplora_kernels.metal
xcrun -sdk macosx metallib -o mplora_kernels.metallib mplora_kernels.air

# Compute deterministic hash
shasum -a 256 mplora_kernels.metallib

# Update manifest
cat > manifests/metallib_manifest.json <<EOF
{
  "version": "1.0.0",
  "metallib_hash": "<computed_hash>",
  "functions": ["fused_qkv_gqa", "fused_mlp"]
}
EOF
```

### Verification Tests

**Location:** Create `tests/test_lora_computation.rs`

```rust
#[test]
fn test_lora_vs_baseline() {
    let device = Device::system_default().unwrap();
    let kernels = MetalKernels::new(Arc::new(device), config)?;

    // Load adapter with known weights
    let adapter_weights = create_test_adapter(rank: 4, alpha: 8.0);
    kernels.load_adapter(0, &adapter_weights)?;

    // Run inference with adapter
    let output_with_lora = kernels.run_inference(&input, &[ActiveAdapter { id: 0, gate: 32767 }])?;

    // Run inference without adapter
    let output_baseline = kernels.run_inference(&input, &[])?;

    // Verify outputs differ (LoRA has effect)
    assert_ne!(output_with_lora, output_baseline);

    // Verify LoRA math is correct
    let expected = compute_lora_cpu(&input, &base_weights, &adapter_weights, gate: 1.0);
    assert_approx_eq!(output_with_lora, expected, epsilon: 1e-5);
}

#[test]
fn test_k_sparse_routing() {
    // Verify K=3 adapters are applied correctly
    let adapters = vec![
        ActiveAdapter { id: 0, gate: 16384 },  // 50% strength
        ActiveAdapter { id: 1, gate: 32767 },  // 100% strength
        ActiveAdapter { id: 2, gate: 8192 },   // 25% strength
    ];

    let output = kernels.run_inference(&input, &adapters)?;

    // Verify weighted sum
    let expected = base_output
        + 0.5 * lora_delta_0
        + 1.0 * lora_delta_1
        + 0.25 * lora_delta_2;

    assert_approx_eq!(output, expected, epsilon: 1e-5);
}

#[test]
fn test_deterministic_output() {
    // Same adapter, same input → identical output
    let output1 = kernels.run_inference(&input, &adapters)?;
    let output2 = kernels.run_inference(&input, &adapters)?;

    assert_eq!(output1, output2);
}
```

---

## Optimization Considerations

### Memory Bandwidth

LoRA computation is memory-bound:
- Base computation: 1 read (W_base), 1 read (x), 1 write (output)
- LoRA delta: 2 reads (A, B), 2 intermediate writes, rank-dimension computation

**Optimization:** Fuse LoRA computation with base computation to minimize memory traffic.

### Thread Occupancy

- Use 32-thread warps for coalesced memory access
- Threadgroup size: 32×8 for QKV, 16×16 for MLP
- Ensure rank dimensions are multiples of 4 for vectorization

### Fixed-Point Arithmetic

- Gates are Q15 (16-bit signed): -32768 to 32767 → -1.0 to 1.0
- Consider Q15 quantization for LoRA weights (future optimization)

---

## Success Criteria

- [ ] Metal shaders compile without errors
- [ ] Shader hash matches manifest
- [ ] `test_lora_vs_baseline` passes (outputs differ from baseline)
- [ ] `test_k_sparse_routing` passes (weighted sum correct)
- [ ] `test_deterministic_output` passes (reproducible)
- [ ] Performance: LoRA overhead < 20% vs base model
- [ ] VRAM usage: matches `AdapterWeights::vram_bytes` calculation

---

## Files to Create/Modify

### Create:
1. `crates/adapteros-lora-kernel-mtl/shaders/mplora_kernels.metal` - Metal shader source
2. `tests/test_lora_computation.rs` - Verification tests

### Modify:
1. `crates/adapteros-lora-kernel-mtl/src/fused_qkv.rs:131-181` - Buffer bindings
2. `crates/adapteros-lora-kernel-mtl/src/fused_mlp.rs:118-142` - Buffer bindings
3. `crates/adapteros-lora-kernel-mtl/manifests/metallib_manifest.json` - Update hash
4. `crates/adapteros-lora-kernel-mtl/shaders/mplora_kernels.metallib` - Recompile

---

## Estimated Timeline

- Metal shader implementation: 1-1.5 hours
- Rust buffer binding updates: 30 minutes
- Shader compilation and debugging: 30 minutes
- Test implementation: 30-45 minutes
- Performance tuning: 30 minutes

**Total:** 2-3 hours

---

## Dependencies

**Blocked by:** Access to Metal shader source files (`.metal`)
**Blocks:** Phase 5 (testing), full production deployment

---

## Next Steps

1. Locate or create Metal shader source files
2. Implement LoRA math in `fused_qkv_gqa` kernel
3. Implement LoRA math in `fused_mlp` kernel
4. Update Rust buffer bindings
5. Recompile shaders and update manifest
6. Run verification tests
7. Profile performance

---

**Status:** Specification complete, awaiting Metal shader source access to proceed with implementation.
