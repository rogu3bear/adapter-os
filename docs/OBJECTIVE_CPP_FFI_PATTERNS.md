# Objective-C++ FFI Patterns for AdapterOS

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2025-01-19
**Purpose:** Memory-safe Rust ↔ Objective-C++ FFI patterns for Metal and CoreML backends

---

## Table of Contents

1. [Overview](#overview)
2. [Memory Safety Principles](#memory-safety-principles)
3. [Buffer Transfer Patterns](#buffer-transfer-patterns)
4. [Object Lifetime Management](#object-lifetime-management)
5. [Error Handling](#error-handling)
6. [Metal-Specific Patterns](#metal-specific-patterns)
7. [CoreML-Specific Patterns](#coreml-specific-patterns)
8. [Anti-Patterns](#anti-patterns)
9. [Testing Strategies](#testing-strategies)

---

## Overview

Objective-C++ serves as the FFI bridge between Rust and Apple frameworks (Metal, CoreML). This document establishes canonical patterns for safe, deterministic interoperation.

### Why Objective-C++ over Swift?

| Criterion | Objective-C++ | Swift |
|-----------|---------------|-------|
| **C ABI Compatibility** | ✅ Direct `extern "C"` | ❌ Requires bridging header |
| **Memory Management** | ✅ Manual (deterministic) | ❌ ARC (non-deterministic) |
| **Rust FFI** | ✅ Zero-cost | ❌ Complex, overhead |
| **ABI Stability** | ✅ Stable since macOS 10.0 | ❌ Changed Swift 4→5 |
| **Pointer Control** | ✅ Full control | ❌ Opaque references |

### FFI Architecture

```
┌─────────────────────────────────────────────┐
│ Rust (adapteros-lora-kernel-mtl)           │
│  - MetalKernels struct                      │
│  - Safe Rust API                            │
└─────────────────┬───────────────────────────┘
                  │ extern "C" FFI
                  ↓
┌─────────────────────────────────────────────┐
│ Objective-C++ (metal_kernels.mm)            │
│  - C ABI functions                          │
│  - Metal API calls                          │
│  - Buffer management                        │
└─────────────────┬───────────────────────────┘
                  │ Objective-C API
                  ↓
┌─────────────────────────────────────────────┐
│ Metal Framework (Metal.framework)           │
│  - MTLDevice, MTLLibrary, MTLBuffer         │
│  - GPU execution                            │
└─────────────────────────────────────────────┘
```

---

## Memory Safety Principles

### Golden Rules

1. **Single Ownership:** Only one side (Rust or ObjC++) owns memory at a time
2. **No Double-Free:** Use `freeWhenDone:NO` when wrapping Rust-owned data
3. **Explicit Transfers:** Use `__bridge_retained`/`__bridge_transfer` for ownership transfers
4. **No Leaks:** Pair every allocation with deallocation (RAII on Rust side)
5. **Deterministic Lifetime:** Avoid ARC (Automatic Reference Counting) where possible

### Ownership Transfer Matrix

| Pattern | Rust → ObjC++ | ObjC++ → Rust | Ownership |
|---------|---------------|---------------|-----------|
| **Borrow (Read)** | `*const u8, len` | N/A | Rust keeps ownership |
| **Transfer (Write)** | `*mut u8, len` | `__bridge_retained void*` | ObjC++ keeps ownership |
| **Release** | N/A | `CFRelease(ptr)` | Ownership returned to Rust |

---

## Buffer Transfer Patterns

### Pattern 1: Borrow Rust Buffer (Read-Only)

**Use Case:** Pass serialized plan data to Metal for shader execution

**Rust Side:**
```rust
// crates/adapteros-lora-kernel-mtl/src/lib.rs
use std::ffi::c_void;

extern "C" {
    fn metal_kernel_load(plan: *const u8, plan_len: usize) -> i32;
}

pub struct MetalKernels {
    context: *mut c_void,
}

impl MetalKernels {
    pub fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        unsafe {
            let ret = metal_kernel_load(plan_bytes.as_ptr(), plan_bytes.len());
            if ret != 0 {
                return Err(AosError::Kernel("Metal kernel load failed".into()));
            }
        }
        Ok(())
    }
}
```

**Objective-C++ Side:**
```objective-c++
// crates/adapteros-lora-kernel-mtl/src/metal_kernels.mm
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>

extern "C" int metal_kernel_load(const uint8_t* plan, size_t len) {
    @autoreleasepool {
        // Wrap Rust buffer WITHOUT copying (freeWhenDone:NO)
        NSData* data = [NSData dataWithBytesNoCopy:(void*)plan
                                            length:len
                                      freeWhenDone:NO];

        // Use data (Rust still owns underlying buffer)
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        id<MTLBuffer> buffer = [device newBufferWithBytes:data.bytes
                                                   length:data.length
                                                  options:MTLResourceStorageModeShared];

        // Rust buffer lifetime must outlive this function
        return 0; // Success
    }
}
```

**Key Points:**
- ✅ `freeWhenDone:NO` prevents double-free
- ✅ Rust owns buffer, ObjC++ borrows temporarily
- ✅ No memory leak (Rust deallocates after call returns)

**Lifetime Diagram:**
```
Rust                          ObjC++
┌──────────────────┐
│ plan_bytes: Vec  │─────────→ NSData (borrow, no copy)
│   [lifetime]     │          ┌─────────────────────┐
│   allocated      │──────┐   │ freeWhenDone:NO     │
│   ...            │      │   │ points to Rust buf  │
│   dropped        │←─────┘   └─────────────────────┘
└──────────────────┘          (NSData auto-released, no free)
```

---

### Pattern 2: Transfer Buffer to Rust (Write)

**Use Case:** Retrieve GPU output logits from Metal to Rust

**Rust Side:**
```rust
extern "C" {
    fn metal_kernel_get_logits(
        context: *mut c_void,
        out_buffer: *mut f32,
        capacity: usize
    ) -> usize;
}

impl MetalKernels {
    pub fn get_logits(&self, vocab_size: usize) -> Result<Vec<f32>> {
        let mut logits = vec![0.0f32; vocab_size];

        unsafe {
            let actual_len = metal_kernel_get_logits(
                self.context,
                logits.as_mut_ptr(),
                vocab_size
            );

            if actual_len != vocab_size {
                return Err(AosError::Kernel("Logits size mismatch".into()));
            }
        }

        Ok(logits)
    }
}
```

**Objective-C++ Side:**
```objective-c++
extern "C" size_t metal_kernel_get_logits(
    void* context_ptr,
    float* out_buffer,
    size_t capacity
) {
    @autoreleasepool {
        MetalContext* ctx = (__bridge MetalContext*)context_ptr;
        id<MTLBuffer> gpu_buffer = ctx.logitsBuffer;

        // Copy from GPU to CPU (Rust-owned buffer)
        size_t byte_size = capacity * sizeof(float);
        if (gpu_buffer.length < byte_size) {
            return 0; // Error: buffer too small
        }

        memcpy(out_buffer, gpu_buffer.contents, byte_size);
        return capacity; // Success
    }
}
```

**Key Points:**
- ✅ Rust allocates buffer, ObjC++ writes to it
- ✅ No ownership transfer (Rust owns buffer before and after)
- ✅ Size validation (return actual length written)

---

## Object Lifetime Management

### Pattern 3: Create Opaque Object (ObjC++ → Rust)

**Use Case:** Create Metal device/library context, return opaque pointer to Rust

**Rust Side:**
```rust
use std::ffi::{c_char, c_void};

extern "C" {
    fn metal_create_context(metallib_path: *const c_char) -> *mut c_void;
    fn metal_release_context(context: *mut c_void);
}

pub struct MetalKernels {
    context: *mut c_void,
}

impl MetalKernels {
    pub fn new() -> Result<Self> {
        let metallib_path = std::ffi::CString::new("/path/to/kernels.metallib")?;

        unsafe {
            let context = metal_create_context(metallib_path.as_ptr());
            if context.is_null() {
                return Err(AosError::Kernel("Failed to create Metal context".into()));
            }

            Ok(Self { context })
        }
    }
}

impl Drop for MetalKernels {
    fn drop(&mut self) {
        unsafe {
            if !self.context.is_null() {
                metal_release_context(self.context);
                self.context = std::ptr::null_mut();
            }
        }
    }
}
```

**Objective-C++ Side:**
```objective-c++
// Metal context (non-ARC managed for determinism)
@interface MetalContext : NSObject
@property (nonatomic, retain) id<MTLDevice> device;
@property (nonatomic, retain) id<MTLLibrary> library;
@property (nonatomic, retain) id<MTLCommandQueue> queue;
@end

@implementation MetalContext
@end

extern "C" void* metal_create_context(const char* metallib_path) {
    @autoreleasepool {
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        if (!device) return nullptr;

        NSString* path = @(metallib_path);
        NSError* error = nil;
        id<MTLLibrary> library = [device newLibraryWithFile:path error:&error];
        if (error) return nullptr;

        MetalContext* ctx = [[MetalContext alloc] init];
        ctx.device = device;
        ctx.library = library;
        ctx.queue = [device newCommandQueue];

        // Transfer ownership to Rust (Rust must call metal_release_context)
        return (__bridge_retained void*)ctx;
    }
}

extern "C" void metal_release_context(void* context_ptr) {
    if (context_ptr) {
        // Transfer ownership back from Rust, release
        CFRelease(context_ptr);
    }
}
```

**Key Points:**
- ✅ `__bridge_retained` transfers ownership to Rust
- ✅ Rust calls `metal_release_context` in `Drop`
- ✅ Paired allocation/deallocation (RAII pattern)

**Ownership Timeline:**
```
1. ObjC++ alloc    → MetalContext* (ObjC++ owns)
2. __bridge_retained → void* (Rust owns, retain count++)
3. Rust uses       → *mut c_void (opaque pointer)
4. Rust Drop       → metal_release_context (retain count--)
5. CFRelease       → MetalContext deallocated
```

---

### Pattern 4: Borrow Opaque Object (Rust → ObjC++)

**Use Case:** Execute Metal kernel with existing context

**Rust Side:**
```rust
extern "C" {
    fn metal_execute_kernel(
        context: *mut c_void,
        ring: *const RouterRing,
        io: *mut IoBuffers
    ) -> i32;
}

impl MetalKernels {
    pub fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        unsafe {
            let ret = metal_execute_kernel(self.context, ring, io);
            if ret != 0 {
                return Err(AosError::Kernel("Kernel execution failed".into()));
            }
        }
        Ok(())
    }
}
```

**Objective-C++ Side:**
```objective-c++
extern "C" int metal_execute_kernel(
    void* context_ptr,
    const RouterRing* ring,
    IoBuffers* io
) {
    @autoreleasepool {
        // Borrow context (no ownership transfer)
        MetalContext* ctx = (__bridge MetalContext*)context_ptr;

        // Use context (Rust still owns it)
        id<MTLCommandBuffer> cmdBuffer = [ctx.queue commandBuffer];
        id<MTLComputeCommandEncoder> encoder = [cmdBuffer computeCommandEncoder];

        // Execute kernel...
        [encoder endEncoding];
        [cmdBuffer commit];
        [cmdBuffer waitUntilCompleted];

        return 0; // Success
    }
}
```

**Key Points:**
- ✅ `__bridge` borrows without ownership transfer
- ✅ Rust retains ownership throughout
- ✅ No retain/release (no reference count change)

---

## Error Handling

### Pattern 5: Error Code Return

**Objective-C++ Side:**
```objective-c++
// Error codes (C-compatible)
enum MetalErrorCode {
    METAL_SUCCESS = 0,
    METAL_ERROR_DEVICE_NOT_FOUND = 1,
    METAL_ERROR_LIBRARY_LOAD_FAILED = 2,
    METAL_ERROR_KERNEL_COMPILE_FAILED = 3,
    METAL_ERROR_EXECUTION_FAILED = 4,
};

extern "C" int metal_execute_with_error(
    void* context_ptr,
    const uint8_t* plan,
    size_t len,
    char* error_buffer,
    size_t error_buffer_size
) {
    @autoreleasepool {
        MetalContext* ctx = (__bridge MetalContext*)context_ptr;

        NSError* error = nil;
        // Execute operation...

        if (error) {
            // Write error message to Rust-provided buffer
            NSString* msg = error.localizedDescription;
            const char* cstr = [msg UTF8String];
            size_t msg_len = strlen(cstr);
            size_t copy_len = MIN(msg_len, error_buffer_size - 1);

            memcpy(error_buffer, cstr, copy_len);
            error_buffer[copy_len] = '\0';

            return METAL_ERROR_EXECUTION_FAILED;
        }

        return METAL_SUCCESS;
    }
}
```

**Rust Side:**
```rust
const ERROR_BUFFER_SIZE: usize = 1024;

extern "C" {
    fn metal_execute_with_error(
        context: *mut c_void,
        plan: *const u8,
        len: usize,
        error_buffer: *mut c_char,
        error_buffer_size: usize,
    ) -> i32;
}

impl MetalKernels {
    pub fn execute_with_error(&self, plan: &[u8]) -> Result<()> {
        let mut error_buffer = vec![0u8; ERROR_BUFFER_SIZE];

        unsafe {
            let ret = metal_execute_with_error(
                self.context,
                plan.as_ptr(),
                plan.len(),
                error_buffer.as_mut_ptr() as *mut c_char,
                ERROR_BUFFER_SIZE,
            );

            if ret != 0 {
                let error_msg = std::ffi::CStr::from_ptr(error_buffer.as_ptr() as *const c_char)
                    .to_string_lossy()
                    .into_owned();

                return Err(AosError::Kernel(format!(
                    "Metal execution failed (code {}): {}",
                    ret, error_msg
                )));
            }
        }

        Ok(())
    }
}
```

**Key Points:**
- ✅ Error code as return value (C-compatible)
- ✅ Error message via out-parameter (Rust-owned buffer)
- ✅ Size-bounded string copy (prevents buffer overflow)

---

## Metal-Specific Patterns

### Pattern 6: Load Precompiled Metallib

**Objective-C++ Side:**
```objective-c++
extern "C" void* metal_load_metallib(const char* path, uint8_t* hash_out) {
    @autoreleasepool {
        NSString* libPath = @(path);

        // Compute BLAKE3 hash of metallib file
        NSData* fileData = [NSData dataWithContentsOfFile:libPath];
        if (!fileData) return nullptr;

        // Call Rust BLAKE3 hasher (assumes linked)
        extern void blake3_hash(const uint8_t* data, size_t len, uint8_t* out);
        blake3_hash((const uint8_t*)fileData.bytes, fileData.length, hash_out);

        // Load metallib
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        NSError* error = nil;
        id<MTLLibrary> library = [device newLibraryWithFile:libPath error:&error];
        if (error) return nullptr;

        MetalContext* ctx = [[MetalContext alloc] init];
        ctx.device = device;
        ctx.library = library;

        return (__bridge_retained void*)ctx;
    }
}
```

**Rust Side:**
```rust
#[repr(C)]
pub struct MetallibHash([u8; 32]);

extern "C" {
    fn metal_load_metallib(path: *const c_char, hash_out: *mut u8) -> *mut c_void;
}

impl MetalKernels {
    pub fn from_metallib(path: &Path) -> Result<(Self, B3Hash)> {
        let path_cstr = std::ffi::CString::new(path.to_str().unwrap())?;
        let mut hash_bytes = [0u8; 32];

        unsafe {
            let context = metal_load_metallib(path_cstr.as_ptr(), hash_bytes.as_mut_ptr());
            if context.is_null() {
                return Err(AosError::Kernel("Failed to load metallib".into()));
            }

            let hash = B3Hash::from_bytes(hash_bytes);
            Ok((Self { context }, hash))
        }
    }
}
```

**Key Points:**
- ✅ Content-addressed metallib (BLAKE3 hash)
- ✅ Hash computed during load (single file read)
- ✅ Determinism validation (compare hash against manifest)

---

### Pattern 7: Unified Memory Buffer (Zero-Copy)

**Objective-C++ Side:**
```objective-c++
extern "C" void* metal_create_shared_buffer(void* context_ptr, size_t size) {
    @autoreleasepool {
        MetalContext* ctx = (__bridge MetalContext*)context_ptr;

        // Create shared buffer (unified memory, zero-copy CPU↔GPU)
        id<MTLBuffer> buffer = [ctx.device newBufferWithLength:size
                                                        options:MTLResourceStorageModeShared];

        return (__bridge_retained void*)buffer;
    }
}

extern "C" void* metal_buffer_contents(void* buffer_ptr) {
    id<MTLBuffer> buffer = (__bridge id<MTLBuffer>)buffer_ptr;
    return buffer.contents; // Direct CPU access to GPU memory
}

extern "C" void metal_release_buffer(void* buffer_ptr) {
    if (buffer_ptr) {
        CFRelease(buffer_ptr);
    }
}
```

**Rust Side:**
```rust
pub struct MetalBuffer {
    buffer_ptr: *mut c_void,
    size: usize,
}

impl MetalBuffer {
    pub fn new(context: &MetalKernels, size: usize) -> Result<Self> {
        unsafe {
            let buffer_ptr = metal_create_shared_buffer(context.context, size);
            if buffer_ptr.is_null() {
                return Err(AosError::Kernel("Failed to create buffer".into()));
            }
            Ok(Self { buffer_ptr, size })
        }
    }

    pub fn as_slice_mut(&mut self) -> &mut [f32] {
        unsafe {
            let contents = metal_buffer_contents(self.buffer_ptr) as *mut f32;
            std::slice::from_raw_parts_mut(contents, self.size / 4)
        }
    }
}

impl Drop for MetalBuffer {
    fn drop(&mut self) {
        unsafe {
            if !self.buffer_ptr.is_null() {
                metal_release_buffer(self.buffer_ptr);
            }
        }
    }
}
```

**Key Points:**
- ✅ Unified memory (zero-copy on Apple Silicon)
- ✅ Direct CPU write, GPU read (no `memcpy`)
- ✅ RAII lifetime management

---

## CoreML-Specific Patterns

### Pattern 8: Load CoreML Model (.mlpackage)

**Objective-C++ Side:**
```objective-c++
#import <CoreML/CoreML.h>

extern "C" void* coreml_load_model(const char* model_path, char* error_buffer, size_t error_size) {
    @autoreleasepool {
        NSURL* url = [NSURL fileURLWithPath:@(model_path)];

        MLModelConfiguration* config = [[MLModelConfiguration alloc] init];
        config.computeUnits = MLComputeUnitsAll; // CPU/GPU/ANE auto-selection

        NSError* error = nil;
        MLModel* model = [MLModel modelWithContentsOfURL:url configuration:config error:&error];

        if (error) {
            const char* msg = [error.localizedDescription UTF8String];
            strncpy(error_buffer, msg, error_size - 1);
            error_buffer[error_size - 1] = '\0';
            return nullptr;
        }

        return (__bridge_retained void*)model;
    }
}

extern "C" void coreml_release_model(void* model_ptr) {
    if (model_ptr) {
        CFRelease(model_ptr);
    }
}
```

**Rust Side:**
```rust
pub struct CoreMLModel {
    model_ptr: *mut c_void,
}

impl CoreMLModel {
    pub fn load(model_path: &Path) -> Result<Self> {
        let path_cstr = std::ffi::CString::new(model_path.to_str().unwrap())?;
        let mut error_buffer = vec![0u8; 1024];

        unsafe {
            let model_ptr = coreml_load_model(
                path_cstr.as_ptr(),
                error_buffer.as_mut_ptr() as *mut c_char,
                1024,
            );

            if model_ptr.is_null() {
                let error_msg = std::ffi::CStr::from_ptr(error_buffer.as_ptr() as *const c_char)
                    .to_string_lossy()
                    .into_owned();
                return Err(AosError::Kernel(format!("CoreML load failed: {}", error_msg)));
            }

            Ok(Self { model_ptr })
        }
    }
}

impl Drop for CoreMLModel {
    fn drop(&mut self) {
        unsafe {
            if !self.model_ptr.is_null() {
                coreml_release_model(self.model_ptr);
            }
        }
    }
}
```

---

### Pattern 9: CoreML Prediction with ANE Detection

**Objective-C++ Side:**
```objective-c++
typedef struct {
    int32_t success;
    int32_t used_ane; // 1 if ANE used, 0 if GPU fallback
} CoreMLPredictionResult;

extern "C" CoreMLPredictionResult coreml_predict(
    void* model_ptr,
    const uint32_t* input_ids,
    size_t input_len,
    float* output_logits,
    size_t output_size
) {
    @autoreleasepool {
        MLModel* model = (__bridge MLModel*)model_ptr;

        // Create input feature (MLMultiArray)
        NSError* error = nil;
        MLMultiArray* inputArray = [[MLMultiArray alloc]
            initWithShape:@[@(input_len)]
            dataType:MLMultiArrayDataTypeInt32
            error:&error];

        for (size_t i = 0; i < input_len; i++) {
            inputArray[i] = @(input_ids[i]);
        }

        // Create input provider
        MLDictionaryFeatureProvider* inputProvider = [[MLDictionaryFeatureProvider alloc]
            initWithDictionary:@{@"input_ids": inputArray}
            error:&error];

        // Make prediction
        MLPredictionOptions* options = [[MLPredictionOptions alloc] init];
        id<MLFeatureProvider> output = [model predictionFromFeatures:inputProvider
                                                              options:options
                                                                error:&error];

        if (error) {
            return (CoreMLPredictionResult){.success = 0, .used_ane = 0};
        }

        // Extract logits
        MLMultiArray* logitsArray = [output featureValueForName:@"logits"].multiArrayValue;
        for (size_t i = 0; i < output_size; i++) {
            output_logits[i] = [logitsArray[i] floatValue];
        }

        // Detect ANE usage (heuristic: ANE is faster, check prediction time)
        // In practice, use MLModelConfiguration.preferredMetalDevice == nil as proxy
        int32_t used_ane = 1; // Placeholder (requires profiling or private API)

        return (CoreMLPredictionResult){.success = 1, .used_ane = used_ane};
    }
}
```

**Rust Side:**
```rust
#[repr(C)]
struct CoreMLPredictionResult {
    success: i32,
    used_ane: i32,
}

extern "C" {
    fn coreml_predict(
        model_ptr: *mut c_void,
        input_ids: *const u32,
        input_len: usize,
        output_logits: *mut f32,
        output_size: usize,
    ) -> CoreMLPredictionResult;
}

impl CoreMLModel {
    pub fn predict(&self, input_ids: &[u32], vocab_size: usize) -> Result<(Vec<f32>, bool)> {
        let mut logits = vec![0.0f32; vocab_size];

        unsafe {
            let result = coreml_predict(
                self.model_ptr,
                input_ids.as_ptr(),
                input_ids.len(),
                logits.as_mut_ptr(),
                vocab_size,
            );

            if result.success == 0 {
                return Err(AosError::Kernel("CoreML prediction failed".into()));
            }

            let used_ane = result.used_ane != 0;
            Ok((logits, used_ane))
        }
    }
}
```

---

## Anti-Patterns

### ❌ Anti-Pattern 1: Double-Free

**BAD:**
```objective-c++
extern "C" int bad_load(const uint8_t* plan, size_t len) {
    NSData* data = [NSData dataWithBytesNoCopy:(void*)plan
                                        length:len
                                  freeWhenDone:YES]; // ❌ BAD!
    // When data is released, it will free() the Rust buffer
    // Rust will also free the buffer → double-free crash
}
```

**GOOD:**
```objective-c++
extern "C" int good_load(const uint8_t* plan, size_t len) {
    NSData* data = [NSData dataWithBytesNoCopy:(void*)plan
                                        length:len
                                  freeWhenDone:NO]; // ✅ GOOD
    // Rust retains ownership, no double-free
}
```

---

### ❌ Anti-Pattern 2: Memory Leak

**BAD:**
```rust
extern "C" {
    fn metal_create_context() -> *mut c_void;
}

fn bad_usage() -> Result<()> {
    unsafe {
        let context = metal_create_context();
        // ❌ Never released → memory leak
        Ok(())
    }
}
```

**GOOD:**
```rust
pub struct MetalKernels {
    context: *mut c_void,
}

impl Drop for MetalKernels {
    fn drop(&mut self) {
        unsafe {
            if !self.context.is_null() {
                metal_release_context(self.context);
            }
        }
    }
}
```

---

### ❌ Anti-Pattern 3: Use-After-Free

**BAD:**
```objective-c++
extern "C" int bad_execute(const uint8_t* plan, size_t len) {
    NSData* data = [NSData dataWithBytesNoCopy:(void*)plan length:len freeWhenDone:NO];
    dispatch_async(dispatch_get_global_queue(0, 0), ^{
        // ❌ plan pointer may be freed by Rust before this runs
        process_data(data);
    });
    return 0;
}
```

**GOOD:**
```objective-c++
extern "C" int good_execute(const uint8_t* plan, size_t len) {
    // Copy data to ObjC++-owned buffer for async use
    NSData* data = [NSData dataWithBytes:plan length:len]; // ✅ Copy
    dispatch_async(dispatch_get_global_queue(0, 0), ^{
        process_data(data);
    });
    return 0;
}
```

---

## Testing Strategies

### Unit Test: Verify No Memory Leaks

**Objective-C++ Test:**
```objective-c++
#include <gtest/gtest.h>

TEST(MetalFFI, NoMemoryLeaks) {
    void* ctx = metal_create_context("/path/to/kernels.metallib");
    ASSERT_NE(ctx, nullptr);

    // Simulate usage...

    metal_release_context(ctx);

    // Run with: leaks --atExit -- ./test_binary
    // Expect: "0 leaks for 0 total leaked bytes"
}
```

**Rust Test:**
```rust
#[test]
fn test_metal_kernels_drop() {
    let kernels = MetalKernels::new().unwrap();
    // Drop should release context without leak
    drop(kernels);

    // Run with: cargo test --features leak-check
}
```

---

### Integration Test: Determinism Across FFI Boundary

```rust
#[test]
fn test_deterministic_execution() {
    let mut kernels1 = MetalKernels::new().unwrap();
    let mut kernels2 = MetalKernels::new().unwrap();

    let plan = create_test_plan();
    kernels1.load(&plan).unwrap();
    kernels2.load(&plan).unwrap();

    let ring = RouterRing::new(3);
    let mut io1 = IoBuffers::new(32000);
    let mut io2 = IoBuffers::new(32000);

    kernels1.run_step(&ring, &mut io1).unwrap();
    kernels2.run_step(&ring, &mut io2).unwrap();

    // Verify bit-identical output
    assert_eq!(io1.output_logits, io2.output_logits);
}
```

---

## References

- [docs/ADR_MULTI_BACKEND_STRATEGY.md](./ADR_MULTI_BACKEND_STRATEGY.md) - Backend selection rationale
- [crates/adapteros-lora-kernel-mtl/src/lib.rs](../crates/adapteros-lora-kernel-mtl/src/lib.rs) - Metal backend implementation
- [Apple Metal Best Practices](https://developer.apple.com/metal/Metal-Best-Practices-Guide.pdf)
- [Objective-C++ FFI in The Rust FFI Omnibus](https://jakegoulding.com/rust-ffi-omnibus/)

---

**Signed:** James KC Auchterlonie
**Date:** 2025-01-19
