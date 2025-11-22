# Inference Pipeline Integration Status

**Last Updated:** 2025-11-22
**Verified By:** Integration Test Suite (`tests/mlx_inference_e2e.rs`)

## Overview

This document details the integration status of the complete inference pipeline from UI request to MLX backend response, including verified components and identified gaps.

## Pipeline Architecture

```
UI (React) → REST API (/v1/infer) → Router (K-sparse) → Backend (MLX/CoreML) → Adapter Loading → Response
```

## Verified Components

### 1. Unified Adapter File Format (.aos)

**Status:** WORKING

The `.aos` files use a unified archive format (no version variants):

```
[0-3]   manifest_offset (u32 LE)
[4-7]   manifest_len (u32 LE)
[8...offset-1]  safetensors weights
[offset...]     JSON manifest
```

**Implementation:** `crates/adapteros-aos/src/aos2_writer.rs`

**Loading Example:**
```rust
use adapteros_aos::AosFile;

// Load and parse .aos file directly
let aos = AosFile::load_from_path("adapter.aos")?;
let manifest = aos.manifest();
let weights = aos.weights_bytes();
```

All adapters are loaded using the unified `adapteros-aos` crate with no format detection or compatibility layer required.

### 2. Adapter Files

**Status:** VERIFIED

| Adapter | File | Rank | Alpha | Base Model | Tensors |
|---------|------|------|-------|------------|---------|
| code-assistant | `adapters/code-assistant.aos` | 16 | 32.0 | qwen2.5-7b | 224 |
| creative-writer | `adapters/creative-writer.aos` | 12 | 24.0 | qwen2.5-7b | 224 |
| readme-writer | `adapters/readme-writer.aos` | 8 | 16.0 | qwen2.5-7b | 224 |

All adapters contain:
- LoRA A matrices (low-rank down projection)
- LoRA B matrices (low-rank up projection)
- Target modules: `q_proj`, `k_proj`, `v_proj`, `o_proj`

### 3. Base Model Configuration (Qwen 2.5 7B)

**Status:** DOCUMENTED

| Parameter | Value |
|-----------|-------|
| hidden_size | 3584 |
| vocab_size | 152064 |
| num_hidden_layers | 28 |
| num_attention_heads | 28 |
| num_key_value_heads | 4 |
| intermediate_size | 18944 |
| head_dim | 128 |
| GQA ratio | 7 |

### 4. Router Integration

**Status:** WORKING

- `RouterRing` with K-sparse selection
- Q15 quantized gates (16-bit fixed-point)
- Up to 8 adapters per ring

**Implementation:** `crates/adapteros-lora-kernel-api/src/lib.rs`

```rust
let mut ring = RouterRing::new(2);
ring.indices[0] = 0;  // First adapter
ring.indices[1] = 1;  // Second adapter
ring.gates_q15[0] = 16384;  // ~50% weight
ring.gates_q15[1] = 16383;  // ~50% weight
```

### 5. IO Buffers

**Status:** WORKING

```rust
let io = IoBuffers {
    input_ids: vec![...],
    output_logits: vec![0.0; 152064],  // vocab_size
    position: 0,
};
```

### 6. Backend Factory

**Status:** IMPLEMENTED

| Backend | Status | Use Case |
|---------|--------|----------|
| CoreML | Implemented | ANE acceleration (production) |
| MLX | Implemented | Research, training |
| Metal | Building | Legacy fallback |

**Implementation:** `crates/adapteros-lora-worker/src/backend_factory.rs`

## Integration Gaps

### Gap 1: Model File Availability

**Issue:** Full inference tests require the Qwen 2.5 7B model files to be downloaded locally.

**Expected Path:**
```
~/.cache/huggingface/hub/models--Qwen--Qwen2.5-7B-Instruct/snapshots/
```

**Resolution:**
```bash
huggingface-cli download Qwen/Qwen2.5-7B-Instruct
```

### Gap 2: Server Integration Tests

**Issue:** API endpoint tests (`/v1/infer`, `/v1/infer/batch`) require a running server.

**Resolution:** Start server before running full integration tests:
```bash
cargo run --bin aos-server &
cargo test --test mlx_inference_e2e -- --ignored
```

### Gap 3: Streaming Response Verification

**Issue:** SSE streaming tests require active connections and are not included in unit tests.

**Implementation:** `crates/adapteros-server-api/src/handlers/streaming.rs`

**Test Approach:** Use `axum-test` or integration test with SSE client.

### Gap 4: Multi-Adapter Routing E2E

**Issue:** While RouterRing works in isolation, full E2E testing with multiple adapters loaded simultaneously needs the MLX backend running.

**Required Setup:**
1. Load base model
2. Load multiple adapters
3. Route inference through K-sparse selection
4. Verify weighted outputs

## Test Execution

### Basic Tests (No Hardware Required)

```bash
cargo test --test mlx_inference_e2e -- --nocapture
```

**Coverage:**
- Adapter file existence
- Unified .aos format loading
- Manifest validation
- Safetensors weight loading
- LoRA structure verification
- Router ring creation
- IO buffer sizing
- Model configuration validation

### Full Hardware Tests

```bash
AOS_RUN_HARDWARE_TESTS=1 cargo test --test mlx_inference_e2e -- --nocapture
```

**Additional Coverage:**
- Model loading
- Actual inference
- Adapter application
- Token generation

## File Reference

| File | Purpose |
|------|---------|
| `tests/mlx_inference_e2e.rs` | Integration test suite |
| `adapters/catalog.json` | Adapter catalog |
| `adapters/*.aos` | Adapter archives |
| `crates/adapteros-aos/src/aos2_writer.rs` | .aos format implementation |
| `crates/adapteros-aos/src/lib.rs` | AosFile loader and API |
| `crates/adapteros-lora-kernel-api/src/lib.rs` | RouterRing, IoBuffers |
| `crates/adapteros-lora-worker/src/backend_factory.rs` | Backend creation |
| `crates/adapteros-lora-mlx-ffi/src/lib.rs` | MLX backend |
| `crates/adapteros-lora-kernel-coreml/src/lib.rs` | CoreML backend |

## Weight Dimension Compatibility

All adapter weights have been verified compatible with Qwen 2.5 7B:

| Layer Type | LoRA A Shape | LoRA B Shape | Compatible |
|------------|--------------|--------------|------------|
| q_proj | [rank, 3584] | [3584, rank] | Yes |
| k_proj | [rank, 512] | [512, rank] | Yes |
| v_proj | [rank, 512] | [512, rank] | Yes |
| o_proj | [rank, 3584] | [3584, rank] | Yes |

Where rank varies by adapter: 16 (code-assistant), 12 (creative-writer), 8 (readme-writer).

## Recommendations

1. **Automated Model Download:** Add script to download Qwen 2.5 7B for CI/CD
2. **Mock Backend for Tests:** Create mock MLX backend for unit tests without model
3. **SSE Test Harness:** Add `axum-test` based streaming tests
4. **Performance Baseline:** Establish inference latency baselines per adapter
5. **Memory Profiling:** Track VRAM usage during multi-adapter loading

## Conclusion

The inference pipeline infrastructure is complete and verified:
- Adapter files load correctly
- Weight dimensions are compatible
- Router and IO systems function
- Backend factory is operational

Full end-to-end inference requires:
1. Downloaded Qwen 2.5 7B model files
2. Running server for API tests
3. Sufficient GPU memory for model + adapters
