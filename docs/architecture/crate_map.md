# AdapterOS Crate Architecture Map

**Purpose:** Document how `adapteros-lora-worker` integrates with router, kernel-api, and server crates.

**Last Updated:** 2025-01-18
**Maintained by:** AdapterOS Team

---

## System Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│                  adapteros-server                         │
│  ┌────────────────────────────────────────────────────┐  │
│  │          adapteros-server-api                      │  │
│  │  ┌──────────────────────────────────────────────┐  │  │
│  │  │         AppState (Integration Hub)           │  │  │
│  │  │  • db: Db                                    │  │  │
│  │  │  • worker: Worker<K> (optional, in-process)  │  │  │
│  │  │  • uma_monitor: UmaPressureMonitor           │  │  │
│  │  │  • lifecycle_manager: LifecycleManager       │  │  │
│  │  └──────────────────────────────────────────────┘  │  │
│  │                       ↓                            │  │
│  │              REST Handlers                         │  │
│  │  • /api/chat/completions → Worker::infer()        │  │
│  │  • /api/adapters/swap → HotSwapManager            │  │
│  │  • /api/training/start → MicroLoRATrainer         │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
                           ↓
┌──────────────────────────────────────────────────────────┐
│              adapteros-lora-worker                        │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Worker<K: FusedKernels>                          │  │
│  │    • router: Router (K-sparse selection)          │  │
│  │    • kernels: K (Metal/MLX/Mock backend)          │  │
│  │    • hotswap: HotSwapManager                      │  │
│  │    • lifecycle: LifecycleManager                  │  │
│  │    • generator: Generator (token sampling)        │  │
│  │    • kv_cache: KvCache                            │  │
│  │    • training: MicroLoRATrainer                   │  │
│  └────────────────────────────────────────────────────┘  │
│               ↓                      ↓                    │
│      ┌────────────────┐   ┌────────────────────────┐     │
│      │  Router        │   │  Kernel Backend        │     │
│      │  (routing)     │   │  (GPU execution)       │     │
│      └────────────────┘   └────────────────────────┘     │
└──────────────────────────────────────────────────────────┘
                ↓                          ↓
┌────────────────────────┐   ┌────────────────────────────┐
│ adapteros-lora-router  │   │ adapteros-lora-kernel-api  │
│  • Router              │   │  • FusedKernels (trait)    │
│  • Decision            │   │  • RouterRing              │
│  • CodeFeatures        │   │  • IoBuffers               │
│  • RouterWeights       │   │  • DeterminismReport       │
└────────────────────────┘   └────────────────────────────┘
                                          ↓
                             ┌────────────────────────────┐
                             │ adapteros-lora-kernel-mtl  │
                             │  • MetalKernels            │
                             │  • AdapterWeights          │
                             │  • VramTracker             │
                             │  • RingBuffer              │
                             └────────────────────────────┘
```

---

## Crate Details

### 1. `adapteros-lora-worker` (Core Inference Engine)

**Location:** `crates/adapteros-lora-worker/`
**Purpose:** Main inference orchestration with safety mechanisms, training pipeline, and lifecycle management.

#### Key Public Types

##### **Worker<K: FusedKernels>**
[source: crates/adapteros-lora-worker/src/lib.rs:256-286]

```rust
pub struct Worker<K: FusedKernels + Send + Sync> {
    manifest: ManifestV3,
    policy: PolicyEngine,
    router: Router,                    // From adapteros-lora-router
    kernels: Arc<Mutex<K>>,           // From adapteros-lora-kernel-api
    tokenizer: Arc<QwenTokenizer>,
    generator: Generator,
    kv_cache: KvCache,
    hotswap: HotSwapManager<K>,
    lifecycle: LifecycleManager,
    // Safety mechanisms
    circuit_breaker: CircuitBreaker,
    health_monitor: HealthMonitor,
    telemetry: TelemetryWriter,
}
```

**Core Methods:**
- `Worker::new(manifest, kernels, rag, tokenizer, ...) -> Result<Self>` [L290-419]
- `Worker::infer(&mut self, request: InferenceRequest) -> Result<InferenceResponse>` [L421-637]
- `Worker::propose_patch(...) -> Result<PatchProposalResponse>` [L639-832]
- `Worker::execute_adapter_command(&mut self, cmd: AdapterCommand) -> Result<AdapterCommandResult>` [L834-864]
- `Worker::verify_gpu_integrity(&self) -> Result<()>` [L866-945]

##### **InferenceRequest / InferenceResponse**
[source: crates/adapteros-lora-worker/src/lib.rs:50-91]

```rust
pub struct InferenceRequest {
    pub cpid: String,
    pub prompt: String,
    pub max_tokens: usize,
    pub require_evidence: bool,
    pub request_type: RequestType,  // Normal | PatchProposal
    pub stack_id: Option<String>,
}

pub struct InferenceResponse {
    pub text: Option<String>,
    pub status: String,
    pub trace: ResponseTrace,
    pub refusal: Option<RefusalResponse>,
    pub patch_proposal: Option<PatchProposalResponse>,
}
```

##### **HotSwapManager<K>**
[source: crates/adapteros-lora-worker/src/adapter_hotswap.rs:89-142]

```rust
pub struct HotSwapManager<K: FusedKernels> {
    kernels: Arc<Mutex<K>>,
    staged: HashMap<String, (B3Hash, u64)>,  // Preloaded adapters
    active: HashSet<String>,
    rollback_state: Option<RollbackState>,
}

