# CoreML Backend FFI Integration Guide

**Agent 3 Deliverable**: Integration points documentation for Agent 2 (Objective-C++ FFI Layer)

## Overview

The CoreML backend (`CoreMLBackend`) is implemented in Rust and provides the high-level API for ANE acceleration. It coordinates with an Objective-C++ FFI layer (Agent 2) that handles actual CoreML model operations.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    CoreMLBackend (Rust)                      │
│  - Model lifecycle management                                │
│  - Buffer conversion (IoBuffers ↔ MLMultiArray)             │
│  - ANE scheduling and detection                              │
│  - Error handling and timeout protection                     │
│  - Implements FusedKernels trait                             │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       │ FFI calls (extern "C")
                       ▼
┌─────────────────────────────────────────────────────────────┐
│              CoreML FFI Layer (Objective-C++)                │
│  - CoreML model loading (MLModel)                            │
│  - MLPrediction execution                                    │
│  - ANE device targeting (MLComputeUnits)                     │
│  - MLMultiArray marshaling                                   │
└─────────────────────────────────────────────────────────────┘
```

## FFI Function Signatures

The following C-compatible functions must be implemented by Agent 2 in the Objective-C++ layer.

### 1. Initialization and Teardown

#### `coreml_bridge_init`
Initialize the CoreML bridge. Called once at backend creation.

```c
extern "C" {
    int coreml_bridge_init();
}
```

**Returns:**
- `0` on success
- Non-zero error code on failure

**Implementation Notes:**
- Initialize any CoreML runtime state
- Register error handlers
- Check CoreML framework availability
- Thread-safe (uses `Once` in Rust)

---

#### `coreml_neural_engine_available`
Check if Neural Engine is available on this device.

```c
extern "C" {
    int coreml_neural_engine_available();
}
```

**Returns:**
- `1` if ANE is available
- `0` if ANE is not available

**Implementation Notes:**
- Query `MLComputeUnits` capabilities
- Check for Apple Silicon chip
- Can be called multiple times (should cache result)

---

#### `coreml_bridge_shutdown`
Cleanup CoreML bridge resources.

```c
extern "C" {
    void coreml_bridge_shutdown();
}
```

**Implementation Notes:**
- Release any global CoreML state
- Called at process exit or backend cleanup
- Should be idempotent

---

### 2. Model Compilation

#### `coreml_compile_model`
Compile a CoreML model from plan bytes.

```c
extern "C" {
    void* coreml_compile_model(
        const uint8_t* plan_bytes,
        size_t plan_len,
        bool use_ane
    );
}
```

**Parameters:**
- `plan_bytes`: Pointer to model plan data (binary format)
- `plan_len`: Length of plan data in bytes
- `use_ane`: If `true`, target ANE; if `false`, fallback to CPU/GPU

**Returns:**
- Opaque pointer to compiled `MLModel` on success
- `NULL` on failure

**Implementation Notes:**
- Parse plan bytes to extract CoreML model specification
- Compile model using `MLModelConfiguration`:
  ```objc
  MLModelConfiguration *config = [[MLModelConfiguration alloc] init];
  config.computeUnits = use_ane ? MLComputeUnitsAll : MLComputeUnitsCPUAndGPU;
  ```
- Store compiled model in heap-allocated object
- Return pointer to model (Rust will store as `usize`)
- Handle errors gracefully (return `NULL`)

**Error Conditions:**
- Invalid plan format
- CoreML compilation failure
- Out of memory
- Model too large for ANE

---

### 3. Model Execution

#### `coreml_predict`
Execute CoreML model prediction.

```c
extern "C" {
    int coreml_predict(
        void* model_handle,
        const uint32_t* input_ids,
        size_t input_len,
        float* output_logits,
        size_t output_len,
        uint64_t timeout_ms
    );
}
```

**Parameters:**
- `model_handle`: Opaque pointer returned by `coreml_compile_model`
- `input_ids`: Pointer to input token IDs (array of `u32`)
- `input_len`: Number of input tokens
- `output_logits`: Pointer to output buffer (array of `f32`)
- `output_len`: Expected number of output logits (vocab_size)
- `timeout_ms`: Timeout in milliseconds (0 = no timeout)

**Returns:**
- `0` on success
- Non-zero error code on failure:
  - `1` = Timeout
  - `2` = Invalid model handle
  - `3` = Buffer size mismatch
  - `4` = CoreML execution error

**Implementation Notes:**
- Convert `input_ids` to `MLMultiArray`:
  ```objc
  MLMultiArray *inputArray = [[MLMultiArray alloc]
      initWithShape:@[@(batch_size), @(seq_len)]
      dataType:MLMultiArrayDataTypeInt32
      error:&error];
  ```
- Execute prediction:
  ```objc
  MLPredictionOptions *options = [[MLPredictionOptions alloc] init];
  if (timeout_ms > 0) {
      options.timeout = timeout_ms / 1000.0; // Convert to seconds
  }
  id<MLFeatureProvider> output = [model predictionFromFeatures:input
                                                        options:options
                                                          error:&error];
  ```
- Extract logits from output `MLMultiArray`
- Copy to `output_logits` buffer
- Implement timeout protection
- Thread-safe (can be called concurrently for different models)

**Error Handling:**
- Validate `model_handle` is non-null
- Check buffer sizes match model expectations
- Handle CoreML execution errors gracefully
- Enforce timeout if specified

---

### 4. Model Cleanup

#### `coreml_release_model`
Release a compiled CoreML model.

```c
extern "C" {
    void coreml_release_model(void* model_handle);
}
```

**Parameters:**
- `model_handle`: Opaque pointer returned by `coreml_compile_model`

**Implementation Notes:**
- Cast pointer back to model object
- Release CoreML model resources
- Free heap-allocated memory
- Should be idempotent (safe to call with `NULL`)

---

## Data Marshaling

### Input: Token IDs

**Rust side:**
```rust
// Vec<u32> of token IDs
let input_ids: Vec<u32> = vec![1, 2, 3, ...];
```

**FFI boundary:**
```c
const uint32_t* input_ids
size_t input_len
```

**Objective-C++ side:**
```objc
MLMultiArray *inputArray = [[MLMultiArray alloc]
    initWithShape:@[@1, @(input_len)]  // [batch_size=1, seq_len]
    dataType:MLMultiArrayDataTypeInt32
    error:&error];

