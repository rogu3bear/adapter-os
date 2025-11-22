# CoreML Async FFI Implementation Guide

**Status:** Pre-Implementation Planning
**Objective:** Detailed technical guide for implementing async prediction FFI for AdapterOS

---

## Overview

This guide provides concrete implementation patterns for exposing CoreML async prediction to Rust through Objective-C++ FFI, integrating with Tokio async runtime.

---

## 1. FFI Layer Design

### 1.1 C Function Signatures (To Add to ffi.rs)

```rust
// In crates/adapteros-lora-kernel-coreml/src/ffi.rs

#[cfg(target_os = "macos")]
extern "C" {
    // ============================================
    // NEW: Async Prediction APIs
    // ============================================

    /// Launch async prediction on background thread
    /// Returns immediately, calls callback when complete
    ///
    /// # Safety
    /// - `handle` must be valid model from `coreml_load_model`
    /// - `input_ids` must point to valid u32 array of size `input_len`
    /// - `callback` will be called exactly once with results
    /// - `user_data` passed to callback unchanged (can be null)
    pub fn coreml_predict_async_native(
        handle: *mut std::ffi::c_void,
        input_ids: *const u32,
        input_len: usize,
        callback: extern "C" fn(
            output_logits: *const f32,
            output_len: usize,
            error_msg: *const i8,  // NULL on success
            user_data: *mut std::ffi::c_void,
        ),
        user_data: *mut std::ffi::c_void,
    );

    /// Async prediction with LoRA adapter support
    /// Same as coreml_predict_async_native but applies LoRA deltas
    pub fn coreml_predict_async_with_lora(
        handle: *mut std::ffi::c_void,
        input_ids: *const u32,
        input_len: usize,
        adapter_indices: *const u16,
        adapter_gates: *const i16,
        num_adapters: usize,
        lora_deltas: *const *const f32,
        delta_lens: *const usize,
        callback: extern "C" fn(
            output_logits: *const f32,
            output_len: usize,
            error_msg: *const i8,
            user_data: *mut std::ffi::c_void,
        ),
        user_data: *mut std::ffi::c_void,
    );

    /// Create MLState for stateful (KV-cached) prediction
    /// Only available on macOS 15.0+
    /// Returns null if unavailable
    pub fn coreml_create_state(handle: *mut std::ffi::c_void) -> *mut std::ffi::c_void;

    /// Async prediction with MLState (GPU-resident KV cache)
    /// macOS 15.0+ only
    pub fn coreml_predict_async_with_state(
        handle: *mut std::ffi::c_void,
        state_handle: *mut std::ffi::c_void,
        input_ids: *const u32,
        input_len: usize,
        callback: extern "C" fn(
            output_logits: *const f32,
            output_len: usize,
            error_msg: *const i8,
            user_data: *mut std::ffi::c_void,
        ),
        user_data: *mut std::ffi::c_void,
    );

    /// Free MLState handle
    pub fn coreml_free_state(state_handle: *mut std::ffi::c_void);

    /// Cancel in-flight async prediction (best-effort)
    pub fn coreml_cancel_async(request_id: u64);
}
```

---

## 2. Objective-C++ Implementation (coreml_bridge.mm)

### 2.1 Async Prediction Implementation

Add to `coreml_bridge.mm`:

