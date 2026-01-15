# Backend Parity Documentation

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.  
**Last Updated:** 2026-01-02  
**Purpose:** Comprehensive comparison of MLX, CoreML, and Metal backends covering numerical precision, determinism guarantees, operator coverage, performance profiles, memory usage, and testing strategies.

---

## Table of Contents

1. [Overview](#overview)
2. [Numerical Precision](#numerical-precision)
3. [Determinism Guarantees](#determinism-guarantees)
4. [Operator Coverage](#operator-coverage)
5. [Performance Profiles](#performance-profiles)
6. [Memory Usage](#memory-usage)
7. [Testing Strategies](#testing-strategies)
8. [Backend Selection Guide](#backend-selection-guide)

---

## Overview

adapterOS supports three primary backends for LoRA inference on Apple Silicon:

| Backend | Primary Use Case | Hardware Target | Status |
|---------|-----------------|-----------------|--------|
| **MLX** | Production inference, training, research | Apple Silicon (M1+) | ✅ Production Ready |
| **CoreML** | Production inference with audit trails, power efficiency | Apple Neural Engine (M1+) | ✅ Production Ready |
| **Metal** | Legacy hardware, development/testing | Metal GPU (Intel/Apple Silicon) | ✅ Implemented |

### Priority Order

The system uses an MLX-first priority chain:

```
MLX → CoreML → MlxBridge → Metal → CPU
```

**Rationale:** MLX provides flexibility and HKDF-seeded determinism. CoreML offers ANE acceleration when MLX is unavailable. Metal serves as a GPU fallback for legacy hardware.

---

## Numerical Precision

### Floating-Point Formats

| Backend | Default Precision | Supported Formats | Notes |
|---------|------------------|-------------------|-------|
| **MLX** | FP32 (CPU), FP16 (GPU) | FP32, FP16, INT8, INT4 | Unified memory, GPU-accelerated |
| **CoreML** | FP16 (ANE), FP32 (GPU/CPU) | FP16, FP32 | ANE optimized for FP16 |
| **Metal** | FP32 | FP32, FP16 | Precompiled shaders, GPU execution |

### Precision Characteristics

#### MLX Backend

- **Default:** FP32 for CPU operations, FP16 for GPU operations
- **Quantization:** Supports INT8 and INT4 quantization for model weights
- **Intermediate Precision:** Uses FP64 for intermediate calculations (Kahan summation)
- **Rounding:** IEEE 754 compliant, no `-ffast-math` flags
- **Determinism:** HKDF-seeded RNG ensures reproducible sampling, but execution order may vary

**Example:**
```rust
// MLX uses higher precision for intermediate calculations
let sum = kahan_sum(&values);  // FP64 accumulator
let result = sum as f32;       // Final FP32 output
```

#### CoreML Backend

- **Default:** FP16 for ANE, FP32 for GPU/CPU fallback
- **ANE Optimization:** ANE performs best with FP16 precision
- **Softmax Precision:** MLTensor path (macOS 15+) may produce slightly different results compared to MLMultiArray path due to FP16 precision differences
- **Rounding:** Hardware-dependent (ANE uses fixed-point arithmetic)

**Known Issue:** On models with large vocabulary sizes (32K+), the MLTensor softmax implementation may produce slightly different results compared to the MLMultiArray path due to FP16 precision differences.

**Workaround:**
```rust
// Force MLMultiArray path on macOS 15+ if strict numerical compatibility is required
unsafe { ffi::swift_coreml_force_mlmultiarray_path(true); }
```

#### Metal Backend

- **Default:** FP32
- **Shader Precision:** Precompiled Metal shaders use FP32 arithmetic
- **Rounding:** Controlled via Metal shader compilation (no runtime variance)
- **Determinism:** Precompiled shaders ensure consistent floating-point operations

### Q15 Quantization (Router Gates)

All backends use Q15 quantization for router gate values:

- **Denominator:** `32767.0` (NOT `32768.0`) - Critical invariant
- **Resolution:** 1/32767 ≈ 3.05e-5 (0.00305%)
- **Range:** [-32768, 32767] for i16 representation
- **Rounding:** Round-to-nearest (`.round()`)

**Implementation:**
```rust
// Forward (f32 → Q15)
let gate_q15 = (gate_f32 * 32767.0).round() as i16;

// Backward (Q15 → f32)
let gate_f32 = gate_q15 as f32 / 32767.0;
```

**Why 32767 and not 32768?**
- i16 range: -32768 to 32767
- Using 32768 would overflow to -32768
- 32767 allows exact representation of 1.0
- Maintains determinism across architectures

### Numerical Stability Techniques

All backends implement:

1. **Kahan Summation:** Prevents rounding drift in softmax calculations
2. **IEEE 754 Compliance:** No `-ffast-math` flags allowed
3. **NaN Handling:** Canonical NaN representation for deterministic hashing
4. **Adaptive Scaling:** Handles out-of-range values in training

**Verification:**
```bash
# Check for fast-math flags (should fail)
bash scripts/check_fast_math_flags.sh
```

---

## Determinism Guarantees

### Determinism Levels

| Backend | Determinism Type | Guarantee Level | Attestation |
|---------|-----------------|----------------|-------------|
| **MLX** | Bit-exact | Guaranteed when properly seeded | `deterministic: true` |
| **CoreML** | Bit-exact (ANE), Conditional (GPU) | ANE: guaranteed, GPU: best-effort | `deterministic: ane_available` |
| **Metal** | Bit-exact | Guaranteed with precompiled shaders | `deterministic: true` |

### MLX Determinism

**Status:** Bit-exact determinism when HKDF-seeded and using real MLX (not stub mode)

**Guarantees:**
- ✅ RNG state seeded via HKDF-SHA256 from manifest hash
- ✅ Dropout masks reproducible (same seed → same pattern)
- ✅ Sampling operations reproducible
- ✅ Routing decisions deterministic (Q15 quantized gates)

**Non-Deterministic Aspects:**
- ❌ Execution order (GPU scheduler may reorder async ops)
- ❌ Floating-point rounding (may vary in multi-threaded contexts)
- ❌ Memory access patterns (unfused kernels may execute in different order)
- ❌ Accumulated numerical errors (small variations across runs)

**Attestation Conditions:**
```rust
deterministic: seeded && !is_stub_active && IS_REAL_MLX
determinism_level: if conditions_met { BitExact } else { None }
```

**Attestation Report (when conditions met):**
```rust
DeterminismReport {
    backend_type: BackendType::Mlx,
    rng_seed_method: RngSeedingMethod::HkdfSeeded,
    floating_point_mode: FloatingPointMode::Deterministic,
    deterministic: true,  // ✅ Bit-exact deterministic
}
```

**Seed Derivation:**
```rust
// Global seed from model manifest hash
let manifest_hash = manifest.compute_hash()?;
let global_seed = derive_seed(&manifest_hash, "executor");

// MLX backend seed
let model_hash = B3Hash::hash(model_path.as_bytes());
let mlx_seed = derive_seed(&model_hash, "mlx-backend:{model_path_hash}");
```

### CoreML Determinism

**Status:** Guaranteed determinism when ANE is available

**ANE Determinism Conditions:**
1. ✅ ANE available (`ane_status.available`)
2. ✅ ANE capable (`ane_status.deterministic`)
3. ✅ ANE-only compute units (`CpuAndNeuralEngine` or `CpuOnly`)
4. ✅ MLTensor deterministic (macOS 26+ compute policy or MLTensor disabled)

**Production Mode Enforcement:**
```rust
if config.production_mode {
    // Verify ANE is available
    if !self.ane_status.available {
        return Err(AosError::Config(
            "Production mode requires Neural Engine availability".into()
        ));
    }

    // Verify ANE-only compute units
    if !matches!(self.compute_units, ComputeUnits::CpuAndNeuralEngine | ComputeUnits::CpuOnly) {
        return Err(AosError::Config(
            "Production mode requires CpuAndNeuralEngine compute units".into()
        ));
    }
}
```

**GPU Fallback (Non-Deterministic):**
- ⚠️ May be non-deterministic (depends on Metal implementation)
- ⚠️ Attestation reports `deterministic: false`
- ⚠️ Production mode should reject GPU fallback

**Attestation Report:**
```rust
DeterminismReport {
    backend_type: BackendType::CoreML,
    rng_seed_method: if deterministic {
        RngSeedingMethod::HkdfSeeded
    } else {
        RngSeedingMethod::SystemEntropy
    },
    floating_point_mode: if deterministic {
        FloatingPointMode::Deterministic
    } else {
        FloatingPointMode::Unknown
    },
    deterministic: ane_available && ane_deterministic && using_ane_only,
}
```

### Metal Determinism

**Status:** Guaranteed determinism with precompiled shaders

**Guarantees:**
- ✅ Precompiled `.metallib` shaders (no runtime compilation)
- ✅ BLAKE3 hash verification prevents kernel tampering
- ✅ Fixed execution order (fused kernels)
- ✅ Controlled floating-point rounding

**Shader Compilation:**
```bash
# Metal shaders compiled at build time
xcrun -sdk macosx metal -c -std=metal3.1 kernels.metal -o kernels.air
xcrun -sdk macosx metallib kernels.air -o kernels.metallib

# Hash verification at runtime
let loaded_hash = BLAKE3::hash(&metallib_bytes);
assert_eq!(loaded_hash, expected_hash, "Kernel tampering detected");
```

**Attestation Report:**
```rust
DeterminismReport {
    backend_type: BackendType::Metal,
    metallib_hash: Some(metallib_blake3_hash),
    rng_seed_method: RngSeedingMethod::HkdfSeeded,
    floating_point_mode: FloatingPointMode::Deterministic,
    deterministic: true,  // ✅ Fully deterministic
}
```

### Cross-Backend Parity

**Status:** Not enforced

- Run-by-run reproducibility is **per-backend** today
- CoreML vs. MLX vs. Metal may produce different outputs for same inputs
- Switching backends mid-session invalidates replay
- Multi-backend builds should gate on attestation results

**Recommendation:** Use single backend per deployment for strict determinism. Multi-backend is for development/testing only.

---

## Operator Coverage

### CoreML Supported Operations

**Source:** `crates/adapteros-lora-kernel-coreml/src/export.rs`

CoreML supports 60+ operations including:

**Linear Algebra:**
- `linear`, `matmul`, `inner_product`, `batched_matmul`

**Convolutions:**
- `conv2d`, `convolution`, `conv`

**Normalization:**
- `batchnorm`, `batch_normalization`, `layernorm`, `layer_normalization`, `instancenorm`, `instance_normalization`, `l2_normalize`

**Activations:**
- `relu`, `leaky_relu`, `prelu`, `elu`, `selu`, `gelu`, `sigmoid`, `tanh`, `softmax`, `softplus`, `softsign`, `hard_sigmoid`, `hard_swish`, `silu`, `swish`

**Attention:**
- `attention`, `multihead_attention`, `scaled_dot_product_attention`

**Embedding:**
- `embedding`, `embedding_nd`

**Element-wise:**
- `add`, `sub`, `mul`, `div`, `maximum`, `minimum`

**Shape Operations:**
- `concat`, `reshape`, `transpose`, `permute`, `split`, `gather`, `gather_nd`, `slice`, `slice_by_index`, `slice_by_size`, `squeeze`, `expand_dims`, `flatten`, `tile`, `pad`

**Reduction:**
- `reduce_mean`, `reduce_sum`, `reduce_max`, `reduce_min`, `reduce_prod`, `reduce_l2`

**Math:**
- `sqrt`, `rsqrt`, `exp`, `log`, `pow`, `abs`, `neg`, `sign`, `floor`, `ceil`, `round`, `clip`

**Type Casting:**
- `cast`, `const`, `identity`

**Pooling:**
- `max_pool`, `avg_pool`, `global_avg_pool`, `global_max_pool`

**Misc:**
- `dropout`, `where`, `select`

**Limitations:**
- Custom operations fall back to GPU (non-deterministic)
- Batch size must be 1 for ANE optimization
- Sequence length should be multiple of 8 for ANE

### MLX Supported Operations

MLX provides comprehensive operator support through the C++ FFI:

**Core Operations:**
- Matrix multiplication (`matmul`)
- Element-wise operations (`add`, `mul`, `div`, etc.)
- Activation functions (`gelu`, `relu`, `silu`, etc.)
- Normalization (`layer_norm`, `rms_norm`)
- Attention mechanisms (`scaled_dot_product_attention`)
- Embedding layers
- Convolutions

**Advanced Features:**
- LoRA adapter application
- Multi-adapter routing with K-sparse selection
- Hot-swap adapter loading/unloading
- GPU-accelerated sampling
- KV cache management
- Streaming token generation

**Limitations:**
- MoE (Mixture of Experts) models require MLX Bridge subprocess
- Execution order not guaranteed (async GPU operations)

### Metal Supported Operations

Metal backend implements custom kernels for:

**Fused Kernels:**
- `fused_mlp` - Multi-layer perceptron fusion
- `fused_qkv` - Query/Key/Value projection fusion
- `flash_attention` - Flash Attention implementation
- `vocabulary_projection` - Logits projection

**Kernel Location:**
- Source: `metal/src/kernels/adapteros_kernels.metal`
- Compiled: `crates/adapteros-lora-kernel-mtl/shaders/adapteros_kernels.metallib`

**Limitations:**
- Custom kernels must be precompiled (no runtime compilation)
- Limited to implemented kernels (not all PyTorch ops)
- Requires Metal 3.1 support (macOS 12.5+)

### Operator Coverage Comparison

| Operation Category | MLX | CoreML | Metal |
|-------------------|-----|--------|-------|
| **Basic Math** | ✅ Full | ✅ Full | ✅ Custom kernels |
| **Linear Algebra** | ✅ Full | ✅ Full | ✅ Custom kernels |
| **Convolutions** | ✅ Full | ✅ Full | ⚠️ Limited |
| **Attention** | ✅ Full | ✅ Full | ✅ Flash Attention |
| **Normalization** | ✅ Full | ✅ Full | ✅ Custom kernels |
| **LoRA Operations** | ✅ Full | ⚠️ Pre-fusion only | ✅ Full |
| **MoE Support** | ⚠️ Via Bridge | ❌ No | ❌ No |
| **Hot-Swap** | ✅ Full | ❌ No | ⚠️ Limited |

---

## Performance Profiles

### Throughput Comparison

| Backend | Hardware | Typical Throughput | Latency (7B fp16) | Power Draw |
|---------|----------|-------------------|-------------------|------------|
| **CoreML** | M1 ANE | 15.8 TOPS | 40-60ms | **Low** (50% reduction) |
| **CoreML** | M2/M3/M4 ANE | 17.0 TOPS | 35-55ms | **Low** (50% reduction) |
| **MLX** | M1/M2 Unified | Variable (GPU) | 50-80ms | Moderate |
| **MLX Bridge** | M1/M2 + Python | Variable (GPU) | 60-100ms | Moderate |
| **Metal** | M1/M2 GPU | Variable (GPU) | 50-80ms | Moderate |
| **Metal** | Intel GPU | Variable (GPU) | 80-120ms | Moderate |

**Note:** Throughput varies significantly based on model architecture, quantization, and batch size. MLX Bridge has ~10-20% higher latency due to subprocess overhead.

### Startup Time

| Backend | Cold Start | Warm Start (Cached) | Notes |
|---------|------------|---------------------|-------|
| **CoreML** | 2-5s | 500ms-1s | `.mlmodelc` compilation + ANE load |
| **MLX** | 1-3s | 200ms-500ms | Model load + HKDF seed derivation |
| **MLX Bridge** | 3-5s | 1-2s | Python subprocess + model load |
| **Metal** | 500ms-1s | 100ms-300ms | Shader compilation (precompiled) |

**Cache Benefits:**
- Model cache eliminates redundant model loads
- Subsequent requests reuse cached models (O(1) lookup)
- Cache key identity ensures consistent reuse

### Inference Latency Benchmarks

**MLX Backend (7B Model, M2 Max):**

| Operation | Latency | Notes |
|-----------|---------|-------|
| Model load | 500ms | One-time |
| Forward pass (1 token) | 15ms | Cold cache |
| Forward pass (batched) | 30ms | Batch size 4 |
| Text generation (100 tokens) | 2000ms | With sampling |
| Adapter hot-swap | 50ms | Runtime load |

**CoreML Backend (7B Model, M2 Max):**

| Operation | MLMultiArray (CPU) | MLTensor (GPU/ANE) | Speedup |
|-----------|--------------------|--------------------|---------|
| Inference (2K tokens) | 45ms | 22ms | 2x |
| LoRA delta application | 8ms | 3ms | 2.7x |
| Softmax (8K logits) | 1.2ms | 0.4ms | 3x |

**Note:** MLTensor requires macOS 15+ (Sequoia). On older systems, the backend automatically falls back to MLMultiArray.

### Performance Optimization

#### CoreML Optimization

1. **Batch Size = 1:** ANE optimized for single-sequence inference
2. **Sequence Length Alignment:** Align to multiples of 8 for ANE
3. **FP16 Precision:** ANE performs best with FP16
4. **Avoid Custom Ops:** Custom operations fall back to GPU

#### MLX Optimization

1. **KV Cache:** Enable KV cache for repeated sequences
2. **Adapter Preloading:** Preload adapters for hot-swap scenarios
3. **Memory Pool:** Use memory pool for buffer reuse
4. **Batch Processing:** Batch multiple requests when possible

#### Metal Optimization

1. **Precompiled Shaders:** Shaders compiled at build time (no runtime overhead)
2. **Unified Memory:** Zero-copy on Apple Silicon
3. **Fused Kernels:** Combine operations to reduce kernel launches
4. **Grouped Query Attention:** Optimized attention for GQA models

---

## Memory Usage

### Memory Footprint

| Backend | Overhead | Sharing | Notes |
|---------|----------|---------|-------|
| **CoreML** | Low | Per-model cache | Compiled `.mlmodelc` cached on disk |
| **MLX** | Moderate | Unified memory | Shares system RAM/GPU memory |
| **MLX Bridge** | Moderate-High | Separate Python process | Extra overhead for subprocess + IPC |
| **Metal** | Low-Moderate | Arc-backed buffers | Zero-copy on Apple Silicon |

### Memory Usage Breakdown (7B Model)

| Component | Memory | Notes |
|-----------|--------|-------|
| Model weights | 4.5GB | INT8 quantized |
| KV cache | 1.2GB | Max sequence length |
| Adapters (5x) | 0.5GB | ~100MB each |
| Runtime overhead | 0.3GB | FFI, allocation tracking |
| **Total** | **~6.5GB** | Typical deployment |

### Unified Memory Architecture

**Apple Silicon Advantage:**
- Single memory space shared by CPU, GPU, and ANE
- No CPU↔GPU copying (zero-copy)
- Consistent memory view across processors
- Predictable access patterns

**Memory Management:**

```rust
// Unified memory allocation
let buffer = metal_create_shared_buffer(context, size);
// Direct CPU access to GPU memory
let contents = metal_buffer_contents(buffer);
```

### Memory Limits and Headroom

**Policy Pack #12:** Maintain 15% headroom

| Level | Headroom Range | Action |
|-------|----------------|--------|
| **Low** | ≥ 25% | None |
| **Medium** | 15-25% | Evict low priority adapters |
| **High** | 10-15% | Cross-backend eviction (Metal → MLX → CoreML) |
| **Critical** | < 10% | Emergency eviction (all unpinned adapters) |

**Memory Pool Configuration:**

```toml
[mlx.memory_pool]
pressure_threshold = 0.85
idle_timeout_secs = 300
target_headroom = 0.15
```

### Model Cache Budget

**Shared across all backends:**
- Cache key: `(backend_type, manifest_hash, quantization, fusion, kernel_version)`
- Eviction policy: LRU with memory budget enforcement

**Recommended Budgets:**

```
7B models (4-bit):   4096 MB (4GB)
7B models (fp16):   16384 MB (16GB)
13B models (4-bit):  8192 MB (8GB)
32B+ models:        24576+ MB (24GB+)
```

---

## Testing Strategies

### Determinism Testing

#### MLX Determinism Tests

**Location:** `crates/adapteros-lora-mlx-ffi/tests/determinism_tests.rs`

**Test Coverage:**
- HKDF seed derivation verification
- RNG reproducibility across runs
- Router gate quantization (Q15)
- Floating-point precision handling

**Run Tests:**
```bash
cargo test -p adapteros-lora-mlx-ffi --test determinism_tests
```

#### CoreML Determinism Tests

**Location:** `crates/adapteros-lora-kernel-coreml/tests/determinism_tests.rs`

**Test Coverage:**
- ANE availability detection
- Production mode enforcement
- MLTensor vs MLMultiArray path comparison
- Attestation report validation

**Run Tests:**
```bash
cargo test -p adapteros-lora-kernel-coreml --test determinism_tests
```

#### Metal Determinism Tests

**Location:** `crates/adapteros-lora-kernel-mtl/tests/metal_determinism.rs`

**Test Coverage:**
- Kernel hash verification
- Shader compilation determinism
- Floating-point operation consistency
- Memory buffer integrity

**Run Tests:**
```bash
cargo test -p adapteros-lora-kernel-mtl --test metal_determinism
```

### Cross-Backend Comparison Tests

**Location:** `tests/determinism/platform_validation.rs`

**Test Coverage:**
- Floating-point precision consistency
- SIMD operation determinism
- GPU kernel compilation determinism
- Backend selection consistency

**Run Tests:**
```bash
cargo test --test determinism_core_suite -- --test-threads=8
cargo test -p adapteros-lora-router --test determinism
bash scripts/check_fast_math_flags.sh
```

### Performance Benchmarks

**Location:** `tests/benchmark/benches/e2e_performance.rs`

**Benchmark Targets:**
- Inference latency: ≥40 tok/s (≤25ms per token)
- Training time: <5 min for 1000 examples
- Hot-swap latency: <100ms p95
- Memory overhead: ≤10%
- API response time: p95 <200ms

**Run Benchmarks:**
```bash
cargo bench --package adapteros-lora-worker --bench mlx_bridge_streaming
```

### Replay Testing

**Location:** `tests/determinism_replay_harness.rs`

**Test Properties:**
- Runs identical request twice with same seed
- Asserts exact equality of:
  - `decision_hash`
  - `gates_q15` chain
  - `output_digest`
  - `receipt_digest`
  - `seed_lineage_hash`

**Run Tests:**
```bash
cargo test --test determinism_replay_harness -- --test-threads=1 --nocapture
```

### Integration Testing

**Backend Factory Tests:**
- Capability detection
- Backend selection logic
- Fallback chain validation
- Model cache integration

**Run Tests:**
```bash
cargo test -p adapteros-lora-worker --test backend_factory_integration
```

### E2E Testing

**Location:** `tests/e2e/determinism_workflow.rs`

**Test Coverage:**
- End-to-end inference workflows
- Multi-backend scenarios
- Error handling and recovery
- Performance under load

**Run Tests:**
```bash
cargo test --test e2e --features extended-tests
```

### Test Isolation Strategies

| Source | Isolation Strategy |
|--------|-------------------|
| **Time** | `DeterminismConfig::fixed_timestamp` + mock `SystemTime` |
| **Random** | `HKDF(fixed_seed, label)` for all RNG |
| **Scheduling** | `--test-threads=1` + sequential execution |
| **Floating-point** | No `-ffast-math`, verified in CI |
| **Backend variance** | `MockKernels` for pure determinism |

---

## Backend Selection Guide

### Use Case Recommendations

#### Production Inference (Audit Trails)

**Recommended:** CoreML  
**Fallback:** MLX → Metal

**Rationale:**
- Guaranteed determinism with ANE
- 50% power savings
- Audit trail compatibility

**Configuration:**
```bash
export AOS_COREML_COMPUTE_PREFERENCE=cpu_and_ne
export AOS_COREML_PRODUCTION_MODE=true
export AOS_MODEL_CACHE_MAX_MB=16384
```

#### Training & Research

**Recommended:** MLX  
**Fallback:** None (fail if MLX unavailable)

**Rationale:**
- HKDF-seeded determinism for reproducible training
- Unified memory architecture
- Circuit breaker resilience
- Multi-adapter routing

**Configuration:**
```bash
export AOS_BACKEND=mlx
export AOS_MODEL_PATH=/var/model-cache/models/qwen2.5-7b-instruct-bf16
export AOS_FUSION_INTERVAL_MODE=per_token
```

#### Power-Constrained Deployment

**Recommended:** CoreML  
**Fallback:** None (fail if ANE unavailable)

**Rationale:**
- ANE provides 50% power reduction vs GPU
- Lower thermal footprint
- Longer battery life on mobile deployments

#### Legacy Hardware (Intel Macs)

**Recommended:** Metal  
**Fallback:** None

**Rationale:**
- Only GPU backend available on Intel Macs
- No ANE support
- CoreML/MLX require Apple Silicon

#### MoE Models (Mixture of Experts)

**Recommended:** MLX Bridge  
**Fallback:** None

**Rationale:**
- MoE models detected from `config.json` (`num_experts > 1`)
- Automatic subprocess bridge creation
- Python bridge for complex routing logic

### Decision Flowchart

```
1. Check explicit backend request
   ├─ CoreML → Verify ANE available → Use CoreML
   ├─ MLX → Verify MLX available → Use MLX
   └─ Metal → Verify Metal available → Use Metal

2. Auto-selection (MLX-first priority)
   ├─ MLX available? → Use MLX
   ├─ CoreML + ANE available? → Use CoreML
   ├─ MlxBridge available? → Use MlxBridge (MoE only)
   ├─ Metal available? → Use Metal
   └─ None available → Error
```

---

## Related Documentation

- [BACKEND_SELECTION.md](BACKEND_SELECTION.md) - Complete backend selection guide
- [COREML_BACKEND.md](COREML_BACKEND.md) - CoreML backend guide
- [METAL_BACKEND.md](METAL_BACKEND.md) - Metal backend guide
- [MLX_GUIDE.md](MLX_GUIDE.md) - MLX backend guide
- [DETERMINISM.md](DETERMINISM.md) - Determinism guarantees
- [BACKEND_ARCHITECTURE.md](BACKEND_ARCHITECTURE.md) - Backend architecture overview

---

**Signed:** James KC Auchterlonie  
**Date:** 2026-01-02  
**Status:** Approved for Production Use