// Copy token IDs
for (size_t i = 0; i < input_len; i++) {
    [inputArray setObject:@(input_ids[i]) atIndexedSubscript:i];
}
```

---

### Output: Logits

**Objective-C++ side:**
```objc
MLMultiArray *outputArray = output.featureValueForName(@"logits").multiArrayValue;

// Extract logits (vocab_size floats)
const float* logitsPtr = (const float*)outputArray.dataPointer;
size_t vocab_size = outputArray.shape[1].unsignedIntegerValue;
```

**FFI boundary:**
```c
float* output_logits  // Pre-allocated buffer of size vocab_size
size_t output_len     // Expected vocab_size
```

**Rust side:**
```rust
let mut output_buffer = vec![0.0f32; vocab_size];
// FFI call fills output_buffer
io.output_logits.extend_from_slice(&output_buffer);
```

---

## Error Handling Strategy

### Rust Side

```rust
// All errors return AosError::CoreML
pub enum AosError {
    #[error("CoreML error: {0}")]
    CoreML(String),
    // ...
}
```

### FFI Boundary

- Use integer error codes (0 = success, non-zero = error)
- Document error codes in comments
- Never panic across FFI boundary

### Objective-C++ Side

```objc
// Catch all exceptions and convert to error codes
@try {
    // CoreML operations
} @catch (NSException *exception) {
    NSLog(@"CoreML error: %@", exception);
    return 4; // Generic error code
}
```

---

## ANE Scheduling Strategy

### Device Selection

```objc
MLModelConfiguration *config = [[MLModelConfiguration alloc] init];

if (use_ane) {
    // Prefer ANE, fallback to GPU/CPU if unavailable
    config.computeUnits = MLComputeUnitsAll;
} else {
    // Explicit CPU/GPU only
    config.computeUnits = MLComputeUnitsCPUAndGPU;
}
```

### ANE Detection

```objc
// Check if ANE is available via system_profiler or IOKit
+ (BOOL)isNeuralEngineAvailable {
    // Check for Apple Silicon
    struct utsname systemInfo;
    uname(&systemInfo);

    NSString *machine = [NSString stringWithCString:systemInfo.machine
                                           encoding:NSUTF8StringEncoding];

    // M1/M2/M3/M4 chips have ANE
    return [machine containsString:@"arm64"];
}
```

---

## Thread Safety

### Rust Side
- `CoreMLBackend` is `Send + Sync`
- Can be used across threads
- Model compilation cache is thread-safe (`HashMap` access is synchronized)

### Objective-C++ Side
- `coreml_predict` must be thread-safe for concurrent calls
- Use separate `MLModel` instances per thread or synchronize access
- Avoid global mutable state

---

## Memory Management

### Model Lifetime

```
Rust                          FFI                    Objective-C++
────────────────────────────────────────────────────────────────
load_model()
  ├─> coreml_compile_model() ────────────> MLModel* model
  └─> store model_handle                   [model retain]

