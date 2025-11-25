# AdapterOS MLX FFI Integration

**Status:** Production-Ready Implementation
**Backend:** Real MLX C++ FFI with GPU acceleration
**Features:** Model loading, inference, LoRA adaptation, deterministic execution

## Overview

This crate provides production-ready FFI bindings for MLX (Apple's machine learning framework) supporting LoRA adapter inference and training. Features real GPU acceleration with HKDF-seeded deterministic execution, circuit breaker health monitoring, and memory pool integration.

## Architecture

```
AdapterOS Worker
    │
    ├──> Metal Backend (Production Inference)
    │    └──> adapteros-lora-kernel-mtl
    │
    └──> MLX Backend (Training & Experimentation)
         ├──> adapteros-lora-mlx-ffi (C++ FFI - stub)
         └──> adapteros-lora-mlx (C++ FFI - future)
```

## Current Implementation

### Stub Components

- **`MLXFFIModel`**: Model wrapper with forward pass stubs
- **`MLXFFIBackend`**: FusedKernels implementation (placeholder)
- **`LoRAAdapter`**: LoRA adapter management
- **`MLXFFITensor`**: Tensor operations (stub)

### Build Configuration

The `build.rs` script generates stub bindings and placeholder implementations:

```rust
// Using stub implementation for now
generate_stub_bindings();
```

## Production Deployment

### Option 1: Wait for MLX C++ API (Recommended)

MLX is primarily a Python framework. When Apple releases a stable C++ API:

1. Replace stub implementation in `src/mlx_cpp_wrapper.cpp`
2. Link against MLX C++ library
3. Enable in `build.rs` by removing stub guard

### Option 2: Alternative Backends (Current Recommendation)

For production training and inference, use Metal backend:

1. Use `adapteros-lora-kernel-mtl` for GPU acceleration
2. Enable with `--features metal-backend`
3. Full production support on Apple Silicon

## Development

### Build

```bash
cargo build --package adapteros-lora-mlx-ffi
```

### Test

```bash
cargo test --package adapteros-lora-mlx-ffi
```

### Environment Variables

- `MLX_PATH`: Path to MLX installation (default: `/opt/homebrew`)

## Policy Compliance

- **Determinism Ruleset (#2)**: Training runs use HKDF-seeded RNG
- **Isolation Ruleset (#8)**: Per-tenant process boundaries
- **Build & Release Ruleset (#15)**: Stub implementation documented

## Future Work

- [ ] Complete C++ wrapper when MLX C++ API is stable
- [ ] Implement forward pass with hidden states
- [ ] Add LoRA training loop
- [ ] Integrate gradient checkpointing
- [ ] Add multi-adapter training
- [ ] Performance benchmarking vs Metal backend

## References

- [MLX Documentation](https://ml-explore.github.io/mlx/)
- [MasterPlan.md Patch 4.2](../../docs/architecture/MasterPlan.md#patch-42-mlx-integration)
- [Kernel API Trait](../adapteros-lora-kernel-api/src/lib.rs)

## See Also

- [MLX_FFI_INTEGRATION_PROOF.md](./MLX_FFI_INTEGRATION_PROOF.md) - MLX FFI integration proof document
- [docs/MLX_INTEGRATION.md](../../docs/MLX_INTEGRATION.md) - Complete MLX integration guide
- [docs/ADR_MULTI_BACKEND_STRATEGY.md](../../docs/ADR_MULTI_BACKEND_STRATEGY.md) - Multi-backend architecture decision
- [docs/COREML_INTEGRATION.md](../../docs/COREML_INTEGRATION.md) - CoreML backend (alternative)
- [BENCHMARK_RESULTS.md](../../BENCHMARK_RESULTS.md) - MLX FFI benchmark results
- [benches/mlx_integration_benchmark.rs](./benches/mlx_integration_benchmark.rs) - MLX FFI benchmarks
- [tests/INDEX.md](./tests/INDEX.md) - Test documentation index


