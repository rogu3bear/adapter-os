# MLX Backend HKDF Seeding Implementation

**Status:** Complete
**Last Updated:** 2025-11-19
**Maintained by:** James KC Auchterlonie

---

## Overview

This document describes the deterministic RNG seeding implementation for the MLX experimental backend using HKDF-derived seeds. Unlike the Metal backend (guaranteed deterministic), the MLX backend uses HKDF seeding to control operations that depend on randomness (dropout, sampling) while acknowledging that execution order itself is not deterministic.

---

## Architecture

### HKDF Seed Derivation

The seed derivation follows the standard AdapterOS pattern:

```rust
// Global seed from model manifest hash
let manifest_hash = manifest.compute_hash()?;
let global_seed = derive_seed(&manifest_hash, "executor");

// MLX backend seed with model path
let model_hash = B3Hash::hash(model_path.as_bytes());
let mlx_seed = derive_seed(&model_hash, "mlx-backend:{model_path_hash}");

// Plan-specific reseed (when loading inference plan)
let plan_hash = B3Hash::hash(plan_bytes);
let step_seed = derive_seed(&base_seed, "mlx-plan:{plan_hash}");

// Adapter-specific seed
let adapter_seed = derive_seed(&base_seed, "mlx-adapter:{adapter_id}");

// Per-step seed for dropout/sampling
let token_seed = derive_seed(&base_seed, "mlx-step:{token_position}");
```

**Seed length:** 32 bytes (256 bits)
**Derivation function:** HKDF-SHA256
**Determinism scope:** Operation-level RNG, not execution order

### Seeding Flow

```
┌─────────────────────────────────────┐
│ Model Manifest Hash (BLAKE3)        │
└──────────┬──────────────────────────┘
           │
           ▼
┌─────────────────────────────────────┐
│ derive_seed(&hash, "mlx-backend")   │
│ → 32-byte seed                      │
└──────────┬──────────────────────────┘
           │
           ▼
┌─────────────────────────────────────┐
│ mlx_set_seed_from_bytes(&seed)      │
│ (Rust FFI wrapper)                  │
└──────────┬──────────────────────────┘
           │
           ▼
┌─────────────────────────────────────┐
│ mlx_set_seed(seed_ptr, seed_len)    │
│ (C FFI binding)                     │
└──────────┬──────────────────────────┘
           │
           ▼
┌─────────────────────────────────────┐
│ mx::random::seed(seed_value)        │
│ (MLX C++ API)                       │
└─────────────────────────────────────┘
```

---

## Implementation Details

### 1. C Header Declaration (`wrapper.h`)

```c
// RNG seeding (for deterministic dropout/sampling)
// Sets MLX's global random seed from a 32-byte seed buffer (HKDF-derived)
// Note: MLX's backend may not guarantee full execution order determinism,
// but seeded operations (dropout, sampling) will be deterministic.
void mlx_set_seed(const uint8_t* seed, size_t seed_len);
```

### 2. C++ Implementation (`mlx_cpp_wrapper_real.cpp`)

```cpp
extern "C" void mlx_set_seed(const uint8_t* seed, size_t seed_len) {
    if (!seed || seed_len == 0) {
        g_last_error = "Invalid seed: pointer is null or length is 0";
        return;
    }

    try {
        // Convert seed bytes to uint64_t
        // MLX's random::seed() takes a uint64_t, so we use the first 8 bytes
        uint64_t seed_value = 0;

        if (seed_len >= 8) {
            // Use first 8 bytes as big-endian uint64
            for (size_t i = 0; i < 8; i++) {
                seed_value = (seed_value << 8) | seed[i];
            }
        } else {
            // Pad shorter seeds with zeros
            for (size_t i = 0; i < seed_len; i++) {
                seed_value = (seed_value << 8) | seed[i];
            }
            // Shift to align if seed_len < 8
            seed_value <<= (8 - seed_len) * 8;
        }

        // Set MLX's global random seed
        mx::random::seed(seed_value);

    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to set MLX seed: ") + e.what();
    }
}
```

**Key points:**
- Converts 32-byte HKDF seed to 64-bit value using first 8 bytes (big-endian)
- Handles padding for shorter seeds
- Uses `mx::random::seed()` to set MLX's global RNG state
- Error handling via thread-local error string

### 3. Rust FFI Binding (`lib.rs`)

```rust
pub fn mlx_set_seed_from_bytes(seed: &[u8]) -> Result<()> {
    if seed.is_empty() {
        return Err(AosError::Internal("Seed buffer cannot be empty".to_string()));
    }

    unsafe {
        mlx_set_seed(seed.as_ptr(), seed.len());

        // Check if there was an error during seed setting
        let error_msg = mlx_get_last_error();
        if !error_msg.is_null() {
            let error_str = std::ffi::CStr::from_ptr(error_msg)
                .to_string_lossy()
                .to_string();
            if !error_str.is_empty() {
                mlx_clear_error();
                tracing::warn!("MLX seed setting warning: {}", error_str);
            }
        }
    }

    tracing::debug!(
        seed_len = seed.len(),
        "MLX backend seeded for deterministic dropout/sampling"
    );

    Ok(())
}
```