```objc
// ============================================
// Async Prediction Implementation
// ============================================

// Callback typedef for clarity
typedef void (*AsyncPredictionCallback)(
    const float* output_logits,
    size_t output_len,
    const char* error_msg,
    void* user_data
);

// Async request tracking
static dispatch_queue_t g_async_queue = NULL;
static NSMutableDictionary *g_pending_requests = NULL;
static uint64_t g_next_request_id = 1;

// Initialize async infrastructure (call once at startup)
static void init_async_infrastructure() {
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        g_async_queue = dispatch_queue_create(
            "com.adapteros.coreml.async",
            dispatch_queue_attr_make_with_qos_class(
                DISPATCH_QUEUE_SERIAL,
                QOS_CLASS_USER_INITIATED,
                0
            )
        );
        g_pending_requests = [NSMutableDictionary dictionary];
    });
}

// Main async prediction implementation
void coreml_predict_async_native(
    void* handle,
    const uint32_t* input_ids,
    size_t input_len,
    AsyncPredictionCallback callback,
    void* user_data
) {
    init_async_infrastructure();

    if (!handle || !callback) {
        snprintf(g_last_error, sizeof(g_last_error), "Invalid arguments");
        callback(NULL, 0, g_last_error, user_data);
        return;
    }

    // Capture everything needed for the block
    MLModel *model = (__bridge MLModel*)handle;
    uint32_t *input_copy = (uint32_t*)malloc(input_len * sizeof(uint32_t));
    memcpy(input_copy, input_ids, input_len * sizeof(uint32_t));

    // Request ID for cancellation (future use)
    uint64_t request_id = __sync_fetch_and_add(&g_next_request_id, 1);

    // Dispatch to background queue
    dispatch_async(g_async_queue, ^{
        @autoreleasepool {  // CRITICAL: autorelease pool for background thread
            // Create input array
            NSArray<NSNumber*> *shape = @[@(input_len)];
            NSError *error = nil;
            MLMultiArray *inputArray = [[MLMultiArray alloc]
                initWithShape:shape
                dataType:MLMultiArrayDataTypeInt32
                error:&error];

            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "%s",
                    [[error localizedDescription] UTF8String]);
                callback(NULL, 0, g_last_error, user_data);
                free(input_copy);
                return;
            }

            // Copy input
            int32_t *inputPtr = (int32_t*)inputArray.dataPointer;
            for (size_t i = 0; i < input_len; i++) {
                inputPtr[i] = (int32_t)input_copy[i];
            }

            // Create feature provider
            MLDictionaryFeatureProvider *inputProvider =
                [[MLDictionaryFeatureProvider alloc]
                    initWithDictionary:@{@"input_ids": inputArray}
                    error:&error];

            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "%s",
                    [[error localizedDescription] UTF8String]);
                callback(NULL, 0, g_last_error, user_data);
                free(input_copy);
                return;
            }

            // Run prediction (still synchronous, but on background thread)
            id<MLFeatureProvider> outputProvider =
                [model predictionFromFeatures:inputProvider error:&error];

            if (error) {
                snprintf(g_last_error, sizeof(g_last_error), "%s",
                    [[error localizedDescription] UTF8String]);
                callback(NULL, 0, g_last_error, user_data);
                free(input_copy);
                return;
            }

            // Extract output
            MLFeatureValue *outputValue = [outputProvider featureValueForName:@"logits"];
            if (!outputValue || outputValue.type != MLFeatureTypeMultiArray) {
                snprintf(g_last_error, sizeof(g_last_error), "Output logits not found");
                callback(NULL, 0, g_last_error, user_data);
                free(input_copy);
                return;
            }

            // Copy output to temporary buffer (CRITICAL: callback pointer may not persist)
            MLMultiArray *outputArray = outputValue.multiArrayValue;
            float *outputPtr = (float*)outputArray.dataPointer;
            size_t output_len = (size_t)outputArray.count;

            // Create owned copy of output
            float *output_copy = (float*)malloc(output_len * sizeof(float));
            memcpy(output_copy, outputPtr, output_len * sizeof(float));

            // Invoke callback with owned data
            // Caller is responsible for freeing output_copy if needed
            callback(output_copy, output_len, NULL, user_data);

            // Don't free output_copy - caller owns it
            // (Rust will wrap in Box for cleanup)
            free(input_copy);
        }  // @autoreleasepool exits here
    });
}
```

### 2.2 Async with LoRA Implementation

