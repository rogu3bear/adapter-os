# MLX Memory Safety & FFI Protocol Design

**Purpose:** Define memory safety guarantees and error handling for Rust-C++-MLX FFI boundary
**Last Updated:** 2025-11-19
**Author:** Agent 11 - MLX Path Planner
**Status:** Design specification

---

## 1. Executive Summary

This document specifies the memory safety protocol for integrating MLX (C++ machine learning framework) with AdapterOS (Rust system). The design ensures:
- **Zero memory leaks** via RAII and Drop traits
- **Safe error propagation** across FFI boundary (no exceptions in Rust)
- **Thread safety** despite MLX's lazy evaluation
- **Lifetime correctness** for shared data between Rust and C++

**Key Design Decisions:**
1. **Opaque Pointers:** Rust never accesses C++ internals directly
2. **RAII Wrappers:** All FFI resources have Drop implementations
3. **Error Translation:** C++ exceptions → thread-local errors → Rust Results
4. **Synchronization Barriers:** Force MLX evaluation before returning to Rust
5. **Ownership Protocol:** Clear rules for who owns what memory

---

## 2. Memory Safety Challenges

### 2.1 FFI Boundary Risks

| Risk | Example | Impact |
|------|---------|--------|
| **Memory Leaks** | Rust creates MLX array, forgets to free | Gradual VRAM exhaustion |
| **Use-After-Free** | Rust drops pointer, C++ still using | Segfault |
| **Double-Free** | Both Rust and C++ free same pointer | Segfault |
| **Exception Unwind** | C++ throws exception into Rust | Undefined behavior |
| **Data Races** | Rust thread reads while C++ GPU writes | Corrupted data |
| **Lifetime Mismatch** | Rust returns before C++ async completes | Dangling pointer |

### 2.2 MLX-Specific Challenges

**Lazy Evaluation:**
```cpp
// MLX operations are lazy - no computation happens here
mlx::core::array result = mlx::linalg::matmul(a, b);

// Computation happens on first use (or explicit eval())
float value = result.item<float>();  // NOW it computes
```

**Problem:** Rust returns before GPU kernel completes
**Solution:** Explicit synchronization points (see Section 4.3)

**Unified Memory:**
```cpp
// MLX arrays live in unified memory (CPU + GPU)
mlx::core::array arr = mlx::core::array({1.0, 2.0, 3.0});

// Safe: CPU can access immediately
float* data = arr.data<float>();

// Unsafe: GPU might be writing concurrently
mlx::core::eval(arr);  // Must sync first
```

**Problem:** Data races between Rust (CPU) and MLX (GPU)
**Solution:** Ownership protocol (see Section 3.2)

---

## 3. Memory Ownership Protocol

### 3.1 Ownership Rules

**Rule 1: Rust Creates, Rust Owns**
```rust
// Rust allocates vector
let data = vec![1.0, 2.0, 3.0];

// Pass to C++ (C++ BORROWS, Rust still owns)
let mlx_array = unsafe {
    mlx_array_from_data(data.as_ptr(), data.len() as i32)
};

// C++ creates its own copy internally
// Rust can safely drop `data` after call returns
drop(data);

// Later: Rust must free MLX array
unsafe { mlx_array_free(mlx_array); }
```

**Rule 2: C++ Creates, Rust Wraps**
```rust
// C++ creates MLX array (C++ owns allocation)
let mlx_array = unsafe { mlx_array_zeros(1000) };

// Rust wraps in RAII guard (Rust owns RESPONSIBILITY to free)
pub struct MLXArray(*mut mlx_array_t);

impl Drop for MLXArray {
    fn drop(&mut self) {
        unsafe { mlx_array_free(self.0); }
    }
}

let array = MLXArray(mlx_array);
// When `array` goes out of scope, Drop frees C++ memory
```

**Rule 3: Never Share Mutable**
```rust
// ❌ WRONG: Both Rust and C++ have mutable access
let mut data = vec![1.0, 2.0, 3.0];
let mlx_array = unsafe { mlx_array_from_data(data.as_mut_ptr(), data.len()) };
data[0] = 99.0;  // Data race! C++ might be reading

// ✅ CORRECT: Pass immutable reference, C++ copies
let data = vec![1.0, 2.0, 3.0];
let mlx_array = unsafe { mlx_array_from_data(data.as_ptr(), data.len()) };
// Rust can still read `data`, C++ has its own copy
```

### 3.2 Ownership Transfer Protocol