run_step()
  └─> coreml_predict()       ────────────> [model predict...]

cleanup()
  └─> coreml_release_model() ────────────> [model release]
```

### Buffer Ownership

- **Input buffers**: Rust owns, FFI reads (no copy needed)
- **Output buffers**: Rust owns, FFI writes (copy from MLMultiArray)
- **Plan bytes**: Rust owns, FFI reads during compilation

---

## Performance Considerations

### Optimization Guidelines

1. **Buffer Reuse**
   - Reuse `MLMultiArray` objects across predictions
   - Avoid allocating new arrays per inference

2. **Batch Processing**
   - If possible, batch multiple token predictions
   - Reduces ANE invocation overhead

3. **Model Caching**
   - Cache compiled models by plan hash (Rust side handles this)
   - Avoid recompiling identical models

4. **ANE Warmup**
   - First ANE inference may be slower (model loading)
   - Consider warmup call after compilation

---

## Testing Strategy

### Unit Tests (Objective-C++)

```objc
- (void)testCoreMLCompilation {
    uint8_t planBytes[] = {...};
    void* model = coreml_compile_model(planBytes, sizeof(planBytes), true);
    XCTAssertNotNil(model);
    coreml_release_model(model);
}

- (void)testPrediction {
    void* model = coreml_compile_model(...);
    uint32_t inputIds[] = {1, 2, 3};
    float outputLogits[152064] = {0};

    int result = coreml_predict(model, inputIds, 3, outputLogits, 152064, 5000);
    XCTAssertEqual(result, 0);

    coreml_release_model(model);
}
```

### Integration Tests (Rust)

```rust
#[test]
#[cfg(feature = "coreml-backend")]
fn test_coreml_backend_integration() {
    let mut backend = CoreMLBackend::new().unwrap();
    let plan_bytes = vec![...]; // Test model

    backend.load(&plan_bytes).unwrap();

    let ring = RouterRing::new(0);
    let mut io = IoBuffers::new(152064);
    io.input_ids = vec![1, 2, 3];

    backend.run_step(&ring, &mut io).unwrap();
    assert_eq!(io.output_logits.len(), 152064);
}
```

---

## Debugging Tips

### Enable Verbose Logging

**Rust:**
```bash
RUST_LOG=adapteros_lora_kernel_mtl=debug cargo test
```

**Objective-C++:**
```objc
// In FFI implementation
NSLog(@"CoreML: Compiling model, size=%zu, ane=%d", plan_len, use_ane);
```

### Common Issues

1. **Model Compilation Failure**
   - Check plan format is valid CoreML spec
   - Verify model size < 1GB for ANE
   - Check macOS version supports CoreML features

2. **Prediction Timeout**
   - Increase `execution_timeout` in Rust
   - Check ANE is actually being used (not CPU fallback)
   - Profile model complexity

3. **Buffer Size Mismatch**
   - Verify vocab_size matches model output shape
   - Check input sequence length is within model limits

---

## Future Enhancements

### Planned Features (Agent 2 Responsibility)

1. **Adapter Integration**
   - Support LoRA adapters compiled into CoreML model
   - Runtime adapter fusion in MLModel

2. **Quantization Support**
   - FP16/INT8 model compilation
   - Quantized LoRA adapters

3. **Batch Inference**
   - Multi-token batch predictions
   - Batch size > 1

4. **Metal Integration**
   - Hybrid Metal + CoreML pipeline
   - Share GPU buffers between backends

---

## References

- **CoreML Documentation**: https://developer.apple.com/documentation/coreml
- **ANE Research**: https://github.com/hollance/neural-engine
- **MLComputeUnits**: https://developer.apple.com/documentation/coreml/mlcomputeunits
- **FFI Best Practices**: https://doc.rust-lang.org/nomicon/ffi.html

---

## Contact

For questions about this integration:
- **Agent 3 (Rust Backend)**: See `crates/adapteros-lora-kernel-mtl/src/coreml_backend.rs`
- **Agent 2 (FFI Layer)**: Implement functions in this document

---

**Status**: Ready for Agent 2 implementation
**Last Updated**: 2025-11-19
**Version**: 1.0