```objc
void coreml_predict_async_with_lora(
    void* handle,
    const uint32_t* input_ids,
    size_t input_len,
    const uint16_t* adapter_indices,
    const int16_t* adapter_gates,
    size_t num_adapters,
    const float* const* lora_deltas,
    const size_t* delta_lens,
    AsyncPredictionCallback callback,
    void* user_data
) {
    init_async_infrastructure();

    if (!handle || !callback) {
        snprintf(g_last_error, sizeof(g_last_error), "Invalid arguments");
        callback(NULL, 0, g_last_error, user_data);
        return;
    }

    MLModel *model = (__bridge MLModel*)handle;

    // Copy all input data
    uint32_t *input_copy = (uint32_t*)malloc(input_len * sizeof(uint32_t));
    memcpy(input_copy, input_ids, input_len * sizeof(uint32_t));

    uint16_t *indices_copy = (uint16_t*)malloc(num_adapters * sizeof(uint16_t));
    memcpy(indices_copy, adapter_indices, num_adapters * sizeof(uint16_t));

    int16_t *gates_copy = (int16_t*)malloc(num_adapters * sizeof(int16_t));
    memcpy(gates_copy, adapter_gates, num_adapters * sizeof(int16_t));

    // Copy LoRA delta pointers and lengths
    float **deltas_copy = (float**)malloc(num_adapters * sizeof(float*));
    size_t *delta_lens_copy = (size_t*)malloc(num_adapters * sizeof(size_t));

    for (size_t i = 0; i < num_adapters; i++) {
        delta_lens_copy[i] = delta_lens[i];
        if (lora_deltas[i] && delta_lens[i] > 0) {
            deltas_copy[i] = (float*)malloc(delta_lens[i] * sizeof(float));
            memcpy(deltas_copy[i], lora_deltas[i], delta_lens[i] * sizeof(float));
        } else {
            deltas_copy[i] = NULL;
        }
    }

    dispatch_async(g_async_queue, ^{
        @autoreleasepool {
            // ... same prediction setup as coreml_predict_async_native ...

            // (See previous function for MLMultiArray setup)
            // Run prediction and extract base output

            // Apply LoRA deltas (same formula as sync version)
            if (num_adapters > 0 && gates_copy && deltas_copy && delta_lens_copy) {
                const float q15_scale = 1.0f / 32767.0f;

                for (size_t adapter_idx = 0; adapter_idx < num_adapters; adapter_idx++) {
                    float gate = (float)gates_copy[adapter_idx] * q15_scale;

                    if (gate == 0.0f) continue;

                    float *delta = deltas_copy[adapter_idx];
                    size_t delta_len = delta_lens_copy[adapter_idx];

                    if (!delta || delta_len == 0) continue;

                    size_t apply_len = output_len < delta_len ? output_len : delta_len;

                    for (size_t i = 0; i < apply_len; i++) {
                        output_copy[i] += gate * delta[i];
                    }
                }
            }

            callback(output_copy, output_len, NULL, user_data);

            // Free all temporary data
            free(input_copy);
            free(indices_copy);
            free(gates_copy);
            for (size_t i = 0; i < num_adapters; i++) {
                free(deltas_copy[i]);
            }
            free(deltas_copy);
            free(delta_lens_copy);
        }
    });
}
```

---

## 3. Rust Wrapper Layer

### 3.1 Async Channel Setup

In `adapteros-lora-kernel-coreml/src/lib.rs`:

```rust
use std::sync::Arc;
use tokio::sync::mpsc;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

// ============================================
// Async Prediction Channel
// ============================================

#[derive(Debug, Clone)]
struct AsyncResult {
    output: Result<Vec<f32>>,
}

// Global channel for async results
// Each task gets a oneshot channel paired with request ID
static ASYNC_RESULTS: Lazy<
    Mutex<HashMap<u64, tokio::sync::oneshot::Sender<AsyncResult>>>
> = Lazy::new(|| Mutex::new(HashMap::new()));

static mut NEXT_REQUEST_ID: u64 = 1;

fn next_request_id() -> u64 {
    unsafe {
        NEXT_REQUEST_ID += 1;
        NEXT_REQUEST_ID
    }
}

// ============================================
// C Callback (called from Objective-C++)
// ============================================

extern "C" fn async_prediction_callback(
    output_logits: *const f32,
    output_len: usize,
    error_msg: *const i8,
    user_data: *mut std::ffi::c_void,
) {
    let request_id = user_data as u64;

    let result = if error_msg.is_null() {
        let logits = unsafe {
            std::slice::from_raw_parts(output_logits, output_len).to_vec()
        };
        // Free the malloc'd pointer from Objective-C++
        unsafe {
            libc::free(output_logits as *mut _);
        }
        Ok(logits)
    } else {
        let error_str = unsafe {
            std::ffi::CStr::from_ptr(error_msg)
                .to_string_lossy()
                .to_string()
        };
        Err(AosError::Kernel(format!("CoreML error: {}", error_str)))
    };

    // Send result to waiting task
    if let Ok(mut senders) = ASYNC_RESULTS.lock() {
        if let Some(sender) = senders.remove(&request_id) {
            let _ = sender.send(AsyncResult { output: result });
        }
    }
}
```