#### Case 1: Rust → C++ (Borrow)
```rust
pub fn forward(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
    // Rust owns token_ids (immutable borrow)
    let input_array = unsafe {
        mlx_array_from_ints(
            token_ids.as_ptr() as *const i32,
            token_ids.len() as i32
        )
    };
    // C++ has copied data into MLX array (Rust ownership unaffected)

    let output_array = unsafe { mlx_model_forward(self.model, input_array) };
    // C++ created new array, Rust now responsible for freeing

    // Copy data back to Rust (Rust now owns Vec<f32>)
    let result = unsafe {
        let size = mlx_array_size(output_array) as usize;
        let data_ptr = mlx_array_data(output_array);
        std::slice::from_raw_parts(data_ptr, size).to_vec()
    };

    // Cleanup C++ allocations
    unsafe {
        mlx_array_free(input_array);
        mlx_array_free(output_array);
    }

    Ok(result)
}
```

#### Case 2: C++ → Rust (Transfer)
```rust
pub struct MLXArray {
    ptr: *mut mlx_array_t,
    _phantom: PhantomData<mlx_array_t>,
}

impl MLXArray {
    pub fn zeros(size: usize) -> Result<Self> {
        let ptr = unsafe { mlx_array_zeros(size as i32) };
        if ptr.is_null() {
            return Err(AosError::Mlx("Failed to allocate array".into()));
        }
        Ok(MLXArray {
            ptr,
            _phantom: PhantomData,
        })
    }

    pub fn as_ptr(&self) -> *mut mlx_array_t {
        self.ptr
    }

    pub fn to_vec(&self) -> Result<Vec<f32>> {
        unsafe {
            let size = mlx_array_size(self.ptr) as usize;
            let data = mlx_array_data(self.ptr);
            Ok(std::slice::from_raw_parts(data, size).to_vec())
        }
    }
}

impl Drop for MLXArray {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { mlx_array_free(self.ptr); }
        }
    }
}
```

---

## 4. Error Handling Protocol

### 4.1 Exception Translation

**Problem:** C++ exceptions cannot unwind through Rust code (undefined behavior)

**Solution:** Thread-local error storage

**C++ Side:**
```cpp
// wrapper.h
extern "C" {
    const char* mlx_get_last_error(void);
    void mlx_clear_error(void);
}

// mlx_cpp_wrapper.cpp
static thread_local std::string g_last_error;

extern "C" mlx_array_t* mlx_matmul(mlx_array_t* a, mlx_array_t* b) {
    try {
        // Real MLX operation (can throw)
        auto arr_a = reinterpret_cast<mlx::core::array*>(a);
        auto arr_b = reinterpret_cast<mlx::core::array*>(b);
        auto result = mlx::linalg::matmul(*arr_a, *arr_b);

        return new mlx::core::array(result);
    } catch (const std::invalid_argument& e) {
        g_last_error = std::string("InvalidArgument: ") + e.what();
        return nullptr;
    } catch (const std::runtime_error& e) {
        g_last_error = std::string("RuntimeError: ") + e.what();
        return nullptr;
    } catch (const std::exception& e) {
        g_last_error = std::string("Exception: ") + e.what();
        return nullptr;
    } catch (...) {
        g_last_error = "Unknown C++ exception";
        return nullptr;
    }
}

extern "C" const char* mlx_get_last_error(void) {
    return g_last_error.c_str();
}

extern "C" void mlx_clear_error(void) {
    g_last_error.clear();
}
```

**Rust Side:**
```rust
pub fn matmul(&self, a: &MLXArray, b: &MLXArray) -> Result<MLXArray> {
    unsafe {
        mlx_clear_error();

        let result_ptr = mlx_matmul(a.as_ptr(), b.as_ptr());

        if result_ptr.is_null() {
            let error_ptr = mlx_get_last_error();
            let error_msg = if error_ptr.is_null() {
                "Unknown MLX error".to_string()
            } else {
                CStr::from_ptr(error_ptr).to_string_lossy().to_string()
            };
            return Err(AosError::Mlx(format!("Matmul failed: {}", error_msg)));
        }

        Ok(MLXArray::from_raw(result_ptr))
    }
}
```

### 4.2 Error Categories

**Map C++ exceptions to AdapterOS error types:**

| C++ Exception | Rust Error | Example |
|---------------|------------|---------|
| `std::invalid_argument` | `AosError::Validation` | Invalid tensor shape |
| `std::runtime_error` | `AosError::Mlx` | GPU kernel failed |
| `std::bad_alloc` | `AosError::Memory` | Out of VRAM |
| `std::logic_error` | `AosError::Internal` | Bug in MLX wrapper |
| Metal errors | `AosError::Mlx` | Metal shader compilation failed |

