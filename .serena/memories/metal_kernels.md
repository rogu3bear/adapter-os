# Metal Kernel System - AdapterOS

## Overview

The Metal kernel system in AdapterOS provides GPU-accelerated inference for transformer models on Apple Silicon. It is located in:
- **Rust crate**: `crates/adapteros-lora-kernel-mtl/`
- **Metal shaders**: `metal/src/kernels/`

## Build Process

### Shader Compilation Pipeline

1. **Source files** (`metal/src/kernels/`):
   - `adapteros_kernels.metal` - Main entry point, includes all modules
   - `common.metal` - Shared structs (LoraConfig, GqaConfig, RingBuffer) and helper functions
   - `mlp.metal` - Fused MLP kernel with SwiGLU activation
   - `attention.metal` - Fused QKV with Grouped Query Attention (GQA)
   - `flash_attention.metal` - Memory-efficient attention computation
   - `mplora.metal` - LoRA adapter fusion
   - `rms_norm.metal` - RMS normalization

2. **Build script** (`metal/build.sh`):
   - Compiles `.metal` to `.air` (Apple Intermediate Representation)
   - Links `.air` to `.metallib` (Metal library)
   - Computes BLAKE3 hash for determinism verification
   - Verifies against baseline hash in `metal/baselines/kernel_hash.txt`
   - Copies output to `crates/adapteros-lora-kernel-mtl/shaders/`

3. **Rust build.rs** (`crates/adapteros-lora-kernel-mtl/build.rs`):
   - Caches compiled metallib by source hash
   - Generates signed manifest with Ed25519 (deterministic test keys)
   - Writes `kernel_hash.txt` and `metallib_manifest.json`

4. **Embedding**: The metallib is embedded in the binary via:
   ```rust
   const METALLIB_BYTES: &[u8] = include_bytes!("../shaders/adapteros_kernels.metallib");
   const METALLIB_HASH: &str = include_str!("../shaders/kernel_hash.txt");
   ```

### Determinism Enforcement

- `#pragma clang fp contract(off)` disables fast-math for IEEE 754 compliance
- SOURCE_DATE_EPOCH for reproducible timestamps
- BLAKE3 hash verification at load time
- Manifest signature verification with Ed25519

## Key Compute Kernels

### 1. Fused MLP (`fused_mlp`)
- **Location**: `metal/src/kernels/mlp.metal`, `src/fused_mlp.rs`
- **Function**: SwiGLU activation with LoRA support
- **Formula**: `output = down_proj(SiLU(gate_proj(x)) * up_proj(x))`
- **LoRA**: Adds `Σᵢ (gateᵢ/32767) * (alpha/rank) * (Bᵢ @ (Aᵢ @ x))` per projection
- **Buffer layout**: 21 buffers (input, output, weights, biases, LoRA A/B, config)

### 2. Fused QKV with GQA (`fused_qkv_gqa`)
- **Location**: `metal/src/kernels/attention.metal`, `src/fused_qkv.rs`
- **Function**: Computes Q, K, V projections with Grouped Query Attention
- **GQA**: Supports different head counts (e.g., 28 attention heads, 4 KV heads for Qwen2.5-7B)
- **LoRA**: Per-adapter Q/K/V LoRA weights with gate-weighted fusion
- **Config struct**: `GqaConfig` with num_attention_heads, num_kv_heads, head_dim, rope_theta

### 3. Flash Attention (`flash_attention`)
- **Location**: `metal/src/kernels/flash_attention.metal`, `src/fused_qkv.rs`
- **Function**: Memory-efficient attention with numerical stability
- **Algorithm**: Two-pass (find max, then softmax) for stable softmax
- **GQA support**: Maps attention heads to KV heads via `heads_per_kv = num_heads / num_kv_heads`

### 4. Vocabulary Projection (`vocabulary_projection`)
- **Location**: `metal/src/kernels/adapteros_kernels.metal`
- **Function**: Final layer mapping hidden states to vocabulary logits
- **Optimization**: 4-way loop unrolling, FMA instructions
- **Tiled variant**: `vocabulary_projection_tiled` uses shared memory for large vocabularies

### 5. Embedding Lookup (`embedding_lookup`)
- **Location**: `metal/src/kernels/adapteros_kernels.metal`
- **Function**: Maps token IDs to embedding vectors
- **Dispatch**: 1 thread per token, loops over hidden_size internally