### 4. Backend Factory Integration (`backend_factory.rs`)

Seeds are set at two critical points:

**a) Plan Load (Inference Initialization)**
```rust
fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
    let plan_hash = B3Hash::hash(plan_bytes);
    let label = format!("mlx-plan:{}", plan_hash.to_short_hex());
    let reseeded = derive_seed(&self.base_seed, &label);
    self.base_seed = B3Hash::from_bytes(reseeded);

    // Set MLX's RNG seed for deterministic dropout/sampling
    let seed_slice: [u8; 32] = self.base_seed.as_bytes().try_into()?;
    mlx_set_seed_from_bytes(&seed_slice)?;

    tracing::info!(
        plan_len = plan_bytes.len(),
        seed_preview = %self.base_seed.to_short_hex(),
        "MLX backend loaded plan and seeded RNG deterministically"
    );

    Ok(())
}
```

**b) Adapter Load (Per-Adapter Seeding)**
```rust
fn load_adapter(&mut self, id: u16, _weights: &[u8]) -> Result<()> {
    let adapter_seed = self.derive_adapter_seed(id);
    mlx_set_seed_from_bytes(&adapter_seed)?;

    tracing::info!(
        adapter_id = id,
        seed_preview = %hex::encode(&adapter_seed[..4]),
        "MLX backend registered adapter with deterministic RNG seeding"
    );
    Ok(())
}
```

### 5. Attestation Report

```rust
fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
    Ok(attestation::DeterminismReport {
        backend_type: attestation::BackendType::Mlx,
        metallib_hash: None,
        manifest: None,
        rng_seed_method: attestation::RngSeedingMethod::HkdfSeeded,
        floating_point_mode: attestation::FloatingPointMode::Unknown,
        compiler_flags: vec!["-DMLX_HKDF_SEEDED".to_string()],
        deterministic: false,  // ⚠️ NOT fully deterministic
    })
}
```

**Important:** `deterministic: false` is correct because:
1. Execution order is not deterministic (async GPU operations)
2. Floating-point rounding may vary between runs
3. MLX does not provide order-determinism guarantees

---

## Determinism Scopes

### Deterministic (✓)

- **RNG state:** Seeded via HKDF, so `rand()` calls produce same sequence
- **Dropout masks:** Same seed → same dropout pattern across runs
- **Sampling operations:** Seeded sampling is reproducible
- **Routing decisions:** Q15 quantized gates deterministic
- **Model weights:** Frozen weights deterministic
- **Adapter selection:** Deterministic (via router)

### Non-Deterministic (✗)

- **Execution order:** GPU scheduler may reorder async ops
- **Floating-point rounding:** May vary in multi-threaded contexts
- **Memory access patterns:** Unfused kernels may execute in different order
- **Accumulated numerical errors:** Small variations across runs
- **GPU memory layout:** Optimization strategies may vary

### Example: Deterministic Dropout

```rust
// Same seed → same dropout pattern
seed = derive_seed(base, "step:0");
mlx_set_seed_from_bytes(&seed);
dropout_mask = generate_dropout_mask();  // Same mask every time

// But execution order might differ:
result1 = model.forward(input);  // May reorder operations
result2 = model.forward(input);  // Different execution order, but same result (usually)
```

---

## Testing & Validation

### Seeded Operations Test

```rust
#[test]
fn test_mlx_seeded_dropout_deterministic() {
    // Same seed should produce same dropout pattern
    let seed = [1u8; 32];

    mlx_set_seed_from_bytes(&seed).unwrap();
    let mask1 = mlx_generate_dropout_mask(0.5);

    mlx_set_seed_from_bytes(&seed).unwrap();
    let mask2 = mlx_generate_dropout_mask(0.5);

    assert_eq!(mask1, mask2);
}
```

### Execution Order Sensitivity Test

```rust
#[test]
fn test_mlx_execution_order_may_vary() {
    // Different runs may have slight numerical differences
    let seed = [1u8; 32];

    let results: Vec<f32> = (0..10)
        .map(|_| {
            mlx_set_seed_from_bytes(&seed).unwrap();
            mlx_model_forward(&input).unwrap()
        })
        .collect();

    // Results may vary slightly due to async execution order
    for window in results.windows(2) {
        let diff = (window[0] - window[1]).abs();
        // Allow small numerical differences due to execution order variation
        assert!(diff < 1e-4, "Numerical difference: {}", diff);
    }
}
```

---

## Limitations & Caveats

### 1. GPU Scheduling Non-Determinism

MLX uses async GPU operations that may execute in different orders:

```
Run 1: [matmul, dropout, relu, softmax]
Run 2: [dropout, matmul, relu, softmax]  ← Different order
```