pub enum AdapterCommand {
    Preload { adapter_id: String, hash: B3Hash },
    Swap { add_ids: Vec<String>, remove_ids: Vec<String> },
    Rollback,
    VerifyStack,
}
```

**Methods:**
- `HotSwapManager::preload(id, hash, vram_mb) -> Result<()>` [L185-223]
- `HotSwapManager::swap(add_ids, remove_ids) -> Result<B3Hash>` [L225-337]
- `HotSwapManager::rollback() -> Result<()>` [L339-384]

##### **Training Pipeline**
[source: crates/adapteros-lora-worker/src/training/trainer.rs:45-87]

```rust
pub struct MicroLoRATrainer {
    config: TrainingConfig,
    tokenizer: Arc<QwenTokenizer>,
}

pub struct TrainingConfig {
    pub rank: usize,              // LoRA rank (4-64)
    pub alpha: f32,               // Scaling factor
    pub learning_rate: f32,
    pub epochs: usize,
    pub batch_size: usize,
}

pub struct TrainingExample {
    pub text: String,
    pub labels: Option<Vec<usize>>,
}

pub struct TrainingResult {
    pub loss: f32,
    pub weights: LoRAWeights,
    pub metrics: HashMap<String, f32>,
}
```

**Methods:**
- `MicroLoRATrainer::train(examples, adapter_id) -> Result<TrainingResult>` [L142-389]

##### **AdapterPackager**
[source: crates/adapteros-lora-worker/src/training/packager.rs:23-37]

```rust
pub struct AdapterPackager {
    output_dir: PathBuf,
}

pub struct PackagedAdapter {
    pub aos_path: PathBuf,
    pub manifest: ManifestV3,
    pub hash: B3Hash,
}
```

**Methods:**
- `AdapterPackager::package(weights, manifest) -> Result<PackagedAdapter>` [L68-187]

##### **Safety Mechanisms**
[source: crates/adapteros-lora-worker/src/timeout.rs:28-51, limiter.rs:34-56, health.rs:42-68]

```rust
// Circuit breaker
pub struct CircuitBreaker {
    state: Arc<Mutex<CircuitState>>,  // Open | Closed | HalfOpen
}

// Resource limiter
pub struct ResourceLimiter {
    limits: ResourceLimits,  // concurrent, tokens/sec, memory, cpu
}

// Health monitor
pub struct HealthMonitor {
    status: Arc<RwLock<HealthStatus>>,  // Healthy | Warning | Critical | Failing
}
```

##### **Memory Monitoring**
[source: crates/adapteros-lora-worker/src/memory.rs:12-44]

```rust
pub struct UmaPressureMonitor {
    min_headroom_pct: u8,
    telemetry: Option<TelemetryWriter>,
}

pub enum MemoryPressureLevel {
    Low,      // <30% usage
    Medium,   // 20-30%
    High,     // 15-20%
    Critical, // <15%
}
```

**Methods:**
- `UmaPressureMonitor::headroom_pct() -> u8` [L46-82]
- `UmaPressureMonitor::check_pressure() -> MemoryPressureLevel` [L84-121]

#### Outbound Dependencies (What Worker Calls)

| Dependency | Purpose | Key Calls |
|------------|---------|-----------|
| **adapteros-lora-router** | K-sparse adapter selection | `Router::route_with_code_features(&features, &adapter_info)` [lib.rs:512-528] |
| **adapteros-lora-kernel-api** | GPU execution interface | `FusedKernels::run_step(&ring, &mut io)` [lib.rs:545-567] |
| **adapteros-lora-kernel-mtl** | Metal backend (if using Metal) | `MetalKernels::load_adapter(id, weights)` [adapter_hotswap.rs:198-215] |
| **adapteros-policy** | Policy validation | `PolicyEngine::validate_inference(&request)` [lib.rs:434-456] |
| **adapteros-lora-rag** | Evidence retrieval | `RagSystem::retrieve_evidence(&query)` [lib.rs:458-489] |
| **adapteros-telemetry** | Event logging | `TelemetryWriter::emit_event(&event)` [lib.rs:578-594] |
| **adapteros-lora-lifecycle** | Adapter state machine | `LifecycleManager::record_router_decision(&selected)` [lib.rs:530-542] |

---

### 2. `adapteros-lora-router` (K-Sparse Adapter Selection)

**Location:** `crates/adapteros-lora-router/`
**Purpose:** Feature-based K-sparse adapter routing with Q15 quantization and telemetry.

#### Key Public Types

##### **Router**
[source: crates/adapteros-lora-router/src/lib.rs:67-104]

```rust
pub struct Router {
    feature_weights: RouterWeights,       // 8 weights (language, framework, etc.)
    k: usize,                             // Top-K selection (max 8)
    tau: f32,                             // Softmax temperature
    eps: f32,                             // Entropy floor

    orthogonal_constraints: Option<OrthogonalConstraints>,  // MPLoRA
    active_stack_name: Option<String>,
    active_stack_adapter_ids: Option<Vec<String>>,
    active_stack_hash: Option<B3Hash>,

