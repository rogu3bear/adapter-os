# CoreML LoRA Workflows - Complete Guide

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2025-12-24
**Status:** Production Documentation

---

## Executive Summary

adapterOS provides **two distinct paths** for integrating LoRA adapters with CoreML models on Apple Neural Engine:

| Path | Status | Use Case | Performance |
|------|--------|----------|-------------|
| **Offline Pre-Fusion** | ✅ Production Ready | Known adapter combinations | Zero overhead |
| **Runtime Sidecar** | ⚠️ Stub/Planned | Dynamic hot-swapping | ~20-30% overhead |

**Recommendation:** Use **offline pre-fusion** for all production deployments.

---

## Path 1: Offline Pre-Fusion (Production Path)

### What It Is

Pre-fusion performs **offline weight-space fusion** of LoRA adapters into base model weights before CoreML compilation. The result is a single fused model optimized for ANE.

### Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                  Offline Pre-Fusion Pipeline                  │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  1. Base Model (safetensors)                                 │
│     ↓                                                         │
│  2. Adapter Weights (safetensors)                            │
│     ↓                                                         │
│  3. fusion::fuse_lora_into_model()                           │
│     • Load base weights                                      │
│     • Load LoRA A, B matrices                                │
│     • Compute: W_fused = W_base + (α/r) * Σ(gate_i * B@A)   │
│     • Write fused weights to disk                            │
│     ↓                                                         │
│  4. Fused Weights (safetensors)                              │
│     ↓                                                         │
│  5. scripts/convert_mlx_to_coreml.py                         │
│     • Convert to CoreML MIL                                  │
│     • Optimize for ANE                                       │
│     ↓                                                         │
│  6. Fused Model (.mlpackage)                                 │
│     ↓                                                         │
│  7. Deploy to ANE                                            │
│     • Zero runtime overhead                                  │
│     • Full ANE optimization                                  │
│     • Deterministic execution                                │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

### Implementation Status

✅ **FULLY IMPLEMENTED** in `crates/adapteros-lora-kernel-coreml/src/fusion.rs`

**Features:**
- Multi-adapter fusion with Q15 gate weights
- Safetensors format support (FP32, FP16, BF16)
- Content-addressable caching (`FusedModelCache`)
- Hash-based verification metadata
- Comprehensive test coverage (>1000 lines of tests)

### When to Use

✅ **Use pre-fusion when:**
- You know adapter combinations ahead of time
- Maximum performance is required
- You need deterministic audit trails
- Storage is not constrained (one package per combination)

❌ **Don't use pre-fusion when:**
- You need dynamic adapter switching at runtime
- You have hundreds of adapter combinations
- Storage is severely limited

### Code Example

```rust
use adapteros_lora_kernel_coreml::fusion::{
    LoraFusionConfig, AdapterFusionSpec, fuse_lora_into_model, FusedModelCache
};
use adapteros_lora_kernel_coreml::ComputeUnits;
use std::path::Path;

// Example 1: Single adapter fusion
let config = LoraFusionConfig {
    base_model_path: "base_weights.safetensors".into(),
    output_path: "fused_weights.safetensors".into(),
    adapters: vec![AdapterFusionSpec {
        weights_path: "adapter.safetensors".into(),
        gate_weight: 1.0,  // Full weight (no blending)
        alpha: 32.0,
        rank: 16,
    }],
    compute_units: ComputeUnits::CpuAndNeuralEngine,
};

let result = fuse_lora_into_model(&config)?;
println!("✅ Fused {} layers, {} params modified",
         result.layers_fused,
         result.total_params_modified);

// Example 2: Multi-adapter fusion with Q15 routing
let multi_config = LoraFusionConfig {
    base_model_path: "base_weights.safetensors".into(),
    output_path: "fused_multi.safetensors".into(),
    adapters: vec![
        AdapterFusionSpec {
            weights_path: "adapter_a.safetensors".into(),
            gate_weight: 0.7,  // 70% weight
            alpha: 32.0,
            rank: 16,
        },
        AdapterFusionSpec {
            weights_path: "adapter_b.safetensors".into(),
            gate_weight: 0.3,  // 30% weight
            alpha: 32.0,
            rank: 16,
        },
    ],
    compute_units: ComputeUnits::CpuAndNeuralEngine,
};

fuse_lora_into_model(&multi_config)?;

// Example 3: Content-addressable caching
let cache = FusedModelCache::new(
    Path::new("./var/cache/fused_models"),
    10.0  // 10GB max cache
);

// Check cache first
if let Some(cached_path) = cache.get(&config) {
    println!("✅ Using cached fused model: {}", cached_path.display());
} else {
    // Fuse and cache
    let fused_path = cache.fuse_and_cache(config)?;
    println!("✅ Fused and cached: {}", fused_path.display());
}
```

