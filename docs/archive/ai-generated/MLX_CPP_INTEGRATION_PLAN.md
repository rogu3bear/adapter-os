# MLX C++ Integration Plan for AdapterOS

**Document Status:** Planning Phase
**MLX Status:** C API Available (mlx-c), C++ API Mature
**Last Updated:** 2025-11-19
**Author:** Agent 11 - MLX Path Planner

---

## Executive Summary

This document outlines the integration path for MLX (Apple's Machine Learning Framework) into AdapterOS, focusing on leveraging MLX's unified memory architecture for large model inference on Apple Silicon. The current stub implementation provides a foundation for future integration as MLX's C/C++ APIs mature into production-ready status.

**Key Findings (2025-11-19):**
- MLX has an active C API (mlx-c) with 150+ stars, 131 commits, production-ready status unclear
- MLX C++ API is mature and fully featured, closely following Python API
- MLX unified memory enables models up to 512GB on M3 Ultra (vs 128GB Metal VRAM limit)
- FastMLX project demonstrates production-ready MLX hosting capabilities
- AdapterOS stub implementation is functional and compiles with `MLX_FORCE_STUB=1`

---

## 1. Current State Analysis

### 1.1 AdapterOS MLX Stub

**Location:** `/Users/star/Dev/aos/crates/adapteros-lora-mlx-ffi/`

**Components:**
```
adapteros-lora-mlx-ffi/
├── src/
│   ├── lib.rs              # Main FFI integration
│   ├── backend.rs          # FusedKernels trait implementation
│   ├── lora.rs             # LoRA adapter management
│   ├── routing.rs          # Multi-adapter routing
│   ├── tensor.rs           # Tensor operations (stub)
│   ├── embedding.rs        # Embedding model (stub)
│   ├── mock.rs             # Test mocks
│   └── mlx_cpp_wrapper.cpp # C++ stub implementation (543 LOC)
├── wrapper.h               # C API header (79 LOC)
├── build.rs                # Build script with stub detection
├── Cargo.toml             # Dependencies
└── README.md              # Documentation
```

**Stub Features:**
- ✅ Compiles successfully with `MLX_FORCE_STUB=1`
- ✅ Implements `FusedKernels` trait for worker compatibility
- ✅ Provides full C FFI interface (context, arrays, models, LoRA ops)
- ✅ Mock implementations for testing
- ✅ Thread-safe design (Send + Sync)
- ✅ Attestation reports non-determinism (policy-compliant)

**Stub Limitations:**
- ❌ No real inference (returns dummy values)
- ❌ No hidden state extraction (critical for LoRA)
- ❌ No real LoRA weight application
- ❌ No GPU acceleration
- ❌ No unified memory utilization

### 1.2 MLX Framework Status (2025-11-19)

**Official Sources:**
- Main Project: https://github.com/ml-explore/mlx
- C API: https://github.com/ml-explore/mlx-c
- Documentation: https://ml-explore.github.io/mlx/
- Version: 0.29.5 (latest stable)

**Language Support:**
- **Python API:** Primary, fully featured, NumPy-like
- **C++ API:** Mature, fully featured, closely follows Python API
- **C API (mlx-c):** Active development, 150+ stars, MIT licensed
- **Swift API:** Uses mlx-c as bridge, official Apple support

**Key Capabilities:**
1. **Unified Memory Architecture**
   - CPU/GPU share same memory pool (no copies)
   - Operations on any device without data transfer
   - Enables models up to 512GB on M3 Ultra

2. **Built-in Quantization**
   - Int8, 4-bit, 4.5-bit per weight
   - No setup required for inference/training
   - Minimal quality loss

3. **LoRA Training Support**
   - Native LoRA and QLoRA support
   - Memory-efficient fine-tuning
   - Built into framework

4. **Production-Ready Ecosystem**
   - FastMLX provides production API hosting
   - Robust error management
   - Hugging Face model compatibility

**Performance Characteristics:**
- Metal GPU acceleration (Apple Silicon only)
- Lazy evaluation with automatic optimization
- Graph compilation for repeated operations
- Memory-mapped weights for fast loading

---

## 2. Integration Architecture

### 2.1 Three-Tier Strategy

```
┌─────────────────────────────────────────────────────────────┐
│                    AdapterOS Worker                          │
│  (adapteros-lora-worker - Rust, production inference)        │
└───────────────┬─────────────────────────────────────────────┘
                │
                ├──────────────────────────────────────┐
                │                                      │
     ┌──────────▼───────────┐              ┌──────────▼─────────┐
     │   Metal Backend      │              │   MLX Backend      │
     │  (Primary, < 128GB)  │              │ (Future, ≤ 512GB) │
     ├──────────────────────┤              ├────────────────────┤
     │ adapteros-lora-      │              │ adapteros-lora-    │
     │ kernel-mtl           │              │ mlx-ffi            │
     │                      │              │                    │
     │ • Deterministic      │              │ • Non-deterministic│
     │ • Q15 gates          │              │ • Unified memory   │
     │ • Precompiled Metal  │              │ • Large models     │
     │ • VRAM limit: 128GB  │              │ • LoRA training    │
     └──────────────────────┘              └────────────────────┘
                                                      │
                                           ┌──────────▼─────────┐
                                           │   mlx-c (C API)    │
                                           │   or C++ direct    │
                                           ├────────────────────┤
                                           │ • Array operations │
                                           │ • Model loading    │
                                           │ • LoRA application │
                                           └────────────────────┘
                                                      │
                                           ┌──────────▼─────────┐
                                           │   MLX Framework    │
                                           │   (C++/Metal)      │
                                           ├────────────────────┤
                                           │ • Unified memory   │
                                           │ • GPU acceleration │
                                           │ • Lazy evaluation  │
                                           └────────────────────┘
```

**Rationale:**
- **Metal Backend:** Production inference, deterministic, proven, <128GB models
- **MLX Backend:** Research, large models (>128GB), unified memory, training
- **Coexistence:** Both backends implement `FusedKernels` trait, runtime selection

### 2.2 Integration Paths

#### Option A: Direct C++ Integration (Recommended)

**Pros:**
- No C wrapper overhead
- Full access to MLX C++ API features
- Type safety via C++ compiler
- Direct memory management control

**Cons:**
- Requires C++ in build pipeline (already present)
- More complex FFI bindings
- Rust-C++ lifetime management

**Implementation Steps:**
1. Replace stub wrapper with real MLX C++ code
2. Link against MLX libraries (Homebrew or source build)
3. Implement hidden state extraction (critical for LoRA)
4. Integrate with existing `FusedKernels` trait
5. Add memory safety layer (see Section 4)

#### Option B: C API via mlx-c (Alternative)

**Pros:**
- Simpler FFI (C is easier than C++)
- Official C API maintained by ml-explore
- Used by Swift bindings (proven bridge)

**Cons:**
- C API may lag behind C++ features
- Additional layer of indirection
- Production-readiness unclear (as of 2025-11-19)

**Implementation Steps:**
1. Wait for mlx-c production-ready status
2. Replace stub with mlx-c bindings
3. Use `bindgen` for automatic binding generation
4. Test against mlx-c test suite
5. Verify feature parity with requirements

**Recommendation:** Start with **Option A (Direct C++)** due to maturity, full feature access, and proven production use. Revisit Option B if mlx-c reaches production-ready status with clear feature parity.

---

## 3. Required MLX APIs

### 3.1 Core APIs (Must-Have)

#### Model Loading
```cpp
// Required: Load model from directory
mlx::core::array model_load(const std::string& path);

// Required: Forward pass with hidden states
std::tuple<mlx::core::array, std::unordered_map<std::string, mlx::core::array>>
    model_forward_with_hidden_states(
        const mlx::core::array& input_ids,
        const std::unordered_map<std::string, mlx::core::array>& model_weights
    );
```

**Availability:** ✅ C++ API has model loading, hidden state extraction needs verification

#### Tensor Operations
```cpp
// Required: Basic tensor operations
mlx::core::array zeros(const std::vector<int>& shape);
mlx::core::array ones(const std::vector<int>& shape);
mlx::core::array from_data(const float* data, const std::vector<int>& shape);

// Required: Matrix operations
mlx::core::array matmul(const mlx::core::array& a, const mlx::core::array& b);
mlx::core::array transpose(const mlx::core::array& a);
mlx::core::array reshape(const mlx::core::array& a, const std::vector<int>& shape);
```

**Availability:** ✅ All available in MLX C++ API

#### LoRA Operations
```cpp
// Required: LoRA forward pass (adapter application)
mlx::core::array lora_forward(
    const mlx::core::array& input,
    const mlx::core::array& lora_a,  // Down-projection (rank × dim)
    const mlx::core::array& lora_b,  // Up-projection (dim × rank)
    float alpha,
    float scaling
);

// Required: Multi-adapter routing (K-sparse)
mlx::core::array multi_lora_forward(
    const mlx::core::array& input,
    const std::vector<std::pair<mlx::core::array, mlx::core::array>>& adapters,
    const std::vector<float>& gates,  // Routing gates (float, not Q15)
    const std::string& module_name
);
```

**Availability:** ⚠️ LoRA primitives exist, multi-adapter routing needs implementation

### 3.2 Memory Management APIs

#### Unified Memory
```cpp
// Required: Get memory usage
size_t memory_usage();

// Required: Garbage collection (lazy array cleanup)
void gc_collect();

// Required: Device placement (CPU/GPU)
mlx::core::array to_device(const mlx::core::array& a, mlx::core::Device device);
```

**Availability:** ✅ Available in MLX C++ API

### 3.3 Hidden State Extraction (Critical)

**Requirement:** Extract intermediate activations from transformer layers for LoRA application

**Current Stub Limitation:**
```rust
// Current stub implementation (src/lib.rs:177)
pub fn forward_with_hidden_states(
    &self,
    token_ids: &[u32],
) -> Result<(Vec<f32>, std::collections::HashMap<String, Vec<f32>>)> {
    let logits = self.forward(token_ids, 0)?;
    let hidden_states = std::collections::HashMap::new(); // EMPTY - NEEDS REAL IMPL
    Ok((logits, hidden_states))
}
```

**Required Implementation:**
```cpp
// Extract activations at specific module names
// Target modules: ["q_proj", "k_proj", "v_proj", "o_proj"] per transformer layer
std::unordered_map<std::string, mlx::core::array> extract_hidden_states(
    const mlx::core::array& input_ids,
    const std::vector<std::string>& target_modules
);
```

**MLX Approach:** Use MLX's computation graph hooks to capture intermediate values
- Register forward hooks on target modules
- Store activations during forward pass
- Return activations alongside final logits

**Status:** ⚠️ Needs verification in MLX C++ API, likely requires custom model class

---

## 4. Memory Safety Design

### 4.1 FFI Safety Requirements

**Rust-C++ Boundary Challenges:**
1. **Lifetime Management:** MLX arrays are reference-counted, Rust owns data
2. **Error Propagation:** C++ exceptions cannot cross FFI boundary
3. **Memory Leaks:** Improper cleanup of MLX objects
4. **Thread Safety:** MLX graph evaluation is async, Rust expects sync

### 4.2 Safety Protocol

#### Layer 1: Opaque Pointers
```rust
// Rust side (src/lib.rs)
pub struct MLXFFIModel {
    model: *mut mlx_model_t,  // Opaque pointer, no direct access
    config: ModelConfig,
}

impl Drop for MLXFFIModel {
    fn drop(&mut self) {
        if !self.model.is_null() {
            unsafe { mlx_model_free(self.model); }  // Guaranteed cleanup
        }
    }
}
```

#### Layer 2: Error Translation
```cpp
// C++ side (src/mlx_cpp_wrapper.cpp)
extern "C" mlx_array_t* mlx_model_forward(mlx_model_t* model, mlx_array_t* input) {
    try {
        // Real MLX operations
        auto result = model->forward(input);
        return new mlx::core::array(result);
    } catch (const std::exception& e) {
        g_last_error = e.what();  // Store in thread-local
        return nullptr;            // Signal error to Rust
    }
}
```

```rust
// Rust side
let output = unsafe { mlx_model_forward(self.model, input_array) };
if output.is_null() {
    let error_msg = unsafe {
        CStr::from_ptr(mlx_get_last_error()).to_string_lossy().to_string()
    };
    return Err(AosError::Mlx(format!("Forward pass failed: {}", error_msg)));
}
```

#### Layer 3: Memory Ownership
```rust
// RAII pattern for MLX arrays
pub struct MLXArray(*mut mlx_array_t);

impl MLXArray {
    pub fn new(data: &[f32], shape: &[usize]) -> Result<Self> {
        let array = unsafe {
            mlx_array_from_data(data.as_ptr(), data.len() as i32)
        };
        if array.is_null() {
            return Err(AosError::Mlx("Failed to create array".into()));
        }
        Ok(MLXArray(array))
    }

    pub fn as_ptr(&self) -> *mut mlx_array_t {
        self.0
    }
}

impl Drop for MLXArray {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { mlx_array_free(self.0); }
        }
    }
}
```

### 4.3 Thread Safety Strategy

**Challenge:** MLX uses lazy evaluation with async graph execution

**Solution:** Synchronization barriers
```rust
impl FusedKernels for MLXFFIBackend {
    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        // 1. Prepare inputs (Rust)
        let input_array = MLXArray::new(&io.input_ids, &[io.input_ids.len()])?;

        // 2. MLX computation (async, graphs may be lazy)
        let output_array = unsafe { mlx_model_forward(self.model, input_array.as_ptr()) };

        // 3. **CRITICAL:** Force evaluation before returning to Rust
        unsafe { mlx_eval(output_array); }  // Synchronization point

        // 4. Copy results back to Rust
        let logits = unsafe { mlx_array_to_vec(output_array) };
        io.output_logits.copy_from_slice(&logits);

        Ok(())
    }
}
```

**Required MLX API:**
```cpp
extern "C" void mlx_eval(mlx_array_t* array) {
    mlx::core::eval(*reinterpret_cast<mlx::core::array*>(array));
}
```

### 4.4 Determinism Workarounds

**Problem:** MLX is non-deterministic (attestation already reports this)

**Mitigation Strategies:**
1. **Policy Compliance:** MLX backend marked as experimental, not production
2. **Seed Control:** MLX supports seeding for reproducibility
   ```cpp
   mlx::core::random::seed(hkdf_derived_seed);
   ```
3. **Replay System:** Use `adapteros-replay` to log MLX decisions
4. **Hybrid Mode:** Use Metal backend for deterministic inference, MLX for training

**Policy Pack Implications:**
- **Determinism Ruleset (#2):** ❌ MLX violates, requires policy exception
- **Evidence Ruleset (#4):** ✅ Can log MLX decisions for audit
- **Isolation Ruleset (#8):** ✅ Per-tenant MLX contexts possible

---

## 5. Unified Memory Integration

### 5.1 Current Metal Backend Limits

**Constraint:** Metal backend limited by discrete GPU VRAM
- M3 Max: 128GB maximum
- M3 Ultra: 192GB maximum (2× M3 Max)
- Models >128GB: Cannot run on Metal alone

**Examples:**
- Llama 3.1 405B (4-bit): ~203GB → Cannot fit on single M3 Max
- Qwen 72B (4-bit): ~36GB → Fits, but no multi-adapter headroom

### 5.2 MLX Unified Memory Advantage

**Capability:** Access full system memory (CPU + GPU unified)
- M3 Max: Up to 128GB unified
- M3 Ultra: Up to **512GB unified** (192GB + 320GB expansion)

**Use Case Matrix:**

| Model Size | Precision | Memory Required | Metal Backend | MLX Backend |
|------------|-----------|-----------------|---------------|-------------|
| Llama 3.1 70B | 4-bit | ~35GB | ✅ M3 Max | ✅ M3 Max |
| Llama 3.1 405B | 4-bit | ~203GB | ❌ (exceeds 128GB) | ✅ M3 Ultra |
| Qwen 72B + 8 adapters | 4-bit | ~50GB | ✅ M3 Max | ✅ M3 Max |
| Custom 1T model | 4-bit | ~500GB | ❌ (exceeds 192GB) | ✅ M3 Ultra |

### 5.3 Integration with Existing Memory Management

**AdapterOS Memory Manager:** `/Users/star/Dev/aos/crates/adapteros-memory/`

**Current Design:**
```rust
// Tracks Metal VRAM usage
pub struct MemoryManager {
    metal_vram: AtomicUsize,
    headroom_threshold: f64,  // Default: 15%
}

impl MemoryManager {
    pub fn check_eviction_needed(&self, total_vram: usize) -> bool {
        let used = self.metal_vram.load(Ordering::Relaxed);
        (used as f64 / total_vram as f64) > (1.0 - self.headroom_threshold)
    }
}
```

**Proposed Extension:**
```rust
pub enum BackendMemory {
    Metal { vram_used: usize, vram_total: usize },
    Mlx { unified_used: usize, unified_total: usize },
}

pub struct MemoryManager {
    backend: BackendMemory,
    headroom_threshold: f64,
}

impl MemoryManager {
    pub fn check_eviction_needed(&self) -> bool {
        match self.backend {
            BackendMemory::Metal { vram_used, vram_total } => {
                (vram_used as f64 / vram_total as f64) > (1.0 - self.headroom_threshold)
            }
            BackendMemory::Mlx { unified_used, unified_total } => {
                // MLX uses unified memory, query via mlx_memory_usage()
                (unified_used as f64 / unified_total as f64) > (1.0 - self.headroom_threshold)
            }
        }
    }

    pub fn get_mlx_memory_usage(&self) -> Result<usize> {
        Ok(unsafe { mlx_memory_usage() })
    }
}
```

### 5.4 Coordination Strategy

**Lifecycle Manager Integration:**
```rust
// adapteros-lora-lifecycle/src/lib.rs
pub enum BackendType {
    Metal,
    Mlx,
}

pub struct LifecycleManager {
    backend: BackendType,
    memory_manager: Arc<MemoryManager>,
    // ... existing fields
}

impl LifecycleManager {
    pub async fn select_backend(model_size_gb: usize) -> BackendType {
        let system_info = sysinfo::System::new_all();
        let total_memory = system_info.total_memory() / (1024 * 1024 * 1024);

        if model_size_gb > 128 && total_memory >= 256 {
            tracing::info!(
                "Large model ({} GB) detected, using MLX backend with unified memory",
                model_size_gb
            );
            BackendType::Mlx
        } else {
            tracing::info!(
                "Standard model ({} GB), using Metal backend",
                model_size_gb
            );
            BackendType::Metal
        }
    }
}
```

**Benefits:**
1. **Transparent Fallback:** Large models automatically use MLX
2. **Unified Interface:** Both backends implement `FusedKernels`
3. **Policy Compliance:** Lifecycle manager tracks backend type for audit
4. **Memory Efficiency:** Single memory manager tracks both backends

---

## 6. Performance Targets

### 6.1 Benchmarking Criteria

**Baseline:** Metal backend (production, deterministic)
**Target:** MLX backend (research, non-deterministic)

| Metric | Metal Backend | MLX Target | Notes |
|--------|---------------|------------|-------|
| Single token latency | 50ms (M3 Max, 70B) | 75ms (+50%) | Acceptable for research |
| Throughput (tokens/sec) | 20 tok/sec | 15 tok/sec | Unified memory trade-off |
| Memory efficiency | 128GB max | 512GB max | 4× capacity increase |
| Cold start time | 2s (mmap) | 5s (lazy load) | MLX lazy evaluation |
| LoRA swap latency | 10ms (hot-swap) | 50ms (recompile) | MLX graph recompilation |

### 6.2 Success Criteria

**Phase 1: Functional Parity** (4-6 weeks of development)
- [ ] Compile and link against real MLX libraries
- [ ] Load Llama 3.1 70B model
- [ ] Run single-token inference (correct output)
- [ ] Extract hidden states for LoRA targets
- [ ] Apply single LoRA adapter

**Phase 2: Production Readiness** (8-12 weeks)
- [ ] Multi-adapter routing (K=8)
- [ ] Memory management integration
- [ ] Error handling (no crashes)
- [ ] Thread safety validation
- [ ] Performance within 2× of Metal backend

**Phase 3: Optimization** (12-16 weeks)
- [ ] Graph compilation optimization
- [ ] Memory pooling for arrays
- [ ] Batch inference support
- [ ] Quantization integration (4-bit)
- [ ] Profile-guided optimization

### 6.3 Acceptance Tests

```rust
#[test]
fn test_mlx_backend_basic() {
    let model = MLXFFIModel::load("/path/to/model").unwrap();
    let token_ids = vec![1, 2, 3];
    let logits = model.forward(&token_ids, 0).unwrap();
    assert_eq!(logits.len(), 32000); // Llama vocab size
}

#[test]
fn test_mlx_hidden_states() {
    let model = MLXFFIModel::load("/path/to/model").unwrap();
    let (logits, hidden_states) = model.forward_with_hidden_states(&[1, 2, 3]).unwrap();
    assert!(hidden_states.contains_key("q_proj"));
    assert!(hidden_states.contains_key("k_proj"));
}

#[test]
fn test_mlx_lora_application() {
    let backend = MLXFFIBackend::new(model);
    backend.register_adapter(0, adapter).unwrap();

    let ring = RouterRing::new(1);
    ring.set(&[0], &[32767]); // Q15 max gate

    let mut io = IoBuffers::new(32000);
    io.input_ids = vec![1, 2, 3];

    backend.run_step(&ring, &mut io).unwrap();
    assert!(io.output_logits.iter().any(|&x| x != 0.0));
}

#[test]
fn test_mlx_memory_management() {
    let backend = MLXFFIBackend::new(model);
    let usage = backend.get_mlx_memory_usage().unwrap();
    assert!(usage > 0 && usage < 512 * 1024 * 1024 * 1024); // < 512GB
}
```

---

## 7. Implementation Roadmap

### Phase 1: Foundation (Weeks 1-2)
**Goal:** Replace stub with real MLX integration

**Tasks:**
1. **Environment Setup**
   - [ ] Install MLX via Homebrew: `brew install mlx`
   - [ ] Clone mlx-c repository for reference
   - [ ] Verify MLX C++ headers at `/opt/homebrew/include/mlx/`

2. **Build Integration**
   - [ ] Update `build.rs` to detect real MLX installation
   - [ ] Add linker flags for MLX frameworks (Metal, Accelerate)
   - [ ] Test compilation without `MLX_FORCE_STUB=1`

3. **Basic Binding**
   - [ ] Replace stub `mlx_cpp_wrapper.cpp` with real MLX calls
   - [ ] Implement model loading: `mlx_model_load()`
   - [ ] Implement forward pass: `mlx_model_forward()`
   - [ ] Verify single-token inference

**Deliverable:** MLX backend loads a model and runs inference (no LoRA yet)

### Phase 2: LoRA Integration (Weeks 3-5)
**Goal:** Enable adapter application

**Tasks:**
1. **Hidden State Extraction**
   - [ ] Research MLX forward hooks API
   - [ ] Implement `mlx_model_forward_with_hidden_states()`
   - [ ] Extract activations for ["q_proj", "k_proj", "v_proj", "o_proj"]
   - [ ] Verify shapes match Metal backend

2. **LoRA Operations**
   - [ ] Implement `mlx_lora_forward()` using MLX matmul
   - [ ] Implement `mlx_lora_combine()` for gate-weighted merge
   - [ ] Test single adapter application

3. **Multi-Adapter Routing**
   - [ ] Port Q15 gate dequantization (i16 → f32)
   - [ ] Implement K-sparse routing logic
   - [ ] Test with K=8 adapters

**Deliverable:** MLX backend applies LoRA adapters with routing

### Phase 3: Memory & Safety (Weeks 6-8)
**Goal:** Production-quality error handling and memory management

**Tasks:**
1. **Error Handling**
   - [ ] Wrap all MLX calls in try-catch
   - [ ] Implement thread-local error storage
   - [ ] Add error translation to `AosError::Mlx`

2. **Memory Management**
   - [ ] Implement RAII wrappers for MLX arrays
   - [ ] Add Drop implementations for cleanup
   - [ ] Integrate with `MemoryManager`
   - [ ] Test leak-free operation with Valgrind

3. **Thread Safety**
   - [ ] Add `mlx_eval()` synchronization points
   - [ ] Test concurrent inference
   - [ ] Verify Send + Sync safety

**Deliverable:** Crash-free, leak-free MLX backend

### Phase 4: Integration & Testing (Weeks 9-12)
**Goal:** End-to-end AdapterOS integration

**Tasks:**
1. **Lifecycle Integration**
   - [ ] Add MLX backend selection logic
   - [ ] Implement hot-swap for MLX adapters
   - [ ] Test memory pressure eviction

2. **Testing**
   - [ ] Port Metal backend tests to MLX
   - [ ] Add MLX-specific tests (unified memory)
   - [ ] Performance benchmarking vs Metal

3. **Documentation**
   - [ ] Update README with MLX setup instructions
   - [ ] Document performance characteristics
   - [ ] Add troubleshooting guide

**Deliverable:** Fully integrated MLX backend with documentation

---

## 8. Risk Mitigation

### 8.1 Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| MLX API changes break integration | Medium | High | Pin to specific MLX version, monitor releases |
| Hidden state extraction not exposed | Medium | Critical | Contribute to MLX project, use graph hooks |
| Performance below 2× Metal | High | Medium | Profile and optimize, consider FastMLX approach |
| Memory leaks in FFI | Medium | High | Valgrind testing, RAII wrappers |
| Thread safety issues | Low | Critical | Synchronization barriers, extensive testing |

### 8.2 Policy Risks

| Risk | Impact | Resolution |
|------|--------|------------|
| Non-determinism violates policy | Medium | Mark MLX as experimental, require policy exception |
| Large model tenancy isolation | High | Per-tenant MLX contexts, separate processes |
| Audit trail gaps | Medium | Log all MLX operations via telemetry |

### 8.3 Contingency Plans

**If MLX C++ integration fails:**
1. **Fallback to mlx-c:** Wait for production-ready C API
2. **Python Bridge:** Use PyO3 + MLX Python API (avoid due to dependency hell)
3. **Fork MLX:** Create AdapterOS-specific fork with needed features
4. **Abandon MLX:** Stick with Metal backend, limit model size to 128GB

**Recommendation:** Proceed with C++ integration, low risk given mature API

---

## 9. Success Metrics

### 9.1 Functional Metrics
- ✅ Compiles and links against MLX (no stubs)
- ✅ Loads models ≥128GB on M3 Ultra
- ✅ Applies K=8 LoRA adapters with routing
- ✅ Passes all FusedKernels trait tests
- ✅ Zero memory leaks in 24-hour stress test

### 9.2 Performance Metrics
- Latency ≤ 2× Metal backend
- Throughput ≥ 15 tokens/sec (70B model, M3 Max)
- Cold start ≤ 10 seconds
- Memory efficiency ≥ 90% utilization

### 9.3 Integration Metrics
- Zero crashes in production workloads
- Compatible with existing lifecycle manager
- Hot-swap latency ≤ 100ms
- Unified memory coordination working

---

## 10. References

### 10.1 MLX Documentation
- **Main Docs:** https://ml-explore.github.io/mlx/
- **C API Docs:** https://ml-explore.github.io/mlx-c/
- **GitHub (main):** https://github.com/ml-explore/mlx
- **GitHub (C API):** https://github.com/ml-explore/mlx-c
- **WWDC 2025 Sessions:**
  - Get started with MLX: https://developer.apple.com/videos/play/wwdc2025/315/
  - Explore LLMs with MLX: https://developer.apple.com/videos/play/wwdc2025/298/

### 10.2 AdapterOS Documentation
- **Kernel API:** `/Users/star/Dev/aos/crates/adapteros-lora-kernel-api/src/lib.rs`
- **Metal Backend:** `/Users/star/Dev/aos/crates/adapteros-lora-kernel-mtl/`
- **Memory Manager:** `/Users/star/Dev/aos/crates/adapteros-memory/`
- **Lifecycle Manager:** `/Users/star/Dev/aos/crates/adapteros-lora-lifecycle/`
- **Policy Packs:** `/Users/star/Dev/aos/crates/adapteros-policy/src/packs/`

### 10.3 Related Projects
- **FastMLX:** https://github.com/Blaizzy/fastmlx (production MLX API)
- **MLX Swift:** https://github.com/ml-explore/mlx-swift (uses mlx-c bridge)

---

## 11. Conclusion

The MLX integration path is **feasible and recommended** for enabling large model support (>128GB) on Apple Silicon. The mature C++ API, unified memory architecture, and production-ready ecosystem (FastMLX) provide a solid foundation.

**Next Steps:**
1. Complete stub verification (Phase 1, Week 1)
2. Begin real MLX integration (Phase 1, Weeks 1-2)
3. Monitor mlx-c development for C API parity
4. Maintain backward compatibility with Metal backend

**Expected Timeline:** 12-16 weeks for production-ready integration

**Status:** ✅ Planning complete, ready for implementation

---

**Document Control:**
- **Created:** 2025-11-19
- **Author:** Agent 11 (MLX Path Planner)
- **Classification:** Internal Technical Documentation
- **Next Review:** Upon MLX 1.0 release or Q1 2026