    telemetry_writer: Option<RouterDecisionWriter>,
    step_counter: usize,
}
```

**Core Methods:**
- `Router::new(weights, k, tau, eps) -> Self` [L106-135]
- `Router::route(&mut self, features: &[f32], priors: &[f32]) -> Decision` [L137-198]
- `Router::route_with_code_features(&mut self, features, adapter_info) -> Decision` [L200-267]
- `Router::set_active_stack(name, adapter_ids, hash)` [L269-298]
- `Router::set_telemetry_writer(writer)` [L300-315]

##### **Decision**
[source: crates/adapteros-lora-router/src/lib.rs:317-348]

```rust
pub struct Decision {
    pub indices: SmallVec<[u16; 8]>,      // Selected adapter indices
    pub gates_q15: SmallVec<[i16; 8]>,    // Q15 quantized gates
    pub entropy: f32,                     // Shannon entropy
    pub candidates: Vec<DecisionCandidate>,
}

impl Decision {
    pub fn gates_f32(&self) -> Vec<f32>  // Dequantize Q15 -> f32
    pub fn to_router_ring(&self) -> RouterRing  // Convert to kernel format
}
```

##### **CodeFeatures**
[source: crates/adapteros-lora-router/src/features.rs:25-61]

```rust
pub struct CodeFeatures {
    pub lang_one_hot: Vec<f32>,           // 8 dims (Python, Rust, TS, JS, Go, Java, C, C++)
    pub framework_prior: HashMap<String, f32>,  // Django, Flask, React, etc.
    pub symbol_hits: f32,                 // CamelCase/snake_case density
    pub path_tokens: Vec<String>,         // File path components
    pub commit_hint: Option<String>,      // Git commit context
    pub prompt_verb: PromptVerb,          // Explain, Implement, Fix, etc.
    pub attn_entropy: Option<f32>,        // Model uncertainty signal
}

impl CodeFeatures {
    pub fn from_context(context: &str) -> Self  // [L63-145]
    pub fn to_vector(&self) -> Vec<f32>         // [L147-189] (22-dim)
    pub fn to_vector_extended(&self) -> Vec<f32> // [L191-234] (25-dim for MPLoRA)
}
```

##### **RouterWeights**
[source: crates/adapteros-lora-router/src/lib.rs:350-385]

```rust
pub struct RouterWeights {
    pub language_weight: f32,         // 0.27 (default)
    pub framework_weight: f32,        // 0.23
    pub symbol_hits_weight: f32,      // 0.18
    pub path_tokens_weight: f32,      // 0.14
    pub prompt_verb_weight: f32,      // 0.09
    pub orthogonal_weight: f32,       // 0.05 (MPLoRA)
    pub diversity_weight: f32,        // 0.03 (MPLoRA)
    pub similarity_penalty: f32,      // 0.02 (MPLoRA)
}

impl Default for RouterWeights { /* canonical weights */ }
```

##### **ScoringFunction** (Trait)
[source: crates/adapteros-lora-router/src/scoring.rs:18-32]

```rust
pub trait ScoringFunction: Send + Sync {
    fn name(&self) -> &str;
    fn score(&mut self, features: &[f32], priors: &[f32], k: usize,
             tau: f32, eps: f32) -> Decision;
}

// Implementations:
pub struct WeightedScorer { /* default Router logic */ }
pub struct EntropyFloorScorer { /* uniform distribution enforcer */ }
```

#### Inbound Calls (Who Calls Router)

| Caller | Location | Call Pattern |
|--------|----------|--------------|
| **Worker::infer()** | worker/src/lib.rs:512-528 | `router.route_with_code_features(&features, &adapter_info)` |
| **InferencePipeline** | worker/src/inference_pipeline.rs:14 | Direct field access: `self.router.route(...)` |

#### Outbound Dependencies (What Router Calls)

| Dependency | Purpose | Key Calls |
|------------|---------|-----------|
| **adapteros-lora-kernel-api** | Convert to kernel format | `Decision::to_router_ring() -> RouterRing` [lib.rs:340-348] |
| **adapteros-telemetry** | Event emission | `RouterDecisionWriter::write(&decision)` [lib.rs:578-594] |
| **adapteros-core** | HKDF seeding | `derive_seed(&manifest_hash, "router")` (external to Router) |

---

### 3. `adapteros-lora-kernel-api` (Platform-Agnostic Kernel Interface)

**Location:** `crates/adapteros-lora-kernel-api/`
**Purpose:** Trait definitions and shared types for GPU backends (Metal, MLX, Mock).

#### Key Public Types

##### **FusedKernels** (Trait)
[source: crates/adapteros-lora-kernel-api/src/lib.rs:34-89]

```rust
pub trait FusedKernels: Send + Sync {
    // Base execution
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()>;
    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()>;
    fn device_name(&self) -> &str;

    // Determinism attestation
    fn attest_determinism(&self) -> Result<DeterminismReport>;

    // Hot-swap API (optional)
    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()>;
    fn unload_adapter(&mut self, id: u16) -> Result<()>;

    // GPU integrity verification (optional)
    fn verify_adapter_buffers(&self, id: u16) -> Result<(u64, Vec<u8>, Vec<u8>, Vec<u8>)>;
    fn store_gpu_fingerprint(&mut self, id: u16, buffer_size: u64, checkpoint_hash_hex: &str);
    fn verify_gpu_fingerprint(&self, id: u16, buffer_size: u64, checkpoint_hash_hex: &str) -> Result<bool>;
    fn check_memory_footprint(&self, id: u16, buffer_size: u64) -> (bool, f64, Option<(f64, f64, usize)>);
}
```

##### **RouterRing**
[source: crates/adapteros-lora-kernel-api/src/lib.rs:91-112]

```rust
pub struct RouterRing {
    pub indices: [u16; 8],        // Adapter IDs (max K=8)
    pub gates_q15: [i16; 8],      // Q15 quantized gates (fixed-point)
    pub k: usize,                 // Number of active adapters
    pub position: usize,          // Token position
}

