# Remaining Work: Complete Kernel Hot-Swap Rectification

**Date:** 2025-01-16
**Status:** 18/22 tasks remaining
**Critical Path:** Phase 2 (Kernel Integration)
**Estimated Effort:** 16-22 hours

---

## Overview

This document provides detailed implementation instructions for the remaining 18 corner cuts that need to be rectified. Tasks are organized by priority and dependency, with the critical path (Phase 2) highlighted first.

**Completed:** 4/22 (determinism, async I/O, VRAM tracking, safety docs)
**Remaining:** 18/22 (kernel integration, validation, testing)

---

## 🔴 CRITICAL PATH: Phase 2 - Kernel Integration (6-8 hours)

**Priority:** P0 - BLOCKING
**Dependencies:** None (ready to start)
**Impact:** Makes adapter weights actually work in kernel execution

### Overview

Currently, adapter weights are loaded into GPU VRAM but never passed to Metal kernels. The kernels receive only adapter IDs and gates, not the actual weight buffers. This phase wires the loaded weights into kernel execution.

### Task 2.3: Wire Weights to Kernel Execution (2-3 hours)

**File:** `crates/adapteros-lora-kernel-mtl/src/lib.rs`
**Function:** `run_transformer_layers()`
**Lines:** ~759-764

**Current Code (BROKEN):**
```rust
fn run_transformer_layers(&mut self, adapters: &[ActiveAdapter], io: &mut IoBuffers) -> Result<()> {
    // ... setup ...

    // Execute Fused QKV Kernel
    if let Some(ref mut qkv_kernel) = self.qkv_kernel {
        let lora_config = fused_qkv::LoraConfig::default();
        qkv_kernel.execute(
            &intermediate_buffers.hidden_states,
            &transformer_weights.q_weight,
            &transformer_weights.k_weight,
            &transformer_weights.v_weight,
            &intermediate_buffers.q_output,
            &intermediate_buffers.k_output,
            &intermediate_buffers.v_output,
            &lora_config,  // ← Default config, NO WEIGHTS
            self.ring_buffer.as_ref().unwrap(),
        )?;
    }

    // Execute Fused MLP Kernel
    if let Some(ref mut mlp_kernel) = self.mlp_kernel {
        let lora_config = fused_mlp::LoraConfig::default();
        mlp_kernel.execute(
            &intermediate_buffers.attention_output,
            &transformer_weights.gate_weight,
            &transformer_weights.up_weight,
            &transformer_weights.down_weight,
            &intermediate_buffers.mlp_output,
            &lora_config,  // ← Default config, NO WEIGHTS
            adapters,      // ← Only IDs and gates
        )?;
    }

    Ok(())
}
```

**Target Code (FIXED):**
```rust
fn run_transformer_layers(&mut self, adapters: &[ActiveAdapter], io: &mut IoBuffers) -> Result<()> {
    // ... setup ...

    // NEW: Lookup adapter weights from self.adapter_weights HashMap
    let adapter_weight_refs: Vec<&AdapterWeights> = adapters
        .iter()
        .filter_map(|a| {
            let id_u16 = (a.id & 0xFFFF) as u16;  // Truncate u32 to u16
            self.adapter_weights.get(&id_u16)
        })
        .collect();

    // NEW: Verify all adapters have loaded weights (fail fast)
    if adapter_weight_refs.len() != adapters.len() {
        let missing: Vec<u32> = adapters
            .iter()
            .filter(|a| {
                let id_u16 = (a.id & 0xFFFF) as u16;
                !self.adapter_weights.contains_key(&id_u16)
            })
            .map(|a| a.id)
            .collect();

        return Err(AosError::Kernel(format!(
            "Adapters not loaded in GPU: {:?}. Call load_adapter() first.", missing
        )));
    }

    tracing::debug!(
        num_active_adapters = adapters.len(),
        num_weights_loaded = adapter_weight_refs.len(),
        "Executing transformer with loaded adapter weights"
    );

    // Execute Fused QKV Kernel with adapter weights
    if let Some(ref mut qkv_kernel) = self.qkv_kernel {
        qkv_kernel.execute(
            &intermediate_buffers.hidden_states,
            &transformer_weights.q_weight,
            &transformer_weights.k_weight,
            &transformer_weights.v_weight,
            &intermediate_buffers.q_output,
            &intermediate_buffers.k_output,
            &intermediate_buffers.v_output,
            &adapter_weight_refs,  // ← Pass actual weights
            adapters,               // ← IDs and gates
            self.ring_buffer.as_ref().unwrap(),
        )?;
    }

    // Execute Fused MLP Kernel with adapter weights
    if let Some(ref mut mlp_kernel) = self.mlp_kernel {
        mlp_kernel.execute(
            &intermediate_buffers.attention_output,
            &transformer_weights.gate_weight,
            &transformer_weights.up_weight,
            &transformer_weights.down_weight,
            &intermediate_buffers.mlp_output,
            &adapter_weight_refs,  // ← Pass actual weights
            adapters,               // ← IDs and gates
        )?;
    }

    Ok(())
}
```

**Implementation Steps:**
1. Add weight lookup logic before kernel execution
2. Verify all adapters have loaded weights (error if missing)
3. Pass `&adapter_weight_refs` to kernel execute() calls
4. Update debug logging to show loaded vs active counts
5. Remove `LoraConfig::default()` calls (replaced by actual weights)

**Testing:**
```rust
// Should fail if adapter not loaded
let adapters = vec![ActiveAdapter { id: 999, gate: 32767 }];
assert!(kernels.run_transformer_layers(&adapters, &mut io).is_err());

// Should succeed after loading
kernels.load_adapter(999, &test_weights)?;
assert!(kernels.run_transformer_layers(&adapters, &mut io).is_ok());
```

---

### Task 2.1: Update FusedQkvKernel Signature (1-2 hours)

**File:** `crates/adapteros-lora-kernel-mtl/src/fused_qkv.rs`
**Function:** `execute()`

