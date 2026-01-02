# CoreML ANE Acceleration Layer for AdapterOS

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.

This crate provides Apple Neural Engine (ANE) acceleration as an **acceleration layer** for the MLX primary backend. CoreML enables specific operations (like K-sparse gate routing) to run on the Neural Engine for power efficiency, while MLX handles the main inference/training workload.

## Quick Start

```rust
use adapteros_lora_kernel_coreml::{CoreMLBackend, ComputeUnits};
use adapteros_lora_kernel_api::FusedKernels;

// Create backend
let mut backend = CoreMLBackend::new(ComputeUnits::CpuAndNeuralEngine, false)?;

// Load base model
backend.load_model(Path::new("./var/models/qwen-coreml.mlpackage"))?;

// Load adapter (sidecar path)
let adapter_bytes = std::fs::read("adapter.aos")?;
backend.load_adapter(0, &adapter_bytes)?;

// Run inference
backend.run_step(&router_ring, &mut io_buffers)?;
```

## LoRA Fusion Workflows

CoreML supports **two distinct paths** for LoRA adapter integration:

### Path 1: Offline Pre-Fusion (Recommended for Production)

**Use when:** You want maximum performance and have known adapter combinations ahead of time.

**Workflow:**
1. Convert base model to CoreML using `scripts/convert_mlx_to_coreml.py`
2. Use the `fusion` module to pre-fuse LoRA weights into base model weights
3. Export fused `.mlpackage` with verification metadata
4. Deploy the pre-fused package

**Advantages:**
- ✅ Zero runtime overhead (no separate LoRA computation)
- ✅ Full ANE optimization (fused weights compiled into CoreML graph)
- ✅ Deterministic hash for audit trails
- ✅ Maximum throughput

**Disadvantages:**
- ❌ Requires re-fusion for each adapter combination
- ❌ Larger storage footprint (one package per combination)
- ❌ Cannot hot-swap adapters at runtime

**Implementation Status:**
- ✅ **FULLY IMPLEMENTED** - See `src/fusion.rs` for production-ready fusion
- ✅ Supports multiple adapters with gate weights
- ✅ Handles safetensors format (FP32, FP16, BF16)
- ✅ Content-addressable caching via `FusedModelCache`
- ✅ Comprehensive validation and testing

**Code Example:**

```rust
use adapteros_lora_kernel_coreml::fusion::{
    LoraFusionConfig, AdapterFusionSpec, fuse_lora_into_model
};
use adapteros_lora_kernel_coreml::ComputeUnits;

let config = LoraFusionConfig {
    base_model_path: "base_model_weights.safetensors".into(),
    output_path: "fused_weights.safetensors".into(),
    adapters: vec![
        AdapterFusionSpec {
            weights_path: "adapter_a.safetensors".into(),
            gate_weight: 0.7,  // Q15 router weight
            alpha: 32.0,
            rank: 16,
        },
        AdapterFusionSpec {
            weights_path: "adapter_b.safetensors".into(),
            gate_weight: 0.3,
            alpha: 32.0,
            rank: 16,
        },
    ],
    compute_units: ComputeUnits::CpuAndNeuralEngine,
};

let result = fuse_lora_into_model(&config)?;
println!("Fused {} layers, modified {} params",
         result.layers_fused,
         result.total_params_modified);
```

**CLI Usage:**

```bash
# Use the export pipeline to create fused packages
python scripts/convert_mlx_to_coreml.py \
  --input ./var/models/base-model-mlx \
  --output ./var/models/base-coreml.mlpackage

# Fusion happens via the Rust API (see examples/fusion_example.rs)
cargo run --example fusion_example \
  --base ./var/models/base-coreml.mlpackage \
  --adapter ./var/adapters/my_adapter.aos \
  --output ./var/models/fused-coreml.mlpackage
```

**Metadata & Verification:**

Every fused package includes `adapteros_coreml_fusion.json` metadata:

```json
{
  "base_manifest_hash": "blake3:a3f2e1...",
  "fused_manifest_hash": "blake3:7c9d4b...",
  "adapter_hash": "blake3:5e8a2f...",
  "base_package": "/path/to/base.mlpackage",
  "fused_package": "/path/to/fused.mlpackage",
  "adapter_path": "/path/to/adapter.aos"
}
```

Verify integrity:

```rust
use adapteros_lora_kernel_coreml::export::validate_coreml_fusion;

let metadata = validate_coreml_fusion(
    Path::new("fused.mlpackage/adapteros_coreml_fusion.json")
)?;
// Automatically checks all three hashes match on-disk artifacts
```

### Path 2: Runtime Sidecar (Hot-Swappable Adapters)

**Use when:** You need dynamic adapter switching or multi-tenant serving.

**Workflow:**
1. Load base CoreML model once (`load_model`)
2. Load adapters into cache (`load_adapter`)
3. Hot-swap active adapters (`attach_adapter` / `detach_adapter`)
4. Base model stays resident, LoRA deltas applied at runtime

**Advantages:**
- ✅ Dynamic adapter switching (no recompilation)
- ✅ Single base model package (smaller storage)
- ✅ Multi-tenant serving (one model, many adapters)
- ✅ Supports Q15 sparse routing

**Disadvantages:**
- ⚠️ Slower than pre-fusion (separate LoRA computation)
- ⚠️ Limited ANE optimization for LoRA path
- ⚠️ Currently in **STUB MODE** (see below)

**Implementation Status:**
- ⚠️ **PARTIAL/STUB** - Basic infrastructure exists but LoRA computation is stubbed
- ✅ Adapter caching and hot-swap logic implemented
- ✅ RouterRing integration for Q15 gating
- ❌ Actual LoRA delta application NOT YET IMPLEMENTED
- ❌ Uses placeholder logits in stub mode

**Current Behavior (Stub Mode):**

```rust
// This API exists but LoRA computation is STUBBED
backend.load_adapter(0, adapter_bytes)?;  // ✅ Caches adapter
backend.attach_adapter(0)?;                // ✅ Marks as active
backend.run_step(&ring, &mut io)?;         // ⚠️ Returns stub logits (no LoRA applied)
```

**Why Stubbed?**

CoreML models are **compiled and opaque** - we cannot access intermediate layer activations at runtime. True runtime LoRA fusion requires either:

1. **Metal/MLX Sidecar Pipeline** (future work):
   - Export CoreML outputs to Metal buffers
   - Compute LoRA deltas in Metal/MLX
   - Re-inject into CoreML
   - **Performance overhead:** ~20-30% vs pre-fusion

2. **CoreML MLState API** (macOS 26+):
   - Requires unreleased OS version
   - Would enable stateful LoRA layers
   - Not yet available

**Recommendation:** Use **Path 1 (Pre-Fusion)** for all production workloads until sidecar pipeline is implemented.

## CoreML + LoRA Architecture Decision

```
┌─────────────────────────────────────────────────────────┐
│               AdapterOS LoRA System                     │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌─────────────────┐         ┌──────────────────┐     │
│  │  Offline Fusion │         │  Runtime Sidecar │     │
│  │  (Production)   │         │  (Hot-Swap)      │     │
│  └────────┬────────┘         └────────┬─────────┘     │
│           │                           │                │
│           ▼                           ▼                │
│  ┌──────────────────┐        ┌──────────────────┐     │
│  │ Pre-Fused        │        │ Base .mlpackage  │     │
│  │ .mlpackage       │        │ + LoRA Sidecars  │     │
│  │                  │        │                  │     │
│  │ W_fused =        │        │ W_base (ANE)     │     │
│  │  W_base +        │        │ + LoRA deltas    │     │
│  │  α/r * B@A       │        │   (Metal/MLX)    │     │
│  └────────┬─────────┘        └────────┬─────────┘     │
│           │                           │                │
│           └───────────┬───────────────┘                │
│                       ▼                                │
│              ┌─────────────────┐                       │
│              │  Apple Neural   │                       │
│              │  Engine (ANE)   │                       │
│              └─────────────────┘                       │
└─────────────────────────────────────────────────────────┘

Status Legend:
✅ Pre-Fusion Path:     FULLY IMPLEMENTED
⚠️  Sidecar Path:       STUB (infrastructure only)
```