impl RouterRing {
    pub fn from_slices(indices: &[u16], gates_q15: &[i16]) -> Self
    pub fn to_metal_buffer(&self) -> Vec<u8>  // Metal-compatible layout
}
```

##### **IoBuffers**
[source: crates/adapteros-lora-kernel-api/src/lib.rs:114-134]

```rust
pub struct IoBuffers {
    pub input_ids: Vec<u32>,           // Token IDs
    pub output_logits: Vec<f32>,       // Vocabulary logits
    pub position: usize,               // Current position
}

impl IoBuffers {
    pub fn new(vocab_size: usize) -> Self
    pub fn reset(&mut self)
}
```

##### **MockKernels** (Test Backend)
[source: crates/adapteros-lora-kernel-api/src/lib.rs:136-198]

```rust
pub struct MockKernels {
    device_name: String,
    loaded: bool,
}

impl FusedKernels for MockKernels {
    // Deterministic test patterns (no actual computation)
    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        // Fill output_logits with deterministic pattern based on ring
    }
}
```

##### **DeterminismReport** (Attestation)
[source: crates/adapteros-lora-kernel-api/src/attestation.rs:18-52]

```rust
pub struct DeterminismReport {
    pub backend_type: BackendType,              // Metal, Mlx, CoreML, Mock
    pub metallib_hash: Option<B3Hash>,          // Metal binary hash
    pub manifest: Option<KernelManifest>,       // Build metadata
    pub rng_seed_method: RngSeedingMethod,      // HkdfSeeded, FixedSeed, SystemEntropy
    pub floating_point_mode: FloatingPointMode, // Deterministic, FastMath, Unknown
    pub compiler_flags: Vec<String>,            // Compilation flags
    pub deterministic: bool,                    // Overall attestation
}

pub enum BackendType {
    Metal,
    Mlx,
    CoreML,
    Mock,
}

impl DeterminismReport {
    pub fn validate(&self) -> Result<()>  // Policy enforcement [L54-89]
    pub fn summary(&self) -> String       // Logging [L91-134]
}
```

#### Inbound Calls (Who Calls FusedKernels)

| Caller | Location | Call Pattern |
|--------|----------|--------------|
| **Worker::infer()** | worker/src/lib.rs:545-567 | `kernels.lock().await.run_step(&ring, &mut io)` |
| **HotSwapManager::preload()** | worker/src/adapter_hotswap.rs:198-215 | `kernels.lock().await.load_adapter(id, weights)` |
| **Worker::verify_gpu_integrity()** | worker/src/lib.rs:866-945 | `kernels.lock().await.verify_adapter_buffers(id)` |
| **backend_factory::create_backend()** | worker/src/backend_factory.rs:45-78 | `backend.attest_determinism()?.validate()` |

#### Implementations

| Implementation | Location | Purpose |
|---------------|----------|---------|
| **MetalKernels** | adapteros-lora-kernel-mtl/src/lib.rs:67-145 | macOS Metal GPU backend |
| **MockKernels** | adapteros-lora-kernel-api/src/lib.rs:136-198 | Testing backend |
| **MlxKernels** | adapteros-lora-mlx-ffi (disabled in alpha) | Python MLX backend (experimental) |

---

### 4. `adapteros-lora-kernel-mtl` (Metal GPU Backend)

**Location:** `crates/adapteros-lora-kernel-mtl/`
**Purpose:** macOS Metal GPU implementation with deterministic execution and hot-swap support.

#### Key Public Types

##### **MetalKernels**
[source: crates/adapteros-lora-kernel-mtl/src/lib.rs:67-145]

```rust
pub struct MetalKernels {
    device: Arc<Device>,                                // Metal device
    library: Option<Library>,                           // Compiled shaders
    mlp_kernel: Option<FusedMlpKernel>,                // MLP fusion
    qkv_kernel: Option<FusedQkvKernel>,                // QKV fusion
    flash_attention_kernel: Option<FlashAttentionKernel>, // Flash attention
    ring_buffer: Option<RingBuffer>,                   // Adapter ring buffer
    vram_tracker: VramTracker,                         // VRAM attribution
    debugger: KernelDebugger,                          // Debug utilities
    recovery: RecoveryWrapper,                         // Error recovery
    noise_tracker: NoiseTracker,                       // Numerical stability
    embedding_buffer: Option<Buffer>,                  // Token embeddings
    transformer_weights: Option<TransformerWeights>,   // Base model weights
    lm_head_weights: Option<LmHeadWeights>,           // Vocab projection
    adapter_weights: HashMap<u16, AdapterWeights>,     // Hot-swappable LoRA weights
}

