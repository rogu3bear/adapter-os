# CoreML ANE Acceleration Layer Patterns

## Overview

The `adapteros-lora-kernel-coreml` crate provides CoreML backend integration for Apple Neural Engine (ANE) acceleration. Located at `crates/adapteros-lora-kernel-coreml/`.

## Architecture

### Module Structure
- `lib.rs` - Main CoreMLBackend + MLTensor safe wrapper API
- `ffi.rs` - FFI bindings to CoreML.framework and Swift bridge
- `hybrid.rs` - HybridCoreMLBackend (CoreML + Rust/Accelerate for LoRA)
- `fusion.rs` - Offline LoRA weight fusion into base models
- `placement.rs` - CoreML graph import and LoRA placement resolution
- `matmul.rs` - Apple Accelerate BLAS operations (cblas_sgemm, cblas_sgemv)
- `config.rs` - ComputeUnits and CoreMLModelParams configuration
- `export.rs` - CoreML adapter export pipeline with validation
- `aos_loader.rs` - .aos archive loading utilities

## CoreML Model Loading & Compilation

### Loading Flow
1. Check for pre-compiled `.mlmodelc` (skip compilation)
2. Otherwise, compile `.mlpackage` using `xcrun coremlc compile`
3. Cache compiled models in `PlatformUtils::aos_model_cache_dir()/coreml/{hash}/`
4. Track model memory in ANE metrics via `ffi::record_model_load()`

### Compute Units
```rust
pub enum ComputeUnits {
    CpuOnly,           // Deterministic, slowest
    CpuAndGpu,         // May be non-deterministic
    CpuAndNeuralEngine, // Default - deterministic, power-efficient
    All,               // Optimal performance, determinism varies
}
```

**Production mode** enforces `CpuAndNeuralEngine` for guaranteed determinism.

## ANE Dispatch Patterns

### Operation Type Scheduling
```rust
pub enum OperationType {
    MatMul,      // ANE optimized (2-3x faster than GPU)
    Attention,   // ANE optimized
    Softmax,     // GPU preferred (transcendental exp() not on ANE)
    ElementWise, // GPU preferred
    TensorOp,    // Let CoreML decide
}
```

### Production vs Development Mode
- **Production**: All operations use `CpuAndNeuralEngine` for reproducibility
- **Development**: Per-operation optimization (MatMul/Attention → ANE, Softmax → GPU)

### API Version Detection
```rust
pub enum MltensorApiVersion {
    NotAvailable, // Pre-macOS 15
    Sequoia,      // macOS 15.x - Basic MLTensor
    Tahoe,        // macOS 26.x - Enhanced MLComputePolicy (required for deterministic scheduling)
}
```

## MLX Integration

### Hybrid Architecture
The `HybridCoreMLBackend` implements a split architecture:
1. **CoreML/ANE** handles base transformer inference → outputs `hidden_states`
2. **Rust/Accelerate** handles LM head projection + LoRA application

This enables **<1ms adapter hot-swap** while maintaining ANE acceleration for transformer computation.

### Hybrid Inference Flow
```
input_ids
    ↓
[CoreML: Embeddings + Transformer Layers]
    ↓
hidden_states [batch, seq, hidden_size]
    ↓
[Rust/Accelerate: LM Head + LoRA Fusion]
    ↓
logits [batch, seq, vocab_size]
```

### LoRA Application (Runtime)
```rust
// For each active adapter in router ring:
let a_out = matvec_accelerate(&lora.lora_a, last_hidden, rank, hidden_size);
let delta = matvec_accelerate(&lora.lora_b, &a_out, vocab_size, rank);
let combined_scale = gate_f32 * lora.scale;
axpy(combined_scale, &delta, &mut logits);
```

## Performance Optimization Patterns

### 1. Model Caching
- Compiled models cached by BLAKE3 hash of path
- `FusedModelCache` for pre-fused adapter combinations
- Memory-mapped file access via `memmap2`

### 2. Accelerate Framework Integration
```rust
// Uses cblas_sgemm for matrix multiply
// Uses cblas_sgemv for matrix-vector
// Uses cblas_saxpy for scaled addition
#[link(name = "Accelerate", kind = "framework")]
```