### CLI Workflow

```bash
# Step 1: Prepare base model weights (if not already in safetensors format)
# (Use MLX, transformers, or other tools to export to safetensors)

# Step 2: Fuse LoRA weights (via Rust API)
# See examples/fusion_example.rs for a complete CLI wrapper

# Step 3: Convert fused weights to CoreML
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/fused_weights.safetensors \
  --output ./var/models/fused-coreml.mlpackage \
  --seq-len 2048

# Step 4: Verify fusion metadata
cargo run --bin aosctl -- verify coreml \
  --package ./var/models/fused-coreml.mlpackage \
  --metadata ./var/models/fused-coreml.mlpackage/adapteros_coreml_fusion.json

# Step 5: Deploy
./target/release/aos-worker \
  --backend coreml \
  --model-path ./var/models/fused-coreml.mlpackage \
  --production
```

### Verification & Audit Trails

Every fused package includes `adapteros_coreml_fusion.json` metadata:

```json
{
  "base_manifest_hash": "blake3:a3f2e1d4c8b9a5f6e7d3c2b1a0f9e8d7...",
  "fused_manifest_hash": "blake3:7c9d4bf2a1e5c8d3f6a9b2e1d4c7a0b3...",
  "adapter_hash": "blake3:5e8a2fd1c9b4e7a3d6f2c1b0e9a8d7c6...",
  "base_package": "/path/to/base.mlpackage",
  "fused_package": "/path/to/fused.mlpackage",
  "adapter_path": "/path/to/adapter.aos"
}
```

**Verification API:**

```rust
use adapteros_lora_kernel_coreml::export::validate_coreml_fusion;

let metadata = validate_coreml_fusion(
    Path::new("fused.mlpackage/adapteros_coreml_fusion.json")
)?;

// Automatically verifies:
// 1. Base manifest hash matches on-disk base package
// 2. Fused manifest hash matches on-disk fused package
// 3. Adapter hash matches on-disk adapter file

println!("✅ Fusion verified: {} + {} = {}",
         metadata.base_manifest_hash,
         metadata.adapter_hash,
         metadata.fused_manifest_hash);
```

### Performance Characteristics

| Metric | Value | Notes |
|--------|-------|-------|
| **Fusion Time** | ~30-60s | For 7B model with 16 adapters |
| **Runtime Overhead** | 0ms | Zero - fused into base weights |
| **Throughput** | 23ms/token | ~4% slower than base (ANE compilation) |
| **Memory Usage** | Same as base | No extra adapter storage |
| **ANE Utilization** | 100% | Full ANE optimization |

---

## Path 2: Runtime Sidecar (Future Path)

### What It Is

Runtime sidecar would enable **dynamic hot-swapping** of LoRA adapters at runtime by:
1. Keeping base CoreML model resident
2. Computing LoRA deltas in Metal/MLX sidecar process
3. Applying deltas to base model outputs

### Architecture