impl FusedKernels for MetalKernels { /* ... */ }
```

**Core Methods:**
- `MetalKernels::new() -> Result<Self>` [L147-234]
- `MetalKernels::load_library(&mut self) -> Result<()>` [L236-298] (validates metallib hash)

##### **AdapterWeights**
[source: crates/adapteros-lora-kernel-mtl/src/lib.rs:300-345]

```rust
pub struct AdapterWeights {
    pub lora_a_buffers: Vec<Buffer>,  // LoRA A matrices [rank × in_dim]
    pub lora_b_buffers: Vec<Buffer>,  // LoRA B matrices [out_dim × rank]
    pub rank: usize,                  // LoRA rank (4-64)
    pub alpha: f32,                   // Scaling factor
    pub vram_bytes: u64,              // VRAM usage
    pub hash_b3: B3Hash,              // Integrity hash
}

impl AdapterWeights {
    pub fn scaling_factor(&self) -> f32 { self.alpha / (self.rank as f32) }
}
```

##### **VramTracker**
[source: crates/adapteros-lora-kernel-mtl/src/vram.rs:23-67]

```rust
pub struct VramTracker {
    allocations: Arc<RwLock<HashMap<u32, u64>>>,  // adapter_id -> bytes
    fingerprints: Arc<RwLock<HashMap<u32, GpuBufferFingerprint>>>, // integrity
    baselines: Arc<RwLock<HashMap<u32, MemoryFootprintBaseline>>>, // anomaly detection
}

pub struct GpuBufferFingerprint {
    pub buffer_bytes: u64,
    pub allocated_at: u64,
    pub checkpoint_hash: B3Hash,  // Hash of first/last/mid 4KB samples
}

impl GpuBufferFingerprint {
    pub fn new(buffer_bytes: u64, first_sample: &[u8], last_sample: &[u8], mid_sample: &[u8]) -> Self
    pub fn matches(&self, other: &Self) -> bool  // Integrity verification
}
```

**Methods:**
- `VramTracker::track_allocation(id, bytes)` [vram.rs:69-95]
- `VramTracker::store_fingerprint(id, fingerprint)` [vram.rs:97-123]
- `VramTracker::verify_fingerprint(id, expected_hash) -> bool` [vram.rs:125-156]

##### **RingBuffer**
[source: crates/adapteros-lora-kernel-mtl/src/ring_buffer.rs:18-62]

```rust
pub struct RingBuffer {
    top_k: usize,                    // Max adapters (≤8)
    adapter_indices: Vec<u32>,       // Active adapter IDs
    gates: Vec<u16>,                 // Q15 gates
    buffer: Option<Buffer>,          // Metal buffer
}

pub struct ActiveAdapter {
    pub id: u32,
    pub gate_q15: i16,
}

impl RingBuffer {
    pub fn new(device: Arc<Device>, top_k: usize) -> Result<Self>
    pub fn update(&mut self, adapters: &[ActiveAdapter]) -> Result<()>
    pub fn get_buffer(&self) -> Option<&Buffer>
}
```

#### Hot-Swap Implementation
[source: crates/adapteros-lora-kernel-mtl/src/lib.rs:447-589]

```rust
impl FusedKernels for MetalKernels {
    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
        // 1. Parse SafeTensors format
        let tensors = SafeTensors::deserialize(weights)?;

        // 2. Extract LoRA A/B matrices for target modules
        let target_modules = vec!["q_proj", "k_proj", "v_proj", "mlp.down_proj", "mlp.up_proj"];

        // 3. Create Metal buffers and upload to VRAM
        for module in &target_modules {
            let a_buffer = self.device.new_buffer_with_data(/* ... */);
            let b_buffer = self.device.new_buffer_with_data(/* ... */);
        }

        // 4. Store in adapter_weights HashMap
        self.adapter_weights.insert(id, AdapterWeights { /* ... */ });

        // 5. Track VRAM allocation
        self.vram_tracker.track_allocation(id, vram_bytes);

        Ok(())
    }
}
```

#### Deterministic Compilation
[source: crates/adapteros-lora-kernel-mtl/src/lib.rs:236-298]

```rust
const METALLIB_BYTES: &[u8] = include_bytes!("../shaders/aos_kernels.metallib");
const METALLIB_HASH: &str = include_str!("../shaders/kernel_hash.txt");