### 3.2 CoreMLBackend Async Methods

```rust
impl CoreMLBackend {
    /// Run async prediction (non-blocking)
    pub async fn run_inference_async(
        &mut self,
        input_ids: &[u32],
    ) -> Result<Vec<f32>> {
        #[cfg(not(target_os = "macos"))]
        {
            return Err(AosError::Kernel("CoreML not available".to_string()));
        }

        #[cfg(target_os = "macos")]
        {
            if self.model_handle.is_null() {
                return Err(AosError::Kernel("Model not loaded".to_string()));
            }

            let request_id = next_request_id();

            // Create channel for result
            let (tx, rx) = tokio::sync::oneshot::channel();

            // Register sender
            {
                let mut senders = ASYNC_RESULTS.lock()
                    .map_err(|_| AosError::Kernel("Lock poisoned".into()))?;
                senders.insert(request_id, tx);
            }

            // Launch async prediction
            unsafe {
                ffi::coreml_predict_async_native(
                    self.model_handle,
                    input_ids.as_ptr(),
                    input_ids.len(),
                    async_prediction_callback,
                    request_id as *mut std::ffi::c_void,
                );
            }

            // Wait for result with timeout
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                rx,
            )
            .await
            .map_err(|_| AosError::Timeout("Prediction timeout".into()))?
            .map_err(|_| AosError::Kernel("Channel error".into()))?;

            self.metrics.total_operations += 1;
            self.metrics.successful_operations += 1;

            result.output
        }
    }

    /// Run async prediction with LoRA adapters
    pub async fn run_inference_async_with_lora(
        &mut self,
        input_ids: &[u32],
        adapter_indices: &[u16],
        adapter_gates: &[i16],
    ) -> Result<Vec<f32>> {
        #[cfg(not(target_os = "macos"))]
        {
            return Err(AosError::Kernel("CoreML not available".to_string()));
        }

        #[cfg(target_os = "macos")]
        {
            if self.model_handle.is_null() {
                return Err(AosError::Kernel("Model not loaded".to_string()));
            }

            // Prepare LoRA deltas from cache
            let mut lora_delta_ptrs: Vec<*const f32> = Vec::with_capacity(adapter_indices.len());
            let mut delta_lens: Vec<usize> = Vec::with_capacity(adapter_indices.len());

            for &idx in adapter_indices.iter() {
                if let Some(weights) = self.adapter_cache.get(&idx) {
                    lora_delta_ptrs.push(weights.as_ptr());
                    delta_lens.push(weights.len());
                } else {
                    lora_delta_ptrs.push(std::ptr::null());
                    delta_lens.push(0);
                }
            }

            let request_id = next_request_id();
            let (tx, rx) = tokio::sync::oneshot::channel();

            {
                let mut senders = ASYNC_RESULTS.lock()?;
                senders.insert(request_id, tx);
            }

            unsafe {
                ffi::coreml_predict_async_with_lora(
                    self.model_handle,
                    input_ids.as_ptr(),
                    input_ids.len(),
                    adapter_indices.as_ptr(),
                    adapter_gates.as_ptr(),
                    adapter_indices.len(),
                    lora_delta_ptrs.as_ptr() as *const *const f32,
                    delta_lens.as_ptr(),
                    async_prediction_callback,
                    request_id as *mut std::ffi::c_void,
                );
            }

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                rx,
            )
            .await
            .map_err(|_| AosError::Timeout("Prediction timeout".into()))?
            .map_err(|_| AosError::Kernel("Channel error".into()))?;

            self.metrics.total_operations += 1;
            self.metrics.successful_operations += 1;

            result.output
        }
    }
}
```

### 3.3 FusedKernels Trait Extension

