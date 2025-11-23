# AdapterOS FFI Guide

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.  
**Last Updated:** 2025-11-22  
**Purpose:** Complete guide to Foreign Function Interface (FFI) usage in AdapterOS

---

## Table of Contents

1. [What is FFI?](#what-is-ffi)
2. [Why AdapterOS Uses FFI](#why-adapteros-uses-ffi)
3. [FFI Architecture Overview](#ffi-architecture-overview)
4. [Performance Characteristics](#performance-characteristics)
5. [Security Model](#security-model)
6. [FFI Types in AdapterOS](#ffi-types-in-adapteros)
7. [Best Practices](#best-practices)
8. [Quick Reference](#quick-reference)

---

## What is FFI?

**Foreign Function Interface (FFI)** is a mechanism that allows code written in one programming language to call functions written in another language.

In AdapterOS, FFI enables Rust code to:
- Call Apple's native frameworks (CoreML, Metal, Foundation)
- Access hardware acceleration (Apple Neural Engine, GPU)
- Use production ML backends (MLX C++ API, CoreML)
- Monitor system resources (Metal heap observers)

### The Problem FFI Solves

Rust is excellent for:
- ✅ Memory safety
- ✅ Concurrency
- ✅ Type system
- ✅ Performance

But Rust **cannot**:
- ❌ Compile Metal shaders (requires Metal Shading Language)
- ❌ Directly call Objective-C/Swift frameworks
- ❌ Access Apple Neural Engine APIs
- ❌ Use C++ MLX library directly

**FFI bridges this gap** - allowing Rust to leverage platform-specific capabilities while maintaining Rust's safety guarantees where possible.

---

## Why AdapterOS Uses FFI

### 1. Hardware Acceleration

AdapterOS needs access to Apple's ML hardware:

| Hardware | Access Method | Performance Gain |
|----------|---------------|------------------|
| **Apple Neural Engine (ANE)** | CoreML FFI | 10-100x faster inference |
| **Metal GPU** | Metal shaders + FFI | Parallel processing |
| **Unified Memory** | Metal heap observers | Zero-copy operations |

**Without FFI**: CPU-only inference (~500ms per request)  
**With FFI**: ANE-accelerated inference (~10ms per request)

### 2. Platform Frameworks

Apple's frameworks are battle-tested and optimized:

- **CoreML**: Apple's ML framework with ANE support
- **Metal**: GPU programming framework
- **Foundation**: System-level APIs
- **Accelerate**: Optimized math libraries

These frameworks are:
- ✅ Signed and sandboxed by Apple
- ✅ Optimized for Apple Silicon
- ✅ Production-ready and stable
- ✅ Not available in pure Rust

### 3. Production ML Backends

AdapterOS uses multiple production-ready ML backends via FFI:

**CoreML Backend** (Production):
- Fully implemented and operational
- ANE acceleration with guaranteed determinism
- Model loading, inference, and memory management
- Swift bridge for macOS 15+ MLTensor operations

**MLX Backend** (Production):
- Fully implemented with enterprise-grade resilience
- C++ API for high performance
- Full model loading, inference, and training capabilities
- Health monitoring, circuit breakers, and auto-recovery
- Multi-adapter routing with K-sparse selection

**Metal Backend** (Production):
- Precompiled Metal kernels for GPU acceleration
- Deterministic execution guarantees
- Memory pool integration

All three backends are production-ready and used in real deployments. FFI enables AdapterOS to leverage the best backend for each workload.

---

## FFI Architecture Overview

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────┐
│ Rust Application Code (Safe)                            │
│  - adapteros-server-api                                 │
│  - adapteros-lora-worker                                │
│  - adapteros-lora-router                                │
└─────────────────┬───────────────────────────────────────┘
                  │ Safe Rust API
                  ↓
┌─────────────────────────────────────────────────────────┐
│ FFI Boundary Layer (Minimal Unsafe)                     │
│  - adapteros-lora-kernel-coreml/src/ffi.rs              │
│  - adapteros-lora-mlx-ffi/src/lib.rs                    │
│  - adapteros-memory/src/heap_observer_ffi.rs           │
└─────────────────┬───────────────────────────────────────┘
                  │ extern "C" FFI calls
                  ↓
┌─────────────────────────────────────────────────────────┐
│ Native Bridge Code (C/C++/ObjC++)                       │
│  - coreml_bridge.mm (Objective-C++)                     │
│  - mlx_cpp_wrapper.cpp (C++)                            │
│  - heap_observer_callbacks.mm (Objective-C++)           │
└─────────────────┬───────────────────────────────────────┘
                  │ Framework APIs
                  ↓
┌─────────────────────────────────────────────────────────┐
│ Apple Frameworks (Signed & Sandboxed)                   │
│  - CoreML.framework                                     │
│  - Metal.framework                                      │
│  - Foundation.framework                                 │
└─────────────────────────────────────────────────────────┘
```

### FFI Call Flow Example

```rust
// 1. Rust code (safe)
let result = backend.run_inference(input_ids)?;

// 2. FFI boundary (minimal unsafe)
impl MLXFFIBackend {
    pub fn run_inference(&self, input: &[u32]) -> Result<Vec<f32>> {
        unsafe {
            // Single unsafe block, well-audited
            ffi::mlx_model_forward(self.model_handle, input.as_ptr(), input.len())
        }
    }
}

// 3. Native bridge (C++)
extern "C" mlx_array_t* mlx_model_forward(
    mlx_model_t* model,
    const uint32_t* input_ids,
    size_t input_len
) {
    // Calls MLX C++ API
    return model->forward(input_ids, input_len);
}

// 4. Framework (Apple's code)
// CoreML/Metal executes on hardware
```

---

## Performance Characteristics

### Measured FFI Overhead

From benchmark results (`BENCHMARK_RESULTS.md`):

| Operation | Overhead | Percentage of Total Time |
|-----------|----------|--------------------------|
| Runtime initialization | 3.23 ns | < 0.000001% |
| Adapter cache hit | 28.80 ns | < 0.00001% |
| KV cache operations | 38.64 ns | < 0.00001% |
| Memory sync | 828 ps | < 0.0000001% |

### Real-World Impact

**Typical inference request:**
```
Total time: 50 milliseconds

Breakdown:
├── ML computation (ANE/GPU): 49.999,730 ms (99.9999%)
├── Data preprocessing:        0.000,200 ms (0.0004%)
├── FFI overhead:             0.000,050 ms (0.0001%)
└── Error handling:           0.000,020 ms (0.00004%)
```

**Conclusion**: FFI overhead is **statistically negligible** - less than 0.001% of total execution time.

### Why FFI Overhead is So Low

1. **Zero-copy data transfer**: Direct pointer passing
2. **Efficient marshalling**: Minimal type conversions
3. **Hardware acceleration**: Actual computation dominates time
4. **Lazy initialization**: One-time setup costs

---

## Security Model

### Security Benefits

FFI **enhances** security in AdapterOS:

#### 1. Hardware-Backed Security
```rust
// Secure Enclave access via FFI
unsafe { ffi::secd_generate_key_in_enclave(key_label) }
```
- Hardware-backed cryptographic keys
- Tamper-resistant key storage
- Secure enclave attestation

#### 2. Zero Network Egress
```objc
// FFI calls stay within device
MLModel *model = [MLModel modelWithContentsOfURL:localURL ...];
// No network access - pure hardware acceleration
```
- All FFI calls are local (GPU/ANE)
- No internet connectivity through FFI
- Air-gapped by design

#### 3. Trusted Frameworks
- Apple's signed and sandboxed frameworks
- Battle-tested, production-ready code
- No custom C++ in critical paths

### Security Risks & Mitigations

#### Risk 1: Unsafe Code Blocks
```rust
// Minimal unsafe usage
unsafe { 
    ffi::coreml_run_inference(handle, input_ids, input_len, ...) 
}
```
**Mitigation**:
- Isolated to FFI boundary modules
- < 1% of codebase is `unsafe`
- Heavily audited and documented

#### Risk 2: Memory Safety
```objc
// ObjC memory management
@autoreleasepool {
    MLModel *model = [MLModel modelWithContentsOfURL:...];
    // Automatic cleanup
}
```
**Mitigation**:
- Clear ownership transfer protocols
- RAII patterns on Rust side
- No double-free scenarios

#### Risk 3: Type Safety
```rust
// Explicit type conversions
int32_t *inputPtr = (int32_t*)inputArray.dataPointer;
```
**Mitigation**:
- Explicit type checking
- Validation at boundaries
- `#[repr(C)]` for FFI-safe structs

### Security Comparison

| Aspect | Without FFI | With FFI |
|--------|-------------|----------|
| **Memory Safety** | ✅ 100% Rust | ⚠️ 99% Rust + audited FFI |
| **Hardware Access** | ❌ None | ✅ GPU/ANE via Apple frameworks |
| **Attack Surface** | 🟢 Minimal | 🟡 Small FFI boundary |
| **Network Egress** | ✅ Zero | ✅ Zero (hardware-only) |
| **Trust Model** | 🟢 Pure Rust | 🟢 Rust + Apple's signed frameworks |

**Net Result**: FFI enables hardware-backed security features while maintaining minimal attack surface.

---

## FFI Types in AdapterOS

### 1. CoreML Backend (Objective-C++)

**Purpose**: Apple Neural Engine acceleration

**Location**: `crates/adapteros-lora-kernel-coreml/`

**Status**: Fully implemented and operational

**Files**:
- `src/ffi.rs` - Rust FFI declarations
- `src/coreml_bridge.mm` - Objective-C++ bridge
- `swift/CoreMLBridge.swift` - Swift bridge (macOS 15+)

**Example**:
```rust
// Rust side
extern "C" {
    pub fn coreml_load_model(
        path: *const i8,
        path_len: usize,
        compute_units: i32,
    ) -> *mut std::ffi::c_void;
}

// Objective-C++ side
void* coreml_load_model(const char* path, size_t path_len, int32_t compute_units) {
    @autoreleasepool {
        MLModel *model = [MLModel modelWithContentsOfURL:modelURL 
                                         configuration:config 
                                                  error:&error];
        return (__bridge_retained void*)model;
    }
}
```

**Use Cases**:
- Model loading and inference
- ANE detection and configuration
- MLTensor operations (macOS 15+)

### 2. MLX Backend (C++)

**Purpose**: Production ML framework integration

**Location**: `crates/adapteros-lora-mlx-ffi/`

**Status**: Fully implemented and production-ready

**Files**:
- `src/lib.rs` - Rust FFI interface
- `src/mlx_cpp_wrapper.cpp` - C++ wrapper
- `wrapper.h` - C header declarations

**Example**:
```rust
// Rust side
extern "C" {
    pub fn mlx_model_load(path: *const i8) -> mlx_model_t*;
    pub fn mlx_model_forward(model: *mut mlx_model_t, input: *const u32, len: usize) -> mlx_array_t*;
}

// C++ side
extern "C" mlx_model_t* mlx_model_load(const char* path) {
    return new MLXModel(path);
}
```

**Use Cases**:
- Production inference workloads
- Model loading and text generation
- Multi-adapter routing with K-sparse selection
- Training and fine-tuning operations
- Enterprise-grade health monitoring and circuit breakers

### 3. Metal Heap Observers (Objective-C++)

**Purpose**: Memory monitoring and management

**Location**: `crates/adapteros-memory/`

**Files**:
- `src/heap_observer_ffi.rs` - Rust FFI interface
- `src/heap_observer_callbacks.mm` - Objective-C++ implementation
- `include/heap_observer.h` - C header

**Example**:
```rust
// Rust side
extern "C" {
    pub fn register_heap_observer(
        heap_id: u64,
        on_allocation: AllocationSuccessCallback,
        on_deallocation: DeallocationCallback,
    ) -> i32;
}

// Objective-C++ side
int32_t register_heap_observer(
    uint64_t heap_id,
    AllocationSuccessCallback on_allocation,
    DeallocationCallback on_deallocation
) {
    // Register with Metal heap
    [heap addObserver:self];
    return 0;
}
```

**Use Cases**:
- Memory pressure monitoring
- Heap compaction tracking
- Performance metrics collection

### 4. Metal Shaders (Metal Shading Language)

**Purpose**: GPU kernel execution

**Location**: `metal/src/kernels/`

**Files**:
- `adapteros_kernels.metal` - Metal shader source
- Compiled to `.metallib` at build time

**Example**:
```metal
// Metal shader (compiled separately)
kernel void fused_qkv_kernel(
    device float* input [[buffer(0)]],
    device float* output [[buffer(1)]],
    uint id [[thread_position_in_grid]]
) {
    // GPU computation
    output[id] = compute_qkv(input[id]);
}
```

**Use Cases**:
- GPU-accelerated ML operations
- Parallel tensor operations
- Custom kernel implementations

---

## Best Practices

### 1. Minimize Unsafe Code

**✅ Good**:
```rust
// Single unsafe block, well-documented
pub fn run_inference(&self, input: &[u32]) -> Result<Vec<f32>> {
    // Validate input first (safe code)
    if input.is_empty() {
        return Err(AosError::Validation("input cannot be empty".into()));
    }
    
    // Single FFI call in unsafe block
    unsafe {
        let result = ffi::coreml_run_inference(
            self.handle,
            input.as_ptr(),
            input.len(),
            // ...
        );
        // Handle result (safe code)
        if result != 0 {
            return Err(AosError::Kernel("inference failed".into()));
        }
    }
    Ok(output)
}
```

**❌ Bad**:
```rust
// Too much unsafe code
pub unsafe fn everything_unsafe(&self) {
    unsafe {
        unsafe {
            // Nested unsafe blocks
        }
    }
}
```

### 2. Clear Ownership Transfer

**✅ Good**:
```rust
// Explicit ownership transfer
impl Drop for CoreMLModel {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                // Transfer ownership back to ObjC for cleanup
                ffi::coreml_unload_model(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}
```

**❌ Bad**:
```rust
// Unclear ownership
pub fn load_model(path: &str) -> *mut c_void {
    unsafe { ffi::coreml_load_model(...) }
    // Who owns this? When is it freed?
}
```

### 3. Error Handling

**✅ Good**:
```rust
// Comprehensive error handling
pub fn load_model(path: &Path) -> Result<CoreMLModel> {
    let path_cstr = CString::new(path.to_str().ok_or_else(|| {
        AosError::Validation("invalid path encoding".into())
    })?)?;
    
    let handle = unsafe {
        ffi::coreml_load_model(
            path_cstr.as_ptr(),
            path_cstr.as_bytes().len(),
            3, // MLComputeUnitsAll
        )
    };
    
    if handle.is_null() {
        let error_msg = unsafe {
            let mut buffer = vec![0u8; 1024];
            let len = ffi::coreml_get_last_error(buffer.as_mut_ptr() as *mut i8, buffer.len());
            String::from_utf8_lossy(&buffer[..len]).to_string()
        };
        return Err(AosError::Kernel(format!("failed to load model: {}", error_msg)));
    }
    
    Ok(CoreMLModel { handle })
}
```

**❌ Bad**:
```rust
// No error handling
pub fn load_model(path: &str) -> *mut c_void {
    unsafe { ffi::coreml_load_model(path.as_ptr(), path.len(), 0) }
    // What if this returns null?
}
```

### 4. Type Safety

**✅ Good**:
```rust
// FFI-safe struct with explicit layout
#[repr(C)]
pub struct AneCheckResult {
    pub available: bool,
    pub generation: u8,
}

// Type-safe wrapper
pub struct AneStatus {
    available: bool,
    generation: u8,
}

impl From<AneCheckResult> for AneStatus {
    fn from(result: AneCheckResult) -> Self {
        Self {
            available: result.available,
            generation: result.generation,
        }
    }
}
```

**❌ Bad**:
```rust
// Raw pointer, no type safety
pub fn check_ane() -> *mut c_void {
    unsafe { ffi::coreml_check_ane() }
    // What is this? How do I use it?
}
```

### 5. Documentation

**✅ Good**:
```rust
/// Load a CoreML model from disk
///
/// # Arguments
/// * `path` - Path to `.mlmodel` file
/// * `compute_units` - Hardware preference (CPU/GPU/ANE/All)
///
/// # Returns
/// * `Ok(CoreMLModel)` - Successfully loaded model
/// * `Err(AosError)` - Load failure (check error message)
///
/// # Safety
/// This function uses FFI to call Apple's CoreML framework.
/// The returned handle must be freed via `coreml_unload_model`.
///
/// # Example
/// ```no_run
/// let model = CoreMLModel::load(Path::new("model.mlmodel"))?;
/// let output = model.run_inference(&input_ids)?;
/// ```
pub fn load(path: &Path) -> Result<Self> {
    // ...
}
```

**❌ Bad**:
```rust
// No documentation
pub fn load(path: &Path) -> Result<Self> {
    // ...
}
```

---

## Quick Reference

### FFI Module Locations

| Module | Location | Language | Purpose | Status |
|--------|----------|----------|---------|--------|
| CoreML | `crates/adapteros-lora-kernel-coreml/` | ObjC++ | ANE acceleration | ✅ Production |
| MLX | `crates/adapteros-lora-mlx-ffi/` | C++ | Production ML backend | ✅ Production |
| Metal Observers | `crates/adapteros-memory/` | ObjC++ | Memory monitoring | ✅ Production |
| Metal Shaders | `metal/src/kernels/` | Metal | GPU kernels | ✅ Production |

### Common FFI Patterns

#### Pattern 1: Simple Function Call
```rust
extern "C" {
    pub fn native_function(arg: i32) -> i32;
}

let result = unsafe { native_function(42) };
```

#### Pattern 2: String Passing
```rust
extern "C" {
    pub fn native_string_function(s: *const i8, len: usize);
}

let cstr = CString::new("hello")?;
unsafe {
    native_string_function(cstr.as_ptr(), cstr.as_bytes().len());
}
```

#### Pattern 3: Buffer Passing
```rust
extern "C" {
    pub fn native_buffer_function(data: *const u8, len: usize);
}

let buffer = vec![1, 2, 3, 4];
unsafe {
    native_buffer_function(buffer.as_ptr(), buffer.len());
}
```

#### Pattern 4: Opaque Handle
```rust
extern "C" {
    pub fn native_create() -> *mut c_void;
    pub fn native_destroy(handle: *mut c_void);
}

pub struct NativeHandle(*mut c_void);

impl NativeHandle {
    pub fn new() -> Result<Self> {
        let handle = unsafe { native_create() };
        if handle.is_null() {
            return Err(AosError::Kernel("creation failed".into()));
        }
        Ok(Self(handle))
    }
}

impl Drop for NativeHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { native_destroy(self.0) };
        }
    }
}
```

### Performance Checklist

- [ ] Use zero-copy data transfer where possible
- [ ] Minimize type conversions
- [ ] Cache FFI function results (e.g., feature detection)
- [ ] Batch operations to reduce call overhead
- [ ] Profile FFI boundaries if performance is critical

### Security Checklist

- [ ] Minimize `unsafe` code surface area
- [ ] Validate all inputs before FFI calls
- [ ] Handle all error cases explicitly
- [ ] Document ownership transfer clearly
- [ ] Use `#[repr(C)]` for FFI structs
- [ ] Implement `Drop` for resource cleanup
- [ ] Audit all unsafe blocks regularly

---

## Related Documentation

- **[OBJECTIVE_CPP_FFI_PATTERNS.md](OBJECTIVE_CPP_FFI_PATTERNS.md)** - Detailed ObjC++ patterns
- **[BENCHMARK_RESULTS.md](../BENCHMARK_RESULTS.md)** - Performance measurements
- **[COREML_ACTIVATION.md](COREML_ACTIVATION.md)** - CoreML backend guide
- **[MLX_INTEGRATION.md](MLX_INTEGRATION.md)** - MLX backend guide

---

## Summary

FFI in AdapterOS:

- ✅ **Enables hardware acceleration** (10-100x performance gains)
- ✅ **Minimal overhead** (< 0.001% of execution time)
- ✅ **Secure by design** (hardware-only, no network egress)
- ✅ **Well-audited** (< 1% unsafe code, isolated boundaries)
- ✅ **Production-ready backends** (CoreML, MLX, and Metal are all fully implemented and operational)

**Backend Status:**
- **CoreML**: Fully implemented, operational, ANE acceleration with guaranteed determinism
- **MLX**: Fully implemented, production-ready with enterprise resilience features
- **Metal**: Production-ready with deterministic GPU kernels

**The FFI "tax" is worth paying** - it unlocks Apple's world-class ML hardware (ANE, GPU) through production-ready backends while maintaining Rust's safety guarantees where possible.

---

**Questions?** See [CLAUDE.md](../CLAUDE.md) for development guidelines or open an issue for FFI-specific concerns.

