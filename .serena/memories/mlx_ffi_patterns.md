# MLX FFI Patterns - AdapterOS

## Overview

The `adapteros-lora-mlx-ffi` crate provides C FFI bindings for Apple's MLX C++ library, enabling GPU-accelerated ML inference and training on Apple Silicon. This is the **primary/production backend** for AdapterOS.

## Architecture

### Two Implementation Paths

1. **Real MLX Backend** (`mlx_cpp_wrapper_real.cpp`)
   - Compiled when `--features mlx` is enabled and MLX is installed
   - Links against Homebrew MLX (`/opt/homebrew/lib/libmlx.dylib`)
   - Defines `MLX_REAL=1` during compilation
   - Sets `cargo:rustc-cfg=mlx_real`

2. **Stub Backend** (`mlx_cpp_wrapper.cpp`)
   - Used when MLX is not available (non-Apple-Silicon builds, CI)
   - Provides minimal CPU-backed behavior for test coverage
   - Returns errors for training operations
   - Sets `cargo:rustc-cfg=mlx_stub`

### Build System (`build.rs`)

- Multi-method MLX detection: `MLX_PATH` env var, pkg-config, common paths, Homebrew Cellar
- Compile+link probe verifies header/library compatibility before build
- Links frameworks: Metal, Accelerate, Foundation, CoreML, MetalPerformanceShaders
- Explicitly prohibits `-ffast-math` to maintain IEEE 754 determinism

## C++ FFI Structure

### Header File (`wrapper.h`)

Defines the C-compatible interface with `extern "C"` linkage:

```c
// Opaque types
typedef struct mlx_array mlx_array_t;
typedef struct mlx_model mlx_model_t;
typedef struct mlx_kv_cache mlx_kv_cache_t;
typedef struct mlx_weights mlx_weights_t;
typedef struct mlx_optimizer mlx_optimizer_t;
```

### Key FFI Function Categories

1. **Runtime Initialization**
   - `mlx_init(device_type)` / `mlx_init_default()`
   - `mlx_shutdown()`, `mlx_is_initialized()`
   - `mlx_backend_info()` - queries Metal/GPU capabilities

2. **Array Operations**
   - `mlx_array_from_data()`, `mlx_array_from_ints()`
   - `mlx_array_data()`, `mlx_array_size()`, `mlx_array_shape()`
   - `mlx_array_reshape()`, `mlx_array_transpose()`
   - `mlx_add()`, `mlx_multiply()`, `mlx_matmul()`

3. **Model Operations**
   - `mlx_model_load(path)` - loads SafeTensors weights
   - `mlx_model_forward()` - inference
   - `mlx_model_forward_with_hidden_states()` - captures intermediate activations
   - `mlx_model_get_weight()` - extracts specific weights (e.g., lm_head)

4. **Training Operations**
   - `mlx_cross_entropy_loss()`, `mlx_mse_loss()`
   - `mlx_lora_backward()` - gradient computation via `mx::value_and_grad`
   - `mlx_lora_backward_ce()` - cross-entropy variant with output projection
   - `mlx_optimizer_sgd()`, `mlx_optimizer_adam()`
   - `mlx_optimizer_step()`, `mlx_clip_grad_norm()`

5. **Determinism**
   - `mlx_set_seed(bytes, len)` - seeds MLX RNG from 32-byte HKDF seed
   - Converts seed to `uint64_t` and calls `mx::random::seed()`

## Rust FFI Bindings

### Error Handling (`ffi_error.rs`)

Thread-local error state pattern:
```rust
// Clear before operation
ffi_error::clear_ffi_error();

// Call FFI function
let result = unsafe { mlx_some_function(...) };

// Check result with context
ffi_error::check_ffi_ptr(result, "operation description")?;
```

Key utilities:
- `MlxArrayGuard` - RAII guard that calls `mlx_array_free()` on drop
- `MlxArrayVecGuard` - manages multiple arrays
- `get_and_clear_ffi_error()` - retrieves C++ error string

### Tensor Wrapper (`tensor.rs`)

`MLXFFITensor` wraps raw `*mut mlx_array_t` with:
- Shape tracking on Rust side (synced via `sync_shape()`)
- Type-safe operations: `add()`, `multiply()`, `matmul()`, `reshape()`, `transpose()`
- RAII cleanup via `Drop` trait
- `Send + Sync` implementations (MLX uses Metal command buffers with driver-level sync)

