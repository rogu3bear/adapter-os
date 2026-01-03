# AdapterOS MLX FFI Integration

**Status:** Production-Ready Implementation
**Backend:** MLX (FFI primary; mlx-rs fallback optional)
**Features:** Model loading, inference, LoRA adaptation, deterministic execution

## Overview

This crate provides production-ready FFI bindings for MLX (Apple's machine learning framework) supporting LoRA adapter inference and training. The primary path is the C++ FFI backend; an optional `mlx-rs-backend` feature enables an experimental Rust fallback when FFI initialization fails.

## Architecture

```
AdapterOS Worker
    │
    └──> MLX Backend (PRIMARY - Inference & Training)
         ├──> CoreML (ANE acceleration for specific layers)
         └──> Metal Kernels (GPU compute primitives)
```

MLX is the primary backend for all inference and training workloads on Apple Silicon.
CoreML provides Neural Engine acceleration for specific operations. Metal provides
low-level GPU compute primitives.

## Current Implementation

### Core Components (C++ FFI)

- **`MLXFFIModel`**: Model wrapper with forward pass + hidden states
- **`MLXFFIBackend`**: Kernel backend integration
- **`LoRAAdapter`**: Adapter management + hot-swap cache
- **`MLXFFITensor`**: Tensor ops backed by MLX arrays
- **KV cache + memory tracking**: Deterministic execution + GC hints

### Build Configuration

`build.rs` auto-detects MLX headers/libs. With `--features mlx`, it compiles the
real wrapper (`src/mlx_cpp_wrapper_real.cpp`). If MLX is missing or
`MLX_FORCE_STUB=1` is set, it falls back to the stub wrapper
(`src/mlx_cpp_wrapper.cpp`).

To enable the experimental Rust fallback, build with `--features mlx-rs-backend`.
At runtime, the MLX implementation is selected automatically; override with
`AOS_MLX_IMPL=ffi|rs|auto` for internal debugging.

## Production Deployment

1. Install MLX: `brew install mlx`
2. Build real MLX: `cargo build -p adapteros-lora-mlx-ffi --features mlx --release`
3. Optional fallback: add `mlx-rs-backend` to build flags for experimental Rust fallback
4. Optional: set `MLX_PATH`/`MLX_INCLUDE_DIR`/`MLX_LIB_DIR` for custom installs
5. Verify: build output shows `MLX FFI build: REAL` or check `mlx_version()`

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
- `AOS_MLX_IMPL`: Internal MLX implementation override (`auto`, `ffi`, `rs`)

## Policy Compliance

- **Determinism Ruleset (#2)**: Training runs use HKDF-seeded RNG
- **Isolation Ruleset (#8)**: Per-tenant process boundaries
- **Build & Release Ruleset (#15)**: Stub implementation documented

## Future Work

- [ ] Expand operator coverage and error reporting
- [ ] Extend LoRA training + gradient checkpointing
- [ ] Tighten model loading/quantization paths
- [ ] Continue benchmarking vs Metal/CoreML

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