```rust
// In adapteros-lora-kernel-api/src/lib.rs

pub trait FusedKernels {
    // ... existing sync methods ...

    /// Async version of run_step
    /// Default implementation: fallback to sync
    async fn run_step_async(
        &mut self,
        ring: &RouterRing,
        io: &mut IoBuffers,
    ) -> Result<()> {
        // Default: delegate to sync version
        self.run_step(ring, io)
    }
}

// CoreML backend implements async
impl FusedKernels for CoreMLBackend {
    // ... existing sync ...

    async fn run_step_async(
        &mut self,
        ring: &RouterRing,
        io: &mut IoBuffers,
    ) -> Result<()> {
        #[cfg(not(target_os = "macos"))]
        {
            return Err(AosError::Kernel("CoreML not available".to_string()));
        }

        #[cfg(target_os = "macos")]
        {
            let indices = ring.active_indices();
            let gates = ring.active_gates();
            let has_active_adapters = gates.iter().any(|&g| g != 0);

            let output = if has_active_adapters {
                self.run_inference_async_with_lora(
                    &io.input_ids,
                    indices,
                    gates,
                )
                .await?
            } else {
                self.run_inference_async(&io.input_ids).await?
            };

            io.output_logits = output;
            io.position += 1;
            self.metrics.total_operations += 1;
            self.metrics.successful_operations += 1;

            Ok(())
        }
    }
}
```

---

## 4. Integration with InferencePipeline

### 4.1 Updated Inference Pipeline

In `adapteros-lora-worker/src/inference_pipeline.rs`:

```rust
pub struct InferencePipeline {
    backend: Box<dyn FusedKernels>,
    tokenizer: Arc<Tokenizer>,
    use_async: bool,  // Feature flag
}

impl InferencePipeline {
    /// Process sequence with async prediction
    pub async fn process_async(
        &mut self,
        tokens: &[u32],
    ) -> Result<Vec<Vec<f32>>> {
        let mut outputs = Vec::with_capacity(tokens.len());

        for token in tokens {
            let ring = self.backend.select_adapters(&[*token])?;

            let mut io = IoBuffers::new(&[*token]);

            // Use async if available and enabled
            if self.use_async {
                self.backend.run_step_async(&ring, &mut io).await?;
            } else {
                self.backend.run_step(&ring, &mut io)?;
            }

            outputs.push(io.output_logits);
        }

        Ok(outputs)
    }

    /// Process with streaming output
    pub async fn process_streaming(
        &mut self,
        tokens: Vec<u32>,
    ) -> Result<impl Stream<Item = Result<Vec<f32>>>> {
        use futures::stream;

        let backend = self.backend.clone();  // Requires Clone trait
        let outputs = stream::iter(tokens)
            .then(move |token| {
                let mut backend = backend.clone();
                async move {
                    let ring = backend.select_adapters(&[token])?;
                    let mut io = IoBuffers::new(&[token]);
                    backend.run_step_async(&ring, &mut io).await?;
                    Ok(io.output_logits)
                }
            });

        Ok(outputs)
    }
}
```

---

## 5. Testing Strategy

### 5.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_async_prediction_basic() {
        let mut backend = CoreMLBackend::new(ComputeUnits::CpuAndGpu).unwrap();
        backend.load_model(&PathBuf::from("test_model.mlmodelc")).unwrap();

        let input = vec![1, 2, 3, 4, 5];
        let output = backend.run_inference_async(&input).await.unwrap();

        assert!(!output.is_empty());
        assert!(output.iter().all(|&x| !x.is_nan()));
    }

    #[tokio::test]
    async fn test_async_timeout() {
        // Test 30s timeout with mock slow prediction
    }

    #[tokio::test]
    async fn test_concurrent_predictions() {
        let backend = Arc::new(Mutex::new(CoreMLBackend::new(...).unwrap()));

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let backend = Arc::clone(&backend);
                tokio::spawn(async move {
                    let mut b = backend.lock().await;
                    b.run_inference_async(&[1, 2, 3]).await
                })
            })
            .collect();

        for handle in handles {
            let _ = handle.await.unwrap();
        }
    }
}
```

### 5.2 Integration Tests

```rust
#[tokio::test]
async fn test_inference_pipeline_async() {
    let mut pipeline = InferencePipeline::new_test_with_async();
    let tokens = vec![1, 2, 3, 4, 5];

    let outputs = pipeline.process_async(&tokens).await.unwrap();

    assert_eq!(outputs.len(), tokens.len());
}