**Current Signature:**
```rust
pub fn execute(
    &mut self,
    hidden_states: &Buffer,
    q_weight: &Buffer,
    k_weight: &Buffer,
    v_weight: &Buffer,
    q_output: &Buffer,
    k_output: &Buffer,
    v_output: &Buffer,
    lora_config: &LoraConfig,  // ← Generic config
    ring_buffer: &RingBuffer,
) -> Result<()>
```

**Target Signature:**
```rust
pub fn execute(
    &mut self,
    hidden_states: &Buffer,
    q_weight: &Buffer,
    k_weight: &Buffer,
    v_weight: &Buffer,
    q_output: &Buffer,
    k_output: &Buffer,
    v_output: &Buffer,
    adapter_weights: &[&AdapterWeights],  // ← Actual weights
    adapters: &[ActiveAdapter],            // ← IDs and gates
    ring_buffer: &RingBuffer,
) -> Result<()>
```

**Implementation Changes:**

1. **Remove LoraConfig usage:**
```rust
// Delete:
let lora_config = &self.lora_config;

// Replace with actual adapter data from adapter_weights
```

2. **Bind adapter weight buffers to Metal encoder:**
```rust
// For each adapter, bind its LoRA A and B buffers
for (i, weights) in adapter_weights.iter().enumerate() {
    // Bind Q projection LoRA weights
    if let Some(q_a_buffer) = weights.lora_a_buffers.get(0) {
        encoder.set_buffer(
            10 + (i * 6),  // Buffer indices for adapter i
            Some(q_a_buffer),
            0
        );
    }
    if let Some(q_b_buffer) = weights.lora_b_buffers.get(0) {
        encoder.set_buffer(
            11 + (i * 6),
            Some(q_b_buffer),
            0
        );
    }

    // Repeat for K and V projections
    // K: indices 12-13, V: indices 14-15
}

// Also pass adapter metadata as uniform buffer
struct AdapterMetadata {
    rank: u32,
    alpha: f32,
    num_adapters: u32,
}
let metadata = adapter_weights.iter().map(|w| AdapterMetadata {
    rank: w.rank as u32,
    alpha: w.alpha,
    num_adapters: adapter_weights.len() as u32,
}).collect::<Vec<_>>();
```

3. **Update Metal shader binding:**
```rust
encoder.set_compute_pipeline_state(&self.pipeline_state);
encoder.set_buffer(0, Some(hidden_states), 0);
encoder.set_buffer(1, Some(q_weight), 0);  // Base weights
encoder.set_buffer(2, Some(k_weight), 0);
encoder.set_buffer(3, Some(v_weight), 0);
// ... existing buffers ...

// NEW: Adapter weight buffers (starting at index 10)
// [10-15]: Adapter 0 Q/K/V LoRA A/B
// [16-21]: Adapter 1 Q/K/V LoRA A/B
// [22-27]: Adapter 2 Q/K/V LoRA A/B
```

**Testing:**
```rust
// Test with real weights
let weights = create_test_adapter_weights(rank=4);
qkv_kernel.execute(..., &[&weights], &adapters, ring_buffer)?;

// Verify output != base model output
assert_ne!(q_output, base_q_output);
```

---

### Task 2.2: Update FusedMlpKernel Signature (1-2 hours)

**File:** `crates/adapteros-lora-kernel-mtl/src/fused_mlp.rs`
**Function:** `execute()`

**Current Signature:**
```rust
pub fn execute(
    &mut self,
    input: &Buffer,
    gate_weight: &Buffer,
    up_weight: &Buffer,
    down_weight: &Buffer,
    output: &Buffer,
    lora_config: &LoraConfig,  // ← Generic config
    adapters: &[ActiveAdapter],
) -> Result<()>
```

**Target Signature:**
```rust
pub fn execute(
    &mut self,
    input: &Buffer,
    gate_weight: &Buffer,
    up_weight: &Buffer,
    down_weight: &Buffer,
    output: &Buffer,
    adapter_weights: &[&AdapterWeights],  // ← Actual weights
    adapters: &[ActiveAdapter],
) -> Result<()>
```

**Implementation Changes:**

Similar to Task 2.1, but for MLP projections:

1. **Bind MLP LoRA buffers:**
```rust
for (i, weights) in adapter_weights.iter().enumerate() {
    // MLP down projection LoRA (indices 3-4 in lora_a/b_buffers)
    if let Some(mlp_down_a) = weights.lora_a_buffers.get(3) {
        encoder.set_buffer(10 + (i * 4), Some(mlp_down_a), 0);
    }
    if let Some(mlp_down_b) = weights.lora_b_buffers.get(3) {
        encoder.set_buffer(11 + (i * 4), Some(mlp_down_b), 0);
    }

    // MLP up projection LoRA (indices 4 in lora_a/b_buffers)
    if let Some(mlp_up_a) = weights.lora_a_buffers.get(4) {
        encoder.set_buffer(12 + (i * 4), Some(mlp_up_a), 0);
    }
    if let Some(mlp_up_b) = weights.lora_b_buffers.get(4) {
        encoder.set_buffer(13 + (i * 4), Some(mlp_up_b), 0);
    }
}
```

2. **Pass scaling factors:**
```rust
// Compute scaling: alpha / rank for each adapter
let scalings: Vec<f32> = adapter_weights
    .iter()
    .map(|w| w.alpha / (w.rank as f32))
    .collect();

// Upload to uniform buffer
encoder.set_bytes(
    20,  // Buffer index for scalings
    (scalings.len() * std::mem::size_of::<f32>()) as u64,
    scalings.as_ptr() as *const c_void,
);
```

**Testing:**
```rust
let output_with_adapter = execute_mlp(&adapter_weights)?;
let output_base = execute_mlp(&[])?;  // No adapters
assert_ne!(output_with_adapter, output_base);
```

---

### Task 2.4: Update Metal Shaders (2-3 hours)