### Model Wrapper (`lib.rs`)

`MLXFFIModel` provides:
- Health tracking with circuit breaker pattern (opens after 3 consecutive failures)
- Tokenizer integration (loaded from model directory)
- `forward()` / `forward_with_hidden_states()` / `forward_with_kv_cache()`
- `generate()` for text generation with sampling

## Memory Management

### C++ Side

- `MLXArrayWrapper` struct tracks allocated bytes via `record_allocation()`
- `MLXModelWrapper` tracks total weight memory
- Atomic counters: `g_total_memory_used`, `g_allocation_count`
- Thread-safe tracking with `g_memory_mutex` and `g_allocation_map`

### Rust Side (`memory.rs`)

```rust
memory::gc_collect();              // Hint to reclaim buffers
memory::memory_usage();            // Total bytes allocated
memory::allocation_count();        // Number of allocations
memory::exceeds_threshold(2048.0); // Check if over 2GB
```

## Training Implementation

### Gradient Computation

Uses MLX autograd via `mx::value_and_grad`:

```cpp
auto loss_fn = [&](const std::vector<mx::array>& params) -> mx::array {
    // LoRA forward: output = hidden + (hidden @ A^T @ B^T) * scale
    mx::array lora_out = mx::matmul(mx::matmul(h, mx::transpose(a)), mx::transpose(b));
    // Compute loss...
    return loss;
};

auto grad_fn = mx::value_and_grad(loss_fn);
auto [loss, grads] = grad_fn(params);
```

### Rust Training API (`training.rs`)

```rust
// Compute gradients
let result = mlx_lora_backward_gpu(&hidden, &targets, &lora_a, &lora_b, 16.0, 16, seed)?;

// Clip gradients
let norm = mlx_clip_grad_norm_gpu(&mut [grad_a, grad_b], 1.0);

// Optimizer step
let mut optimizer = MlxOptimizer::adam(0.001, 0.9, 0.999, 1e-8, 0.0)?;
optimizer.step(&mut [lora_a, lora_b], &[grad_a, grad_b])?;
```

## Safety Considerations

1. **Pointer Validation**
   - All FFI functions check for null pointers before dereferencing
   - `check_ffi_ptr()` returns `AosError` for null results

2. **Thread Safety**
   - `MLXFFIModel` is `Send + Sync` (justified by Metal command buffer semantics)
   - Error state is thread-local (`thread_local std::string g_last_error`)

3. **Memory Safety**
   - RAII guards ensure cleanup on all exit paths
   - `into_raw()` transfers ownership without cleanup

4. **Determinism**
   - Seed validation requires 32-byte HKDF-derived seeds
   - Seed failures treated as `DeterminismViolation` errors
   - RNG errors during sampling propagate as determinism violations

## Key Invariants

- **No -ffast-math**: Build prohibits unsafe FP optimizations
- **Force evaluation before data access**: `mx::eval()` and `mx::synchronize()` called before extracting data pointers (prevents lazy evaluation race conditions)
- **Path canonicalization**: Model paths validated with `canonicalize_strict()` to prevent traversal attacks

## File Locations

- `/Users/star/Dev/adapter-os/crates/adapteros-lora-mlx-ffi/wrapper.h` - C header
- `/Users/star/Dev/adapter-os/crates/adapteros-lora-mlx-ffi/src/mlx_cpp_wrapper_real.cpp` - Real implementation
- `/Users/star/Dev/adapter-os/crates/adapteros-lora-mlx-ffi/src/mlx_cpp_wrapper.cpp` - Stub implementation
- `/Users/star/Dev/adapter-os/crates/adapteros-lora-mlx-ffi/src/lib.rs` - Main Rust module
- `/Users/star/Dev/adapter-os/crates/adapteros-lora-mlx-ffi/src/ffi_error.rs` - Error handling
- `/Users/star/Dev/adapter-os/crates/adapteros-lora-mlx-ffi/src/tensor.rs` - Tensor wrapper
- `/Users/star/Dev/adapter-os/crates/adapteros-lora-mlx-ffi/src/training.rs` - Training operations
- `/Users/star/Dev/adapter-os/crates/adapteros-lora-mlx-ffi/src/memory.rs` - Memory API
- `/Users/star/Dev/adapter-os/crates/adapteros-lora-mlx-ffi/build.rs` - Build script