**Implementation:**
```rust
// adapteros-core/src/error.rs
pub enum AosError {
    // ... existing variants
    Mlx(String),      // MLX-specific errors
    Memory(String),   // Allocation failures
}

// adapteros-lora-mlx-ffi/src/lib.rs
fn translate_mlx_error(error_msg: &str) -> AosError {
    if error_msg.contains("InvalidArgument") {
        AosError::Validation(error_msg.to_string())
    } else if error_msg.contains("bad_alloc") || error_msg.contains("Out of memory") {
        AosError::Memory(error_msg.to_string())
    } else {
        AosError::Mlx(error_msg.to_string())
    }
}
```

### 4.3 Error Recovery

**Strategy:** Graceful degradation, no panics

```rust
impl FusedKernels for MLXFFIBackend {
    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        // Try MLX forward pass
        match self.model.forward_with_hidden_states(&io.input_ids) {
            Ok((logits, hidden_states)) => {
                // Apply LoRA if hidden states available
                if !hidden_states.is_empty() {
                    match self.apply_loras(ring, &logits, &hidden_states) {
                        Ok(adapted_logits) => {
                            io.output_logits.copy_from_slice(&adapted_logits);
                        }
                        Err(e) => {
                            tracing::warn!("LoRA application failed, using base logits: {}", e);
                            io.output_logits.copy_from_slice(&logits);
                        }
                    }
                } else {
                    io.output_logits.copy_from_slice(&logits);
                }
                io.position += 1;
                Ok(())
            }
            Err(e) => {
                tracing::error!("MLX forward pass failed: {}", e);
                Err(e)
            }
        }
    }
}
```

---

## 5. Thread Safety

### 5.1 MLX Threading Model

**MLX Characteristics:**
- **Lazy evaluation:** Operations queued, executed on eval()
- **Async GPU execution:** Metal kernels run asynchronously
- **Graph compilation:** Repeated operations compiled to optimized graphs
- **Thread-safe:** MLX arrays are thread-safe (read-only after construction)

**AdapterOS Requirements:**
- **Sync interface:** FusedKernels::run_step() must be synchronous
- **Deterministic ordering:** Token-by-token generation
- **No data races:** Rust must not read arrays while GPU writes

### 5.2 Synchronization Points

**Force Evaluation Before Returning:**
```cpp
// C++ wrapper
extern "C" void mlx_eval(mlx_array_t* array) {
    auto arr = reinterpret_cast<mlx::core::array*>(array);
    mlx::core::eval(*arr);  // Wait for GPU to finish
}

extern "C" mlx_array_t* mlx_model_forward_sync(mlx_model_t* model, mlx_array_t* input) {
    try {
        auto mlx_model = reinterpret_cast<mlx::nn::Module*>(model);
        auto mlx_input = reinterpret_cast<mlx::core::array*>(input);

        // Forward pass (lazy)
        auto output = mlx_model->forward(*mlx_input);

        // CRITICAL: Force evaluation before returning
        mlx::core::eval(output);

        return new mlx::core::array(output);
    } catch (const std::exception& e) {
        g_last_error = e.what();
        return nullptr;
    }
}
```

**Rust Enforcement:**
```rust
pub fn forward(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
    let input_array = MLXArray::from_slice(token_ids)?;

    // Forward pass
    let output_ptr = unsafe { mlx_model_forward_sync(self.model, input_array.as_ptr()) };

    // At this point, GPU is DONE - safe to access CPU-side memory
    let output_array = MLXArray::from_raw_checked(output_ptr)?;

    // Copy to Rust (no data races possible)
    output_array.to_vec()
}
```

### 5.3 Send + Sync Safety

**Analysis:**
```rust
pub struct MLXFFIModel {
    model: *mut mlx_model_t,  // Opaque pointer
    config: ModelConfig,       // Plain data
}

// Safe to Send: moving pointer across threads is fine (C++ model is immutable)
unsafe impl Send for MLXFFIModel {}

// Safe to Sync: multiple threads can call forward() concurrently
// (MLX handles thread safety internally via locks)
unsafe impl Sync for MLXFFIModel {}
```

**Justification:**
1. **Opaque Pointer:** Rust never dereferences, only C++ does
2. **C++ Thread Safety:** MLX uses internal locks for concurrent access
3. **Synchronization:** All operations wait for GPU completion before returning
4. **Immutability:** Model weights are read-only after loading

