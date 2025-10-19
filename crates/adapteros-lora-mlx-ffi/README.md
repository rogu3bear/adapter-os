# AdapterOS MLX FFI Integration

**Status:** Dual-mode (real or stub)  
**Backend:** C++ FFI with real path when MLX headers/libs are present; stub otherwise  
**Notes:** No PyO3; fail-fast on stub when used for inference

## Overview

This crate provides FFI bindings for MLX (Apple's machine learning framework) to support LoRA adapter training. MLX is primarily a Python framework, so this implementation uses C++ FFI stubs for development and testing.

## Architecture

```
AdapterOS Worker
    │
    ├──> Metal Backend (Production Inference)
    │    └──> adapteros-lora-kernel-mtl
    │
    └──> MLX Backend (Training & Experimentation)
         ├──> adapteros-lora-mlx-ffi (C++ FFI - stub)
         └──> adapteros-lora-mlx (PyO3 - future)
```

## Current Implementation

### Stub Components

- **`MLXFFIModel`**: Model wrapper with forward pass stubs
- **`MLXFFIBackend`**: FusedKernels implementation (placeholder)
- **`LoRAAdapter`**: LoRA adapter management
- **`MLXFFITensor`**: Tensor operations (stub)

### Build Configuration

The `build.rs` script auto-detects MLX and emits cfgs:

- `mlx_real` when MLX C++ headers are found (links `-lmlx`) and defines `-DMLX_HAVE_REAL_API`
- `mlx_stub` when headers are not found or `MLX_FORCE_STUB=1` is set

Environment variable precedence (highest to lowest):

1. `MLX_INCLUDE_DIR` and `MLX_LIB_DIR`
2. `MLX_PATH` (uses `MLX_PATH/include` and `MLX_PATH/lib`)
3. Default: `/opt/homebrew/include` and `/opt/homebrew/lib`

The build prints which include/lib directories were selected and whether the FFI is REAL or STUB.

## Production Deployment

### Option 1: Wait for MLX C++ API (Recommended)

MLX is primarily a Python framework. When Apple releases a stable C++ API:

1. Replace stub implementation in `src/mlx_cpp_wrapper.cpp`
2. Link against MLX C++ library
3. Enable in `build.rs` by removing stub guard

### Option 2: PyO3 Integration (Alternative)

For immediate training support, use PyO3 to call MLX Python API:

1. Add PyO3 dependency
2. Implement Python bridge in `adapteros-lora-mlx` crate
3. Handle process isolation for Python runtime

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

- `MLX_INCLUDE_DIR`: Directory containing MLX headers
- `MLX_LIB_DIR`: Directory containing MLX libraries (expects `libmlx`)
- `MLX_PATH`: Base path containing `include/` and `lib/` (fallback)
- `MLX_FORCE_STUB=1`: Force stub build (useful for CI/tests)

### Runtime Behavior

- In `mlx_stub` builds, `MLXFFIModel::load(..)` returns `AosError::Unsupported` with guidance, rather than producing placeholder outputs.
- In `mlx_real` builds, normal FFI calls execute and the wrapper is compiled with `-DMLX_HAVE_REAL_API`.

Verification tips:
- Build logs show: `MLX FFI build: REAL` or `MLX FFI build: STUB`
- You can also call `mlx_wrapper_is_real()` via the FFI to probe at runtime.

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