impl MetalKernels {
    fn load_library(&mut self) -> Result<()> {
        // 1. Hash verification
        let actual_hash = B3Hash::hash(METALLIB_BYTES);
        let expected_hash = B3Hash::from_hex(METALLIB_HASH.trim())?;

        if actual_hash != expected_hash {
            return Err(AosError::DeterminismViolation("Metallib hash mismatch"));
        }

        // 2. Load precompiled library
        self.library = Some(self.device.new_library_with_data(METALLIB_BYTES)?);

        Ok(())
    }
}
```

#### GPU Integrity Verification
[source: crates/adapteros-lora-kernel-mtl/src/lib.rs:591-678]

```rust
impl FusedKernels for MetalKernels {
    fn verify_adapter_buffers(&self, id: u16) -> Result<(u64, Vec<u8>, Vec<u8>, Vec<u8>)> {
        let adapter_weights = self.adapter_weights.get(&id)?;
        let first_buffer = adapter_weights.lora_a_buffers.first()?;

        // Sample first 4KB, last 4KB, midpoint 4KB (no full readback)
        let ptr = first_buffer.contents() as *const u8;
        let buffer_slice = unsafe { std::slice::from_raw_parts(ptr, buffer_bytes) };

        let first_sample = buffer_slice[..4096].to_vec();
        let last_sample = buffer_slice[buffer_bytes-4096..].to_vec();
        let mid_sample = buffer_slice[mid_start..mid_end].to_vec();

        Ok((buffer_bytes, first_sample, last_sample, mid_sample))
    }
}
```

#### Inbound Calls (Who Calls MetalKernels)

| Caller | Location | Call Pattern |
|--------|----------|--------------|
| **backend_factory::create_backend()** | worker/src/backend_factory.rs:45-78 | `MetalKernels::new()` + attestation |
| **Worker::infer()** | worker/src/lib.rs:545-567 | `kernels.run_step(&ring, &mut io)` (via trait) |
| **HotSwapManager::preload()** | worker/src/adapter_hotswap.rs:198-215 | `kernels.load_adapter(id, weights)` (via trait) |

---

### 5. `adapteros-server` + `adapteros-server-api` (Control Plane)

**Location:** `crates/adapteros-server/`, `crates/adapteros-server-api/`
**Purpose:** REST API server for adapter management, inference, training, and monitoring.

#### Dependency Chain

```
adapteros-server (binary) → adapteros-server-api (handlers) → adapteros-lora-worker
```

[source: crates/adapteros-server/Cargo.toml:16, crates/adapteros-server-api/Cargo.toml:15]

#### Key Public Types

##### **AppState** (Integration Hub)
[source: crates/adapteros-server-api/src/state.rs:24-94]

```rust
pub struct AppState {
    pub db: Db,
    pub jwt_secret: Arc<Vec<u8>>,
    pub config: Arc<RwLock<ApiConfig>>,
    pub metrics_exporter: Arc<MetricsExporter>,
    pub training_service: Arc<TrainingService>,

    // Worker integration
    pub lifecycle_manager: Option<Arc<Mutex<LifecycleManager>>>,
    pub worker: Option<Arc<Mutex<Worker<Box<dyn FusedKernels + Send + Sync>>>>>,
    pub uma_monitor: Arc<UmaPressureMonitor>,

    // ... other fields
}

impl AppState {
    pub fn with_worker(mut self, worker: Worker<Box<dyn FusedKernels + Send + Sync>>) -> Self {
        self.worker = Some(Arc::new(Mutex::new(worker)));
        self
    }
}
```

##### **Worker API Types**
[source: crates/adapteros-server-api/src/types.rs:829-854]

```rust
pub struct WorkerInferRequest {
    pub cpid: String,
    pub prompt: String,
    pub max_tokens: usize,
    pub require_evidence: bool,
}

pub struct WorkerInferResponse {
    pub text: Option<String>,
    pub status: String,
    pub trace: WorkerTrace,
}

pub struct WorkerTrace {
    pub router_summary: RouterSummary,
}

pub struct RouterSummary {
    pub adapters_used: Vec<String>,
}
```

##### **UdsClient** (Remote Worker Communication)
[source: crates/adapteros-server-api/src/uds_client.rs:32-138]

```rust
pub struct UdsClient {
    timeout: Duration,
}

impl UdsClient {
    pub fn new(timeout: Duration) -> Self

    pub async fn infer(&self, uds_path: &Path, request: WorkerInferRequest) -> Result<WorkerInferResponse>
    pub async fn health(&self, uds_path: &Path) -> Result<String>
}
```

**Usage Pattern:**
```rust
let uds_client = UdsClient::new(Duration::from_secs(30));
let response = uds_client.infer(&uds_path, worker_request).await?;
```

#### Integration Patterns

##### Pattern A: In-Process Worker Access
[source: crates/adapteros-server-api/src/handlers/adapter_stacks.rs:384-437]

```rust
// Handler accesses worker directly when embedded
if let Some(worker_arc) = &state.worker {
    let mut worker = worker_arc.lock().await;

    // Direct method calls:
    worker.hotswap.swap(&add_ids, &remove_ids)?;
    worker.kv_cache.zeroize_all()?;
    worker.telemetry.emit_custom(...)?;
    *worker.last_stack_hash.write() = Some(new_hash);
}
```

##### Pattern B: UDS-Based Worker Communication
[source: crates/adapteros-server-api/src/handlers/batch.rs:45-170]

```rust
// Handler proxies to remote worker via UDS
let workers = state.db.list_all_workers().await?;
let worker = &workers[0];
let uds_path = PathBuf::from(&worker.uds_path);

let uds_client = UdsClient::new(WORKER_TIMEOUT);
let worker_response = uds_client.infer(uds_path.as_path(), worker_request).await?;
```

#### Key API Endpoints

| Endpoint | Method | Handler | Worker Integration |
|----------|--------|---------|-------------------|
| `/api/chat/completions` | POST | handlers/streaming.rs:45-234 | `worker.infer(request)` or UDS proxy |
| `/api/adapters/swap` | POST | handlers/adapter_stacks.rs:384-437 | `worker.hotswap.swap(...)` (direct) |
| `/api/training/start` | POST | handlers/training.rs:67-189 | Creates `MicroLoRATrainer` instance |
| `/api/batch/infer` | POST | handlers/batch.rs:45-170 | UDS proxy to registered workers |
| `/v1/system/memory` | GET | handlers.rs:813-836 | `uma_monitor.headroom_pct()` |

#### Server Main Entry Point
[source: crates/adapteros-server/src/main.rs:89-745]

**Worker Initialization:**
```rust
// 1. Create backend
let backend = backend_factory::create_backend(BackendChoice::Metal)?;