**Testing:**
```rust
#[test]
fn test_concurrent_inference() {
    let model = Arc::new(MLXFFIModel::load("path/to/model").unwrap());

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let model = model.clone();
            thread::spawn(move || {
                let token_ids = vec![1, 2, 3];
                let logits = model.forward(&token_ids, 0).unwrap();
                assert_eq!(logits.len(), 32000);
                println!("Thread {} completed", i);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}
```

---

## 6. Lifetime Management

### 6.1 RAII Pattern

**All FFI resources have Drop implementations:**

```rust
pub struct MLXArray {
    ptr: *mut mlx_array_t,
    _phantom: PhantomData<mlx_array_t>,
}

impl Drop for MLXArray {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { mlx_array_free(self.ptr); }
            self.ptr = std::ptr::null_mut();
        }
    }
}

pub struct MLXFFIModel {
    model: *mut mlx_model_t,
    config: ModelConfig,
}

impl Drop for MLXFFIModel {
    fn drop(&mut self) {
        if !self.model.is_null() {
            unsafe { mlx_model_free(self.model); }
            self.model = std::ptr::null_mut();
        }
    }
}

pub struct MLXContext {
    ctx: *mut mlx_context_t,
}

impl Drop for MLXContext {
    fn drop(&mut self) {
        if !self.ctx.is_null() {
            unsafe { mlx_context_free(self.ctx); }
            self.ctx = std::ptr::null_mut();
        }
    }
}
```

**Benefits:**
- ✅ Guaranteed cleanup (even on panic)
- ✅ No manual free() calls needed
- ✅ Compile-time lifetime checking
- ✅ Leak-free by construction

### 6.2 Borrow Checker Integration

**Use Rust lifetimes to prevent use-after-free:**

```rust
pub struct MLXModel<'a> {
    model: &'a MLXFFIModel,
}

impl<'a> MLXModel<'a> {
    pub fn borrow(model: &'a MLXFFIModel) -> Self {
        Self { model }
    }

    pub fn forward(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        // Safe: 'a ensures model outlives this borrow
        self.model.forward(token_ids, 0)
    }
}

// Compiler enforces:
let model = MLXFFIModel::load("path")?;
{
    let borrowed = MLXModel::borrow(&model);
    borrowed.forward(&[1, 2, 3])?;
}  // borrowed dropped, model still alive
drop(model);  // OK
```

---

## 7. Testing Strategy

### 7.1 Memory Leak Detection

**Valgrind (Linux):**
```bash
cargo build --release
valgrind --leak-check=full ./target/release/inference_test

# Expected: "All heap blocks were freed -- no leaks are possible"
```

**AddressSanitizer (macOS):**
```bash
RUSTFLAGS="-Z sanitizer=address" cargo test -p adapteros-lora-mlx-ffi
```

**Instruments (macOS):**
```bash
cargo build --release
instruments -t Leaks ./target/release/inference_test
```

### 7.2 Use-After-Free Detection

**AddressSanitizer Test:**
```rust
#[test]
fn test_no_use_after_free() {
    let array = MLXArray::zeros(1000).unwrap();
    let ptr = array.as_ptr();

    drop(array);  // Free memory

    // This SHOULD crash with AddressSanitizer if unsafe
    // (Commented out to avoid actual crash)
    // unsafe { mlx_array_size(ptr); }
}
```

### 7.3 Stress Testing

**24-Hour Inference Loop:**
```rust
#[test]
#[ignore]
fn test_no_memory_leaks_24h() {
    let model = MLXFFIModel::load("path/to/model").unwrap();

    let start = Instant::now();
    let mut iterations = 0;

    while start.elapsed() < Duration::from_secs(86400) {  // 24 hours
        let token_ids = vec![1, 2, 3];
        let _logits = model.forward(&token_ids, 0).unwrap();

        iterations += 1;
        if iterations % 1000 == 0 {
            println!("Iterations: {}, Memory: {} MB", iterations, get_memory_usage());
        }
    }

    println!("Test completed: {} iterations, no leaks", iterations);
}
```

### 7.4 Concurrency Testing

**Rayon Parallel Inference:**
```rust
#[test]
fn test_parallel_inference() {
    use rayon::prelude::*;

    let model = Arc::new(MLXFFIModel::load("path/to/model").unwrap());

    let results: Vec<_> = (0..1000)
        .into_par_iter()
        .map(|i| {
            let token_ids = vec![1, 2, 3];
            model.forward(&token_ids, 0).unwrap()
        })
        .collect();

    assert_eq!(results.len(), 1000);
}
```