**File:** `crates/adapteros-lora-kernel-mtl/src/kernels/mplora.metal`
**Functions:** `fused_mplora_qkv`, `fused_mplora_mlp`

**Current Shader (BROKEN):**
```metal
kernel void fused_mplora_mlp(
    device const float* input [[buffer(0)]],
    device const float* gate_weight [[buffer(1)]],
    device const float* up_weight [[buffer(2)]],
    device const float* down_weight [[buffer(3)]],
    device const float* shared_A [[buffer(4)]],        // ← NEVER SET
    device const float* adapter_Bs [[buffer(5)]],      // ← NEVER SET
    device const float* gates [[buffer(6)]],            // ← Only this works
    device float* output [[buffer(7)]],
    constant MPLoraConfig& config [[buffer(8)]],
    uint2 gid [[thread_position_in_grid]]
)
{
    uint seq_idx = gid.x;
    uint hidden_idx = gid.y;

    // Base model computation
    float result = 0.0;
    for (uint i = 0; i < config.hidden_size; i++) {
        result += input[seq_idx * config.hidden_size + i] *
                  gate_weight[hidden_idx * config.hidden_size + i];
    }

    // LoRA contribution (USES UNINITIALIZED BUFFERS)
    float lora_delta = 0.0;
    for (uint aid = 0; aid < config.num_adapters; aid++) {
        float gate = float(gates[aid]) / 32767.0;
        // ... uses shared_A and adapter_Bs which are garbage ...
    }

    output[seq_idx * config.output_size + hidden_idx] = result + lora_delta;
}
```

**Target Shader (FIXED):**
```metal
// NEW: Adapter metadata structure
struct AdapterMetadata {
    uint rank;
    float alpha;
    uint num_adapters;
};

kernel void fused_mplora_mlp(
    device const float* input [[buffer(0)]],
    device const float* gate_weight [[buffer(1)]],
    device const float* up_weight [[buffer(2)]],
    device const float* down_weight [[buffer(3)]],
    // NEW: Adapter weight buffers (up to 3 adapters * 4 buffers each = 12 buffers)
    device const float* adapter0_down_A [[buffer(10)]],
    device const float* adapter0_down_B [[buffer(11)]],
    device const float* adapter0_up_A [[buffer(12)]],
    device const float* adapter0_up_B [[buffer(13)]],
    device const float* adapter1_down_A [[buffer(14)]],
    device const float* adapter1_down_B [[buffer(15)]],
    device const float* adapter1_up_A [[buffer(16)]],
    device const float* adapter1_up_B [[buffer(17)]],
    device const float* adapter2_down_A [[buffer(18)]],
    device const float* adapter2_down_B [[buffer(19)]],
    device const float* adapter2_up_A [[buffer(20)]],
    device const float* adapter2_up_B [[buffer(21)]],
    // Gate values and metadata
    device const int16_t* gates [[buffer(30)]],
    constant AdapterMetadata* metadata [[buffer(31)]],
    device float* output [[buffer(40)]],
    constant MPLoraConfig& config [[buffer(41)]],
    uint2 gid [[thread_position_in_grid]]
)
{
    uint seq_idx = gid.x;
    uint hidden_idx = gid.y;

    // 1. Base model computation (unchanged)
    float base_result = 0.0;
    for (uint i = 0; i < config.hidden_size; i++) {
        base_result += input[seq_idx * config.hidden_size + i] *
                       gate_weight[hidden_idx * config.hidden_size + i];
    }

    // 2. LoRA adapter contributions (NEW: actually uses loaded weights)
    float lora_delta = 0.0;

    for (uint aid = 0; aid < metadata->num_adapters && aid < 3; aid++) {
        // Get gate value (Q15 format: [-32768, 32767] → [-1.0, 1.0])
        float gate = float(gates[aid]) / 32767.0;

        // Get scaling factor: (alpha / rank) * gate
        float scaling = (metadata->alpha / float(metadata->rank)) * gate;

        // Select correct adapter buffers based on adapter index
        device const float* lora_A;
        device const float* lora_B;

        if (aid == 0) {
            lora_A = adapter0_down_A;
            lora_B = adapter0_down_B;
        } else if (aid == 1) {
            lora_A = adapter1_down_A;
            lora_B = adapter1_down_B;
        } else {  // aid == 2
            lora_A = adapter2_down_A;
            lora_B = adapter2_down_B;
        }

        // Compute LoRA transformation: ΔW @ x = B @ (A @ x)
        // Step 1: Downsample with A matrix [rank × hidden_size]
        //         shared = A @ x
        float shared[64];  // Max rank assumption
        for (uint r = 0; r < metadata->rank; r++) {
            shared[r] = 0.0;
            for (uint h = 0; h < config.hidden_size; h++) {
                shared[r] += input[seq_idx * config.hidden_size + h] *
                            lora_A[r * config.hidden_size + h];
            }
        }

        // Step 2: Upsample with B matrix [hidden_size × rank]
        //         adapter_out = B @ shared
        float adapter_output = 0.0;
        for (uint r = 0; r < metadata->rank; r++) {
            adapter_output += shared[r] *
                             lora_B[hidden_idx * metadata->rank + r];
        }

        // Accumulate scaled LoRA contribution
        lora_delta += scaling * adapter_output;
    }

    // 3. Combine base model and LoRA contributions
    output[seq_idx * config.output_size + hidden_idx] = base_result + lora_delta;
}
```

**Implementation Notes:**

1. **Buffer Layout:**
   - Buffers 0-9: Base model weights and I/O
   - Buffers 10-29: Adapter weight buffers (max 3 adapters × 4 buffers each)
   - Buffers 30-39: Gates and metadata
   - Buffers 40+: Output and config