// 2. Create Worker
let worker = Worker::new(manifest, backend, rag, tokenizer, ...)?;

// 3. Inject into AppState
let app_state = AppState::new(db, config, ...)
    .with_worker(worker);

// 4. Start UMA pressure monitoring (background task)
let uma_monitor = UmaPressureMonitor::new(min_headroom_pct, telemetry);
uma_monitor.start_polling();

// 5. Start server
Server::bind(&addr).serve(app.into_make_service()).await?;
```

**Background Tasks:**
```rust
// TTL cleanup (every 5 minutes)
spawn_deterministic("TTL cleanup".to_string(), async move {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        interval.tick().await;
        if let Ok(expired) = db.find_expired_adapters().await {
            for adapter in expired {
                let _ = db.delete_adapter(&adapter.adapter_id).await;
            }
        }
    }
});

// Heartbeat recovery (every 5 minutes)
spawn_deterministic("Heartbeat recovery".to_string(), async move {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        interval.tick().await;
        let _ = db.recover_stale_adapters(300).await;
    }
});
```

---

## Data Flow Examples

### Example 1: Inference Request Flow

```
1. HTTP Request
   POST /api/chat/completions
   { "prompt": "Explain async Rust", "max_tokens": 100 }

2. Handler (streaming.rs:45-234)
   • Extract request
   • Check auth/RBAC
   • Build InferenceRequest

3. AppState Worker Access
   state.worker.lock().await

4. Worker::infer() (worker/lib.rs:421-637)
   • Extract CodeFeatures::from_context(prompt)
   • router.route_with_code_features(&features, &adapter_info)

5. Router::route_with_code_features() (router/lib.rs:200-267)
   • Compute per-adapter scores
   • Top-K selection + softmax
   • Q15 quantization
   • Return Decision { indices, gates_q15, entropy }

6. Worker converts Decision → RouterRing
   decision.to_router_ring()

7. Kernel Execution (kernel-mtl/lib.rs:447-589)
   kernels.lock().await.run_step(&ring, &mut io)
   • Ring buffer update
   • Metal shader dispatch
   • LoRA weight fusion

8. Token Sampling
   generator.sample(&io.output_logits, temperature)

9. HTTP Response
   SSE stream: data: {"choices":[{"delta":{"content":"Async"}}]}
```

### Example 2: Hot-Swap Flow

```
1. HTTP Request
   POST /api/adapters/swap
   { "add_ids": ["adapter-v2"], "remove_ids": ["adapter-v1"] }

2. Handler (adapter_stacks.rs:384-437)
   • Acquire worker lock
   • Build AdapterCommand::Swap

3. Worker::execute_adapter_command() (worker/lib.rs:834-864)
   hotswap.swap(&add_ids, &remove_ids)

4. HotSwapManager::swap() (worker/adapter_hotswap.rs:225-337)
   • Preload new adapter (if not staged)
   • Acquire mutex lock
   • Atomic pointer flip (Arc::clone + replace)
   • Verify stack hash
   • Update router active_stack
   • Release mutex

5. Kernel Hot-Swap (kernel-mtl/lib.rs:447-589)
   kernels.load_adapter(id, weights)
   • Parse SafeTensors
   • Create Metal buffers
   • Upload to VRAM
   • Store in adapter_weights HashMap
   • Track VRAM allocation

6. GPU Integrity Verification (kernel-mtl/lib.rs:591-678)
   kernels.verify_adapter_buffers(id)
   • Sample first/last/mid 4KB
   • BLAKE3 checkpoint hash
   • Store fingerprint

7. HTTP Response
   { "success": true, "stack_hash": "b3:abc123...", "vram_delta_mb": 150 }
```

### Example 3: Training Pipeline Flow

```
1. HTTP Request
   POST /api/training/start
   { "dataset_id": "ds-123", "config": { "rank": 16, "alpha": 32, "epochs": 3 } }

2. Handler (training.rs:67-189)
   • Validate dataset exists
   • Create TrainingConfig
   • Build TrainingExample list

3. MicroLoRATrainer::train() (worker/training/trainer.rs:142-389)
   • Initialize LoRA A/B matrices (Kaiming uniform)
   • For each epoch:
     - Forward pass (base model + LoRA)
     - Loss computation (cross-entropy)
     - Backward pass (gradient descent)
     - Weight updates
   • Return TrainingResult { loss, weights, metrics }

4. LoRAQuantizer::quantize() (worker/training/quantizer.rs:45-178)
   • Convert f32 → Q15 fixed-point
   • Store in QuantizedLoRAWeights

5. AdapterPackager::package() (worker/training/packager.rs:68-187)
   • Create ManifestV3 (rank, alpha, modules)
   • Serialize weights to SafeTensors
   • Build .aos archive:
     [0-3]   manifest_offset (u32 LE)
     [4-7]   manifest_len (u32 LE)
     [offset] manifest (JSON)
     [offset] weights (safetensors)
   • Compute BLAKE3 hash
   • Return PackagedAdapter { aos_path, manifest, hash }

6. Registry Integration
   db.register_adapter(adapter_id, hash, tier, rank, acl)

7. HTTP Response
   { "job_id": "job-456", "adapter_id": "adapter-789", "hash": "b3:def456..." }