This can cause:
- Accumulated floating-point rounding differences
- Small numerical variations in outputs
- Non-bitwise-identical results

### 2. Floating-Point Rounding

MLX may use different floating-point rounding modes:
- IEEE 754 default (round-to-nearest)
- GPU-optimized rounding (may vary)
- Multi-threaded context (different thread scheduling)

### 3. Unfused Kernels

MLX's unfused kernels may execute in parallel:
```
kernel_a()  ┐
            ├─→ Different order each run
kernel_b()  ┘
```

Unlike Metal (fully fused), MLX cannot guarantee kernel fusion order.

### 4. Async Memory Management

MLX's unified memory is asynchronously managed:
```
Request: array allocation → GPU memory
         actual allocation timing varies
```

Memory layout changes can affect cache behavior and results.

---

## Comparison with Other Backends

| Feature | Metal | CoreML | MLX |
|---------|-------|--------|-----|
| **Determinism** | ✓ Guaranteed | ~ ANE-only | ✗ No |
| **RNG Seeding** | Via global seed | Via global seed | Via HKDF |
| **Execution Order** | ✓ Fused kernels | ~ Conditional | ✗ Async |
| **Float Rounding** | ✓ Controlled | ~ Depends on backend | ✗ Uncontrolled |
| **Use Case** | Production | Power-efficient | Research |
| **Policy Attestation** | `deterministic: true` | `deterministic: ane_available` | `deterministic: false` |

---

## Usage Guidelines

### ✓ Good: Research & Development

```rust
// MLX is ideal for research with HKDF seeding
let mlx_backend = create_backend(BackendChoice::Mlx {
    model_path: PathBuf::from("models/qwen2.5-7b-mlx"),
})?;

// Seeding ensures reproducible RNG behavior for debugging
// Allows iteration on model changes without worrying about
// non-determinism in dropout/sampling
```

### ✗ Bad: Production Inference

```rust
// Do NOT use MLX for production determinism-critical inference
// Metal backend is required for guaranteed determinism
if config.production_mode {
    let backend = create_backend(BackendChoice::Metal)?;  // ✓ Use Metal
    // NOT: create_backend(BackendChoice::Mlx { ... })?;  // ✗ Not suitable
}
```

### ~ Conditional: Validation

```rust
// MLX can be used for validation if small numerical differences acceptable
// HKDF seeding ensures RNG is controlled, but execution order varies

// Check attestation before using
let report = backend.attest_determinism()?;
if report.deterministic {
    // Safe for strict validation
} else {
    // Accept small numerical differences
    use_relaxed_comparison(result1, result2, tolerance=1e-4);
}
```

---

## Future Improvements

### 1. MLX Determinism Option

MLX may add a determinism flag in future versions:

```cpp
// Hypothetical future API
mx::backend_config()
    .set_determinism_level(mx::Determinism::Full);
```

Once available, this could improve MLX's determinism guarantees.

### 2. Fused Kernel Compilation

Custom MLX kernel compilation with fusion:

```cpp
// Custom fused kernel
mlx_array_t* mlx_fused_dropout_relu(
    mlx_array_t* input,
    float dropout_rate,
    uint64_t seed
);
```

This would reduce non-determinism from async operations.

### 3. Deterministic Memory Allocation

Pre-allocate and reuse memory pools:

```rust
// Deterministic memory strategy
mlx_memory_preallocate(total_bytes);
mlx_memory_set_allocation_strategy(Deterministic);
```

---

## Files Modified

- **`crates/adapteros-lora-mlx-ffi/wrapper.h`**
  - Added `mlx_set_seed()` C function declaration

- **`crates/adapteros-lora-mlx-ffi/src/mlx_cpp_wrapper_real.cpp`**
  - Implemented `mlx_set_seed()` using `mx::random::seed()`
  - Seed format conversion (32-byte to 64-bit)

- **`crates/adapteros-lora-mlx-ffi/src/lib.rs`**
  - Added safe Rust wrapper `mlx_set_seed_from_bytes()`
  - Added memory management API documentation
  - Error handling and logging

- **`crates/adapteros-lora-worker/src/backend_factory.rs`**
  - Integrated seed setting in `MlxBackend::load()`
  - Integrated seed setting in `MlxBackend::load_adapter()`
  - Updated `attest_determinism()` to return `deterministic: false`
  - Added comprehensive determinism documentation

---

## References

- [docs/ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - Backend architecture
- [docs/DETERMINISTIC_EXECUTION.md](DETERMINISTIC_EXECUTION.md) - Global determinism strategy
- [docs/ADR_MULTI_BACKEND_STRATEGY.md](ADR_MULTI_BACKEND_STRATEGY.md) - Backend selection rationale
- MLX C++ API: `mx::random::seed()` documentation
- HKDF: RFC 5869 (HMAC-based Extract-and-Expand Key Derivation Function)

---

## Signing

**Signature:** James KC Auchterlonie
**Date:** 2025-11-19
**Hash:** blake3(doc_content)