#[tokio::test]
async fn test_streaming_inference() {
    let mut pipeline = InferencePipeline::new_test();
    let tokens = vec![1, 2, 3, 4, 5];

    let stream = pipeline.process_streaming(tokens).await.unwrap();

    use futures::StreamExt;
    let mut count = 0;
    while let Some(_) = stream.next().await {
        count += 1;
    }

    assert_eq!(count, 5);
}
```

---

## 6. Performance Profiling

### 6.1 Benchmark Template

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_async_vs_sync(c: &mut Criterion) {
    c.bench_function("sync_prediction_1000_tokens", |b| {
        b.iter(|| {
            let mut backend = CoreMLBackend::new(...).unwrap();
            for _ in 0..1000 {
                backend.run_inference_sync(black_box(&[1, 2, 3])).unwrap();
            }
        })
    });

    c.bench_function("async_prediction_1000_tokens", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let mut backend = CoreMLBackend::new(...).unwrap();
                for _ in 0..1000 {
                    backend.run_inference_async(black_box(&[1, 2, 3])).await.unwrap();
                }
            })
    });

    c.bench_function("concurrent_4x_async", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let backend = Arc::new(Mutex::new(CoreMLBackend::new(...).unwrap()));
                let mut handles = vec![];

                for _ in 0..4 {
                    let b = Arc::clone(&backend);
                    handles.push(tokio::spawn(async move {
                        let mut backend = b.lock().await;
                        for _ in 0..250 {
                            backend.run_inference_async(&[1, 2, 3]).await.unwrap();
                        }
                    }));
                }

                for h in handles {
                    h.await.unwrap();
                }
            })
    });
}

criterion_group!(benches, benchmark_async_vs_sync);
criterion_main!(benches);
```

---

## 7. Error Handling

### 7.1 Error Cases

```rust
pub enum CoreMLAsyncError {
    /// Prediction timed out (>30s)
    Timeout,

    /// Model not loaded
    ModelNotLoaded,

    /// Callback not invoked (system error)
    CallbackFailed,

    /// Memory allocation failed
    OutOfMemory,

    /// CoreML framework error
    CoreMLError(String),
}

// Mapping to AosError
impl From<CoreMLAsyncError> for AosError {
    fn from(err: CoreMLAsyncError) -> Self {
        match err {
            CoreMLAsyncError::Timeout => {
                AosError::Timeout("CoreML prediction timeout".into())
            },
            CoreMLAsyncError::ModelNotLoaded => {
                AosError::Kernel("CoreML model not loaded".into())
            },
            CoreMLAsyncError::CallbackFailed => {
                AosError::Kernel("Async callback failed".into())
            },
            CoreMLAsyncError::OutOfMemory => {
                AosError::Io("Memory allocation failed".into())
            },
            CoreMLAsyncError::CoreMLError(msg) => {
                AosError::Kernel(format!("CoreML: {}", msg))
            },
        }
    }
}
```

---

## 8. Migration Path

### Phase 1: Add async methods (non-breaking)
- Add FFI functions
- Add async Rust wrappers
- Sync API unchanged

### Phase 2: Add trait methods (opt-in)
- Extend FusedKernels with async methods
- Default implementations call sync
- CoreML backend implements async

### Phase 3: Update pipeline (feature-gated)
- Add `use_async` feature flag in Cargo.toml
- Conditional compilation
- Tests verify both paths

### Phase 4: Deprecate sync (future)
- After async proves stable
- Migrate dependent crates
- Sunset sync path

---

## 9. Checklist for Implementation

- [ ] Add C function signatures to ffi.rs
- [ ] Implement coreml_predict_async_native in coreml_bridge.mm
- [ ] Implement coreml_predict_async_with_lora in coreml_bridge.mm
- [ ] Add async callback infrastructure
- [ ] Create Rust channel wrapper (ASYNC_RESULTS)
- [ ] Implement async methods in CoreMLBackend
- [ ] Extend FusedKernels trait
- [ ] Update InferencePipeline
- [ ] Add unit tests
- [ ] Add integration tests
- [ ] Add benchmark suite
- [ ] Document in API docs
- [ ] Update CLAUDE.md with async guidance
- [ ] Create migration guide for dependent crates

---

**Status:** Ready for implementation
**Estimated Effort:** 5-7 business days
**Risk Level:** Medium (FFI complexity, callback safety)