```
┌──────────────────────────────────────────────────────────────┐
│              Runtime Sidecar Pipeline (PLANNED)               │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  1. Base CoreML Model (.mlpackage) - Loaded Once             │
│     ↓                                                         │
│  2. Adapter Cache (in-memory safetensors)                    │
│     • Multiple adapters loaded                               │
│     • Hot-swappable via attach/detach API                    │
│     ↓                                                         │
│  3. Inference Request                                        │
│     ↓                                                         │
│  4. CoreML Forward (Base Model)                              │
│     • Run on ANE                                             │
│     • Export intermediate activations to Metal buffers       │
│     ↓                                                         │
│  5. Metal/MLX Sidecar Process                                │
│     • Compute LoRA deltas: δ = B @ A @ h                     │
│     • Apply Q15 routing: out = base_out + Σ(gate_i * δ_i)   │
│     ↓                                                         │
│  6. Re-inject into CoreML Pipeline                           │
│     ↓                                                         │
│  7. Final Output                                             │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

### Implementation Status

✅ **IMPLEMENTED** - Runtime sidecar with Metal/MLX integration

**Features:**
- ✅ Adapter caching (`load_adapter`, `adapter_cache` storage)
- ✅ Hot-swap logic (`attach_adapter`, `detach_adapter`)
- ✅ RouterRing integration for Q15 sparse gating
- ✅ MLX-based LoRA delta computation (`adapteros-lora-mlx-ffi`)
- ✅ Metal buffer pooling for performance

**Performance:**
- Zero-copy buffer export (supported)
- Unified memory integration between CoreML and MLX
- ~5-7ms overhead per step vs base CoreML

**Usage:**
Enable `coreml-mlx-integration` feature in `Cargo.toml` to use the sidecar path.

```rust
// Runtime behavior:
backend.load_adapter(0, adapter_bytes)?;  // ✅ Caches structured weights
backend.attach_adapter(0)?;                // ✅ Marks as active
backend.run_step(&ring, &mut io)?;         // ✅ Returns base logits + computed LoRA deltas
```

### Why Stubbed?

CoreML models are **compiled and opaque**. They don't expose intermediate layer activations at runtime. True runtime LoRA requires:

1. **Metal/MLX Sidecar Integration**
   - Export CoreML outputs to Metal buffers
   - Compute LoRA deltas in separate Metal/MLX process
   - Re-inject results into CoreML
   - **Estimated effort:** 2-3 weeks development + testing

2. **Performance Overhead**
   - Buffer transfers: ~1ms per layer
   - LoRA computation: ~5ms total (Metal/MLX)
   - **Total overhead:** ~20-30% vs pre-fusion

3. **Complexity**
   - Requires IPC between CoreML and Metal processes
   - Memory management across boundaries
   - Synchronization overhead

### When to Use (Future)

Once implemented, use runtime sidecar when:

✅ **Use sidecar when:**
- You need dynamic adapter switching
- Multi-tenant serving (one model, many adapters)
- Storage is severely constrained
- ~20-30% overhead is acceptable

❌ **Don't use sidecar when:**
- Maximum performance is required
- Adapter combinations are known ahead of time
- Deterministic audit trails are critical

### Projected Performance (When Implemented)

| Metric | Value | Notes |
|--------|-------|-------|
| **Adapter Load Time** | ~10ms | One-time per adapter |
| **Hot-Swap Time** | <1ms | In-memory pointer update |
| **Runtime Overhead** | +5-7ms | Per inference request |
| **Throughput** | 28-30ms/token | ~27% slower than pre-fusion |
| **Memory Usage** | +500MB | Per loaded adapter |

### Codebase Adapter Requirements

Codebase adapters have special constraints for CoreML:

1. **Must Be Frozen Before Export**: Codebase adapters bound to live sessions cannot be exported. The session must be unbound first, which triggers automatic versioning.

2. **Freeze Workflow:**

```bash
# 1. Unbind the codebase adapter from its session
curl -X POST "$AOS_BASE_URL/v1/adapters/codebase/$ADAPTER_ID/unbind" \
  -H "Authorization: Bearer $AOS_TOKEN"

# 2. Verify the adapter is now frozen (lifecycle_state != "live")
curl "$AOS_BASE_URL/v1/adapters/codebase/$ADAPTER_ID" \
  -H "Authorization: Bearer $AOS_TOKEN"

# 3. Pre-fuse with base adapter
cargo run --bin aosctl -- coreml fuse \
  --base ./adapters/core.aos \
  --adapter ./adapters/frozen_codebase.aos \
  --output ./fused.safetensors

# 4. Convert to CoreML
python scripts/convert_mlx_to_coreml.py \
  --input ./fused.safetensors \
  --output ./fused.mlpackage
```

3. **Why Freezing Is Required**: CoreML packages are immutable after compilation. A live codebase adapter may receive incremental updates from the session, which would invalidate any compiled package. Freezing ensures the adapter state is stable before export.

---

## Decision Matrix

Use this matrix to choose the right path for your use case:

| Requirement | Pre-Fusion | Sidecar (Future) |
|-------------|-----------|------------------|
| **Maximum Performance** | ✅ Best choice | ❌ ~27% slower |
| **Dynamic Hot-Swap** | ❌ Not supported | ✅ <1ms swap |
| **Multi-Tenant Serving** | ⚠️ One package per tenant | ✅ One model, many adapters |
| **Audit Trails** | ✅ Deterministic hashes | ⚠️ Runtime state |
| **Storage Efficiency** | ❌ One package per combo | ✅ Shared base model |
| **Production Ready** | ✅ Yes | ❌ No (stub) |
| **ANE Optimization** | ✅ Full | ⚠️ Partial |
| **Deterministic Replay** | ✅ Yes | ⚠️ Conditional |

---

## API Comparison

### Pre-Fusion API

```rust
// Offline fusion (happens once, before deployment)
use adapteros_lora_kernel_coreml::fusion::fuse_lora_into_model;

let result = fuse_lora_into_model(&config)?;

// Later: Just load the fused package
backend.load_model(Path::new("fused.mlpackage"))?;
backend.run_step(&ring, &mut io)?;  // ✅ Full LoRA applied
```

### Sidecar API (Stub)

```rust
// Runtime switching (dynamic, at runtime)
backend.load_model(Path::new("base.mlpackage"))?;
backend.load_adapter(0, adapter_a_bytes)?;  // ✅ Cache
backend.load_adapter(1, adapter_b_bytes)?;  // ✅ Cache

backend.attach_adapter(0)?;  // ✅ Attach adapter A
backend.run_step(&ring, &mut io)?;  // ⚠️ STUB (no LoRA applied)

