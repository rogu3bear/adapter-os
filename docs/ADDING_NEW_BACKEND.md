# Adding a New Backend to AdapterOS

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2025-01-19
**Purpose:** Template and checklist for implementing new `FusedKernels` backends

---

## Table of Contents

1. [Overview](#overview)
2. [Prerequisites](#prerequisites)
3. [Implementation Checklist](#implementation-checklist)
4. [Step-by-Step Guide](#step-by-step-guide)
5. [Testing Requirements](#testing-requirements)
6. [Performance Benchmarking](#performance-benchmarking)
7. [Integration](#integration)
8. [Example: Hypothetical CUDA Backend](#example-hypothetical-cuda-backend)

---

## Overview

This document provides a template for adding new inference backends to AdapterOS. All backends must implement the `FusedKernels` trait and provide determinism attestation.

### Design Principles

1. **Trait-Based Abstraction:** All backends implement `FusedKernels`
2. **Determinism First:** Attestation reports validated before serving
3. **Memory Safety:** Use established FFI patterns (see `OBJECTIVE_CPP_FFI_PATTERNS.md`)
4. **Feature Flags:** Experimental backends behind `--features multi-backend`
5. **Zero-Cost Abstraction:** Trait objects have minimal overhead

---

## Prerequisites

Before implementing a new backend, ensure you have:

- [ ] **Hardware access** to test backend (GPU, NPU, TPU, etc.)
- [ ] **SDK/toolchain** installed (Metal SDK, CUDA Toolkit, TensorRT, etc.)
- [ ] **FFI bindings** strategy (Rust → C/C++/Objective-C++)
- [ ] **Determinism strategy** (HKDF seeding, fixed-point math, etc.)
- [ ] **Performance baseline** (compare against Metal backend)

---

## Implementation Checklist

### Phase 1: Crate Setup

- [ ] Create new crate: `crates/adapteros-lora-kernel-<backend>/`
- [ ] Add to workspace: `Cargo.toml` in repo root
- [ ] Add `build.rs` for native library linking
- [ ] Add feature flag: `multi-backend` or production-ready
- [ ] Document backend in `README.md`

### Phase 2: Core Implementation

- [ ] Implement `FusedKernels` trait
  - [ ] `load(&mut self, plan_bytes: &[u8]) -> Result<()>`
  - [ ] `run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()>`
  - [ ] `device_name(&self) -> &str`
  - [ ] `attest_determinism(&self) -> Result<DeterminismReport>`
- [ ] Implement `Drop` trait (cleanup resources)
- [ ] Add FFI bindings (if needed)
- [ ] Add error handling with `AosError` variants

### Phase 3: Determinism Attestation

- [ ] Implement `attest_determinism()` method
  - [ ] Report backend type (add to `BackendType` enum if needed)
  - [ ] Report RNG seeding method (HKDF/FixedSeed/SystemEntropy)
  - [ ] Report floating-point mode (Deterministic/FastMath/Unknown)
  - [ ] Report compiler flags
  - [ ] Set `deterministic` flag (true/false)
- [ ] Validate attestation report in tests

### Phase 4: Testing

- [ ] Unit tests (load, execute, determinism)
- [ ] Integration tests (cross-backend consistency)
- [ ] Property tests (reproducibility)
- [ ] Benchmark tests (performance)
- [ ] Memory leak tests (FFI safety)

### Phase 5: Integration

- [ ] Add to `BackendChoice` enum in `backend_factory.rs`
- [ ] Add to `create_backend()` factory function
- [ ] Update `CLAUDE.md` with backend status
- [ ] Update `docs/ADR_MULTI_BACKEND_STRATEGY.md`
- [ ] Add integration tests in `tests/backend_selection.rs`

---

## Step-by-Step Guide

### Step 1: Create Crate

```bash
# Create crate directory
mkdir -p crates/adapteros-lora-kernel-mybackend
cd crates/adapteros-lora-kernel-mybackend

# Initialize crate
cargo init --lib
```

**`Cargo.toml`:**
```toml
[package]
name = "adapteros-lora-kernel-mybackend"
version = "0.1.0"
edition = "2021"

[dependencies]
adapteros-core = { path = "../adapteros-core" }
adapteros-lora-kernel-api = { path = "../adapteros-lora-kernel-api" }
tracing = "0.1"

[build-dependencies]
cc = "1.0"  # If FFI needed

[features]
default = []
experimental = []  # Gate behind experimental feature

[dev-dependencies]
proptest = "1.0"
criterion = "0.5"
```

**Add to workspace** (`/Cargo.toml`):
```toml
[workspace]
members = [
    # ... existing members
    "crates/adapteros-lora-kernel-mybackend",
]
```

---

### Step 2: Implement `FusedKernels` Trait

**`src/lib.rs`:**
```rust
//! MyBackend implementation for FusedKernels trait

use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::{
    attestation::{BackendType, DeterminismReport, FloatingPointMode, RngSeedingMethod},
    FusedKernels, IoBuffers, RouterRing,
};

pub struct MyBackend {
    // Backend state (device handle, context, etc.)
    device_handle: *mut std::ffi::c_void,
    device_name: String,
}

impl MyBackend {
    /// Create new backend instance
    pub fn new() -> Result<Self> {
        // Initialize backend (allocate resources, create context)
        let device_handle = unsafe { my_backend_create_device() };
        if device_handle.is_null() {
            return Err(AosError::Kernel("Failed to create device".into()));
        }

        Ok(Self {
            device_handle,
            device_name: "MyBackend (Custom Accelerator)".to_string(),
        })
    }
}

impl FusedKernels for MyBackend {
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        // Load execution plan (model weights, graph, etc.)
        unsafe {
            let ret = my_backend_load_plan(
                self.device_handle,
                plan_bytes.as_ptr(),
                plan_bytes.len(),
            );

            if ret != 0 {
                return Err(AosError::Kernel("Plan load failed".into()));
            }
        }

        tracing::info!("MyBackend: Plan loaded ({} bytes)", plan_bytes.len());
        Ok(())
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        // Execute one inference step
        unsafe {
            let ret = my_backend_execute(
                self.device_handle,
                io.input_ids.as_ptr(),
                io.input_ids.len(),
                io.output_logits.as_mut_ptr(),
                io.output_logits.len(),
                ring.indices.as_ptr(),
                ring.gates_q15.as_ptr(),
                ring.k,
            );

            if ret != 0 {
                return Err(AosError::Kernel("Execution failed".into()));
            }
        }

        io.position += 1;

        tracing::debug!(
            "MyBackend: Step complete (position={}, active_adapters={})",
            io.position,
            ring.k
        );

        Ok(())
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn attest_determinism(&self) -> Result<DeterminismReport> {
        // Report determinism guarantees
        Ok(DeterminismReport {
            backend_type: BackendType::Mock, // TODO: Add BackendType::MyBackend
            metallib_hash: None,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded, // Or SystemEntropy if non-deterministic
            floating_point_mode: FloatingPointMode::Deterministic, // Or FastMath/Unknown
            compiler_flags: vec!["-O3".to_string(), "-fno-fast-math".to_string()],
            deterministic: true, // Set to false if non-deterministic
        })
    }
}

impl Drop for MyBackend {
    fn drop(&mut self) {
        unsafe {
            if !self.device_handle.is_null() {
                my_backend_release_device(self.device_handle);
                self.device_handle = std::ptr::null_mut();
            }
        }
    }
}

// FFI declarations (if using native library)
extern "C" {
    fn my_backend_create_device() -> *mut std::ffi::c_void;
    fn my_backend_release_device(device: *mut std::ffi::c_void);
    fn my_backend_load_plan(device: *mut std::ffi::c_void, plan: *const u8, len: usize) -> i32;
    fn my_backend_execute(
        device: *mut std::ffi::c_void,
        input_ids: *const u32,
        input_len: usize,
        output_logits: *mut f32,
        output_size: usize,
        adapter_indices: *const u16,
        adapter_gates: *const i16,
        k: usize,
    ) -> i32;
}
```

---

### Step 3: Add FFI Bindings (if needed)

**`build.rs`:**
```rust
fn main() {
    // Compile C++ implementation
    cc::Build::new()
        .cpp(true)
        .file("src/mybackend_impl.cpp")
        .flag("-std=c++17")
        .flag("-fno-fast-math")  // Ensure determinism
        .flag("-O3")
        .compile("mybackend_impl");

    // Link native library
    println!("cargo:rustc-link-lib=mybackend");  // -lmybackend
    println!("cargo:rustc-link-search=native=/usr/local/lib");
}
```

**`src/mybackend_impl.cpp`:**
```cpp
#include <cstdint>
#include <cstdlib>

extern "C" {

void* my_backend_create_device() {
    // Allocate device context
    // Return opaque pointer to Rust
    return malloc(1024); // Placeholder
}

void my_backend_release_device(void* device) {
    if (device) {
        free(device);
    }
}

int my_backend_load_plan(void* device, const uint8_t* plan, size_t len) {
    // Load plan into device
    return 0; // Success
}

int my_backend_execute(
    void* device,
    const uint32_t* input_ids,
    size_t input_len,
    float* output_logits,
    size_t output_size,
    const uint16_t* adapter_indices,
    const int16_t* adapter_gates,
    size_t k
) {
    // Execute inference
    // Populate output_logits
    return 0; // Success
}

} // extern "C"
```

---

### Step 4: Implement Determinism Strategy

**Option A: HKDF-Seeded RNG (Deterministic)**

```rust
use adapteros_core::{derive_seed, B3Hash};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

pub struct MyBackend {
    base_seed: B3Hash,
    // ...
}

impl MyBackend {
    fn derive_step_seed(&self, position: usize) -> [u8; 32] {
        let label = format!("mybackend-step:{}", position);
        derive_seed(&self.base_seed, &label)
    }

    fn execute_with_seeded_rng(&self, position: usize) -> Vec<f32> {
        let seed = self.derive_step_seed(position);
        let mut rng = ChaCha20Rng::from_seed(seed);

        // Use RNG for deterministic randomness
        let mut logits = vec![0.0f32; 32000];
        for logit in &mut logits {
            *logit = rng.next_u32() as f32 / u32::MAX as f32;
        }

        logits
    }
}
```

**Option B: Fixed-Point Math (Deterministic)**

```cpp
// Use fixed-point arithmetic instead of floating-point
typedef int32_t fixed_t; // Q15.16 fixed-point

fixed_t float_to_fixed(float f) {
    return (fixed_t)(f * 65536.0f);
}

float fixed_to_float(fixed_t x) {
    return (float)x / 65536.0f;
}

// Deterministic matrix multiply
void matmul_fixed(
    const fixed_t* A, const fixed_t* B, fixed_t* C,
    size_t m, size_t n, size_t k
) {
    for (size_t i = 0; i < m; i++) {
        for (size_t j = 0; j < n; j++) {
            int64_t sum = 0;
            for (size_t p = 0; p < k; p++) {
                sum += (int64_t)A[i*k + p] * (int64_t)B[p*n + j];
            }
            C[i*n + j] = (fixed_t)(sum >> 16); // Shift back to Q15.16
        }
    }
}
```

**Option C: System Entropy (Non-Deterministic)**

```rust
impl MyBackend {
    fn attest_determinism(&self) -> Result<DeterminismReport> {
        Ok(DeterminismReport {
            backend_type: BackendType::Mock,
            rng_seed_method: RngSeedingMethod::SystemEntropy, // Non-deterministic
            floating_point_mode: FloatingPointMode::FastMath,
            deterministic: false, // Explicitly non-deterministic
            // ...
        })
    }
}
```

---

## Testing Requirements

### Unit Tests

**`src/lib.rs`:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_lora_kernel_api::{IoBuffers, RouterRing};

    #[test]
    fn test_backend_creation() {
        let backend = MyBackend::new();
        assert!(backend.is_ok());
    }

    #[test]
    fn test_plan_loading() {
        let mut backend = MyBackend::new().unwrap();
        let plan = vec![0u8; 1024];
        assert!(backend.load(&plan).is_ok());
    }

    #[test]
    fn test_inference_step() {
        let mut backend = MyBackend::new().unwrap();
        let plan = vec![0u8; 1024];
        backend.load(&plan).unwrap();

        let mut io = IoBuffers::new(32000);
        io.input_ids = vec![1, 2, 3, 4];
        let ring = RouterRing::new(0);

        assert!(backend.run_step(&ring, &mut io).is_ok());
        assert_eq!(io.position, 1);
    }

    #[test]
    fn test_determinism_attestation() {
        let backend = MyBackend::new().unwrap();
        let report = backend.attest_determinism().unwrap();

        // Validate report
        assert!(report.validate().is_ok());
        assert!(report.deterministic); // Should be true for production backends
    }
}
```

---

### Determinism Test (Critical)

**`tests/determinism.rs`:**
```rust
use adapteros_lora_kernel_mybackend::MyBackend;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

#[test]
fn test_reproducibility() {
    let mut backend1 = MyBackend::new().unwrap();
    let mut backend2 = MyBackend::new().unwrap();

    let plan = create_test_plan();
    backend1.load(&plan).unwrap();
    backend2.load(&plan).unwrap();

    let mut io1 = IoBuffers::new(32000);
    let mut io2 = IoBuffers::new(32000);
    io1.input_ids = vec![1, 2, 3, 4, 5];
    io2.input_ids = vec![1, 2, 3, 4, 5];

    let ring = RouterRing::new(0);

    // Run multiple steps
    for _ in 0..10 {
        backend1.run_step(&ring, &mut io1).unwrap();
        backend2.run_step(&ring, &mut io2).unwrap();
    }

    // Verify bit-identical output
    assert_eq!(
        io1.output_logits, io2.output_logits,
        "Backend is non-deterministic"
    );
}

fn create_test_plan() -> Vec<u8> {
    // Create test plan (simplified)
    vec![0u8; 1024]
}
```

---

### Property Tests

**`tests/proptest.rs`:**
```rust
use proptest::prelude::*;
use adapteros_lora_kernel_mybackend::MyBackend;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

proptest! {
    #[test]
    fn test_any_input_produces_valid_output(input_ids in prop::collection::vec(0u32..32000, 1..128)) {
        let mut backend = MyBackend::new().unwrap();
        let plan = vec![0u8; 1024];
        backend.load(&plan).unwrap();

        let mut io = IoBuffers::new(32000);
        io.input_ids = input_ids;
        let ring = RouterRing::new(0);

        // Should not panic for any input
        backend.run_step(&ring, &mut io).unwrap();

        // Output should be finite
        assert!(io.output_logits.iter().all(|&x| x.is_finite()));
    }
}
```

---

## Performance Benchmarking

**`benches/inference.rs`:**
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use adapteros_lora_kernel_mybackend::MyBackend;
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

fn benchmark_inference(c: &mut Criterion) {
    let mut backend = MyBackend::new().unwrap();
    let plan = vec![0u8; 1024];
    backend.load(&plan).unwrap();

    let mut io = IoBuffers::new(32000);
    io.input_ids = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let ring = RouterRing::new(0);

    c.bench_function("mybackend_inference_step", |b| {
        b.iter(|| {
            backend.run_step(black_box(&ring), black_box(&mut io)).unwrap();
        })
    });
}

criterion_group!(benches, benchmark_inference);
criterion_main!(benches);
```

**Run:**
```bash
cargo bench -p adapteros-lora-kernel-mybackend
```

---

## Integration

### Add to `BackendChoice` Enum

**`crates/adapteros-lora-worker/src/backend_factory.rs`:**
```rust
#[derive(Debug, Clone)]
pub enum BackendChoice {
    Metal,
    Mlx { model_path: PathBuf },
    CoreML,
    MyBackend, // ← Add new variant
}
```

### Add to Factory Function

```rust
fn create_backend_internal(choice: BackendChoice) -> Result<Box<dyn FusedKernels>> {
    match choice {
        // ... existing cases

        BackendChoice::MyBackend => {
            #[cfg(feature = "multi-backend")]
            {
                let backend = adapteros_lora_kernel_mybackend::MyBackend::new()?;
                tracing::info!("Created MyBackend: {}", backend.device_name());
                Ok(Box::new(backend))
            }

            #[cfg(not(feature = "multi-backend"))]
            {
                Err(AosError::PolicyViolation(
                    "MyBackend requires --features multi-backend".to_string()
                ))
            }
        }
    }
}
```

### Update Documentation

**`CLAUDE.md`:**
```markdown
| Backend | Status | Determinism | Primary Use Case |
|---------|--------|-------------|------------------|
| **Metal** | **Production** | **Guaranteed** | M1/M2/M3/M4 GPU |
| **CoreML** | **Active** | **Conditional** | ANE acceleration |
| **MLX** | **Future** | **Experimental** | Research |
| **MyBackend** | **Experimental** | **Guaranteed** | Custom accelerator |
```

---

## Example: Hypothetical CUDA Backend

**Crate:** `crates/adapteros-lora-kernel-cuda/`

**Features:**
- NVIDIA GPU acceleration (Linux/Windows)
- CUDA 12.0+ required
- Deterministic via cuBLAS deterministic mode

**Implementation:**
```rust
// crates/adapteros-lora-kernel-cuda/src/lib.rs

use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

pub struct CudaBackend {
    cuda_stream: *mut std::ffi::c_void,
}

impl CudaBackend {
    pub fn new(device_id: i32) -> Result<Self> {
        unsafe {
            cuda_set_device(device_id);
            cuda_enable_deterministic_mode(); // cuBLAS deterministic
            let stream = cuda_stream_create();
            Ok(Self { cuda_stream: stream })
        }
    }
}

impl FusedKernels for CudaBackend {
    fn attest_determinism(&self) -> Result<DeterminismReport> {
        Ok(DeterminismReport {
            backend_type: BackendType::Cuda, // TODO: Add to enum
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            compiler_flags: vec!["-DCUBLAS_DETERMINISTIC".to_string()],
            deterministic: true,
            // ...
        })
    }

    // ... implement other methods
}

extern "C" {
    fn cuda_set_device(device_id: i32);
    fn cuda_enable_deterministic_mode();
    fn cuda_stream_create() -> *mut std::ffi::c_void;
}
```

---

## References

- [docs/ADR_MULTI_BACKEND_STRATEGY.md](./ADR_MULTI_BACKEND_STRATEGY.md) - Backend selection rationale
- [docs/OBJECTIVE_CPP_FFI_PATTERNS.md](./OBJECTIVE_CPP_FFI_PATTERNS.md) - FFI memory safety
- [crates/adapteros-lora-kernel-api/src/lib.rs](../crates/adapteros-lora-kernel-api/src/lib.rs) - `FusedKernels` trait
- [crates/adapteros-lora-kernel-mtl/](../crates/adapteros-lora-kernel-mtl/) - Metal backend reference implementation

---

**Signed:** James KC Auchterlonie
**Date:** 2025-01-19