## Kernel Dispatch Patterns

### Rust-side Dispatch (FusedMlpKernel::execute)
```rust
let threadgroup_size = MTLSize::new(16, 16, 1);
let grid_size = MTLSize::new(batch_size as u64, hidden_size as u64, 1);
encoder.dispatch_thread_groups(grid_size, threadgroup_size);
```

### Buffer Binding Convention
- Buffers 0-7: Input, output, base weights
- Buffers 8-13: LoRA A/B weights (Q/K/V or gate/up/down)
- Buffer 14: GqaConfig or LoraConfig
- Buffer 15: LoraConfig or RingBuffer
- Buffer 16: RingBuffer or dropout_seed
- Buffers 17+: Dimension constants (hidden_size, intermediate_size, batch_size, max_adapters)

### Multi-Adapter Routing
- Up to K=8 adapters simultaneously via RingBuffer
- Q15 quantized gates (i16, denominator 32767.0)
- Metal struct `RingBuffer`: top_k, current_pos, adapter_indices[8], gates[8]

## GPU Memory Management

### GpuMemoryPool (`src/gpu_memory_pool.rs`)
- **Buffer pooling**: Reuses buffers by size bucket (power of 2)
- **Residency tracking**: HOT (non-purgeable) vs COLD (purgeable) classification
- **Eviction policy**: ColdOnly or ColdThenHot, sorted by LRU
- **KV quota**: Optional quota limit with atomic reservation
- **Memory pressure**: Callbacks and automatic cleanup

### KV Cache (`src/kv_cache.rs`)
- **Per-layer buffers**: Key and Value caches for autoregressive generation
- **Config**: num_layers, max_seq_len, num_kv_heads, head_dim
- **Residency promotion**: HOT after HOT_PROMOTION_THRESHOLD accesses
- **Residency demotion**: COLD after COLD_DEMOTION_IDLE_TIME idle
- **Purgeable integration**: Uses Metal purgeable state for memory hints

### Ring Buffer (`src/ring_buffer.rs`)
- **Purpose**: Manages top-K active adapters for kernel dispatch
- **Layout**: 40 bytes (u32 top_k, u32 current_pos, u16[8] indices, i16[8] gates)
- **Q15 format**: Signed i16 gates with denominator 32767.0

## VRAM Tracking (`src/vram.rs`)
- Per-adapter VRAM attribution
- Tracks adapter weights and KV cache estimates
- Exposed via `vram_tracker()` on MetalKernels

## Integration with Rust Layer

### MetalKernels (`src/lib.rs`)
Main struct holding:
- Metal Device, CommandQueue, Library
- FusedMlpKernel, FusedQkvKernel, FlashAttentionKernel
- RingBuffer for adapter routing
- Embedding/LM head weights and pipelines
- Adapter weights HashMap (id -> AdapterWeights)
- GpuMemoryPool for buffer management

### FusedKernels Trait Implementation
- `load(plan_bytes)`: Parse SafeTensors, load weights, create pipelines
- `run_step(ring, io)`: Embedding lookup -> Transformer layers -> Vocab projection
- `load_adapter(id, weights)`: Load LoRA weights to GPU
- `unload_adapter(id)`: Remove adapter from GPU
- `attest_determinism()`: Return determinism report with metallib hash

## Key Files Summary

| File | Purpose |
|------|---------|
| `metal/build.sh` | Compile shaders to metallib |
| `metal/src/kernels/adapteros_kernels.metal` | Main kernel entry |
| `metal/src/kernels/common.metal` | Shared structs and functions |
| `metal/kernels.json` | Kernel registry with parameters |
| `crates/adapteros-lora-kernel-mtl/src/lib.rs` | MetalKernels struct |
| `crates/adapteros-lora-kernel-mtl/src/fused_mlp.rs` | MLP kernel dispatch |
| `crates/adapteros-lora-kernel-mtl/src/fused_qkv.rs` | QKV/Flash attention dispatch |
| `crates/adapteros-lora-kernel-mtl/src/gpu_memory_pool.rs` | Buffer pooling |
| `crates/adapteros-lora-kernel-mtl/src/kv_cache.rs` | KV cache management |
| `crates/adapteros-lora-kernel-mtl/src/ring_buffer.rs` | Adapter routing buffer |
| `crates/adapteros-lora-kernel-mtl/build.rs` | Rust build script |