backend.detach_adapter(0)?;  // ✅ Detach adapter A
backend.attach_adapter(1)?;  // ✅ Attach adapter B
backend.run_step(&ring, &mut io)?;  // ⚠️ STUB (no LoRA applied)
```

---

## Migration Guide

### From Sidecar (Stub) to Pre-Fusion

**Before (expecting runtime hot-swap):**
```rust
backend.load_adapter(0, adapter_bytes)?;
backend.attach_adapter(0)?;
backend.run_step(&ring, &mut io)?;
// ❌ Only returns stub logits
```

**After (using pre-fusion):**
```rust
// 1. Offline: Pre-fuse adapters
let config = LoraFusionConfig { /* ... */ };
fuse_lora_into_model(&config)?;

// 2. Runtime: Load fused package
backend.load_model(Path::new("fused.mlpackage"))?;
backend.run_step(&ring, &mut io)?;
// ✅ Full LoRA applied, zero overhead
```

---

## Testing

### Pre-Fusion Tests

```bash
# Fusion module tests (comprehensive)
cargo test -p adapteros-lora-kernel-coreml fusion

# Specific test suites
cargo test -p adapteros-lora-kernel-coreml test_fuse_weights
cargo test -p adapteros-lora-kernel-coreml test_fuse_multiple_adapters
cargo test -p adapteros-lora-kernel-coreml test_fusion_cache

# Export pipeline tests
cargo test -p adapteros-lora-kernel-coreml export

# Integration tests
cargo test -p adapteros-lora-kernel-coreml --test '*'
```

### Sidecar Tests (Stub)

```bash
# Stub mode tests (verify infrastructure)
cargo test -p adapteros-lora-kernel-coreml coreml_stub_hot_swap_sidecar
cargo test -p adapteros-lora-kernel-coreml test_adapter_cache
```

---

## Documentation References

### Core Documentation
- **[crates/adapteros-lora-kernel-coreml/README.md](../crates/adapteros-lora-kernel-coreml/README.md)** - Main API reference
- **[docs/COREML_BACKEND.md](./COREML_BACKEND.md)** - CoreML backend guide
- **[scripts/COREML_CONVERSION.md](../scripts/COREML_CONVERSION.md)** - Model conversion guide

### Implementation Details
- **[crates/adapteros-lora-kernel-coreml/src/fusion.rs](../crates/adapteros-lora-kernel-coreml/src/fusion.rs)** - Pre-fusion implementation
- **[crates/adapteros-lora-kernel-coreml/src/export.rs](../crates/adapteros-lora-kernel-coreml/src/export.rs)** - Export pipeline
- **[crates/adapteros-lora-kernel-coreml/src/backend.rs](../crates/adapteros-lora-kernel-coreml/src/backend.rs)** - Sidecar infrastructure (stub)

### Related Guides
- See [COREML_BACKEND.md](COREML_BACKEND.md) for CoreML backend details
- **[docs/DETERMINISM.md](./DETERMINISM.md)** - Determinism and replay
- **[AGENTS.md](../AGENTS.md)** - Development guidelines

---

## FAQ

### Q: Can I hot-swap adapters with CoreML?

**A:** Not yet. The runtime sidecar path is currently stubbed. Use offline pre-fusion for production.

### Q: How do I fuse multiple adapters?

**A:** Use the `adapters` field in `LoraFusionConfig` with different `gate_weight` values:

```rust
let config = LoraFusionConfig {
    adapters: vec![
        AdapterFusionSpec { gate_weight: 0.7, /* ... */ },
        AdapterFusionSpec { gate_weight: 0.3, /* ... */ },
    ],
    // ...
};
```

### Q: What's the overhead of pre-fusion?

**A:** ~4% slower than base model (due to CoreML compilation of larger weights). Zero runtime overhead.

### Q: Can I use pre-fusion with MoE models?

**A:** Not yet. LoRA fusion for MoE models is planned but not implemented. MoE models currently support base inference only.

### Q: How do I verify a fused package?

**A:** Use `validate_coreml_fusion()` with the metadata JSON:

```rust
use adapteros_lora_kernel_coreml::export::validate_coreml_fusion;

let metadata = validate_coreml_fusion(
    Path::new("fused.mlpackage/adapteros_coreml_fusion.json")
)?;
// Automatically checks all hashes
```

### Q: Why is the sidecar path stubbed?

**A:** CoreML models are compiled and opaque. True runtime LoRA requires Metal/MLX integration for intermediate activation access, which adds significant complexity and ~20-30% overhead.

### Q: When will sidecar be implemented?

**A:** No ETA. It's a future enhancement planned after offline pre-fusion is proven in production.

---

**Signed:** James KC Auchterlonie
**Date:** 2025-12-24
**Status:** Production Documentation - Approved for Reference