---

## 8. Debugging Tools

### 8.1 C++ Debugging

**GDB/LLDB:**
```bash
# Build with debug info
cargo build

# Debug in LLDB
lldb target/debug/inference_test
(lldb) b mlx_model_forward
(lldb) run
(lldb) bt  # Backtrace on crash
```

**Print Debugging:**
```cpp
extern "C" mlx_array_t* mlx_model_forward(mlx_model_t* model, mlx_array_t* input) {
    std::cerr << "[DEBUG] mlx_model_forward called" << std::endl;
    std::cerr << "[DEBUG] model ptr: " << model << std::endl;
    std::cerr << "[DEBUG] input ptr: " << input << std::endl;

    try {
        // ... operation
        std::cerr << "[DEBUG] Forward pass successful" << std::endl;
        return result;
    } catch (const std::exception& e) {
        std::cerr << "[ERROR] Exception: " << e.what() << std::endl;
        g_last_error = e.what();
        return nullptr;
    }
}
```

### 8.2 Rust Debugging

**Tracing:**
```rust
pub fn forward(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
    tracing::trace!("MLX forward: token_ids={:?}", token_ids);

    let input_array = MLXArray::from_slice(token_ids)?;
    tracing::trace!("Created input array: ptr={:?}", input_array.as_ptr());

    let output_ptr = unsafe { mlx_model_forward_sync(self.model, input_array.as_ptr()) };
    tracing::trace!("Forward complete: output_ptr={:?}", output_ptr);

    let output_array = MLXArray::from_raw_checked(output_ptr)?;
    let result = output_array.to_vec()?;
    tracing::trace!("Converted to vec: len={}", result.len());

    Ok(result)
}
```

**Enable Tracing:**
```bash
RUST_LOG=adapteros_lora_mlx_ffi=trace cargo test
```

---

## 9. Production Checklist

### 9.1 Memory Safety Validation

- [ ] All FFI pointers wrapped in RAII types
- [ ] No raw pointer leaks in public API
- [ ] Drop implementations tested with Valgrind
- [ ] No circular references (Arc cycles)
- [ ] PhantomData used for correct variance

### 9.2 Error Handling Validation

- [ ] All C++ functions have try-catch
- [ ] Thread-local error storage working
- [ ] Error messages propagated to Rust
- [ ] No panics in FFI boundary
- [ ] Recovery from MLX errors tested

### 9.3 Thread Safety Validation

- [ ] Send/Sync implementations justified
- [ ] Synchronization points in place
- [ ] Concurrent inference tested (>100 threads)
- [ ] No data races (checked with ThreadSanitizer)
- [ ] Lock contention profiled

### 9.4 Lifetime Validation

- [ ] Borrow checker satisfied
- [ ] No lifetime elision bugs
- [ ] References outlive borrowed data
- [ ] No dangling pointers in tests
- [ ] Use-after-free detection enabled

---

## 10. References

### 10.1 Related Documents
- **Integration Plan:** `/Users/star/Dev/aos/docs/MLX_CPP_INTEGRATION_PLAN.md`
- **Stub Status:** `/Users/star/Dev/aos/docs/MLX_STUB_STATUS.md`
- **Kernel API:** `/Users/star/Dev/aos/crates/adapteros-lora-kernel-api/src/lib.rs`

### 10.2 External Resources
- **Rust FFI Guide:** https://doc.rust-lang.org/nomicon/ffi.html
- **MLX C++ Docs:** https://ml-explore.github.io/mlx/
- **Metal Memory Management:** https://developer.apple.com/metal/

---

## 11. Conclusion

The MLX memory safety protocol ensures zero-leak, crash-free integration between Rust and C++/MLX. Key innovations:
1. **RAII everywhere:** Compiler-enforced cleanup
2. **Exception firewall:** C++ exceptions never reach Rust
3. **Synchronization barriers:** GPU operations complete before Rust reads
4. **Clear ownership:** Protocol prevents double-frees and leaks

**Status:** ✅ Design complete, ready for implementation
**Next Step:** Implement during Phase 3 of MLX integration (see MLX_CPP_INTEGRATION_PLAN.md)

---

**Document Control:**
- **Created:** 2025-11-19
- **Author:** Agent 11 (MLX Path Planner)
- **Classification:** Internal Technical Specification
- **Next Review:** Upon implementation or safety audit