2. **Adapter Selection Logic:**
   - Use if/else chain for adapter index (Metal doesn't support dynamic buffer indexing)
   - Max 3 adapters hardcoded (can extend by adding more buffer parameters)
   - For >3 adapters, need to use buffer arrays or texture arrays

3. **LoRA Math Validation:**
   - Formula: `output = W_base @ x + Σᵢ (gateᵢ / 32767) * (alpha / rank) * (Bᵢ @ (Aᵢ @ x))`
   - A matrix: downsamples hidden_size → rank
   - B matrix: upsamples rank → hidden_size
   - Gate: weight in [-1, 1] from Q15 encoding
   - Scaling: (alpha / rank) controls LoRA strength

**Testing:**
```metal
// Test kernel with known inputs
float input[4] = {1.0, 0.0, 0.0, 0.0};  // One-hot
float lora_A[4*4] = {1.0, 0.0, ...};    // Identity-like
float lora_B[4*4] = {2.0, 0.0, ...};    // Scale by 2
int16_t gate = 32767;                    // Full weight

// Expected output: base + (1.0) * (8.0 / 4.0) * (2.0 * 1.0) = base + 4.0
```

---

## 🟡 MEDIUM PRIORITY: Phase 3 - Validation & Robustness (3-4 hours)

**Dependencies:** Can be done in parallel with Phase 2

### Task 3.1: Read Alpha from SafeTensors Metadata (1 hour)

**File:** `crates/adapteros-lora-kernel-mtl/src/lib.rs`
**Function:** `load_adapter()`
**Lines:** ~1077-1078

**Current Code:**
```rust
// Assume alpha = 2 * rank (common default)
let alpha = (2 * rank) as f32;
```

**Target Code:**
```rust
// Try to read alpha from SafeTensors metadata first
let alpha = if let Some(metadata) = tensors.metadata() {
    metadata
        .get("lora_alpha")
        .and_then(|v| v.parse::<f32>().ok())
        .or_else(|| {
            // Try alternate key names
            metadata.get("alpha")
                .and_then(|v| v.parse::<f32>().ok())
        })
        .unwrap_or_else(|| {
            tracing::warn!(
                adapter_id = id,
                "No lora_alpha in metadata, using default: 2*rank"
            );
            (2 * rank) as f32
        })
} else {
    // No metadata, use heuristic
    tracing::warn!(
        adapter_id = id,
        "No SafeTensors metadata, using default alpha: 2*rank"
    );
    (2 * rank) as f32
};

tracing::info!(adapter_id = id, rank = rank, alpha = alpha, "LoRA hyperparameters detected");
```

**Testing:**
```rust
// Create test SafeTensors with metadata
let mut metadata = HashMap::new();
metadata.insert("lora_alpha".to_string(), "16.0".to_string());
let tensors = SafeTensors::serialize_with_metadata(&weights, &metadata)?;

// Verify alpha is read correctly
assert_eq!(loaded_adapter.alpha, 16.0);
```

---

### Task 3.3: Add Tensor Shape Validation (1 hour)

**File:** `crates/adapteros-lora-kernel-mtl/src/lib.rs`
**Function:** `load_adapter()`
**Location:** After extracting tensors, before creating buffers

**Add Helper Function:**
```rust
/// Validate LoRA tensor shape matches expected dimensions
fn validate_lora_tensor_shape(
    tensor: &safetensors::tensor::TensorView,
    module_name: &str,
    expected_rank: usize,
    expected_dim: usize,
    is_a_matrix: bool,
) -> Result<()> {
    let shape = tensor.shape();

    // Must be 2D
    if shape.len() != 2 {
        return Err(AosError::Validation(format!(
            "{}: Expected 2D tensor, got {}D with shape {:?}",
            module_name, shape.len(), shape
        )));
    }

    if is_a_matrix {
        // A matrix: [rank, in_dim]
        if shape[0] != expected_rank {
            return Err(AosError::Validation(format!(
                "{}: A matrix rank mismatch - expected {}, got {}",
                module_name, expected_rank, shape[0]
            )));
        }
        if shape[1] != expected_dim {
            return Err(AosError::Validation(format!(
                "{}: A matrix input dim mismatch - expected {}, got {}",
                module_name, expected_dim, shape[1]
            )));
        }
    } else {
        // B matrix: [out_dim, rank]
        if shape[1] != expected_rank {
            return Err(AosError::Validation(format!(
                "{}: B matrix rank mismatch - expected {}, got {}",
                module_name, expected_rank, shape[1]
            )));
        }
        if shape[0] != expected_dim {
            return Err(AosError::Validation(format!(
                "{}: B matrix output dim mismatch - expected {}, got {}",
                module_name, expected_dim, shape[0]
            )));
        }
    }

    Ok(())
}
```

**Use in load_adapter():**
```rust
// After getting tensor data
if let Some(a_tensor) = a_data {
    validate_lora_tensor_shape(
        &a_tensor,
        &a_name,
        rank,
        4096,  // hidden_size for q/k/v, different for MLP
        true   // is A matrix
    )?;

    // ... create buffer ...
}
```

**Testing:**
```rust
// Test with wrong shape
let bad_tensor = create_tensor(shape=[8, 4096]);  // rank=8 but expected 4
assert!(load_adapter(id, &bad_weights).is_err());
```

---

### Task 3.4: Make Target Modules Configurable (30 min)

**File:** `crates/adapteros-lora-kernel-mtl/src/lib.rs`
**Function:** `load_adapter()`
**Lines:** ~1081

**Current Code:**
```rust
let target_modules = vec!["q_proj", "k_proj", "v_proj", "mlp.down_proj", "mlp.up_proj"];
```

**Target Code:**
```rust
// Try to read target modules from manifest
let target_modules: Vec<String> = if let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(weights) {
    manifest
        .get("target_modules")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(|| default_target_modules_for_architecture())
} else {
    default_target_modules_for_architecture()
};

fn default_target_modules_for_architecture() -> Vec<String> {
    // Default for Qwen2.5/Llama-style architectures
    vec![
        "q_proj".to_string(),
        "k_proj".to_string(),
        "v_proj".to_string(),
        "mlp.down_proj".to_string(),
        "mlp.up_proj".to_string(),
    ]
}
```

**Testing:**
```rust
// Test with custom target modules in manifest
let manifest = json!({
    "target_modules": ["q_proj", "v_proj", "mlp.gate_proj"]
});
// Verify only those modules are loaded
```

---

### Task 3.5: Strict Mode for Missing Tensors (30 min)

**File:** `crates/adapteros-lora-kernel-mtl/src/lib.rs`
**Function:** `load_adapter()`
**Lines:** ~1091, 1098 (where .ok() swallows errors)

**Current Code:**
```rust
let a_data = tensors.tensor(&a_name)
    .map_err(|_| {
        warn!(module = %module, "LoRA A matrix not found, using zero buffer");
        AosError::Validation(format!("Missing {}", a_name))
    })
    .ok();  // ← Swallows error!
```

**Target Code:**
```rust
let strict_mode = std::env::var("AOS_STRICT_ADAPTER_LOADING").is_ok();

let a_data = tensors.tensor(&a_name).map_err(|e| {
    if strict_mode {
        // Strict mode: fail immediately
        return Err(AosError::Validation(format!(
            "Missing required tensor '{}': {}. Set AOS_STRICT_ADAPTER_LOADING=false to use zero fallback.",
            a_name, e
        )));
    } else {
        // Permissive mode: warn and use zero buffer
        warn!(
            module = %module,
            tensor = %a_name,
            "LoRA matrix not found, using zero buffer fallback"
        );
        Ok(None)  // Signal to use zero buffer
    }
})?;

// Handle fallback
let a_buffer = if let Some(tensor) = a_data {
    // Create buffer from tensor data
    self.device.new_buffer_with_data(...)
} else {
    // Create zero buffer
    warn!(module = %module, "Creating zero buffer for missing LoRA A matrix");
    self.device.new_buffer(...)
};
```

**Documentation:**
```bash
# Strict mode: fail on any missing tensor
export AOS_STRICT_ADAPTER_LOADING=1

# Permissive mode (default): use zero buffers for missing tensors
unset AOS_STRICT_ADAPTER_LOADING
```

---

## 🟢 LOW PRIORITY: Phase 4 - Safety & Polish (2-3 hours)

**Dependencies:** Can be done anytime

### Task 4.2: Fix TOCTOU Race with Entry API (30 min)

**File:** `crates/adapteros-lora-kernel-mtl/src/lib.rs`
**Function:** `load_adapter()`
**Lines:** ~1051-1055

**Current Code (TOCTOU):**
```rust
// Check if adapter already loaded
if self.adapter_weights.contains_key(&id) {  // ← Check
    warn!(adapter_id = id, "Adapter already loaded, unloading first");
    self.unload_adapter(id)?;                 // ← Use (race here)
}
```

**Target Code (Atomic):**
```rust
use std::collections::hash_map::Entry;

// Atomically check and remove if present
match self.adapter_weights.entry(id) {
    Entry::Occupied(entry) => {
        warn!(
            adapter_id = id,
            vram_bytes = entry.get().vram_bytes,
            "Adapter already loaded, replacing"
        );
        entry.remove();  // Atomic removal
    }
    Entry::Vacant(_) => {
        // Not present, proceed with loading
    }
}
```

**Benefits:**
- No race between check and action
- Atomic operation (no concurrent modification possible)
- Cleaner code (one HashMap access instead of two)

---

### Task 4.3: Cleanup on Partial Failure (1 hour)

**File:** `crates/adapteros-lora-kernel-mtl/src/lib.rs`
**Function:** `load_adapter()`
**Lines:** ~1086-1151 (buffer creation loop)

**Current Code (Leaks on Error):**
```rust
for module in &target_modules {
    // ...
    let a_buffer = self.device.new_buffer_with_data(...);  // ← May fail here
    lora_a_buffers.push(a_buffer);

    let b_buffer = self.device.new_buffer_with_data(...);  // ← Or here
    lora_b_buffers.push(b_buffer);

    // If creation fails, already-created buffers leak!
}
```

**Target Code (RAII Cleanup):**
```rust
// Collect Results, early return cleans up automatically
let lora_buffers: Result<(Vec<Buffer>, Vec<Buffer>)> = target_modules
    .iter()
    .map(|module| {
        let a_name = format!("{}.lora_A", module);
        let b_name = format!("{}.lora_B", module);

        // Get tensor data
        let a_tensor = tensors.tensor(&a_name)?;
        let b_tensor = tensors.tensor(&b_name)?;

        // Create buffers (both or neither)
        let a_buffer = self.device.new_buffer_with_data(
            a_tensor.data().as_ptr() as *const c_void,
            a_tensor.data().len() as u64,
            metal::MTLResourceOptions::StorageModeShared,
        );

        let b_buffer = self.device.new_buffer_with_data(
            b_tensor.data().as_ptr() as *const c_void,
            b_tensor.data().len() as u64,
            metal::MTLResourceOptions::StorageModeShared,
        );

        Ok((a_buffer, b_buffer))
    })
    .collect::<Result<Vec<(Buffer, Buffer)>>>()?;

// Unzip into separate vectors
let (lora_a_buffers, lora_b_buffers): (Vec<_>, Vec<_>) = lora_buffers.into_iter().unzip();
```

**Benefits:**
- If any buffer creation fails, iterator stops and returns Err
- No manual cleanup needed (Metal buffers drop automatically)
- Type-safe (Result<Vec<_>> forces error handling)

---

## 🧪 TESTING: Phase 5 - Comprehensive Verification (4-6 hours)

**Dependencies:** Requires Phase 2 complete

### Task 5.1: Determinism Tests (2-3 hours)

**File:** `tests/hotswap_determinism.rs` (create new)

**Test 1: Adapter ID Determinism**
```rust
use adapteros_lora_worker::adapter_hotswap::adapter_id_to_u16;

#[test]
fn test_adapter_id_determinism() {
    // Same input → same output
    let id1 = adapter_id_to_u16("my_adapter");
    let id2 = adapter_id_to_u16("my_adapter");
    assert_eq!(id1, id2, "Adapter ID mapping must be deterministic");

    // Different inputs → different outputs (high probability)
    let id3 = adapter_id_to_u16("different_adapter");
    assert_ne!(id1, id3, "Different adapters should have different IDs");

    // Test collision resistance
    let mut ids = std::collections::HashSet::new();
    for i in 0..1000 {
        let id = adapter_id_to_u16(&format!("adapter_{}", i));
        assert!(ids.insert(id), "Collision detected at iteration {}", i);
    }
}
```

**Test 2: Hot-Swap Determinism**
```rust
use adapteros_lora_kernel_mtl::MetalKernels;
use adapteros_core::B3Hash;

#[tokio::test]
async fn test_hotswap_determinism_with_real_weights() -> Result<()> {
    // Create test adapters
    let adapter_a = create_test_adapter("test_a", rank=4, alpha=8.0)?;
    let adapter_b = create_test_adapter("test_b", rank=4, alpha=8.0)?;
    let seed = B3Hash::hash(b"test_seed");
    let prompt = vec![1, 2, 3, 4, 5];  // Test input tokens

    // Scenario 1: Load A, generate
    let mut kernels1 = MetalKernels::new()?;
    kernels1.load(...)?;  // Load base model
    kernels1.load_adapter(1, &adapter_a)?;

    let output_a1 = run_inference(&mut kernels1, &prompt, seed)?;

    // Scenario 2: Hot-swap to B, generate
    kernels1.unload_adapter(1)?;
    kernels1.load_adapter(2, &adapter_b)?;

    let output_b1 = run_inference(&mut kernels1, &prompt, seed)?;

    // Scenario 3: Fresh start with B pre-loaded, generate
    let mut kernels2 = MetalKernels::new()?;
    kernels2.load(...)?;
    kernels2.load_adapter(2, &adapter_b)?;

    let output_b2 = run_inference(&mut kernels2, &prompt, seed)?;

    // Assertions
    assert_ne!(
        output_a1, output_b1,
        "Different adapters must produce different outputs"
    );

    assert_eq!(
        output_b1, output_b2,
        "Same adapter must be deterministic (hot-swap vs pre-loaded)"
    );

    Ok(())
}

/// Helper: Run inference with current adapter configuration
fn run_inference(
    kernels: &mut MetalKernels,
    prompt: &[u32],
    seed: B3Hash,
) -> Result<Vec<f32>> {
    let mut io = IoBuffers::new(prompt, vocab_size=152064)?;
    let adapters = vec![ActiveAdapter { id: 1, gate: 32767 }];

    kernels.run_step(&RouterRing::from_adapters(&adapters), &mut io)?;

    Ok(io.output_logits.clone())
}
```

**Test 3: Weights Actually Used**
```rust
#[tokio::test]
async fn test_adapter_weights_actually_loaded() -> Result<()> {
    let mut kernels = MetalKernels::new()?;
    kernels.load(...)?;

    // Create adapter with known LoRA weights
    let adapter = create_known_lora_adapter(
        rank=4,
        alpha=8.0,
        // A matrix: simple scaling by 0.5
        a_values=vec![0.5; 4*4096],
        // B matrix: simple scaling by 2.0
        b_values=vec![2.0; 4096*4],
    )?;

    kernels.load_adapter(1, &adapter)?;

    // Run inference with known input
    let input = vec![1.0; 4096];  // All ones
    let output_with_adapter = run_forward_pass(&mut kernels, &input, adapter_active=true)?;
    let output_base = run_forward_pass(&mut kernels, &input, adapter_active=false)?;

    // Compute expected LoRA contribution
    // ΔW @ x = (alpha/rank) * (B @ (A @ x))
    //        = (8.0/4.0) * (2.0 * (0.5 * 1.0))
    //        = 2.0 * 1.0 = 2.0 per element
    let expected_delta = 2.0;

    // Verify adapter modifies output
    assert_ne!(
        output_with_adapter, output_base,
        "Adapter must change output (weights must be used)"
    );

    // Verify delta is approximately correct
    for (i, (&with_adapter, &base)) in output_with_adapter.iter().zip(output_base.iter()).enumerate() {
        let actual_delta = with_adapter - base;
        assert!(
            (actual_delta - expected_delta).abs() < 0.1,
            "Element {}: expected delta {}, got {}",
            i, expected_delta, actual_delta
        );
    }

    Ok(())
}
```

**Helper: Create Test Adapter**
```rust
/// Create small test adapter with known weights for testing
fn create_test_adapter(
    name: &str,
    rank: usize,
    alpha: f32,
) -> Result<Vec<u8>> {
    use safetensors::SafeTensors;
    use rand::SeedableRng;

    // Deterministic RNG for reproducible test adapters
    let seed = B3Hash::hash(name.as_bytes());
    let mut rng = rand_chacha::ChaCha20Rng::from_seed(seed.to_bytes()[..32].try_into().unwrap());

    let mut tensors = HashMap::new();

    // Create LoRA weights for each target module
    for module in &["q_proj", "k_proj", "v_proj", "mlp.down_proj", "mlp.up_proj"] {
        // A matrix: [rank, hidden_size]
        let a_shape = if module.contains("mlp") {
            vec![rank, 11008]  // MLP hidden size
        } else {
            vec![rank, 4096]   // Attention hidden size
        };

        let a_data: Vec<f32> = (0..a_shape.iter().product())
            .map(|_| rng.gen_range(-0.1..0.1))
            .collect();

        tensors.insert(
            format!("{}.lora_A", module),
            (a_data, a_shape)
        );

        // B matrix: [hidden_size, rank]
        let b_shape = vec![a_shape[1], rank];
        let b_data: Vec<f32> = (0..b_shape.iter().product())
            .map(|_| rng.gen_range(-0.1..0.1))
            .collect();

        tensors.insert(
            format!("{}.lora_B", module),
            (b_data, b_shape)
        );
    }

    // Add metadata
    let mut metadata = HashMap::new();
    metadata.insert("lora_alpha".to_string(), alpha.to_string());
    metadata.insert("lora_rank".to_string(), rank.to_string());

    // Serialize to SafeTensors format
    SafeTensors::serialize_with_metadata(&tensors, &metadata)
}
```

---

### Task 5.2: Enable Integration Tests (1 hour)

**File:** `tests/kernel_workflow_integration.rs`

**Current State:**
```rust
#[ignore]  // ← Remove this
#[tokio::test]
async fn test_sequential_workflow() -> Result<()> {
    let backend = MockAdapterBackend::new();  // ← Replace with real Metal
    // ...
}
```

**Target State:**
```rust
#[tokio::test]  // No more #[ignore]
async fn test_sequential_workflow_with_real_metal() -> Result<()> {
    // Use real Metal kernels instead of mock
    let mut kernels = MetalKernels::new()?;
    kernels.load(&plan_bytes)?;

    // Load test adapters
    let adapter_a = create_test_adapter("workflow_test_a", rank=4, alpha=8.0)?;
    let adapter_b = create_test_adapter("workflow_test_b", rank=4, alpha=8.0)?;
    kernels.load_adapter(1, &adapter_a)?;
    kernels.load_adapter(2, &adapter_b)?;

    let backend = Arc::new(KernelAdapterBackend::new(
        Arc::new(Mutex::new(kernels)),
        vec!["test_a".to_string(), "test_b".to_string()],
        152064,  // vocab_size
    ));

    let executor = WorkflowExecutor::new(
        WorkflowType::Sequential,
        vec!["test_a".to_string(), "test_b".to_string()],
        backend,
    );

    let context = WorkflowContext {
        input_tokens: vec![1, 2, 3, 4, 5],
        model_state: HashMap::new(),
        metadata: HashMap::from([
            ("request_id".to_string(), "test-123".to_string())
        ]),
    };

    let result = executor.execute(context).await?;

    // Verify real execution
    assert!(result.output_tokens.len() > 0);
    assert_ne!(result.output_tokens, vec![1, 2, 3, 4, 5], "Output must differ from input");
    assert_eq!(result.stats.adapters_executed, 2);

    Ok(())
}
```

**Apply to All Tests:**
- Remove all `#[ignore]` attributes
- Replace `MockAdapterBackend` with real `MetalKernels`
- Update assertions to verify real outputs (not mock values)
- Ensure cleanup (unload adapters between tests)

---

### Task 5.3: Test Adapter Utility (1 hour)

**File:** `tests/common/test_adapters.rs` (create new)

```rust
//! Test adapter utility for creating small adapters with known weights

use adapteros_core::{B3Hash, Result};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use safetensors::SafeTensors;
use std::collections::HashMap;

/// Create a test adapter with random weights (deterministic from name)
pub fn create_test_adapter(name: &str, rank: usize, alpha: f32) -> Result<Vec<u8>> {
    let seed = B3Hash::hash(name.as_bytes());
    let mut rng = ChaCha20Rng::from_seed(seed.to_bytes()[..32].try_into().unwrap());

    let tensors = create_lora_tensors(&mut rng, rank);
    let metadata = create_metadata(rank, alpha);

    SafeTensors::serialize_with_metadata(&tensors, &metadata)
}

/// Create adapter with known constant weights for testing
pub fn create_known_lora_adapter(
    rank: usize,
    alpha: f32,
    a_scale: f32,  // All A matrix values
    b_scale: f32,  // All B matrix values
) -> Result<Vec<u8>> {
    let mut tensors = HashMap::new();

    for module in &["q_proj", "k_proj", "v_proj", "mlp.down_proj", "mlp.up_proj"] {
        let hidden_size = if module.contains("mlp") { 11008 } else { 4096 };

        // A matrix: all values = a_scale
        let a_data = vec![a_scale; rank * hidden_size];
        tensors.insert(
            format!("{}.lora_A", module),
            (a_data, vec![rank, hidden_size])
        );

        // B matrix: all values = b_scale
        let b_data = vec![b_scale; hidden_size * rank];
        tensors.insert(
            format!("{}.lora_B", module),
            (b_data, vec![hidden_size, rank])
        );
    }

    let metadata = create_metadata(rank, alpha);
    SafeTensors::serialize_with_metadata(&tensors, &metadata)
}

/// Create adapter with identity-like LoRA (for testing passthrough)
pub fn create_identity_adapter(rank: usize) -> Result<Vec<u8>> {
    // A = I (identity up to rank), B = I (identity up to rank)
    // ΔW @ x = (alpha/rank) * (I @ (I @ x)) = (alpha/rank) * x
    create_known_lora_adapter(rank, rank as f32, 1.0, 1.0)
    // Result: (rank/rank) * 1.0 * 1.0 = 1.0 → output = base + x
}

fn create_lora_tensors(rng: &mut ChaCha20Rng, rank: usize) -> HashMap<String, (Vec<f32>, Vec<usize>)> {
    let mut tensors = HashMap::new();

    for module in &["q_proj", "k_proj", "v_proj", "mlp.down_proj", "mlp.up_proj"] {
        let hidden_size = if module.contains("mlp") { 11008 } else { 4096 };

        let a_data: Vec<f32> = (0..rank * hidden_size)
            .map(|_| rng.gen_range(-0.1..0.1))
            .collect();
        tensors.insert(format!("{}.lora_A", module), (a_data, vec![rank, hidden_size]));

        let b_data: Vec<f32> = (0..hidden_size * rank)
            .map(|_| rng.gen_range(-0.1..0.1))
            .collect();
        tensors.insert(format!("{}.lora_B", module), (b_data, vec![hidden_size, rank]));
    }

    tensors
}

fn create_metadata(rank: usize, alpha: f32) -> HashMap<String, String> {
    let mut metadata = HashMap::new();
    metadata.insert("lora_alpha".to_string(), alpha.to_string());
    metadata.insert("lora_rank".to_string(), rank.to_string());
    metadata.insert("created_by".to_string(), "test_utility".to_string());
    metadata
}
```

**Usage in Tests:**
```rust
use crate::common::test_adapters::*;

#[test]
fn test_with_known_adapter() {
    let adapter = create_known_lora_adapter(rank=4, alpha=8.0, a_scale=0.5, b_scale=2.0)?;
    // Expected LoRA delta: (8.0/4.0) * (2.0 * 0.5) = 2.0
}
```

---

## 📋 Summary: Remaining Work Checklist

### Critical Path (Must Do First)
- [ ] **Phase 2.3:** Wire weights to `run_transformer_layers()` (2-3h)
- [ ] **Phase 2.1:** Update `FusedQkvKernel::execute()` signature (1-2h)
- [ ] **Phase 2.2:** Update `FusedMlpKernel::execute()` signature (1-2h)
- [ ] **Phase 2.4:** Update Metal shaders with LoRA math (2-3h)

### High Priority (Do Next)
- [ ] **Phase 5.1:** Add determinism tests (2-3h)
- [ ] **Phase 5.2:** Enable integration tests (1h)
- [ ] **Phase 5.3:** Create test adapter utility (1h)

### Medium Priority (Nice to Have)
- [ ] **Phase 3.1:** Read alpha from metadata (1h)
- [ ] **Phase 3.3:** Add tensor shape validation (1h)
- [ ] **Phase 3.4:** Configurable target modules (30min)
- [ ] **Phase 3.5:** Strict mode for missing tensors (30min)

### Low Priority (Polish)
- [ ] **Phase 4.2:** Fix TOCTOU race (30min)
- [ ] **Phase 4.3:** Cleanup on partial failure (1h)
- [ ] **Phase 4.4:** Optimal Metal storage mode (deferred)
- [ ] **Phase 4.5:** Buffer alignment check (deferred)

### Deferred (Working Fine As-Is)
- [ ] **Phase 1.3:** Use AOS2Loader (code duplication)
- [ ] **Phase 1.4:** Fix type system hack (cosmetic)

---

## 📊 Effort Breakdown

| Phase | Tasks | Estimated Hours | Priority |
|-------|-------|----------------|----------|
| Phase 2 (Kernel Integration) | 4 tasks | 6-8 hours | **CRITICAL** |
| Phase 5 (Testing) | 3 tasks | 4-6 hours | HIGH |
| Phase 3 (Validation) | 4 tasks | 3-4 hours | MEDIUM |
| Phase 4 (Safety) | 2 tasks | 2-3 hours | LOW |
| **Total Remaining** | **13 tasks** | **15-21 hours** | - |

**Already Completed:** 4 tasks, 4-6 hours (determinism, async, VRAM, safety docs)
**Grand Total:** 22 tasks, 20-28 hours (original estimate accurate)

---

## 🎯 Success Criteria

### After Phase 2 (Kernel Integration)
- [ ] Adapter weights passed to Metal shaders
- [ ] Metal shaders implement real LoRA math
- [ ] Output != base model output when adapter loaded
- [ ] No placeholder computation in code

### After Phase 5 (Testing)
- [ ] Determinism tests pass (same adapter → same output)
- [ ] Hot-swap tests pass (swap preserves determinism)
- [ ] Integration tests enabled and passing
- [ ] Test coverage >80% for hot-swap code paths

### After All Phases
- [ ] All 22 corners rectified
- [ ] Production-ready code quality
- [ ] Comprehensive test coverage
- [ ] Full documentation updated

---

## 📁 Files Requiring Changes

| File | Tasks | Est. Hours |
|------|-------|------------|
| `crates/adapteros-lora-kernel-mtl/src/lib.rs` | 2.3, 3.1, 3.3, 3.4, 3.5, 4.2, 4.3 | 6-8h |
| `crates/adapteros-lora-kernel-mtl/src/fused_mlp.rs` | 2.2 | 1-2h |
| `crates/adapteros-lora-kernel-mtl/src/fused_qkv.rs` | 2.1 | 1-2h |
| `crates/adapteros-lora-kernel-mtl/src/kernels/mplora.metal` | 2.4 | 2-3h |
| `tests/hotswap_determinism.rs` (new) | 5.1 | 2-3h |
| `tests/kernel_workflow_integration.rs` | 5.2 | 1h |
| `tests/common/test_adapters.rs` (new) | 5.3 | 1h |

---

## 🚀 Recommended Execution Order

1. **Start:** Phase 2.3 (wire weights to execution) - Immediate impact
2. **Then:** Phase 2.1 + 2.2 (update signatures) - Unblocks shader work
3. **Then:** Phase 2.4 (Metal shaders) - Completes critical path
4. **Verify:** Phase 5.1 (determinism tests) - Prove it works
5. **Polish:** Phase 3 (validation) - Robustness
6. **Finalize:** Phase 4 + 5.2/5.3 - Cleanup and full coverage

**Total Timeline:** 2-3 days of focused work, or 1 week part-time.

---

## 📖 Additional Resources

**References:**
- LoRA Paper: https://arxiv.org/abs/2106.09685
- Metal Shading Language Spec: https://developer.apple.com/metal/Metal-Shading-Language-Specification.pdf
- SafeTensors Format: https://github.com/huggingface/safetensors

**Related Docs:**
- `docs/KERNEL_HOTSWAP_ARCHITECTURE.md` - Architecture overview
- `docs/KERNEL_HOTSWAP_IMPLEMENTATION_STATUS.md` - What's done
- `docs/CORNERS_RECTIFIED.md` - What's fixed so far
- `CLAUDE.md` - Coding standards and patterns

---

**Last Updated:** 2025-01-16
**Status:** Ready to implement
**Next Step:** Begin Phase 2.3 (wire weights to execution)