### 3. MLTensor Bridge Selection
- **Swift bridge**: Better performance on macOS 15+ (`swift_coreml_*` functions)
- **ObjC++ bridge**: Fallback (`coreml_*` functions)
- Runtime dispatch based on availability

### 4. Batch Operations (macOS 26+)
```rust
swift_coreml_batch_matmul() // Batched matrix multiplication with compute unit control
swift_coreml_tensor_to_floats_v2() // Async materialization option
```

## Model Caching Strategy

### Fusion Cache
```rust
pub struct FusedModelCache {
    cache_dir: PathBuf,
    max_cache_size_gb: f64,
}
```
- Content-addressable storage using BLAKE3 hash of fusion config
- LRU eviction when cache exceeds size limit
- Cache key includes: base_model_path + adapter configs (weights_path, gate_weight, alpha, rank)

### Compiled Model Cache
- Location: `{aos_model_cache_dir}/coreml/{path_hash}/`
- Avoids recompilation on subsequent loads
- Checked before calling `xcrun coremlc compile`

## Offline LoRA Fusion

### Formula
```
W_fused = W_base + sum_i(gate_i * alpha_i / rank_i * B_i @ A_i)
```

### Fusion Workflow
1. Load base model weights from safetensors
2. Load LoRA adapter weights (A and B matrices)
3. Compute fused weights per target (q_proj, k_proj, v_proj, o_proj, gate_proj, up_proj, down_proj)
4. Write fused weights to new safetensors file
5. Convert to CoreML `.mlpackage` using coremltools

### Target Modules
```rust
pub enum LoraTarget {
    QProj, KProj, VProj, OProj,  // Attention
    GateProj, UpProj, DownProj,  // MLP
}
```

## ANE Memory Tracking

### FFI Functions
```rust
swift_coreml_get_ane_memory_info() // Query ANE memory stats
swift_coreml_record_model_load()   // Track model load
swift_coreml_record_model_unload() // Track model unload
```

### AneMemoryInfo Structure
- `allocated_bytes` - Total ANE-allocated memory
- `used_bytes` - Currently used memory
- `cached_bytes` - Cached models/weights
- `peak_bytes` - Peak usage since boot
- `throttled` - Thermal throttling status

## Determinism Guarantees

### Production Mode Requirements
1. ANE must be available
2. Enforces `CpuAndNeuralEngine` compute units
3. On macOS < 26, MLTensor is disabled (lacks deterministic scheduling)
4. Stub mode uses `FixedSeed(0)` for reproducibility

### Attestation Report
```rust
DeterminismReport {
    backend_type: BackendType::CoreML,
    floating_point_mode: FloatingPointMode::Deterministic,
    determinism_level: DeterminismLevel::BoundedTolerance,
    compiler_flags: ["-O3", "-fno-fast-math"],
}
```

## Adapter Hot-Swap Semantics

### Two Paths
1. **SidecarDelta**: Base CoreML stays resident; LoRA deltas attached/detached at runtime
2. **FusedPackage**: Pre-fused `.mlmodelc` swapped via `load_model_internal`

### Hot-Swap Flow
```rust
attach_adapter(slot_id)   // Attach sidecar delta
detach_adapter(slot_id)   // Remove from memory, restore base if fused
switch_adapter(slot_id)   // Swap to fused bundle
```

## Key Configuration Tips

1. **Always use `CpuAndNeuralEngine`** in production for determinism
2. **Pre-fuse known adapter combinations** for zero runtime overhead
3. **Use hybrid backend** when you need fast LoRA switching (<1ms)
4. **Check `has_enhanced_api()`** for macOS 26+ features
5. **Monitor ANE memory** via `get_ane_memory_info()` to avoid OOM
6. **Validate ops compatibility** before export with `validate_coreml_ops()`

## Testing

```bash
# Run CoreML kernel tests (requires stub mode)
cargo test -p adapteros-lora-kernel-coreml --features coreml-stub

# Determinism tests
cargo test -p adapteros-lora-kernel-coreml --test determinism_tests
```

Note: Release builds with `coreml-stub` feature trigger compile error to prevent stub deployment to production.