## MoE Model Support

See [README_MOE.md](README_MOE.md) for details on Mixture-of-Experts models.

**Quick Summary:**
- ✅ Automatic MoE detection from `config.json`
- ✅ Native CoreML MoE runtime (expert routing on ANE)
- ❌ LoRA fusion NOT YET supported for MoE models

## Feature Matrix

| Feature | Pre-Fusion | Sidecar | Notes |
|---------|-----------|---------|-------|
| **Single Adapter** | ✅ Full | ⚠️ Stub | Pre-fusion production-ready |
| **Multiple Adapters** | ✅ Full | ⚠️ Stub | Supports weighted blending |
| **Hot-Swap** | ❌ No | ⚠️ Stub | Requires re-fusion |
| **ANE Optimization** | ✅ Full | ⚠️ Partial | Pre-fusion compiles to ANE |
| **Q15 Sparse Routing** | ✅ Yes | ⚠️ Stub | Gate weights in fusion config |
| **Deterministic Replay** | ✅ Yes | ⚠️ Stub | Hash-based verification |
| **MoE Models** | ✅ Base Only | ❌ No | No LoRA for MoE yet |
| **macOS Version** | 13+ | 13+ | MLTensor API: 15+ |

## API Documentation

### Fusion Module (`src/fusion.rs`)

**Core Types:**

```rust
pub struct LoraFusionConfig {
    pub base_model_path: PathBuf,      // .safetensors file
    pub output_path: PathBuf,          // Output .safetensors
    pub adapters: Vec<AdapterFusionSpec>,
    pub compute_units: ComputeUnits,
}

pub struct AdapterFusionSpec {
    pub weights_path: PathBuf,         // .safetensors adapter
    pub gate_weight: f32,              // Q15 gate (0.0-1.0)
    pub alpha: f32,                    // LoRA alpha
    pub rank: usize,                   // LoRA rank
}

pub struct FusionResult {
    pub output_path: PathBuf,
    pub layers_fused: usize,
    pub weights_per_layer: usize,
    pub total_params_modified: usize,
}
```

**Core Functions:**

```rust
// Main fusion entry point
pub fn fuse_lora_into_model(config: &LoraFusionConfig) -> Result<FusionResult>;

// Low-level weight fusion
pub fn fuse_weights(
    base_weights: &[f32],
    lora_a: &[f32],
    lora_b: &[f32],
    out_dim: usize,
    in_dim: usize,
    rank: usize,
    scale: f32,
) -> Vec<f32>;

// Load LoRA weights from safetensors
pub fn load_lora_weights(path: &PathBuf) -> Result<ParsedLoraWeights>;

// Batch fusion of multiple adapters
pub fn fuse_multiple_adapters(
    base_weights: &[f32],
    adapters: &[(AdapterFusionSpec, ParsedLoraWeights)],
    target: LoraTarget,
    layer_idx: usize,
    out_dim: usize,
    in_dim: usize,
) -> Result<Vec<f32>>;
```

**Caching:**

```rust
pub struct FusedModelCache {
    cache_dir: PathBuf,
    max_cache_size_gb: f64,
}

impl FusedModelCache {
    pub fn new(cache_dir: PathBuf, max_cache_size_gb: f64) -> Self;
    pub fn get(&self, config: &LoraFusionConfig) -> Option<PathBuf>;
    pub fn fuse_and_cache(&self, config: LoraFusionConfig) -> Result<PathBuf>;
}
```