```

---

## Key Architectural Patterns

### 1. Trait-Based Backend Polymorphism
- **FusedKernels** trait enables Metal/MLX/Mock backends
- Worker is generic: `Worker<K: FusedKernels>`
- Runtime backend selection via `backend_factory::create_backend()`

### 2. Zero-Copy GPU Execution
- Embedding lookup in Metal shader (not Rust → GPU copy)
- Direct SafeTensors → Metal buffer upload
- Memory-mapped `.aos` archives

### 3. Determinism Enforcement
- Build-time: Embedded metallib hash verification
- Runtime: `attest_determinism()` + policy validation
- HKDF-seeded RNG for all randomness
- Q15 quantization for fixed-point determinism

### 4. Hot-Swap Safety
- RCU-style retirement: Atomic Arc swaps + refcount-deferred unload
- GPU fingerprint verification (BLAKE3 checkpoint hashing)
- Automatic rollback on verification failure
- Tested: Loom (5000+ interleavings), Miri (UB scan), Stress (1000 swaps + 1000 infers)

### 5. Separation of Concerns
- **Router**: Feature extraction + K-sparse selection (no GPU)
- **Kernels**: GPU execution + hot-swap (no routing logic)
- **Worker**: Orchestration + safety mechanisms (no low-level GPU)
- **Server**: REST API + auth/RBAC (no ML logic)

### 6. Telemetry & Observability
- Non-blocking event emission (channel-based)
- Per-step router decisions logged (first 128 tokens)
- UMA pressure monitoring (5s polling)
- Adapter lifecycle state tracking (Unloaded → Resident)

---

## Testing Strategy

### Unit Tests
- **MockKernels** - Deterministic test backend (no GPU required)
- **Router** - Property tests (uniqueness, normalization, entropy)
- **Hot-swap** - Preload, swap, rollback, verify logic

### Integration Tests
- **Worker + Metal** - Full inference pipeline
- **Worker + Mock** - Determinism validation
- **Server + UDS** - Remote worker communication

### Concurrency Tests
- **Loom** - Hot-swap + concurrent inference (5000+ interleavings)
- **Miri** - UB detection in unsafe Metal FFI
- **Stress** - 1000 swaps + 1000 concurrent infers (0 panics, <1% latency regression)

---

## File Location Index

### Core Crates

```
crates/
├── adapteros-lora-worker/
│   ├── src/
│   │   ├── lib.rs                   (Worker struct, infer(), L256-1259)
│   │   ├── adapter_hotswap.rs       (HotSwapManager, L89-515)
│   │   ├── backend_factory.rs       (create_backend, L23-95)
│   │   ├── memory.rs                (UmaPressureMonitor, L12-121)
│   │   ├── timeout.rs               (CircuitBreaker, L28-189)
│   │   ├── limiter.rs               (ResourceLimiter, L34-234)
│   │   ├── health.rs                (HealthMonitor, L42-289)
│   │   ├── training/
│   │   │   ├── trainer.rs           (MicroLoRATrainer, L45-389)
│   │   │   ├── packager.rs          (AdapterPackager, L23-187)
│   │   │   └── quantizer.rs         (LoRAQuantizer, L23-178)
│   │   └── ...
│   └── Cargo.toml
│
├── adapteros-lora-router/
│   ├── src/
│   │   ├── lib.rs                   (Router, Decision, L67-594)
│   │   ├── features.rs              (CodeFeatures, L25-234)
│   │   ├── scoring.rs               (ScoringFunction trait, L18-156)
│   │   └── calibration.rs           (Calibrator, L28-267)
│   └── Cargo.toml
│
├── adapteros-lora-kernel-api/
│   ├── src/
│   │   ├── lib.rs                   (FusedKernels, RouterRing, IoBuffers, L34-198)
│   │   └── attestation.rs           (DeterminismReport, L18-134)
│   └── Cargo.toml
│
├── adapteros-lora-kernel-mtl/
│   ├── src/
│   │   ├── lib.rs                   (MetalKernels, L67-678)
│   │   ├── vram.rs                  (VramTracker, GpuBufferFingerprint, L23-156)
│   │   ├── ring_buffer.rs           (RingBuffer, L18-123)
│   │   └── ...
│   ├── shaders/
│   │   ├── aos_kernels.metallib     (Precompiled Metal binary)
│   │   └── kernel_hash.txt          (BLAKE3 hash)
│   └── Cargo.toml
│
├── adapteros-server/
│   ├── src/
│   │   └── main.rs                  (Server entry point, L89-745)
│   └── Cargo.toml
│
└── adapteros-server-api/
    ├── src/
    │   ├── state.rs                 (AppState, L24-137)
    │   ├── types.rs                 (WorkerInferRequest/Response, L829-854)
    │   ├── uds_client.rs            (UdsClient, L32-138)
    │   └── handlers/
    │       ├── streaming.rs         (Chat completions, L45-234)
    │       ├── adapter_stacks.rs    (Hot-swap, L384-437)
    │       ├── batch.rs             (Batch inference, L45-170)
    │       └── training.rs          (Training API, L67-189)
    └── Cargo.toml
```

---

## References

- [CLAUDE.md](../../CLAUDE.md) - AdapterOS Developer Guide
- [ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) - Full architecture documentation
- [CONTRIBUTING.md](../../CONTRIBUTING.md) - PR guidelines
- [Metal Programming Guide](https://developer.apple.com/metal/) - GPU programming reference
- [MPLoRA Paper](https://openreview.net/pdf?id=jqz6Msm3AF) - Multi-LoRA architecture

---

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
