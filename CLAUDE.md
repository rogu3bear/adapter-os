# AdapterOS Developer Guide

**Purpose:** Developer-focused guide with code examples, architecture patterns, and coding standards.
**For contribution process:** See [CONTRIBUTING.md](CONTRIBUTING.md)
**Last Updated:** 2025-11-15

---

## Table of Contents

- [Code Standards](#code-standards)
- [Error Handling](#error-handling)
- [Logging](#logging)
- [Policy Packs](#policy-packs)
- [Architecture Patterns](#architecture-patterns)
- [Common Patterns](#common-patterns)
- [Integration Testing Patterns](#integration-testing-patterns)
- [Anti-Patterns to Avoid](#anti-patterns-to-avoid)
- [Key Subsystems](#key-subsystems)
- [Streaming Architecture](#streaming-architecture)
- [Document Processing Workflow](#document-processing-workflow)
- [Citation Standards](#citation-standards)
- [Quick Reference](#quick-reference)
- [References](#references)

---

## Code Standards

### Rust Style

```rust
// ✅ GOOD: Use cargo fmt for formatting
// Run: cargo fmt --all

// Use standard Rust naming conventions:
// - Types: PascalCase
// - Functions: snake_case
// - Constants: SCREAMING_SNAKE_CASE
// - Modules: snake_case
```

### Linting

```bash
# Always run clippy before committing
cargo clippy --workspace -- -D warnings

# Check for unused dependencies
cargo udeps
```

### Documentation

```rust
// ✅ GOOD: Document all public APIs
/// Loads an adapter from the specified path.
///
/// # Arguments
/// * `path` - Path to the adapter file
///
/// # Errors
/// Returns `AosError::NotFound` if the file doesn't exist.
/// Returns `AosError::InvalidManifest` if the manifest is malformed.
///
/// # Example
/// ```no_run
/// use adapteros_lora_lifecycle::AdapterLoader;
/// let loader = AdapterLoader::new();
/// let adapter = loader.load_from_path("./adapters/my_adapter.aos").await?;
/// ```
pub async fn load_from_path(path: &Path) -> Result<Adapter> {
    // Implementation
}
```

---

## Error Handling

### Error Type: `AosError`

All errors use the `AosError` enum from `adapteros-core`:

```rust
use adapteros_core::{AosError, Result};

// ✅ GOOD: Use Result<T> for error handling
pub async fn process_request(&self, req: Request) -> Result<Response> {
    let data = load_data(&req.id)
        .await
        .map_err(|e| AosError::NotFound(format!("Failed to load {}: {}", req.id, e)))?;
    
    Ok(Response::new(data))
}

// ❌ BAD: Using Option<T> for errors
pub fn get_value(&self, key: &str) -> Option<String> {
    // Should return Result<String, AosError>
}
```

### Error Propagation

```rust
// ✅ GOOD: Proper error propagation
use adapteros_core::Result;

pub async fn complex_operation(&self) -> Result<()> {
    // Use ? operator for error propagation
    let config = load_config().await?;
    let data = process_data(&config).await?;
    validate_data(&data)?;
    
    Ok(())
}

// ✅ GOOD: Adding context to errors
pub async fn load_adapter(path: &Path) -> Result<Adapter> {
    let bytes = std::fs::read(path)
        .map_err(|e| AosError::Io(format!("Failed to read {}: {}", path.display(), e)))?;
    
    // Further processing...
}
```

### Error Variants

Common error variants in `AosError`:

```rust
// Domain-specific errors
AosError::PolicyViolation("reason")          // Policy enforcement
AosError::DeterminismViolation("reason")    // Non-deterministic behavior
AosError::EgressViolation("reason")         // Network egress blocked
AosError::IsolationViolation("reason")     // Tenant isolation
AosError::Validation("reason")              // Input validation
AosError::Config("reason")                  // Configuration errors

// Infrastructure errors
AosError::Io("reason")                      // I/O errors
AosError::Database("reason")                // Database errors
AosError::Crypto("reason")                  // Cryptographic errors
AosError::Network("reason")                 // Network errors
```

---

## Logging

### Use `tracing` (Not `println!`)

```rust
use tracing::{info, warn, error, debug, trace};

// ✅ GOOD: Structured logging with tracing
pub async fn process_request(&self, req: Request) -> Result<Response> {
    info!(request_id = %req.id, "Processing request");
    
    let result = self.handle(req).await?;
    
    info!(
        request_id = %req.id,
        duration_ms = ?result.duration,
        "Request processed successfully"
    );
    
    Ok(result)
}

// ❌ BAD: Using println! for logging
pub fn log_event(&self, event: &str) {
    println!("Event: {}", event); // DON'T DO THIS
}
```

### Log Levels

```rust
// Use appropriate log levels
trace!("Detailed debugging information");
debug!("Debug information for development");
info!("General informational messages");
warn!("Warning messages that may need attention");
error!("Error messages that require action");
```

### Structured Fields

```rust
// ✅ GOOD: Use structured fields for querying
info!(
    tenant_id = %tenant.id,
    adapter_id = %adapter.id,
    request_size = req.len(),
    "Loading adapter for tenant"
);

// Fields can be queried in log aggregation systems
```

---

## Policy Packs

AdapterOS enforces 23 canonical policy packs. All code must comply with these policies.

### Core Policy Packs

1. **Egress Policy** - Zero network egress during inference
   ```rust
   // Production mode enforces UDS-only serving
   if cfg.server.production_mode {
       if cfg.server.uds_socket.is_none() {
           return Err(AosError::Config(
               "Production mode requires uds_socket".to_string()
           ));
       }
   }
   ```

2. **Determinism Policy** - Reproducible execution
   ```rust
   // All randomness must be seeded
   use adapteros_deterministic_exec::GlobalSeed;
   let seed = GlobalSeed::get_or_init(seed_hash);
   let mut rng = seed.rng();
   ```

3. **Router Policy** - K-sparse LoRA routing with Q15 gates
   ```rust
   // Router uses Q15 quantized gates
   let gate_value = quantize_to_q15(feature_value);
   ```

4. **Evidence Policy** - Audit trail for policy decisions with quality thresholds
   ```rust
   use adapteros_policy::packs::evidence::{
       EvidencePolicy, EvidenceConfig, EvidenceSpan, QualityThresholds
   };

   let config = EvidenceConfig {
       require_open_book: true,
       min_spans: 2,
       quality_thresholds: QualityThresholds {
           min_relevance: 0.75,
           min_confidence: 0.85,
           min_recency_days: 0,
           max_age_days: 365,
       },
       ..Default::default()
   };

   let policy = EvidencePolicy::new(config);
   policy.validate_evidence_spans(&spans)?;
   ```

   **Evidence Types:**
   - `CodeDoc` - Code documentation
   - `ApiDoc` - API specifications
   - `TestCase` - Test case evidence
   - `SecurityAudit` - Security audit reports
   - `Config` - Configuration files

   **Source Requirements:**
   - Optional signature verification
   - Domain allowlist/blocklist
   - Version tracking
   - Timestamp validation

5. **Telemetry Policy** - Structured event logging
   ```rust
   // All events logged as canonical JSON
   telemetry.log_event("event_type", metadata).await?;
   ```

6. **Naming Policy** - Adapter and stack naming conventions
   ```rust
   use adapteros_policy::packs::naming_policy::{
       NamingPolicy, NamingConfig, AdapterNameValidation
   };
   use adapteros_core::AdapterName;

   // Validate adapter name
   let policy = NamingPolicy::new(NamingConfig::default());
   let request = AdapterNameValidation {
       name: "tenant-a/engineering/code-review/r001".to_string(),
       tenant_id: "tenant-a".to_string(),
       parent_name: None,
       latest_revision: None,
   };
   policy.validate_adapter_name(&request)?;

   // Parse semantic name
   let name = AdapterName::parse("shop-floor/hydraulics/troubleshooting/r042")?;
   println!("Display: {}", name.display_name()); // "shop-floor/hydraulics/troubleshooting (rev 42)"
   ```

   **Naming Format:**
   - Adapters: `{tenant}/{domain}/{purpose}/{revision}` (e.g., `shop-floor/hydraulics/troubleshooting/r042`)
   - Stacks: `stack.{namespace}[.{identifier}]` (e.g., `stack.production-env`)
   - Reserved tenants: `system`, `admin`, `root`, `default`, `test`
   - Reserved domains: `core`, `internal`, `deprecated`
   - Max revision gap: 5 (prevents accidental large jumps)

### Policy Compliance Checklist

- [ ] No network egress in production (UDS-only)
- [ ] All randomness is seeded and deterministic
- [ ] Router uses Q15 quantization
- [ ] Evidence tracked for policy decisions
- [ ] Telemetry events use canonical JSON
- [ ] Semantic names follow `{tenant}/{domain}/{purpose}/{revision}` format
- [ ] Reserved namespaces not used in production adapters
- [ ] Revision numbers are monotonically increasing
- [ ] Fork types specified when creating child adapters
- [ ] Input validation on all user inputs
- [ ] Tenant isolation enforced
- [ ] Error handling with typed errors

See `crates/adapteros-policy/src/packs/` for complete policy implementations.

---

## Architecture Patterns

### K-Sparse LoRA Routing

```rust
// Router selects top K adapters using Q15 quantized gates
use adapteros_lora_router::{Router, RouterRequest};

let router = Router::new(config);
let request = RouterRequest {
    prompt_tokens: tokens,
    model_id: model.id.clone(),
    tenant_id: tenant.id.clone(),
};

// Returns top K adapters (typically K=3)
let selected = router.select_adapters(request, k_sparse: 3).await?;
```

### Metal Kernel Pattern

```rust
// Metal kernels use deterministic compilation
use adapteros_lora_kernel_mtl::{FusedKernels, KernelParams};

let kernels = FusedKernels::load_from_metallib("./target/kernels.metallib")?;

let params = KernelParams {
    hidden_size: 4096,
    seq_len: 128,
    // ... other parameters
};

// Kernels are precompiled for deterministic execution
kernels.execute(&params, buffers)?;
```

### Configuration Pattern

```rust
// Configuration uses precedence rules
use adapteros_config::{Config, ConfigSource};

// Precedence: CLI > Environment > Config File > Defaults
let config = Config::load()
    .with_file("configs/cp.toml")?
    .with_env()?
    .with_cli(&args)?
    .build()?;
```

### Memory Management Pattern

```rust
// Adapter eviction maintains ≥15% headroom
use adapteros_memory::{MemoryManager, EvictionPolicy};

let memory = MemoryManager::new(
    EvictionPolicy::default()
        .with_min_headroom_pct(15)
        .with_evict_order(["ephemeral_ttl", "cold_lru"])
);

// Automatically evicts adapters when memory pressure detected
memory.ensure_headroom().await?;
```

### Adapter Lifecycle Pattern

**Location:** `crates/adapteros-lora-lifecycle/src/lib.rs`

AdapterOS manages adapter memory through a state machine:

```
Unloaded → Cold → Warm → Hot → Resident
    ↑                              ↓
    └──────── (eviction) ──────────┘
```

**State Definitions:**
- **Unloaded**: Not in memory
- **Cold**: Loaded but rarely used
- **Warm**: Moderate usage
- **Hot**: Frequently used
- **Resident**: Pinned in memory (cannot be evicted)

```rust
use adapteros_lora_lifecycle::{LifecycleManager, AdapterState};

// Create lifecycle manager
let manager = LifecycleManager::new(
    adapter_names,
    &policies,
    adapters_base_path,
    telemetry,
    initial_k,
);

// Promote adapter through states
manager.promote_adapter(adapter_id)?;  // Unloaded → Cold

// Pin adapter to prevent eviction
manager.pin_adapter(adapter_id)?;      // → Resident

// Record router decisions for activation tracking
manager.record_router_decision(&[adapter_0, adapter_1]).await?;

// Auto-evict low-activation adapters
manager.check_memory_pressure(total_memory, threshold).await?;
```

### Hot-Swap Pattern

**Location:** `crates/adapteros-lora-worker/src/adapter_hotswap.rs`

AdapterOS supports atomic adapter hot-swapping without restarting:

**Two-Phase Protocol:**
1. **Preload**: Load adapter into VRAM (staged area)
2. **Swap**: Atomic pointer flip with mutex-guarded transition
3. **Verify**: Recompute effective-stack hash
4. **Rollback**: Revert to last verified state on failure

```rust
use adapteros_lora_worker::adapter_hotswap::{AdapterTable, AdapterCommand};

let table = AdapterTable::new();

// Phase 1: Preload adapter into staging
table.preload("adapter_1".to_string(), adapter_hash, vram_mb)?;

// Phase 2: Atomic swap (add new, remove old)
let (vram_delta, added_count) = table.swap(
    &["adapter_1".to_string()],           // add
    &["adapter_0".to_string()],           // remove
)?;

// On error, rollback to last verified state
if vram_delta > vram_limit {
    table.rollback()?;
}

// Verify stack integrity
let stack_hash = table.compute_stack_hash()?;
```

**Double-Buffered Architecture:**
- `active`: Currently active adapters
- `staged`: Preloaded adapters waiting for swap
- `rollback_state`: Last verified state for recovery

### .aos Archive Format Pattern

**Location:** `crates/adapteros-aos/src/aos2_implementation.rs`

AdapterOS packages adapters in single-file `.aos` archives:

**File Structure:**
```
[0-3]    manifest_offset (u32, little-endian)
[4-7]    manifest_len (u32, little-endian)
[offset] manifest (JSON metadata)
[offset] weights (safetensors format)
```

**Loading with Zero-Copy:**

```rust
use adapteros_aos::AOS2Loader;

let loader = AOS2Loader::new()?;
let adapter = loader.load_from_path("./adapters/my_adapter.aos").await?;

// Memory-mapped file with zero-copy Metal buffer transfer
// Weights loaded directly into GPU VRAM without CPU copy
```

**Manifest Contents:**
- Adapter ID and version
- LoRA rank and alpha
- Base model compatibility
- Lineage and training metadata
- BLAKE3 hash for integrity

### Content-Addressed Storage Pattern

**Location:** `crates/adapteros-core/src/hash.rs`, `crates/adapteros-registry/src/lib.rs`

All artifacts use BLAKE3 content-addressed storage:

```rust
use adapteros_core::B3Hash;

// Compute content hash
let hash = B3Hash::hash(&adapter_bytes);

// Register with hash
registry.register_adapter(
    adapter_id,
    &hash,
    tier,
    rank,
    acl,
)?;

// Verify integrity on load
let loaded_bytes = std::fs::read(path)?;
let computed_hash = B3Hash::hash(&loaded_bytes);
if computed_hash != expected_hash {
    return Err(AosError::Validation("Hash mismatch".to_string()));
}
```

**Hash Usage:**
- **Adapters**: Content hash for deduplication and integrity
- **Telemetry bundles**: Content-addressed storage
- **Task IDs**: Deterministic task identification
- **Stack verification**: Effective adapter stack hashing

### Deterministic Executor Pattern

**Location:** `crates/adapteros-deterministic-exec/src/lib.rs`

AdapterOS uses a deterministic async executor for reproducible execution:

**Key Features:**
- **Serial Task Execution**: Tasks run in submission order (no concurrency)
- **Tick-Based Time**: Logical tick counter instead of wall-clock
- **Event Logging**: All spawns, completions, and timeouts logged
- **Replay Capability**: Identical execution from event logs

```rust
use adapteros_deterministic_exec::spawn_deterministic;

// Spawn task deterministically
spawn_deterministic("Adapter state update".to_string(), async move {
    if let Err(e) = db.update_adapter_state(&adapter_id, &state, &reason).await {
        warn!("Failed to update adapter state: {}", e);
    }
});

// Tasks execute serially in submission order
// No work-stealing, no concurrent execution
// Fully reproducible across runs
```

**Deterministic Guarantees:**
1. Serial execution order (FIFO queue)
2. Deterministic task IDs from global seed + sequence
3. Event log for auditing and replay
4. HKDF-derived randomness (no `rand::thread_rng()`)

### HKDF Seeding Pattern

**Location:** `crates/adapteros-core/src/hash.rs`, `tests/determinism/hkdf_seeding.rs`

All randomness derives from global seed via HKDF with domain separation:

```rust
use adapteros_core::{B3Hash, derive_seed};

// Derive domain-specific seeds from global seed
let global_seed = B3Hash::hash(b"global_seed_material");

let router_seed = derive_seed(&global_seed, "router");
let dropout_seed = derive_seed(&global_seed, "dropout");
let sampling_seed = derive_seed(&global_seed, "sampling");
let training_seed = derive_seed(&global_seed, "lora_trainer");

// Seeds are deterministic and isolated by label
assert_ne!(router_seed, dropout_seed);  // Domain separation

// Initialize RNG with derived seed
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

let seed_bytes: [u8; 32] = training_seed.try_into().unwrap();
let mut rng = ChaCha20Rng::from_seed(seed_bytes);
```

**HKDF Hierarchy:**
```
Global Seed (BLAKE3)
  ├─ router (K-sparse selection)
  ├─ dropout (kernel dropout)
  ├─ sampling (token sampling)
  ├─ lora_trainer (weight initialization)
  └─ ... (other domains)
```

**Domain Labels:**
- `router`: Adapter selection tie-breaking
- `dropout`: LoRA dropout masks
- `sampling`: Temperature/top-p sampling
- `lora_trainer`: Weight initialization
- `gate_noise`: Router gate perturbation

### SSE Streaming Inference Pattern

**Location:** `crates/adapteros-api/src/streaming.rs`

AdapterOS provides OpenAI-compatible streaming inference with token-by-token responses:

**Key Features:**
- OpenAI `chat.completion.chunk` format
- Server-Sent Events (SSE) protocol
- Token streaming with keep-alive
- Graceful error handling and client disconnect detection

```rust
use adapteros_api::streaming::{StreamingInferenceRequest, streaming_inference_handler};
use axum::response::sse::Sse;

// Create streaming request
let request = StreamingInferenceRequest {
    prompt: "Explain Rust ownership".to_string(),
    model: Some("qwen2.5".to_string()),
    max_tokens: 512,
    temperature: 0.7,
    stream: true,
    adapter_stack: Some("code-review".to_string()),
    ..Default::default()
};

// Create SSE stream
let stream = streaming_inference_handler(State(api_state), Json(request)).await;

// Stream format:
// data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"content":"Hello"},...}]}
// data: [DONE]
```

**Non-Streaming Fallback:**
```rust
use adapteros_api::streaming::completion_handler;

// Returns complete response in single call
let response = completion_handler(State(api_state), Json(request)).await?;
```

**OpenAI Compatibility:**
- Compatible with OpenAI SDK streaming clients
- Supports `temperature`, `max_tokens`, `stop` sequences
- Automatic token usage estimation
- Proper finish reasons (`stop`, `length`)

### Document Ingestion Pipeline

**Location:** `crates/adapteros-ingest-docs/src/`

AdapterOS provides deterministic PDF/Markdown ingestion for RAG and training data generation:

**Core Components:**
1. **DocumentIngestor** - High-level API for ingesting documents
2. **DocumentChunker** - Token-aware chunking with overlap
3. **EmbeddingModel** - MLX-based text embeddings
4. **TrainingDataGenerator** - Convert chunks to training examples

**Basic Usage:**
```rust
use adapteros_ingest_docs::{DocumentIngestor, default_ingest_options, load_tokenizer};

// Initialize ingestor
let tokenizer = load_tokenizer("./tokenizer.json")?;
let chunking_options = default_ingest_options();
let ingestor = DocumentIngestor::new(chunking_options, Some(tokenizer.clone()));

// Ingest PDF
let doc = ingestor.ingest_pdf_path("./docs/api.pdf")?;

// Ingest Markdown
let doc = ingestor.ingest_markdown_path("./README.md")?;

// Access chunks
for chunk in &doc.chunks {
    println!("Chunk {}: {} tokens", chunk.sequence, chunk.text.len());
}
```

**Training Data Generation:**
```rust
use adapteros_ingest_docs::{generate_training_data, TrainingGenConfig, TrainingStrategy};

// Configure training generation
let config = TrainingGenConfig {
    strategy: TrainingStrategy::Identity,  // or QuestionAnswer, MaskedLM
    max_seq_length: 512,
    add_special_tokens: true,
};

// Generate training examples
let training_data = generate_training_data(&ingested_doc, &tokenizer, &config)?;

println!("Generated {} examples", training_data.examples.len());
for example in training_data.examples {
    println!("Input: {} tokens, Target: {} tokens",
        example.input.len(), example.target.len());
}
```

**Training Strategies:**
- **Identity**: Direct chunk tokenization (unsupervised)
- **QuestionAnswer**: Generate Q&A pairs from content
- **MaskedLM**: Masked language modeling (BERT-style)

**RAG Integration:**
```rust
use adapteros_ingest_docs::rag_integration::{prepare_document_for_rag, RagChunkParams};

let params = RagChunkParams {
    max_chunk_tokens: 256,
    overlap_tokens: 50,
    min_chunk_tokens: 100,
};

let rag_ready = prepare_document_for_rag(&ingested_doc, &params)?;
// Returns chunks with embeddings ready for vector DB
```

**Chunking Options:**
```rust
use adapteros_ingest_docs::ChunkingOptions;

let options = ChunkingOptions {
    max_chunk_tokens: 512,
    overlap_tokens: 64,
    prefer_sentence_boundaries: true,
    min_chunk_size: 100,
};
```

### Training Dataset Manager Pattern

**Location:** `crates/adapteros-orchestrator/src/training_dataset_integration.rs`

Bridges document ingestion to training pipeline with database-backed dataset management:

**Workflow:**
1. Ingest documents (PDF, Markdown, code)
2. Generate training examples using document ingestion
3. Save examples to JSONL format
4. Create database record with hash and statistics
5. Link dataset to training jobs

```rust
use adapteros_orchestrator::training_dataset_integration::{
    TrainingDatasetManager, CreateDatasetFromDocumentsRequest, SerializableTrainingConfig
};

// Initialize manager
let manager = TrainingDatasetManager::new(
    db.clone(),
    PathBuf::from("./datasets"),
    Some(PathBuf::from("./tokenizer.json"))
);

// Create dataset from documents
let request = CreateDatasetFromDocumentsRequest {
    name: "rust-api-docs".to_string(),
    description: Some("Rust API documentation training data".to_string()),
    document_paths: vec![
        PathBuf::from("./docs/api.md"),
        PathBuf::from("./docs/guide.pdf"),
    ],
    training_config: SerializableTrainingConfig {
        strategy: "identity".to_string(),
        max_seq_length: 512,
        add_special_tokens: true,
    },
    created_by: Some("admin".to_string()),
};

let result = manager.create_dataset_from_documents(request).await?;

println!("Dataset ID: {}", result.dataset_id);
println!("Examples: {}", result.num_examples);
println!("Total tokens: {}", result.total_tokens);
println!("Hash: {}", result.hash_b3);
```

**Load Dataset for Training:**
```rust
let examples = manager.load_dataset_examples(&result.dataset_id).await?;

// Examples are ready for Worker training
let trainer = MicroLoRATrainer::new(config)?;
trainer.train(examples, adapter_id).await?;
```

**Database Schema:**
- `training_datasets` - Dataset metadata, hash, validation status
- `dataset_files` - Individual files in dataset
- `dataset_statistics` - Cached stats (num examples, avg lengths, token distribution)

**Features:**
- BLAKE3 content-addressed storage
- Automatic validation and statistics computation
- JSONL format for compatibility
- Links to training jobs via foreign keys

### Workflow Execution Infrastructure

**Location:** `crates/adapteros-lora-lifecycle/src/workflow_executor.rs`

Execute multiple adapters with different coordination strategies:

**Workflow Types:**
1. **Sequential** - Adapters execute one after another, output feeds into next
2. **Parallel** - All adapters execute simultaneously, results merged
3. **UpstreamDownstream** - Two-phase execution (upstream → downstream)

```rust
use adapteros_lora_lifecycle::{
    WorkflowExecutor, WorkflowType, WorkflowContext, KernelAdapterBackend
};

// Create kernel backend
let backend = Arc::new(KernelAdapterBackend::new(
    kernels_arc,
    adapter_names.clone(),
    152064  // Vocab size
));

// Create executor
let executor = WorkflowExecutor::new(
    WorkflowType::UpstreamDownstream,
    vec!["code_review".to_string(), "bug_detection".to_string()],
    backend
);

// Execute workflow
let context = WorkflowContext {
    input_tokens: vec![100, 200, 300],
    model_state: HashMap::new(),
    metadata: HashMap::from([
        ("request_id".to_string(), "req-123".to_string())
    ]),
};

let result = executor.execute(context).await?;

println!("Adapters executed: {}", result.stats.adapters_executed);
println!("Total time: {}ms", result.stats.total_time_ms);
for phase in result.stats.phases {
    println!("Phase {}: {} adapters in {}ms",
        phase.name, phase.adapter_ids.len(), phase.duration_ms);
}
```

**Worker Integration:**
```rust
// Worker has built-in workflow execution
let result = worker.execute_workflow(
    WorkflowType::Sequential,
    adapter_ids,
    context
).await?;
```

**Execution Backend Trait:**
```rust
pub trait AdapterExecutionBackend: Send + Sync {
    async fn execute_adapter(
        &self,
        adapter_id: &str,
        input_tokens: &[u32],
        model_state: &HashMap<String, Vec<f32>>,
    ) -> Result<AdapterExecutionResult>;
}
```

**Two Backends:**
- `KernelAdapterBackend` - Real Metal/MLX kernel execution
- `MockAdapterBackend` - Testing without kernels

See `KERNEL_BACKEND_USAGE.md` for detailed Worker integration guide.

### MLX Embedding Model Pattern

**Location:** `crates/adapteros-lora-mlx-ffi/src/embedding.rs`

CPU/MLX-based embedding computation for RAG and semantic search:

```rust
use adapteros_lora_mlx_ffi::embedding::MLXEmbeddingModel;

// Load sentence-transformers model
let model = MLXEmbeddingModel::load("./models/all-MiniLM-L6-v2")?;

// Encode text
let embedding = model.encode_text("Hello, world!")?;
println!("Embedding dimension: {}", model.dimension());
println!("Model hash: {}", model.model_hash());

// Embeddings are normalized L2 vectors for cosine similarity
```

**Model Structure:**
- Loads safetensors weights
- Supports BERT-based architectures
- Mean/CLS pooling strategies
- Configurable normalization
- Content-addressed with BLAKE3 hash

**Configuration:**
```rust
pub struct EmbeddingConfig {
    pub hidden_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub max_position_embeddings: usize,
    pub vocab_size: usize,
    pub pooling_mode: String,  // "mean" or "cls"
    pub normalize_embeddings: bool,
}
```

**Expected Directory Structure:**
```
models/all-MiniLM-L6-v2/
├── model.safetensors
├── config.json
└── tokenizer.json
```

### Training Service Orchestrator

**Location:** `crates/adapteros-orchestrator/src/training.rs`

Manages training job lifecycle with database-backed state tracking:

```rust
use adapteros_orchestrator::training::{
    TrainingService, TrainingConfig, TrainingTemplate, TrainingJobStatus
};

let service = TrainingService::new();

// Use predefined template
let templates = service.list_templates().await?;
let template = service.get_template("general-code").await?;

// Create training job
let job = service.create_job(
    "my-adapter".to_string(),
    Some(template.id),
    Some("dataset-123".to_string()),
    template.config
).await?;

// Monitor progress
let status = service.get_job(&job.id).await?;
println!("Status: {}", status.status);
println!("Progress: {:.1}%", status.progress_pct);
println!("Loss: {:.4}", status.current_loss);
```

**Training Templates:**
- `general-code`: Multi-language coding adapter (rank=16, alpha=32)
- `framework-specific`: Framework adapters (rank=12, alpha=24)
- Templates define default hyperparameters and targets

**Training Configuration:**
```rust
pub struct TrainingConfig {
    pub rank: u32,
    pub alpha: u32,
    pub targets: Vec<String>,  // q_proj, k_proj, v_proj, etc.
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch_size: u32,
    pub warmup_steps: Option<u32>,
    pub max_seq_length: Option<u32>,
    pub gradient_accumulation_steps: Option<u32>,
}
```

**Job Status Tracking:**
- `Pending`, `Running`, `Completed`, `Failed`, `Cancelled`
- Real-time progress percentage and loss metrics
- Tokens per second throughput
- Created/started/completed timestamps

---

## Common Patterns

### Database Access

```rust
use adapteros_db::Db;
use sqlx::query;

// ✅ GOOD: Parameterized queries
let results = query("SELECT * FROM adapters WHERE tenant_id = ?")
    .bind(&tenant_id)
    .fetch_all(&db.pool)
    .await
    .map_err(|e| AosError::Database(format!("Query failed: {}", e)))?;
```

### Async Task Spawning

```rust
use tokio::spawn;

// ✅ GOOD: Proper error handling for spawned tasks
let handle = spawn(async move {
    if let Err(e) = do_work().await {
        error!(error = %e, "Background task failed");
        // Don't panic - log and continue
    }
});

// Store handle for cancellation if needed
```

### CLI Command Pattern

```rust
use adapteros_cli::{Command, Context};
use tracing::info;

#[derive(Args)]
pub struct LoadArgs {
    path: PathBuf,
}

pub async fn execute(args: LoadArgs, ctx: &Context) -> Result<()> {
    info!(path = %args.path.display(), "Loading adapter");
    
    let loader = ctx.adapter_loader();
    let adapter = loader.load_from_path(&args.path).await?;
    
    info!(adapter_id = %adapter.id, "Adapter loaded");
    Ok(())
}
```

### Production Mode Enforcement

```rust
// Production mode enforces M1 security requirements
if config.server.production_mode {
    // UDS-only serving
    if config.server.uds_socket.is_none() {
        return Err(AosError::Config(
            "Production mode requires uds_socket".to_string()
        ));
    }
    
    // Ed25519 JWTs only (no HMAC)
    if config.security.jwt_mode.as_deref() != Some("eddsa") {
        return Err(AosError::Config(
            "Production mode requires jwt_mode='eddsa'".to_string()
        ));
    }
    
    // Zero egress enforced
    if !config.security.require_pf_deny {
        return Err(AosError::Config(
            "Production mode requires require_pf_deny=true".to_string()
        ));
    }
}
```

---

## Integration Testing Patterns

### Streaming Integration Tests

**Location:** `tests/streaming_integration.rs`

Tests OpenAI-compatible streaming with SSE format:

```rust
#[tokio::test]
async fn test_streaming_chat_completion() {
    let worker = create_test_worker().await?;

    let request = ChatCompletionRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        }],
        stream: Some(true),
        ..Default::default()
    };

    let mut stream = worker.infer_streaming(request).await?;

    let mut chunks = Vec::new();
    while let Some(chunk) = stream.next().await {
        chunks.push(chunk?);
    }

    assert!(!chunks.is_empty());
    assert_eq!(chunks[0].object, "chat.completion.chunk");
}
```

### Workflow Integration Tests

**Location:** `tests/kernel_workflow_integration.rs`

Tests workflow execution with real Metal kernels:

```rust
#[tokio::test]
async fn test_upstream_downstream_workflow() {
    let kernels = MetalKernels::new()?;
    let backend = Arc::new(KernelAdapterBackend::new(
        Arc::new(Mutex::new(kernels)),
        adapter_names,
        152064
    ));

    let executor = WorkflowExecutor::new(
        WorkflowType::UpstreamDownstream,
        adapter_names,
        backend
    );

    let result = executor.execute(context).await?;
    assert_eq!(result.stats.phases.len(), 2);
}
```

### Policy Evidence Tests

**Location:** `tests/policy_evidence_integration.rs`

Tests evidence-based retrieval with quality thresholds:

```rust
#[test]
fn test_evidence_quality_thresholds() {
    let policy = EvidencePolicy::new(config);

    let high_quality_span = EvidenceSpan {
        relevance: 0.9,
        confidence: 0.95,
        evidence_type: EvidenceType::CodeDoc,
        ..Default::default()
    };

    assert!(policy.validate_evidence_spans(&[high_quality_span]).is_ok());
}
```

---

## Anti-Patterns to Avoid

### ❌ TODO Comments Without Plans

```rust
// ❌ BAD: TODO with no completion plan
pub async fn start(&mut self) -> Result<()> {
    // TODO: Implement start logic
    Ok(())
}

// ✅ GOOD: Complete implementation or explicit error
pub async fn start(&mut self) -> Result<()> {
    self.watcher.start().await?;
    self.daemon.start().await?;
    Ok(())
}
```

### ❌ Placeholder Logic

```rust
// ❌ BAD: Placeholder that doesn't perform intended function
pub fn process(&self, data: &[u8]) -> Result<Processed> {
    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(Processed::default())
}

// ✅ GOOD: Real implementation
pub fn process(&self, data: &[u8]) -> Result<Processed> {
    let parsed = parse_data(data)?;
    let validated = validate(&parsed)?;
    Ok(Processed::new(validated))
}
```

### ❌ Missing Error Handling

```rust
// ❌ BAD: No error handling for edge cases
pub async fn load(&self, path: &Path) -> Result<Data> {
    let bytes = std::fs::read(path)?;
    Ok(deserialize(&bytes)?)
}

// ✅ GOOD: Comprehensive error handling
pub async fn load(&self, path: &Path) -> Result<Data> {
    let bytes = std::fs::read(path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                AosError::NotFound(format!("File not found: {}", path.display()))
            }
            std::io::ErrorKind::PermissionDenied => {
                AosError::Io(format!("Permission denied: {}", path.display()))
            }
            _ => AosError::Io(format!("Failed to read {}: {}", path.display(), e))
        })?;
    
    deserialize(&bytes)
        .map_err(|e| AosError::Serialization(format!("Invalid data: {}", e)))
}
```

### ❌ Using `println!` for Logging

```rust
// ❌ BAD: println! for logging
pub fn log_event(&self, event: &str) {
    println!("Event: {}", event);
}

// ✅ GOOD: Use tracing
pub fn log_event(&self, event: &str) {
    info!(event = %event, "Event occurred");
}
```

### ❌ Unsafe Code in Production Crates

```rust
// ❌ BAD: Unsafe code in application crates
pub unsafe fn manipulate_data(ptr: *mut u8) {
    *ptr = 42;
}

// ✅ GOOD: Keep unsafe code isolated to designated crates
// Only use unsafe in:
// - adapteros-lora-kernel-mtl (Metal FFI)
// - adapteros-lora-mlx-ffi (PyO3 bindings)
// With extensive documentation and tests
```

### ❌ Blocking Async Operations in Streaming

```rust
// ❌ BAD: Using sleep in async stream
async fn stream_tokens(tokens: Vec<String>) -> impl Stream<Item = String> {
    tokens.into_iter().map(|token| {
        std::thread::sleep(Duration::from_millis(10));  // Blocks executor!
        token
    })
}

// ✅ GOOD: Use tokio::time::sleep
async fn stream_tokens(tokens: Vec<String>) -> impl Stream<Item = String> {
    ReceiverStream::new(rx).map(move |token| {
        tokio::time::sleep(Duration::from_millis(10)).await;
        token
    })
}
```

### ❌ Forgetting to Lock Shared Kernels

```rust
// ❌ BAD: Storing unlocked kernel reference
let kernel_ref = &kernels;  // Doesn't lock Arc<Mutex<K>>
kernel_ref.run_step(...)?;  // Won't compile!

// ✅ GOOD: Lock the mutex
let mut kernels = self.kernels.lock().await;
kernels.run_step(&router_ring, &mut io_buffers)?;
```

### ❌ Not Validating Dataset Before Training

```rust
// ❌ BAD: Training without validation
let dataset = load_dataset(path)?;
trainer.train(dataset, adapter_id).await?;

// ✅ GOOD: Validate dataset first
let dataset = manager.load_dataset_examples(dataset_id).await?;
// Manager checks validation_status = 'valid'
trainer.train(dataset, adapter_id).await?;
```

See [docs/DEPRECATED_PATTERNS.md](docs/DEPRECATED_PATTERNS.md) for more anti-patterns found in deprecated code.

---

## Key Subsystems

### Router (K-Sparse Selection)

**Location:** `crates/adapteros-lora-router/src/`

```rust
use adapteros_lora_router::Router;

// Q15 quantized gates for selection
let router = Router::new(config);
let top_k = router.select_adapters(request, k: 3).await?;
```

### Metal Kernels

**Location:** `crates/adapteros-lora-kernel-mtl/src/`

```rust
use adapteros_lora_kernel_mtl::FusedKernels;

// Deterministic precompiled kernels
let kernels = FusedKernels::load("./target/kernels.metallib")?;
```

### Policy Enforcement

**Location:** `crates/adapteros-policy/src/`

```rust
use adapteros_policy::{PolicyEngine, PolicyPack};

let engine = PolicyEngine::new(policy_packs);
engine.enforce(request).await?;
```

### Memory Management

**Location:** `crates/adapteros-memory/src/`

```rust
use adapteros_memory::MemoryManager;

// Automatic eviction maintains headroom
let memory = MemoryManager::new(eviction_policy);
memory.ensure_headroom().await?;
```

### Adapter Lifecycle

**Location:** `crates/adapteros-lora-lifecycle/src/`

State machine for adapter memory management with automatic promotion/demotion:

```rust
use adapteros_lora_lifecycle::LifecycleManager;

let manager = LifecycleManager::new_with_db(
    adapter_names,
    &policies,
    adapters_base_path,
    telemetry,
    initial_k,
    db,
);

// Auto-promote based on activation percentages
manager.record_router_decision(&selected_adapters).await?;

// Handle memory pressure with eviction
manager.check_memory_pressure(total_memory, 0.85).await?;
```

**State Transition Rules:**
- Promotion: Activation % above threshold
- Demotion: Activation % below threshold + inactivity timeout
- Eviction: Memory pressure + lowest activation %
- Pinning: Resident state (never evicted)

### Hot-Swap Infrastructure

**Location:** `crates/adapteros-lora-worker/src/adapter_hotswap.rs`

Live adapter replacement without worker restart:

```rust
use adapteros_lora_worker::adapter_hotswap::AdapterTable;

let table = AdapterTable::new();

// Two-phase swap with rollback
table.preload("new_adapter".to_string(), hash, vram_mb)?;
match table.swap(&["new_adapter"], &["old_adapter"]) {
    Ok((vram_delta, count)) => {
        info!("Swapped {} adapters, VRAM delta: {} MB", count, vram_delta);
    }
    Err(e) => {
        table.rollback()?;  // Automatic rollback
        return Err(e);
    }
}
```

### Deterministic Executor

**Location:** `crates/adapteros-deterministic-exec/src/`

Serial async executor for reproducible execution:

```rust
use adapteros_deterministic_exec::spawn_deterministic;

// All tasks execute serially in submission order
spawn_deterministic("Database update".to_string(), async move {
    db.update_state(&state).await?;
    Ok(())
});

// No concurrency, no work-stealing, fully deterministic
```

**Guarantees:**
- FIFO task execution (no concurrent tasks)
- Deterministic task IDs (seed + sequence)
- Event logging for replay
- Tick-based logical time

### HKDF Seeding

**Location:** `crates/adapteros-core/src/hash.rs`

Domain-separated seed derivation for all randomness:

```rust
use adapteros_core::{B3Hash, derive_seed};

let global_seed = B3Hash::hash(b"session_seed");

// Derive isolated seeds for each domain
let router_seed = derive_seed(&global_seed, "router");
let dropout_seed = derive_seed(&global_seed, "dropout");

// Use with ChaCha20 RNG
use rand_chacha::ChaCha20Rng;
use rand::SeedableRng;
let mut rng = ChaCha20Rng::from_seed(router_seed);
```

### MicroLoRA Training

**Location:** `crates/adapteros-lora-worker/src/training/`

Complete training pipeline for LoRA adapters:

**Core Modules:**
- `trainer.rs` - Training loop with forward/backward pass
- `dataset.rs` - Dataset generation from patches
- `quantizer.rs` - Q15 weight quantization
- `packager.rs` - Adapter packaging with safetensors

```rust
use adapteros_lora_worker::training::{
    MicroLoRATrainer, TrainingConfig, TrainingExample
};

let config = TrainingConfig {
    rank: 4,
    alpha: 16.0,
    learning_rate: 1e-4,
    batch_size: 8,
    epochs: 3,
    hidden_dim: 768,
};

let trainer = MicroLoRATrainer::new(config)?;
let examples = vec![
    TrainingExample {
        input: vec![1, 2, 3],
        target: vec![4, 5, 6],
        metadata: HashMap::new(),
    }
];

let result = trainer.train(examples, "adapter-id").await?;
```

**Features:**
- Deterministic training with HKDF seeding
- Metal backend integration for GPU acceleration
- Automatic Q15 quantization
- Telemetry logging for training events
- Integration with TrainingDatasetManager

### Database Schema

**Location:** `crates/adapteros-db/src/`, `crates/adapteros-registry/src/`

SQLite registry with WAL mode for adapter and tenant management:

**Core Tables:**
- `adapters`: Adapter metadata, hash, tier, ACL, activation %
- `tenants`: Tenant ID, UID/GID, isolation metadata
- `adapter_stacks`: Predefined adapter combinations with workflow types
  - `id`, `name`, `description`
  - `adapter_ids_json`: JSON array of adapter IDs
  - `workflow_type`: `Parallel`, `UpstreamDownstream`, `Sequential`
  - Supports reusable adapter combinations
- `training_datasets`: Dataset metadata, hash, validation status
  - BLAKE3 content-addressed storage
  - Links to `dataset_files` and `dataset_statistics`
- `dataset_files`: Individual files within datasets
  - File path, size, hash, ingestion metadata
- `dataset_statistics`: Cached statistics (examples, tokens, language distribution)
  - Computed during dataset creation
  - Includes token distributions and average sequence lengths
- `training_jobs`: Training job status, progress, and lineage
  - Links datasets → jobs → trained adapters
  - Tracks hyperparameters, loss metrics, and timestamps

```rust
use adapteros_registry::Registry;

let registry = Registry::open("./registry.db")?;

// Register adapter with content hash
registry.register_adapter(
    "adapter_id",
    &hash,
    "tier_1",
    rank,
    &["tenant_a", "tenant_b"],
)?;

// Check ACL for tenant isolation
let allowed = registry.check_acl("adapter_id", "tenant_a")?;

// Query training datasets
let dataset = registry.get_training_dataset("dataset-123").await?;
let stats = registry.get_dataset_statistics("dataset-123").await?;
println!("Examples: {}, Total tokens: {}",
    stats.num_examples, stats.total_tokens);
```

### Web UI & REST API

**Location:** `crates/adapteros-server-api/src/`, `ui/src/`

React/TypeScript UI with REST API for adapter management:

**API Endpoints:**
- `GET /api/adapters` - List adapters with lifecycle state
- `POST /api/adapters/load` - Load adapter into lifecycle
- `POST /api/adapters/swap` - Hot-swap adapters
- `GET /api/router/config` - Router configuration
- `POST /api/training/start` - Start training job
- `GET /api/stacks` - List adapter stacks
- `POST /api/chat/completions` - OpenAI-compatible inference (streaming/non-streaming)
- `GET /api/adapter-stacks` - List adapter stacks
- `POST /api/adapter-stacks` - Create adapter stack
- `POST /api/training/datasets` - Create dataset from documents
- `GET /api/training/datasets/:id` - Get dataset details
- `POST /api/training/jobs` - Start training job
- `GET /api/training/jobs/:id` - Get job status

**UI Components:**
- `AdapterLifecycleManager.tsx`: State visualization and controls
- `RouterConfig.tsx`: K-sparse routing configuration
- `TrainingStreamPage.tsx`: Live training progress
- `AdapterMemoryMonitor.tsx`: VRAM usage and eviction

**Streaming Inference:**
```typescript
const response = await fetch('/api/chat/completions', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    model: 'qwen2.5',
    messages: [{ role: 'user', content: 'Hello' }],
    stream: true,
    adapter_stack: 'code-review'
  }),
});

const reader = response.body.getReader();
const decoder = new TextDecoder();

while (true) {
  const { done, value } = await reader.read();
  if (done) break;

  const chunk = decoder.decode(value);
  // Parse SSE format: "data: {...}\n\n"
}
```

**Adapter Hot-Swap:**
```typescript
// Example API usage
const response = await fetch('/api/adapters/swap', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    add_ids: ['new_adapter'],
    remove_ids: ['old_adapter'],
  }),
});
```

---

## Streaming Architecture

AdapterOS supports two inference modes:

**1. Batch Inference (Complete Response):**
- Worker generates full response
- Returns `InferenceResponse` with complete text
- Used for non-interactive workloads

**2. Streaming Inference (Token-by-Token):**
- OpenAI-compatible SSE format
- Progressive token generation
- Client can cancel mid-stream
- Supports keep-alive for long generations

**Implementation:**
- Streaming handler wraps Worker inference
- Chunks complete response word-by-word
- Simulates progressive generation for client compatibility
- Real streaming kernel support coming in future release

**Use When:**
- Interactive chat applications
- Real-time UI updates
- Long-form content generation
- Client needs early partial results

---

## Document Processing Workflow

Complete pipeline from documents to trained adapters:

**Step 1: Ingest Documents**
```rust
let ingestor = DocumentIngestor::new(chunking_options, tokenizer);
let doc = ingestor.ingest_pdf_path("./docs/api.pdf")?;
```

**Step 2: Generate Training Data**
```rust
let training_data = generate_training_data(&doc, &tokenizer, &config)?;
```

**Step 3: Create Dataset**
```rust
let manager = TrainingDatasetManager::new(db, storage_path, tokenizer_path);
let result = manager.create_dataset_from_documents(request).await?;
```

**Step 4: Train Adapter**
```rust
let examples = manager.load_dataset_examples(&result.dataset_id).await?;
let trainer = MicroLoRATrainer::new(training_config)?;
trainer.train(examples, adapter_id).await?;
```

**Step 5: Package and Deploy**
```rust
let packager = AdapterPackager::new();
let adapter = packager.package(weights, manifest)?;
registry.register_adapter(&adapter_id, &hash, tier, rank, acl)?;
```

**Complete Example:**
See `tests/workflow_integration.rs` for end-to-end examples.

---

## Citation Standards

When referencing code, use deterministic citations:

```markdown
[source: crates/adapteros-server/src/main.rs L173-L218]
```

Format: `[source: <path> L<start>-L<end>]`

See [CITATIONS.md](CITATIONS.md) for complete citation standards.

---

## Quick Reference

### Build Commands

```bash
# Build workspace
cargo build --release

# Run tests
cargo test --workspace

# Format code
cargo fmt --all

# Lint code
cargo clippy --workspace -- -D warnings

# Check specific crate
cargo check -p adapteros-server
```

### Testing

```bash
# Run all tests
cargo test --workspace

# Run with output
cargo test --workspace -- --nocapture

# Run specific test
cargo test test_adapter_loading

# Integration tests
cargo test --test integration_tests
```

### Common Debugging

```bash
# Check compilation errors
cargo check --workspace --message-format=short

# Find dead code
cargo clippy --workspace -- -W dead_code

# Find unused dependencies
cargo udeps
```

---

## References

- **CONTRIBUTING.md** - Contribution process and PR guidelines
- **README.md** - Project overview and quick start
- **docs/DEPRECATED_PATTERNS.md** - Anti-patterns to avoid
- **docs/ARCHITECTURE_INDEX.md** - Complete architecture reference
- **crates/adapteros-policy/** - Policy pack implementations
- **crates/adapteros-core/src/error.rs** - Error type definitions

---

**Remember:** When in doubt, check existing code patterns in `crates/` and follow established conventions.