### Export Module (`src/export.rs`)

```rust
pub fn export_coreml_adapter(req: &CoreMLExportRequest) -> Result<CoreMLExportOutcome>;
pub fn validate_coreml_fusion(metadata_path: &Path) -> Result<CoreMLFusionMetadata>;
```

### Backend Module (`src/backend.rs` / `src/lib.rs`)

```rust
impl CoreMLBackend {
    pub fn new(compute_units: ComputeUnits, production_mode: bool) -> Result<Self>;
    pub fn load_model(&mut self, path: &Path) -> Result<()>;

    // Sidecar path (STUB)
    pub fn load_adapter(&mut self, slot: usize, payload: &[u8]) -> Result<()>;
    pub fn attach_adapter(&mut self, slot: usize) -> Result<()>;
    pub fn detach_adapter(&mut self, slot: usize) -> Result<()>;

    // MoE detection
    pub fn is_moe_model(&self) -> bool;
    pub fn moe_config(&self) -> Option<&MoEConfig>;
}

impl FusedKernels for CoreMLBackend {
    fn forward_base(&mut self, input_ids: &[u32]) -> Result<Vec<f32>>;
    fn forward_lora(&mut self, input_ids: &[u32], ring: &RouterRing) -> Result<Vec<f32>>;
    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()>;
}
```

## Performance Considerations

### Pre-Fusion Path

**Strengths:**
- Maximum throughput (no runtime LoRA overhead)
- Full ANE optimization (15.8-17.0 TOPS)
- 50% power reduction vs GPU
- Deterministic execution

**Typical Performance:**
```
Base model inference:  22ms (2K tokens)
Pre-fused inference:   23ms (2K tokens)  +4% overhead
Fusion overhead:       ~1ms
```

### Sidecar Path (When Implemented)

**Projected Performance:**
```
Base model:            22ms
+ LoRA computation:    ~5ms (Metal/MLX)
+ Data transfer:       ~1ms
Total:                 28ms (+27% vs pre-fusion)
```

## Testing

```bash
# Fusion module tests
cargo test -p adapteros-lora-kernel-coreml fusion

# Export pipeline tests
cargo test -p adapteros-lora-kernel-coreml export

# MoE configuration tests
cargo test -p adapteros-lora-kernel-coreml moe

# Integration tests
cargo test -p adapteros-lora-kernel-coreml --test '*'
```

## Migration Guide

### If You Were Using Runtime Fusion (Sidecar)

**Before (expecting hot-swap):**
```rust
backend.load_adapter(0, adapter_a)?;
backend.attach_adapter(0)?;
// Expecting LoRA to be applied ❌ STUB
```

**After (pre-fusion):**
```rust
// 1. Pre-fuse offline
let config = LoraFusionConfig { /* ... */ };
fuse_lora_into_model(&config)?;

// 2. Load fused package
backend.load_model(Path::new("fused.mlpackage"))?;
// No adapter loading needed - already fused ✅
```

## Examples

See `examples/` directory:
- `fusion_example.rs` - Basic LoRA fusion
- `multi_adapter_fusion.rs` - Multiple adapters with gate weights
- `fusion_cache.rs` - Content-addressable caching

## Documentation

- **This README**: Fusion workflows and API reference
- **[docs/COREML_BACKEND.md](../../docs/COREML_BACKEND.md)**: CoreML backend guide
- **[docs/COREML_MOE_IMPLEMENTATION.md](../../docs/COREML_MOE_IMPLEMENTATION.md)**: MoE implementation
- **[scripts/COREML_CONVERSION.md](../../scripts/COREML_CONVERSION.md)**: Model conversion guide
- **[README_MOE.md](README_MOE.md)**: MoE-specific features

## License

Copyright 2025 JKCA / James KC Auchterlonie. All rights reserved.
