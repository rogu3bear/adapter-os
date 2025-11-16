# AdapterOS MLX FFI Integration

**Status:** Stub Implementation (MLX is Python-first framework)  
**Backend:** C++ FFI stubs for development  
**Production Path:** PyO3 integration (when MLX C++ API matures)

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


