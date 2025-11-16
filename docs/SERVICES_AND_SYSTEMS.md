# AdapterOS Services and Systems

Complete reference guide for all services and systems in AdapterOS, including detailed explanations of how each component works internally.

---

## Services (Running Processes)

### 1. Control Plane Server (`adapteros-server`)

**Location**: `crates/adapteros-server/src/main.rs`

**Purpose**: Main API server providing REST endpoints for system management.

**Responsibilities**:
- Serves HTTP API on TCP (dev) or Unix Domain Socket (production)
- Manages authentication (HMAC-SHA256 in M0, Ed25519 in production)
- Handles rate limiting (100 requests/minute default)
- Serves embedded UI static files
- Coordinates all other services

**Key Features**:
- Single-writer mode (PID file lock prevents concurrent instances)
- Production mode enforcement (UDS-only, Ed25519 JWTs, zero egress)
- Config hot-reload via SIGHUP
- Graceful shutdown with phased cleanup
- Environment fingerprint validation

**Ports**:
- Development: TCP `127.0.0.1:8080` (default)
- Production: Unix Domain Socket (configurable path)

**Background Tasks**:
- Status cache updater (5s interval)
- Status file writer (5s interval)
- Metrics update task (configurable interval)
- Determinism metrics collector (5s interval)
- Kernel/router latency aggregator (5s interval)
- Queue depth monitor (5s interval)
- Training log cleanup (hourly)
- Telemetry GC (6h interval)
- Ephemeral adapter GC (1h interval)
- Rate limiter cleanup (24h interval)
- Policy hash watcher (60s interval)
- Alert watcher (if enabled)

**How It Works**:

1. **Startup Process**:
   - Acquires PID file lock (prevents concurrent instances)
   - Loads and validates configuration from `configs/cp.toml`
   - Validates production mode requirements (UDS, Ed25519, zero egress)
   - Connects to database with retry logic (3 attempts with exponential backoff)
   - Runs database migrations automatically
   - Initializes deterministic executor with global seed from config
   - Creates policy pack manager and loads all 22 policy packs
   - Initializes lifecycle manager for adapter management
   - Spawns all background tasks using deterministic executor
   - Builds Axum router with API routes and embedded UI
   - Binds to TCP (dev) or UDS socket (production)

2. **Request Handling**:
   - Accepts HTTP connections (TCP or UDS)
   - Routes through middleware stack: CORS → Auth → Rate Limiter → Handler
   - JWT validation extracts tenant ID and roles
   - Rate limiter uses token bucket (100 req/min default, configurable burst)
   - Handler processes request and returns JSON response
   - Telemetry events logged for all operations

3. **Config Hot-Reload**:
   - SIGHUP signal handler reloads config file
   - Updates rate limits, metrics config, and golden gate settings
   - No restart required for config changes
   - Thread-safe updates using `Arc<RwLock<Config>>`

4. **Graceful Shutdown**:
   - Phased shutdown: Drain → Critical → Resources → Final
   - Each phase has timeout (10s, 30s, 60s, 10s)
   - Unloads models and adapters in Resources phase
   - Flushes telemetry buffers in Critical phase
   - Closes database connections in Final phase

---

### 2. Service Supervisor (`adapteros-service-supervisor`)

**Location**: `crates/adapteros-service-supervisor/src/`

**Purpose**: Production-ready service supervisor for managing AdapterOS services.

**Responsibilities**:
- Process lifecycle management (start, stop, restart)
- Health monitoring beyond basic HTTP checks
- Circuit breakers for fault tolerance
- Configuration management via YAML
- Metrics and logging

**Features**:
- JWT authentication with Ed25519 signing
- Restart policies (always, on-failure, never)
- Resource limits (memory, CPU)
- Health check intervals and timeouts
- Service dependencies and startup ordering

**Port**: `3301` (configurable)

---

### 3. Worker Process (`adapteros-lora-worker`)

**Location**: `crates/adapteros-lora-worker/src/`

**Purpose**: Per-tenant inference worker process handling actual model execution.

**Responsibilities**:
- UDS HTTP server for inference requests (`/v1/inference`, `/v1/patch_proposal`)
- Inference pipeline execution
- Router integration for adapter selection
- Metal kernel execution
- RAG evidence retrieval
- Policy enforcement at inference time

**Isolation**:
- Runs in separate process per tenant
- Unique UID/GID per tenant
- Capability-scoped filesystem access
- No shared memory between tenants

**Safety Mechanisms**:
- Circuit breaker (failure tracking)
- Health monitor (memory + CPU)
- Timeout wrapper (per-operation timeouts)
- Deadlock detector (lock monitoring)

**How It Works**:

1. **UDS Server**:
   - Binds to Unix Domain Socket (per-tenant, isolated path)
   - Accepts HTTP requests on `/v1/inference` and `/v1/patch_proposal`
   - Each request spawns async task for processing
   - Response includes trace information for reproducibility

2. **Inference Pipeline Execution**:
   - Receives `InferenceRequest` with prompt and max_tokens
   - Applies chat template (ChatML format for Qwen)
   - Tokenizes input using Qwen tokenizer
   - Validates sequence length against max_seq_len (32768)
   - Initializes generation state with input tokens
   - Runs autoregressive generation loop:
     a. For each step (0 to max_tokens):
        - Prepares input (full prompt on step 0, last token otherwise)
        - Extracts code features from prompt (cached for efficiency)
        - Computes adapter priors from lifecycle manager
        - Routes to select top-K adapters with Q15 gates
        - Validates router entropy against policy
        - Executes Metal kernel with selected adapters
        - Samples next token from output logits
        - Records telemetry (sampled: first 128 tokens, then every 20th)
        - Checks for EOS token or max sequence length
   - Decodes generated tokens to text
   - Validates post-inference router entropy
   - Builds inference trace with all router decisions
   - Returns `InferenceResponse` with text and trace

3. **Router Integration**:
   - Router receives 22-dim feature vector and prior scores
   - Features include: language one-hot, framework priors, symbol hits, path tokens, prompt verb, attention entropy
   - Priors incorporate framework hints and lifecycle activation percentage
   - Router returns `Decision` with adapter indices and Q15 gates
   - Decision converted to `RouterRing` for kernel execution

4. **Metal Kernel Execution**:
   - `RouterRing` contains adapter indices, Q15 gates, and position
   - Kernel performs fused attention + MLP operations
   - LoRA adapters applied with quantized gates
   - Output logits written to `IoBuffers`
   - Latency tracked for telemetry

5. **Policy Enforcement**:
   - Pre-inference: Quarantine check, router entropy validation
   - Post-inference: Average router entropy validation
   - Violations logged to telemetry and may block request

---

### 4. UI Frontend (React Application)

**Location**: `ui/`

**Purpose**: Web-based control plane interface.

**Technology**:
- React 18 + TypeScript
- Vite for dev/build
- Tailwind CSS + shadcn/ui
- Lucide React icons

**Features**:
- JWT authentication with role-based access
- Real-time metrics and health monitoring
- Multi-tenant management
- Adapter lifecycle management
- Policy configuration
- Code intelligence and repository analysis
- Training monitoring and orchestration
- Telemetry bundle export
- Inference playground
- Audit dashboard and compliance

**Port**: `3200` (development), embedded in server (production)

---

### 5. macOS Menu Bar App

**Location**: `menu-bar-app/`

**Purpose**: Native macOS status monitoring application.

**Technology**: SwiftUI

**Features**:
- Zero network calls (reads local JSON)
- Native system metrics via IOKit
- 5-second polling
- Offline operation
- Status indicators (deterministic mode, high load)
- Quick log access

**Status Icons**:
- `⚡︎` - Normal operation, deterministic mode
- `⚡︎/` - Non-deterministic mode or offline
- `[FIRE]` - High CPU load (>70%)

---

## Systems (Architectural Subsystems)

### 1. Inference System

**Components**:
- **Inference Pipeline** (`adapteros-lora-worker/src/inference_pipeline.rs`)
- **Base LLM** (`adapteros-base-llm/`)
- **Adapter Loader** (`adapteros-lora-lifecycle/`)
- **Metal Kernels** (`adapteros-lora-kernel-mtl/`)

**Flow**:
```
Request → Tokenizer → Router → Adapter Selection → Metal Kernels → Generator → Response
```

**Key Features**:
- Deterministic execution with HKDF seeding
- K-sparse LoRA routing (top-K adapter selection)
- Q15 quantized gates for efficiency
- Precompiled Metal kernels for reproducibility
- Support for multiple backends (Metal, MLX)

**How It Works**:

1. **Request Processing** (`inference_pipeline.rs:infer()`):
   - Receives `InferenceRequest` with prompt, max_tokens, cpid
   - Checks quarantine status (policy hash enforcement)
   - Applies chat template (ChatML for Qwen: `<|im_start|>user\n{prompt}<|im_end|>\n<|im_start|>assistant\n`)
   - Tokenizes formatted prompt using Qwen tokenizer
   - Validates sequence length against max_seq_len (32768)

2. **Generation Loop** (autoregressive):
   ```rust
   // Initialize state
   let mut generated_tokens = Vec::new();
   let mut current_tokens = input_tokens.clone();
   let mut router_decisions = Vec::new();
   
   // Cache code features (computed once, reused per step)
   let cached_code_features = self.create_code_features(&formatted_prompt);
   let features_vec = cached_code_features.to_vector();
   
   // Reuse buffers across steps (reduces allocations)
   let mut io_buffers = IoBuffers::new(vocab_size);
   let mut router_ring = RouterRing::new(k);
   
   // Autoregressive generation
   for step in 0..max_tokens {
       // Prepare input (full prompt on step 0, last token otherwise)
       let input_ids = if step == 0 {
           &current_tokens[..]
       } else {
           std::slice::from_ref(generated_tokens.last().unwrap())
       };
       
       // Compute adapter priors
       let priors = self.compute_priors(&features_vec, step);
       
       // Router decision (selects top-K adapters)
       let decision = self.router.route(&features_vec, &priors);
       
       // Validate router entropy
       let entropy = self.calculate_gate_entropy(&decision.gates_q15);
       self.determinism_validator.validate_router_entropy(entropy)?;
       
       // Execute Metal kernel
       io_buffers.input_ids.clear();
       io_buffers.input_ids.extend_from_slice(input_ids);
       io_buffers.position = current_tokens.len() - 1;
       
       router_ring.set(&decision.indices, &decision.gates_q15);
       router_ring.position = step;
       
       self.kernels.run_step(&router_ring, &mut io_buffers)?;
       
       // Sample next token
       let next_token = self.generator.next_token(&io_buffers.output_logits)?;
       
       // Record telemetry (sampled: first 128, then every 20th)
       if step < 128 || step % 20 == 0 {
           self.telemetry.log("inference.step", ...);
       }
       
       // Check stopping criteria
       if next_token == eos_token_id { break; }
       
       generated_tokens.push(next_token);
       current_tokens.push(next_token);
   }
   ```

3. **Token Sampling** (`generation.rs:next_token()`):
   - Applies temperature scaling to logits
   - Top-k filtering (default: 50)
   - Top-p (nucleus) filtering (default: 0.95)
   - Samples from filtered distribution
   - Returns token ID

4. **Deterministic Execution**:
   - Global seed from config (32-byte hex)
   - HKDF-derived seeds for worker, router, inference
   - All RNG operations use derived seeds
   - Same input + seed → same output

5. **Trace Building**:
   - Records all router decisions per step
   - Includes adapter indices, Q15 gates, step number
   - Evidence citations (if RAG enabled)
   - Input and generated tokens
   - Used for deterministic replay

---

### 2. Router System

**Location**: `crates/adapteros-lora-router/src/`

**Purpose**: Selects top-K most relevant LoRA adapters per request.

**Algorithm**:
1. Feature extraction (22-dim vector: language, framework, symbols, paths, etc.)
2. Prior application (framework hints, lifecycle activation)
3. Gate computation with Q15 quantization
4. Entropy floor enforcement (prevents single-adapter collapse)
5. Top-K selection with deterministic tie-breaking

**Features**:
- Q15 quantized gates (fixed-point for efficiency)
- Entropy floor mechanism (minimum gate values)
- Deterministic ordering: `(score DESC, doc_id ASC)`
- Runtime feature-driven scoring

**Configuration**:
- `k_sparse`: Number of adapters to select (default: 3)
- `entropy_floor`: Minimum gate value (default: 0.02)
- `gate_quant`: Quantization mode (Q15)

**How It Works**:

1. **Feature Extraction** (`code_features.rs`):
   - Extracts 22-dimensional feature vector from prompt:
     - Language one-hot encoding (detected from code patterns)
     - Framework priors (Django, React, etc.)
     - Symbol hits (function/class names matching adapter metadata)
     - Path tokens (directory structure tokens)
     - Prompt verb (action type: "write", "refactor", "explain", etc.)
     - Attention entropy (from previous steps)
   - Features cached per prompt for efficiency

2. **Prior Computation**:
   - Lifecycle manager provides adapter states (warm/cold)
   - Framework matching boosts priors for matching frameworks
   - Activation percentage from recent usage
   - Priors normalized to sum to 1.0

3. **Routing Algorithm** (`lib.rs:route()`):
   ```rust
   // 1. Score all adapters from priors (features incorporated upstream)
   let scores: Vec<(usize, f32)> = priors.iter().enumerate()
       .map(|(i, &prior)| (i, prior)).collect();
   
   // 2. Sort by score DESC, then index ASC (deterministic tie-breaking)
   scores.sort_by(|a, b| {
       b.1.partial_cmp(&a.1).unwrap_or(Equal)
           .then_with(|| a.0.cmp(&b.0))
   });
   
   // 3. Take top K
   let top_k = scores.into_iter().take(k).collect();
   
   // 4. Softmax with temperature (tau)
   let max_score = top_k.iter().map(|(_, s)| s).fold(NEG_INFINITY, max);
   let exp_scores: Vec<f32> = top_k.iter()
       .map(|(_, s)| ((s - max_score) / tau).exp()).collect();
   let sum_exp: f32 = exp_scores.iter().sum();
   
   // 5. Normalize and apply entropy floor
   let mut gates: Vec<f32> = exp_scores.iter()
       .map(|e| e / sum_exp).collect();
   let min_gate = eps / k as f32;  // Minimum gate value
   for g in &mut gates {
       *g = g.max(min_gate);  // Enforce entropy floor
   }
   
   // 6. Renormalize (gates may have increased due to floor)
   let sum_gates: f32 = gates.iter().sum();
   for g in &mut gates {
       *g /= sum_gates;
   }
   
   // 7. Quantize to Q15 (fixed-point: -32768 to 32767)
   let gates_q15: Vec<i16> = gates.iter()
       .map(|&g| (g * 32767.0).round() as i16).collect();
   ```

4. **Entropy Floor Mechanism**:
   - Prevents single-adapter collapse (all gates → one adapter)
   - Minimum gate value: `eps / k` (default: 0.02 / 3 = 0.0067)
   - Ensures diversity even when one adapter scores much higher
   - Renormalization after floor application maintains sum = 1.0

5. **Q15 Quantization**:
   - Converts float gates (0.0-1.0) to 16-bit signed integers
   - Range: 0 to 32767 (Q15 format, 1 bit for sign, 15 bits for value)
   - Reduces memory usage and enables efficient GPU operations
   - Precision: 1/32767 ≈ 0.00003 (sufficient for gate values)

6. **Orthogonal Constraints** (optional):
   - Tracks adapter activation history
   - Computes cosine similarity penalty for similar adapters
   - Reduces redundant adapter selection
   - Currently CPU-only (GPU kernel pending)

7. **K0 Detection**:
   - Detects when no adapters qualify (all scores < threshold)
   - Returns empty decision
   - Logs k0 event to telemetry
   - Inference continues with base model only

---

### 3. Policy Enforcement System

**Location**: `crates/adapteros-policy/src/`

**Purpose**: Enforces 22 canonical policy packs across all operations.

**Policy Packs**:
1. **Egress Ruleset**: Zero network during serving, PF enforcement
2. **Determinism Ruleset**: Precompiled kernels, HKDF seeding
3. **Router Ruleset**: K bounds, entropy floor, Q15 gates
4. **Evidence Ruleset**: Mandatory open-book grounding
5. **Refusal Ruleset**: Abstain on low confidence
6. **Memory Ruleset**: Headroom maintenance, eviction policies
7. **Isolation Ruleset**: Tenant isolation, UID/GID separation
8. **Telemetry Ruleset**: Canonical JSON, event logging
9. **Security Ruleset**: Authentication, authorization
10. **Compliance Ruleset**: Audit trails, traceability
11. **Validation Ruleset**: Input validation, output verification
12. **Performance Ruleset**: Latency bounds, throughput targets
13. **Error Handling Ruleset**: Error propagation, recovery
14. **Configuration Ruleset**: Config validation, precedence
15. **Artifact Ruleset**: BLAKE3 hashing, Ed25519 signatures
16. **Database Ruleset**: Migration safety, transaction isolation
17. **Crypto Ruleset**: Key management, secure storage
18. **Network Ruleset**: UDS-only, no egress
19. **Determinism Attestation**: Kernel hash verification
20. **Router Diversity**: Entropy floor enforcement
21. **Memory Safety**: Headroom guarantees
22. **Evidence Grounding**: Citation requirements

**Enforcement Levels**:
- **Info**: Log only, never block
- **Warning**: Block on Error/Critical/Blocker severity
- **Error**: Block on Error/Critical/Blocker severity
- **Critical**: Block all violations

**Integration Points**:
- Server layer: Pre-request validation
- Worker layer: Pre-inference and post-inference validation
- Centralized: `PolicyPackManager` coordinates all packs

**How It Works**:

1. **Policy Pack Registration**:
   - All 22 policy packs registered in `PolicyPackManager`
   - Each pack implements `PolicyPack` trait with validation logic
   - Packs can be enabled/disabled via configuration
   - Enforcement levels set per pack (Info, Warning, Error, Critical)

2. **Request Validation Flow** (`policy_packs.rs:validate_request()`):
   ```rust
   // 1. Iterate through all active policy packs
   for (pack_id, validator) in &self.packs {
       // 2. Check if pack is enabled
       if let Some(config) = self.configs.get(pack_id) {
           if !config.enabled { continue; }
       }
       
       // 3. Validate request against pack
       let validation = validator.validate_request(request)?;
       
       // 4. Collect violations and warnings
       violations.extend(validation.violations);
       warnings.extend(validation.warnings);
       
       // 5. Short-circuit on critical blocker
       if found_critical_blocker { break; }
   }
   ```

3. **Violation Severity Levels**:
   - **Info**: Logged only, never blocks
   - **Warning**: Logged, blocks if severity is Error/Critical/Blocker
   - **Error**: Blocks if severity is Error/Critical/Blocker
   - **Critical**: Always blocks

4. **Enforcement Actions**:
   - **Allow**: Request proceeds
   - **Deny**: Request blocked, error returned
   - **LogViolation**: Violation logged to telemetry
   - **SendAlert**: Alert sent (if alerting enabled)

5. **Short-Circuiting**:
   - If critical blocker violation found, remaining packs skipped
   - Reduces latency for clearly invalid requests
   - Critical packs (Egress, Determinism) validated early

6. **Policy Hash Watcher**:
   - Monitors policy pack hashes for changes
   - Detects policy drift (unauthorized changes)
   - Triggers quarantine if policy hash mismatch detected
   - Runs every 60 seconds in background

7. **Example: Egress Policy Enforcement**:
   - Checks if `require_pf_deny` is true in production
   - Validates PF rules block all egress
   - Blocks server startup if egress not blocked
   - Logs violations to telemetry

8. **Example: Determinism Policy Enforcement**:
   - Validates router entropy ≥ threshold
   - Checks for unseeded RNG usage
   - Verifies Metal kernel hashes match baseline
   - Blocks inference if determinism violated

---

### 4. Memory Management System

**Location**: `crates/adapteros-memory/src/`

**Components**:
- **Memory Watchdog** (`watchdog.rs`): Heap observer, pointer canonicalization
- **Unified Memory Manager** (`unified_memory.rs`): Allocation, headroom tracking
- **Lifecycle Manager** (`adapteros-lora-lifecycle/`): State transitions, eviction controller

**Features**:
- Maintains ≥15% memory headroom
- Automatic adapter eviction when pressure detected
- Eviction order: `["ephemeral_ttl", "cold_lru", "warm_lru"]`
- Memory pressure levels: Normal, Warning, Critical
- Per-tenant memory tracking

**Eviction Policy**:
1. Ephemeral adapters (TTL-bound) - highest priority
2. Cold LRU adapters (least recently used)
3. Warm LRU adapters (moderately used)

**How It Works**:

1. **Memory Pressure Detection** (`unified_interface.rs`):
   - Calculates headroom percentage: `(available_memory / total_memory) * 100`
   - Pressure levels:
     - **Low**: headroom ≥ 30%
     - **Medium**: headroom ≥ 20%
     - **High**: headroom ≥ 15%
     - **Critical**: headroom < 15%
   - Watchdog monitors heap usage continuously
   - Triggers eviction when headroom < threshold (default: 15%)

2. **Eviction Algorithm** (`unified_interface.rs:cleanup_memory()`):
   ```rust
   // 1. Get current memory stats
   let stats = self.get_memory_usage().await?;
   
   // 2. Check if eviction needed
   if stats.headroom_percentage >= threshold {
       return Ok(cleanup_report);  // No eviction needed
   }
   
   // 3. Sort adapters by eviction priority
   let mut adapters: Vec<_> = stats.adapters.into_iter().collect();
   adapters.sort_by(|a, b| {
       // Pinned adapters last (never evict)
       if a.pinned != b.pinned {
           return a.pinned.cmp(&b.pinned);
       }
       // Sort by quality score (lower = evict first)
       match a.quality_score.partial_cmp(&b.quality_score) {
           Some(ord) if ord != Equal => ord,
           // Tiebreaker: BLAKE3 hash for determinism
           _ => {
               let hash_a = blake3::hash(a.adapter_id.as_bytes());
               let hash_b = blake3::hash(b.adapter_id.as_bytes());
               hash_a.as_bytes().cmp(hash_b.as_bytes())
           }
       }
   });
   
   // 4. Evict adapters until headroom restored
   for adapter in adapters {
       if adapter.pinned { continue; }  // Skip pinned
       if headroom_sufficient() { break; }
       
       self.evict_adapter(&adapter.adapter_id).await?;
       memory_freed += adapter.memory_usage_bytes;
   }
   ```

3. **Eviction Order Determinism**:
   - Must be deterministic for reproducible execution
   - Sort order: pinned status → quality score → BLAKE3 hash of ID
   - Same adapters always evicted in same order
   - Hash tiebreaker ensures deterministic ordering

4. **Adapter States** (`adapteros-lora-lifecycle`):
   - **Unloaded**: Not in memory
   - **Loading**: Currently loading from disk
   - **Warm**: In memory, recently used
   - **Cold**: In memory, not recently used
   - **Unloading**: Currently unloading
   - **Evicted**: Forced out due to memory pressure

5. **Lifecycle Manager Integration**:
   - Tracks adapter states and transitions
   - Updates LRU timestamps on access
   - Manages TTL for ephemeral adapters
   - Coordinates with memory manager for eviction

6. **Memory Allocation**:
   - Tracks total allocated memory per adapter
   - Updates on load/unload operations
   - Maintains running total for headroom calculation
   - Thread-safe using `Arc<Mutex<u64>>`

7. **Headroom Maintenance**:
   - Target: ≥15% headroom (configurable)
   - Automatic eviction when below threshold
   - Prevents OOM (Out of Memory) errors
   - Ensures room for new adapter loads

---

### 5. RAG (Retrieval-Augmented Generation) System

**Location**: `crates/adapteros-lora-rag/src/`

**Purpose**: Evidence retrieval for open-book inference.

**Backends**:
- **In-memory**: Default, per-tenant HNSW index
- **PostgreSQL + pgvector**: Production backend (requires `--features rag-pgvector`)

**Features**:
- Deterministic retrieval ordering: `(score DESC, doc_id ASC)`
- HNSW vector search for embeddings
- Configurable embedding dimensions (default: 3584)
- Evidence citation tracking
- Integration with inference pipeline

**Determinism**:
- Fixed ordering ensures reproducible retrieval
- Same query always returns same results in same order

**How It Works**:

1. **Query Processing**:
   - Receives query text from inference request
   - Embeds query using embedding model (3584-dim default)
   - Searches vector index for similar documents

2. **Vector Search** (HNSW algorithm):
   - Hierarchical Navigable Small World graph
   - Fast approximate nearest neighbor search
   - Configurable ef_search parameter (accuracy vs speed)

3. **Deterministic Ordering**:
   ```sql
   -- PostgreSQL + pgvector backend
   SELECT doc_id, content, embedding <=> $1 AS distance
   FROM documents
   WHERE tenant_id = $2
   ORDER BY distance ASC, doc_id ASC  -- Deterministic tie-breaking
   LIMIT $3;
   ```
   - Primary sort: distance (similarity score)
   - Secondary sort: doc_id (deterministic tie-breaker)
   - Same query → same ordering → reproducible results

4. **Evidence Integration**:
   - Top-K documents retrieved (default: 5)
   - Content prepended to prompt as context
   - Citations tracked in inference trace
   - Used for open-book inference (evidence-grounded responses)

5. **In-Memory Backend**:
   - Per-tenant HNSW index in memory
   - Synchronous API (no async overhead)
   - Fast for small to medium document sets
   - Lost on restart (no persistence)

6. **PostgreSQL Backend** (optional):
   - Persistent storage in PostgreSQL
   - pgvector extension for vector operations
   - Supports large document sets
   - Requires `--features rag-pgvector` at build time

---

### 6. Telemetry System

**Location**: `crates/adapteros-telemetry/src/`

**Purpose**: Event logging and observability.

**Features**:
- Canonical JSON (JCS) serialization
- BLAKE3 event hashing
- Merkle-tree bundle signing
- Deterministic replay support
- System metrics collection
- Policy violation tracking

**Components**:
- **Telemetry Writer**: Event capture and bundling
- **Trace Builder**: Audit trail construction
- **Metrics Collector**: Performance monitoring
- **Bundle Store**: Telemetry archives

**Bundling**:
- Max events per bundle: 10,000
- Max bundle size: 50MB
- Automatic bundle rotation
- Retention policy: 12 bundles per CPID (configurable)

**How It Works**:

1. **Event Capture**:
   - Events logged via `TelemetryWriter::log()`
   - Canonical JSON (JCS) serialization
   - BLAKE3 hash computed for each event
   - Timestamp, tenant_id, component, metadata included

2. **Bundle Construction**:
   - Events accumulated in memory buffer
   - Bundle created when:
     - Event count reaches 10,000
     - Bundle size reaches 50MB
     - Time-based rotation (configurable)
   - Merkle tree built from event hashes
   - Bundle signed with Ed25519 keypair

3. **Bundle Storage**:
   - Written to `bundles_root/` directory
   - Filename: `{cpid}_{timestamp}_{hash}.ndjson`
   - NDJSON format (newline-delimited JSON)
   - Immutable (never modified after creation)

4. **Retention Policy**:
   - GC runs every 6 hours
   - Keeps 12 bundles per CPID (configurable)
   - Always keeps incident bundles (policy violations)
   - Always keeps promotion bundles (CAB gates)
   - Evicts oldest bundles first

5. **Replay Support**:
   - Bundles contain all events for deterministic replay
   - Can reconstruct system state from bundle
   - Used for verification and debugging
   - API endpoint: `/api/replay/{bundle_id}`

6. **Broadcast Channel**:
   - Live telemetry streaming via broadcast channel
   - Subscribers receive events in real-time
   - Used for metrics aggregation
   - Capacity: 256 events (configurable)

---

### 7. Database System

**Location**: `crates/adapteros-db/src/`

**Backends**:
- **SQLite**: Default for local/dev (`migrations/`)
- **PostgreSQL**: Production/cluster deployments (`migrations_postgres/`)

**Features**:
- Versioned migrations with rollback support
- Multi-tenant schema isolation
- Connection pooling
- Transaction safety

**Schema**:
- Adapters registry
- Tenants
- Models
- Training jobs
- Telemetry bundles
- Policy hashes
- System metrics
- 30+ tables total

**Migrations**:
- Automatic on startup
- Recovery logic for corrupted databases
- Detailed error messages with recovery suggestions

---

### 8. Configuration System

**Location**: `crates/adapteros-config/src/`

**Purpose**: Deterministic configuration management.

**Precedence** (highest to lowest):
1. CLI arguments
2. Environment variables
3. Config file (`configs/cp.toml`)
4. Defaults

**Features**:
- Configuration freeze with BLAKE3 hashing
- Hot-reload via SIGHUP
- Validation on load
- Startup requirement checks

**Configuration Sections**:
- `[server]`: Port, bind, UDS socket, production mode
- `[db]`: Database path/URL
- `[security]`: JWT secrets, keys, PF enforcement
- `[paths]`: Plan dir, artifact dir, adapters root
- `[router]`: K-sparse, entropy floor, gate quantization
- `[memory]`: Headroom percentage, eviction order
- `[metrics]`: Collection intervals, server config
- `[telemetry]`: Buffer capacities, channel sizes
- `[policies]`: Policy pack configurations
- `[orchestrator]`: Training, code jobs
- `[git]`: Git subsystem configuration

---

### 9. Authentication & Authorization System

**Location**: `crates/adapteros-server-api/src/auth.rs`

**Modes**:
- **M0 (Development)**: HMAC-SHA256 JWTs
- **Production**: Ed25519-signed JWTs

**Features**:
- JWT token validation
- Role-based access control (Admin, Operator, SRE)
- Multi-tenant isolation
- Token expiration
- Secure key storage (keychain integration)

**Roles**:
- **Admin**: Full system access
- **Operator**: Operational tasks (adapter management, training)
- **SRE**: Monitoring and observability

---

### 10. Training System

**Location**: `crates/adapteros-orchestrator/src/training.rs`

**Purpose**: LoRA adapter training orchestration.

**Features**:
- JSON dataset support (text-based or pre-tokenized)
- Directory-based training (codegraph analyzer)
- Automatic packaging and registration
- Job state management (queued, running, completed, failed)
- Log file management
- Cache warmup on startup
- Stuck job reconciliation

**Training Flow**:
1. Job creation via API or CLI
2. Dataset preparation (tokenization if needed)
3. Training execution (Metal kernels optional)
4. Adapter packaging (manifest, signature, public key)
5. Registration in registry DB
6. State update and cleanup

---

### 11. Code Intelligence System

**Location**: `crates/adapteros-codegraph/src/`

**Purpose**: Code analysis and repository understanding.

**Features**:
- Symbol extraction
- Dependency graph construction
- Framework detection
- Path-based analysis
- Directory change tracking
- Git integration

**Use Cases**:
- Training data generation from codebases
- Adapter selection based on code context
- Evidence retrieval for code-related queries

---

### 12. Artifact Store System

**Location**: `crates/adapteros-artifacts/src/`

**Purpose**: Content-addressed storage for adapters and models.

**Features**:
- BLAKE3 hashing for content addressing
- Ed25519 signatures for verification
- SPDX SBOM validation
- Immutable storage
- Version tracking

**Artifacts**:
- LoRA adapter weights (`weights.safetensors`)
- Manifests (`manifest.json`)
- Signatures (`signature.sig`)
- Public keys (`public_key.pem`)

---

### 13. Deterministic Execution System

**Location**: `crates/adapteros-deterministic-exec/src/`

**Purpose**: Ensures reproducible execution across runs.

**Features**:
- HKDF seed derivation from global seed
- Deterministic task scheduling
- Fixed random number generation
- Canonical JSON serialization
- Precompiled Metal kernels (no runtime compilation)
- Deterministic tie-breaking in retrieval

**Global Seed**:
- 32-byte hex string from config
- Used for all RNG operations
- Ensures same input → same output

---

### 14. Metrics & Observability System

**Location**: `crates/adapteros-telemetry/src/metrics.rs`

**Components**:
- **Metrics Collector**: Aggregates system metrics
- **Metrics Registry**: Time series storage
- **Metrics Server**: Prometheus export (HTTP)
- **UDS Metrics Exporter**: Zero-network metrics (Unix socket)

**Metrics**:
- Inference latency (p95)
- Queue depth
- Tokens per second
- Memory usage
- CPU usage
- GPU utilization
- Kernel latency
- Router latency
- Determinism metrics

**Export Formats**:
- Prometheus format (HTTP endpoint)
- UDS socket (zero-network)
- JSON (API endpoints)

---

### 15. Git Integration System

**Location**: `crates/adapteros-git/src/`

**Purpose**: Git repository monitoring and analysis.

**Features**:
- Repository watching
- File change detection
- Commit tracking
- Branch monitoring
- Event broadcasting

**Integration**:
- Optional (enabled via config)
- Broadcasts file change events
- Used by code intelligence and training systems

---

### 16. Federation System

**Location**: `crates/adapteros-federation/src/`

**Purpose**: Cross-tenant adapter synchronization.

**Status**: Designed but not fully integrated (dependencies pending)

**Features** (planned):
- Signed adapter bundle exchange
- Host discovery and synchronization
- Quarantine management
- 5-minute sweep interval

---

### 17. Keychain System

**Location**: `crates/adapteros-crypto/src/providers/keychain.rs`

**Purpose**: Secure key storage across platforms.

**Backends**:
- **macOS**: Secure Enclave (hardware-backed)
- **Linux**: OS keychain services
- **Fallback**: Encrypted keystore file

**Features**:
- Hardware-backed storage (when available)
- Encrypted fallback
- Multi-platform support
- Secure key retrieval

---

### 18. Verification System

**Location**: `crates/adapteros-verify/src/`

**Purpose**: Environment fingerprinting and drift detection.

**Features**:
- Device fingerprinting
- Baseline creation and signing
- Drift detection
- Cryptographic verification
- Automatic baseline creation on first run

**Fingerprint Components**:
- Hardware identifiers
- System configuration
- Toolchain versions
- Environment variables

---

## Service Interaction Flow

### Request Flow

```
Client → Control Plane Server → Authentication → Rate Limiter → 
API Handler → Worker Process → Inference Pipeline → Router → 
Adapter Selection → Metal Kernels → Response → Telemetry
```

### Startup Sequence

1. **Control Plane Server** starts
2. Configuration validation
3. Database connection and migrations
4. Deterministic executor initialization
5. Policy pack manager initialization
6. Lifecycle manager initialization
7. Training service warmup
8. Background tasks spawned
9. API routes built
10. Server listening (TCP or UDS)

### Shutdown Sequence

1. **Drain Phase**: Stop accepting new connections
2. **Critical Phase**: Flush telemetry, save state
3. **Resources Phase**: Unload models and adapters
4. **Final Phase**: Close databases, cleanup temp files

---

## System Dependencies

### Core Dependencies
- **Control Plane Server** → Database, Policy Manager, Lifecycle Manager
- **Worker Process** → Router, Metal Kernels, RAG Engine
- **Router** → Adapter Registry, Memory Manager
- **Training System** → Database, Code Intelligence, Artifact Store

### Optional Dependencies
- **RAG System** → PostgreSQL (if pgvector enabled)
- **Git System** → Database (if enabled in config)
- **Federation** → Database, Telemetry (pending integration)

---

## Configuration Files

### Main Config
- **Location**: `configs/cp.toml`
- **Purpose**: Primary system configuration

### Supervisor Config
- **Location**: `config/supervisor.yaml`
- **Purpose**: Service supervisor configuration

### Database Migrations
- **SQLite**: `migrations/*.sql`
- **PostgreSQL**: `migrations_postgres/*.sql`

---

## Ports and Sockets

| Service | Port/Socket | Purpose |
|---------|------------|---------|
| Control Plane (dev) | TCP 8080 | HTTP API |
| Control Plane (prod) | UDS socket | HTTP API (zero egress) |
| UI Frontend (dev) | TCP 3200 | React dev server |
| Service Supervisor | TCP 3301 | Service management |
| Metrics Server | TCP 9090 | Prometheus export |
| UDS Metrics | Unix socket | Zero-network metrics |
| Worker UDS | Unix socket | Per-tenant inference |

---

## Background Tasks Summary

| Task | Interval | Purpose |
|------|----------|---------|
| Status cache updater | 5s | Update status cache |
| Status file writer | 5s | Write status JSON |
| Metrics update | Configurable | System metrics |
| Determinism metrics | 5s | Seed metrics |
| Kernel latency aggregator | 5s | Aggregate kernel latencies |
| Queue depth monitor | 5s | Monitor request queues |
| Training log cleanup | 1h | Clean old training logs |
| Telemetry GC | 6h | Bundle retention |
| Ephemeral adapter GC | 1h | Clean expired adapters |
| Rate limiter cleanup | 24h | Clean stale rate limiters |
| Policy hash watcher | 60s | Monitor policy changes |

---

## References

- **Architecture**: `docs/architecture.md`
- **Master Plan**: `docs/architecture/MasterPlan.md`
- **Precision Diagrams**: `docs/architecture/precision-diagrams.md`
- **Policy Packs**: `docs/POLICIES.md`
- **Database Schema**: `docs/database-schema/`
- **Control Plane API**: `docs/control-plane.md`

---

---

## Detailed Algorithm Explanations

### Router Algorithm (Step-by-Step)

1. **Input**: 22-dim feature vector, prior scores for all adapters
2. **Scoring**: Use priors directly (features incorporated upstream)
3. **Sorting**: Sort by score DESC, then index ASC (deterministic)
4. **Top-K Selection**: Take first K adapters
5. **Softmax**: Apply temperature-scaled softmax
6. **Entropy Floor**: Clamp gates to minimum value (eps/k)
7. **Renormalization**: Normalize gates to sum to 1.0
8. **Quantization**: Convert to Q15 (16-bit signed integer)
9. **Output**: Adapter indices and Q15 gates

### Memory Eviction Algorithm (Step-by-Step)

1. **Pressure Detection**: Calculate headroom percentage
2. **Threshold Check**: If headroom ≥ 15%, no eviction needed
3. **Candidate Collection**: Get all loaded adapters
4. **Sorting**: Sort by pinned status → quality score → BLAKE3 hash
5. **Eviction Loop**: Evict unpinned adapters until headroom restored
6. **State Update**: Update adapter states and memory totals
7. **Telemetry**: Log eviction events

### Policy Validation Algorithm (Step-by-Step)

1. **Pack Iteration**: Iterate through all active policy packs
2. **Enablement Check**: Skip disabled packs
3. **Validation**: Call pack's `validate_request()` method
4. **Violation Collection**: Collect all violations and warnings
5. **Severity Check**: Determine if violations are blocking
6. **Short-Circuit**: Stop early if critical blocker found
7. **Action Determination**: Allow or deny based on violations
8. **Telemetry**: Log all violations to telemetry

### Inference Generation Algorithm (Step-by-Step)

1. **Initialization**: Tokenize prompt, initialize state
2. **Feature Extraction**: Extract code features (cached)
3. **Generation Loop** (for each token):
   a. Prepare input (full prompt or last token)
   b. Compute adapter priors
   c. Route to select adapters
   d. Validate router entropy
   e. Execute Metal kernel
   f. Sample next token
   g. Record telemetry (sampled)
   h. Check stopping criteria
4. **Decoding**: Decode tokens to text
5. **Validation**: Validate post-inference entropy
6. **Trace Building**: Build inference trace
7. **Response**: Return text and trace

---

## User Presentation (How Users Interact)

### 1. Control Plane Server

**Web UI**:
- **Dashboard** (`/dashboard`): System overview with widgets based on role
  - Admin: Service status, multi-model status, system health, alerts, compliance score
  - Operator: Service status, ML pipeline, adapter status, next steps, alerts
  - SRE: Service status, monitoring metrics, system health, alerts
- **Login** (`/login`): JWT-based authentication with httpOnly cookies
- **Status Page**: Real-time system status via `/api/v1/status`

**API Endpoints**:
- `GET /healthz` - Health check (public)
- `GET /readyz` - Readiness check (public)
- `POST /v1/auth/login` - User login
- `GET /api/v1/status` - System status JSON
- `GET /swagger-ui` - Interactive API documentation

**CLI**:
- No direct CLI commands (server runs as daemon)
- Managed via service supervisor or systemd

**User Workflow**:
1. User logs in via web UI or API
2. JWT token stored in httpOnly cookie
3. Dashboard displays role-specific widgets
4. User navigates to specific pages for operations
5. All API calls authenticated via JWT

---

### 2. Service Supervisor

**Web UI**:
- **Service Panel** (`/service-panel`): Service management interface
  - Start/stop buttons for each service
  - Real-time status indicators (running/stopped)
  - Terminal output viewer for service logs
  - Service grouping (core vs monitoring)

**API Endpoints**:
- `GET /api/services` - List all services with status
- `POST /api/services/start` - Start a service
- `POST /api/services/stop` - Stop a service
- `GET /api/services/{id}/logs` - Get service logs
- `GET /api/health` - System health check

**CLI**:
- No direct CLI (managed via API)

**User Workflow**:
1. Access service panel via web UI
2. View service status (auto-refreshes every 3 seconds)
3. Click start/stop buttons to control services
4. Click service card to view terminal output
5. Monitor service health in real-time

---

### 3. Worker Process

**Web UI**:
- **Inference Playground** (`/inference`): Interactive inference testing
  - Prompt input field
  - Model/adapter selection
  - Real-time token generation
  - Response display with trace
  - Router decision visualization
- **Workers Tab** (`/workers`): Worker process management
  - List all worker processes
  - Worker health status
  - Spawn/stop workers
  - View worker logs

**API Endpoints**:
- `POST /v1/inference` - Run inference (OpenAI-compatible)
- `POST /v1/chat/completions` - OpenAI chat completions API
- `POST /v1/patch_proposal` - Generate code patches
- `GET /v1/workers` - List workers
- `POST /v1/workers/spawn` - Spawn worker (admin/operator)

**CLI**:
- `aosctl infer --prompt "..." --max-tokens 100` - Run inference
- `aosctl serve --plan <plan-id>` - Start worker with plan

**User Workflow**:
1. **Inference via UI**:
   - Navigate to Inference Playground
   - Enter prompt in text area
   - Select model and adapters (if applicable)
   - Click "Run Inference"
   - View generated text and trace
2. **Inference via API**:
   - POST to `/v1/inference` with prompt
   - Receive JSON response with text and trace
3. **Inference via CLI**:
   - Run `aosctl infer` command
   - View output in terminal

---

### 4. Router System

**Web UI**:
- **Router Config Page** (`/routing/config`): Router configuration
  - Feature weight sliders
  - K-sparse value adjustment
  - Entropy floor configuration
  - Router calibration tools
- **Routing Inspector** (`/routing/inspector`): Router decision analysis
  - Real-time router decisions
  - Adapter selection visualization
  - Gate value charts
  - Entropy metrics
- **Router History** (`/routing/history`): Historical routing decisions
  - Past router decisions
  - Adapter activation patterns
  - Performance metrics

**API Endpoints**:
- `GET /v1/routing/debug` - Debug router decisions
- `GET /v1/routing/history` - Get routing history
- `POST /v1/routing/decisions` - Get routing decisions for request
- `GET /v1/metrics/adapters` - Adapter metrics including router stats

**CLI**:
- `aosctl router calibrate --dataset <file> --output <weights.json>` - Calibrate router weights
- `aosctl router validate --dataset <file> --weights <weights.json>` - Validate weights
- `aosctl router show --weights <weights.json>` - Display router weights

**User Workflow**:
1. **Configure Router**:
   - Navigate to Router Config page
   - Adjust feature weights via sliders
   - Set K-sparse value and entropy floor
   - Save configuration
2. **Monitor Router**:
   - View Routing Inspector for real-time decisions
   - Check Router History for patterns
   - Analyze adapter activation metrics
3. **Calibrate Router**:
   - Prepare calibration dataset
   - Run `aosctl router calibrate`
   - Validate on test dataset
   - Deploy new weights

---

### 5. Policy Enforcement System

**Web UI**:
- **Policies Page** (`/policies`): Policy configuration
  - List all 22 policy packs
  - Enable/disable policy packs
  - Set enforcement levels (Info, Warning, Error, Critical)
  - View policy violations
  - Policy pack details and documentation
- **Audit Dashboard** (`/audit`): Policy compliance monitoring
  - Policy violation history
  - Compliance score
  - Violation trends
  - Remediation suggestions

**API Endpoints**:
- `GET /v1/policies` - List all policy packs
- `GET /v1/policies/{pack_id}` - Get policy pack details
- `PUT /v1/policies/{pack_id}` - Update policy pack config
- `GET /v1/audit/results` - Get audit results
- `POST /v1/audit/run` - Run audit suite

**CLI**:
- `aosctl policy list` - List all policy packs
- `aosctl policy show <pack_id>` - Show policy pack details
- `aosctl policy enable <pack_id>` - Enable policy pack
- `aosctl policy disable <pack_id>` - Disable policy pack
- `aosctl audit` - Run audit suite

**User Workflow**:
1. **Configure Policies**:
   - Navigate to Policies page
   - Review policy pack list
   - Enable/disable packs as needed
   - Set enforcement levels
   - Save configuration
2. **Monitor Compliance**:
   - View Audit Dashboard
   - Check compliance score
   - Review violation history
   - Address violations
3. **Run Audits**:
   - Use CLI: `aosctl audit`
   - Or API: `POST /v1/audit/run`
   - Review audit results

---

### 6. Memory Management System

**Web UI**:
- **Adapter Memory Monitor** (`/adapters/memory`): Memory usage visualization
  - Memory usage chart
  - Headroom percentage
  - Adapter memory breakdown
  - Eviction history
- **Adapter Lifecycle Manager** (`/adapters/lifecycle`): Adapter state management
  - List all adapters with states
  - Load/unload adapters
  - Pin/unpin adapters
  - View adapter memory usage

**API Endpoints**:
- `GET /v1/adapters` - List adapters (includes memory info)
- `POST /v1/adapters/{id}/load` - Load adapter into memory
- `POST /v1/adapters/{id}/unload` - Unload adapter from memory
- `GET /v1/metrics/system` - System metrics including memory

**CLI**:
- `aosctl list-adapters` - List adapters with memory info
- `aosctl pin <adapter-id>` - Pin adapter (prevent eviction)
- `aosctl unpin <adapter-id>` - Unpin adapter

**User Workflow**:
1. **Monitor Memory**:
   - View Adapter Memory Monitor
   - Check headroom percentage
   - Review adapter memory usage
   - Monitor eviction events
2. **Manage Adapters**:
   - Use Adapter Lifecycle Manager
   - Load adapters into memory
   - Unload adapters to free memory
   - Pin critical adapters
3. **Troubleshoot**:
   - Check memory pressure levels
   - Review eviction logs
   - Adjust headroom threshold if needed

---

### 7. RAG System

**Web UI**:
- **Code Intelligence** (`/code-intelligence`): Repository analysis
  - Repository registration
  - Code analysis results
  - Symbol extraction
  - Evidence retrieval testing
- **Inference Playground** (with RAG): Evidence-grounded inference
  - Enable evidence retrieval
  - View retrieved documents
  - See citations in response

**API Endpoints**:
- `GET /v1/rag/retrievals` - List RAG retrievals
- `GET /v1/rag/stats` - RAG statistics
- `POST /v1/inference` (with `require_evidence: true`) - Evidence-grounded inference

**CLI**:
- No direct CLI commands (integrated into inference)

**User Workflow**:
1. **Setup RAG**:
   - Register repository via Code Intelligence page
   - Wait for code analysis to complete
   - Verify embeddings are created
2. **Use RAG**:
   - Enable "Require Evidence" in Inference Playground
   - Enter query
   - View retrieved documents
   - See citations in generated response

---

### 8. Telemetry System

**Web UI**:
- **Telemetry Page** (`/telemetry`): Event logs and bundles
  - Recent activity feed
  - Telemetry bundle list
  - Bundle export/download
  - Bundle signature verification
  - Event filtering and search
- **Observability Dashboard** (`/observability`): System observability
  - Real-time metrics
  - Event streams
  - Performance charts

**API Endpoints**:
- `GET /v1/telemetry/bundles` - List telemetry bundles
- `POST /v1/telemetry/bundles/generate` - Generate new bundle
- `GET /v1/telemetry/bundles/{id}/export` - Export bundle
- `POST /v1/telemetry/bundles/{id}/verify` - Verify bundle signature
- `GET /v1/telemetry/stream` - SSE stream of events
- `GET /v1/activity` - Recent activity events

**CLI**:
- `aosctl telemetry list` - List telemetry bundles
- `aosctl telemetry export <bundle-id>` - Export bundle
- `aosctl telemetry verify <bundle-id>` - Verify bundle signature
- `aosctl replay <bundle-path>` - Replay bundle

**User Workflow**:
1. **View Telemetry**:
   - Navigate to Telemetry page
   - Browse recent activity
   - Filter by event type, tenant, time range
   - Search for specific events
2. **Manage Bundles**:
   - View bundle list
   - Generate new bundle
   - Export bundle for analysis
   - Verify bundle signatures
3. **Replay**:
   - Export bundle via UI or CLI
   - Use `aosctl replay` to replay bundle
   - Verify deterministic execution

---

### 9. Training System

**Web UI**:
- **Training Page** (`/training`): Training job management
  - Training job list
  - Start new training job
  - View training logs
  - Monitor training progress
  - Training metrics and charts
- **Training Wizard** (`/training/wizard`): Guided training setup
  - Step-by-step training configuration
  - Dataset selection
  - Hyperparameter tuning
  - Adapter packaging options

**API Endpoints**:
- `GET /v1/training/jobs` - List training jobs
- `POST /v1/training/start` - Start training job
- `GET /v1/training/jobs/{id}` - Get job details
- `GET /v1/training/jobs/{id}/logs` - Get training logs
- `GET /v1/training/jobs/{id}/metrics` - Get training metrics
- `POST /v1/training/jobs/{id}/cancel` - Cancel training job

**CLI**:
- `aosctl train --data <dataset.json> --output <dir> --rank 16 --epochs 1` - Train adapter
- `aosctl train --directory <path> --adapter-id <id>` - Train from directory

**User Workflow**:
1. **Start Training**:
   - Use Training Wizard or CLI
   - Configure dataset, rank, epochs
   - Start training job
2. **Monitor Training**:
   - View training page
   - Check job status
   - View training logs
   - Monitor metrics
3. **Complete Training**:
   - Wait for job completion
   - Review training metrics
   - Package adapter (if enabled)
   - Register adapter (if enabled)

---

### 10. Database System

**Web UI**:
- **Database Management** (Admin only): Database administration
  - Migration status
  - Database schema viewer
  - Query interface (read-only)
  - Backup/restore tools

**API Endpoints**:
- No direct database endpoints (internal only)
- Data accessed via domain-specific endpoints

**CLI**:
- `aosctl db migrate` - Run database migrations (internal)
- Database operations handled automatically

**User Workflow**:
- Database is managed automatically
- Migrations run on server startup
- Users interact via domain APIs, not directly with database

---

### 11. Configuration System

**Web UI**:
- **Settings Page** (`/settings`): System configuration
  - Configuration file editor
  - Config validation
  - Hot-reload trigger
  - Configuration history

**API Endpoints**:
- `GET /v1/config` - Get current configuration
- `PUT /v1/config` - Update configuration (admin only)
- `POST /v1/config/reload` - Trigger config reload (SIGHUP)

**CLI**:
- Configuration managed via `configs/cp.toml` file
- Server reads config on startup
- SIGHUP signal triggers reload

**User Workflow**:
1. **Edit Config**:
   - Edit `configs/cp.toml` file
   - Or use Settings page (admin only)
2. **Reload Config**:
   - Send SIGHUP: `kill -HUP <pid>`
   - Or use API: `POST /v1/config/reload`
   - Config reloaded without restart

---

### 12. Authentication & Authorization

**Web UI**:
- **Login Page** (`/login`): User authentication
  - Email/password login
  - JWT token management
  - Session management
- **User Profile** (`/profile`): User settings
  - Profile information
  - Password change
  - Token rotation
  - Active sessions

**API Endpoints**:
- `POST /v1/auth/login` - User login
- `POST /v1/auth/logout` - User logout
- `POST /v1/auth/refresh` - Refresh JWT token
- `GET /v1/auth/sessions` - List active sessions
- `POST /v1/auth/rotate-token` - Rotate JWT token

**CLI**:
- `aosctl bootstrap-admin --email <email>` - Create admin user

**User Workflow**:
1. **Login**:
   - Navigate to login page
   - Enter email and password
   - Receive JWT token (stored in httpOnly cookie)
2. **Access Control**:
   - Role-based access enforced
   - Admin: Full access
   - Operator: Operational tasks
   - SRE: Monitoring and observability
3. **Session Management**:
   - View active sessions
   - Revoke sessions
   - Rotate tokens

---

## Role-Based Access Summary

| Feature | Admin | Operator | SRE | Compliance | Auditor | Viewer |
|---------|-------|----------|-----|------------|---------|--------|
| **Dashboard** | Full | Limited | Monitoring | Audit | Read-only | Read-only |
| **Adapters** | Full | Manage | View | View | View | View |
| **Training** | Full | Start/Monitor | View | View | View | View |
| **Inference** | Full | Run | View | View | View | View |
| **Policies** | Configure | View | View | Manage | View | View |
| **Telemetry** | Full | View | View | View | Full | View |
| **Audit** | Full | View | View | Full | Full | View |
| **Settings** | Full | View | View | View | View | View |
| **Users** | Manage | View | View | View | View | View |

---

## User Interface Patterns

### Progressive Disclosure
- Basic features visible by default
- Advanced features behind toggle
- Contextual help tooltips
- Role-based feature visibility

### Real-Time Updates
- WebSocket/SSE for live data
- Auto-refresh for status pages
- Polling for metrics (configurable interval)
- Event streams for telemetry

### Error Handling
- User-friendly error messages
- Error code explanations (`aosctl explain <code>`)
- Recovery suggestions
- Error logging to telemetry

### Navigation
- Role-based navigation menus
- Breadcrumb navigation
- Command palette (Cmd+K)
- Quick actions on dashboard

---

## User Interaction Guide (How Users Should Interact)

### Getting Started Paths by Role

#### Admin - First Time Setup

**Recommended Workflow**:
1. **Bootstrap System**:
   ```bash
   # Create admin user
   aosctl bootstrap-admin --email admin@example.com
   
   # Bootstrap installation
   aosctl bootstrap --mode full
   ```

2. **Login to Web UI**:
   - Navigate to `http://localhost:8080/login`
   - Enter admin credentials
   - Review dashboard widgets

3. **Configure Policies**:
   - Navigate to `/policies`
   - Review all 22 policy packs
   - Enable critical packs (Egress, Determinism, Isolation)
   - Set enforcement levels

4. **Setup Monitoring**:
   - Navigate to `/monitoring`
   - Configure alert thresholds
   - Set up notification channels
   - Review system health

5. **Verify System**:
   ```bash
   # Run diagnostics
   aosctl diag --full
   
   # Verify telemetry
   aosctl telemetry verify
   ```

**Time to Complete**: 15-30 minutes

---

#### Operator - ML Operations Workflow

**Recommended Workflow**:
1. **Review Dashboard**:
   - Check ML pipeline status
   - Review adapter status
   - Check for active alerts
   - Review "Next Steps" widget

2. **Import Base Model** (if not done):
   ```bash
   aosctl import-model \
     --name qwen2.5-7b \
     --weights models/qwen2.5-7b-mlx/weights.safetensors \
     --config models/qwen2.5-7b-mlx/config.json \
     --tokenizer models/qwen2.5-7b-mlx/tokenizer.json
   ```

3. **Train Adapter**:
   - **Via UI**: Navigate to `/training/wizard`
     - Upload training data
     - Configure hyperparameters
     - Start training job
     - Monitor progress
   - **Via CLI**:
     ```bash
     aosctl train \
       --data data/training.json \
       --output out/adapter1 \
       --rank 16 --epochs 3 \
       --base-model qwen2.5-7b \
       --pack --register
     ```

4. **Test Adapter**:
   - Navigate to `/inference`
   - Select trained adapter
   - Enter test prompt
   - Review response and trace

5. **Promote Adapter**:
   - Navigate to `/promotion`
   - Review quality gates
   - Submit for promotion
   - Monitor CAB approval

**Time to Complete**: 30-60 minutes (excluding training time)

---

#### SRE - System Reliability Workflow

**Recommended Workflow**:
1. **Monitor System Health**:
   - Navigate to `/monitoring`
   - Review real-time metrics
   - Check alert status
   - Review resource utilization

2. **Inspect Routing**:
   - Navigate to `/routing/inspector`
   - Monitor router decisions
   - Check adapter activation patterns
   - Review entropy metrics

3. **Analyze Telemetry**:
   - Navigate to `/telemetry`
   - Filter events by type/time
   - Export bundles for analysis
   - Verify bundle signatures

4. **Troubleshoot Issues**:
   ```bash
   # Run diagnostics
   aosctl diag --full --bundle ./diag.zip
   
   # Check system status
   curl http://localhost:8080/api/v1/status
   
   # Review logs
   aosctl logs --service adapteros-server
   ```

5. **Respond to Incidents**:
   - Review alert details
   - Check system metrics
   - Review telemetry events
   - Document resolution

**Time to Complete**: 5-15 minutes per check

---

### Detailed Interaction Patterns

#### 1. Control Plane Server

**When to Use**:
- Daily operations (primary interface)
- System administration
- Multi-user collaboration
- Real-time monitoring

**Interaction Pattern**:
```
Login → Dashboard → Navigate to Feature → Perform Action → Review Results
```

**Best Practices**:
- Start with dashboard to check system status
- Use quick actions for common tasks
- Bookmark frequently used pages
- Use command palette (Cmd+K) for quick navigation
- Enable notifications for alerts

**Common Tasks**:
1. **Daily Check-in** (2 minutes):
   - Login → Dashboard
   - Review service status widget
   - Check active alerts
   - Review compliance score

2. **System Configuration** (10 minutes):
   - Navigate to Settings
   - Edit configuration
   - Trigger reload
   - Verify changes

3. **User Management** (5 minutes):
   - Navigate to Admin → Users
   - Create/update users
   - Assign roles
   - Review active sessions

---

#### 2. Worker Process (Inference)

**When to Use**:
- Testing adapters
- Running inference requests
- Debugging model behavior
- Performance testing

**Interaction Pattern**:
```
Select Model → Enter Prompt → Configure Options → Run → Review Response
```

**Best Practices**:
- Use Inference Playground for interactive testing
- Use API for programmatic access
- Use CLI for batch operations
- Review trace information for debugging
- Monitor router decisions

**Common Tasks**:
1. **Quick Test** (1 minute):
   - Navigate to `/inference`
   - Enter prompt: "Write a hello world function"
   - Click "Run Inference"
   - Review response

2. **Adapter Testing** (5 minutes):
   - Select specific adapter
   - Enter test prompts
   - Compare responses
   - Review router decisions

3. **Batch Inference** (via API):
   ```bash
   curl -X POST http://localhost:8080/v1/inference \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"prompt": "Your prompt", "max_tokens": 100}'
   ```

---

#### 3. Router System

**When to Use**:
- Tuning adapter selection
- Debugging routing decisions
- Calibrating feature weights
- Analyzing adapter usage

**Interaction Pattern**:
```
Monitor → Analyze → Calibrate → Validate → Deploy
```

**Best Practices**:
- Start with Routing Inspector to understand current behavior
- Use Router History to identify patterns
- Calibrate on representative dataset
- Validate on test set before deploying
- Monitor entropy metrics

**Common Tasks**:
1. **Monitor Router** (5 minutes):
   - Navigate to `/routing/inspector`
   - Watch real-time decisions
   - Check adapter activation
   - Review entropy values

2. **Calibrate Weights** (30 minutes):
   ```bash
   # Prepare calibration dataset
   # Run calibration
   aosctl router calibrate \
     --dataset calibration.json \
     --output weights.json
   
   # Validate on test set
   aosctl router validate \
     --dataset test.json \
     --weights weights.json
   
   # Deploy new weights
   # (Update config or via API)
   ```

3. **Debug Routing** (10 minutes):
   - Navigate to `/routing/inspector`
   - Enter test prompt
   - Review feature extraction
   - Check adapter scores
   - Verify gate values

---

#### 4. Policy Enforcement System

**When to Use**:
- Initial system setup
- Compliance audits
- Security reviews
- Policy updates

**Interaction Pattern**:
```
Review Policies → Configure → Test → Deploy → Monitor
```

**Best Practices**:
- Start with default policy configuration
- Enable policies incrementally
- Test in non-production first
- Monitor violation rates
- Review audit dashboard regularly

**Common Tasks**:
1. **Initial Setup** (15 minutes):
   - Navigate to `/policies`
   - Review all policy packs
   - Enable critical packs
   - Set enforcement levels
   - Save configuration

2. **Compliance Audit** (30 minutes):
   - Navigate to `/audit`
   - Run audit suite
   - Review compliance score
   - Address violations
   - Generate compliance report

3. **Policy Update** (10 minutes):
   - Navigate to `/policies`
   - Update policy configuration
   - Test changes
   - Deploy to production
   - Monitor for issues

---

#### 5. Memory Management System

**When to Use**:
- Memory pressure issues
- Adapter lifecycle management
- Performance optimization
- Resource planning

**Interaction Pattern**:
```
Monitor → Identify Issue → Take Action → Verify → Document
```

**Best Practices**:
- Monitor headroom regularly
- Pin critical adapters
- Unload unused adapters
- Review eviction history
- Plan adapter loading order

**Common Tasks**:
1. **Monitor Memory** (2 minutes):
   - Navigate to `/adapters/memory`
   - Check headroom percentage
   - Review adapter memory usage
   - Check eviction history

2. **Manage Adapters** (5 minutes):
   - Navigate to `/adapters/lifecycle`
   - Review adapter states
   - Pin critical adapters
   - Unload unused adapters
   - Load required adapters

3. **Troubleshoot Pressure** (10 minutes):
   - Check memory pressure level
   - Review eviction logs
   - Identify memory hogs
   - Adjust headroom threshold if needed
   - Document resolution

---

#### 6. Training System

**When to Use**:
- Creating new adapters
- Fine-tuning for specific tasks
- Updating existing adapters
- Batch training operations

**Interaction Pattern**:
```
Prepare Data → Configure → Train → Monitor → Test → Deploy
```

**Best Practices**:
- Use Training Wizard for first-time training
- Start with small datasets for testing
- Monitor training metrics
- Validate on test set
- Package and register adapters

**Common Tasks**:
1. **Quick Training** (via UI, 15 minutes):
   - Navigate to `/training/wizard`
   - Upload training file
   - Configure parameters (rank: 8, epochs: 3)
   - Start training
   - Monitor progress
   - Test trained adapter

2. **Production Training** (via CLI, 1-2 hours):
   ```bash
   # Prepare dataset
   # Train adapter
   aosctl train \
     --data data/production.json \
     --output out/prod_adapter \
     --rank 16 --epochs 5 \
     --base-model qwen2.5-7b \
     --pack --register \
     --adapter-id prod_adapter_v1
   
   # Monitor training
   # (Check via UI or CLI)
   ```

3. **Directory-Based Training** (via API):
   ```bash
   curl -X POST http://localhost:8080/v1/training/start \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "directory_root": "/path/to/repo",
       "directory_path": "src",
       "adapter_id": "repo_adapter",
       "config": {"rank": 8, "epochs": 1}
     }'
   ```

---

#### 7. Telemetry System

**When to Use**:
- Debugging issues
- Compliance audits
- Performance analysis
- Incident investigation

**Interaction Pattern**:
```
Search Events → Filter → Export → Analyze → Document
```

**Best Practices**:
- Use filters to narrow down events
- Export bundles for offline analysis
- Verify bundle signatures
- Replay bundles for verification
- Archive important bundles

**Common Tasks**:
1. **Quick Event Search** (2 minutes):
   - Navigate to `/telemetry`
   - Enter search query
   - Filter by type/time
   - Review events

2. **Export Bundle** (5 minutes):
   - Navigate to `/telemetry`
   - Select time range
   - Generate bundle
   - Download bundle
   - Verify signature

3. **Replay Bundle** (10 minutes):
   ```bash
   # Export bundle via UI
   # Replay bundle
   aosctl replay var/bundles/bundle_001.ndjson
   
   # Verify deterministic execution
   # Compare outputs
   ```

---

### Progressive Learning Paths

#### Beginner (First Week)

**Day 1-2: Setup**
- Bootstrap system
- Login to web UI
- Explore dashboard
- Review documentation

**Day 3-4: Basic Operations**
- Run inference via playground
- View adapter list
- Check system status
- Review telemetry

**Day 5-7: Training**
- Train first adapter
- Test adapter
- Review training metrics
- Understand adapter lifecycle

---

#### Intermediate (Weeks 2-4)

**Week 2: Router & Policies**
- Understand router system
- Configure router weights
- Review policy packs
- Run compliance audit

**Week 3: Advanced Training**
- Train from directories
- Calibrate router weights
- Optimize adapter selection
- Monitor memory usage

**Week 4: Production Operations**
- Deploy adapters
- Monitor system health
- Respond to alerts
- Generate reports

---

#### Advanced (Month 2+)

**Advanced Topics**:
- Custom policy packs
- Router calibration
- Performance optimization
- Multi-tenant management
- Federation setup

---

### Common Use Cases

#### Use Case 1: Train Adapter for Code Generation

**Steps**:
1. Prepare code dataset (JSON format)
2. Navigate to `/training/wizard`
3. Upload dataset
4. Configure: rank=16, epochs=3
5. Start training
6. Monitor progress
7. Test with code prompts
8. Register adapter
9. Use in inference

**Time**: 30-60 minutes (excluding training time)

---

#### Use Case 2: Debug Inference Issue

**Steps**:
1. Navigate to `/inference`
2. Reproduce issue
3. Review trace information
4. Check router decisions
5. Review telemetry events
6. Filter events by request ID
7. Export bundle for analysis
8. Replay bundle to verify

**Time**: 10-20 minutes

---

#### Use Case 3: Optimize Memory Usage

**Steps**:
1. Navigate to `/adapters/memory`
2. Review memory usage
3. Identify unused adapters
4. Unload unused adapters
5. Pin critical adapters
6. Monitor headroom
7. Adjust eviction policy if needed

**Time**: 5-10 minutes

---

#### Use Case 4: Compliance Audit

**Steps**:
1. Navigate to `/policies`
2. Review policy configuration
3. Navigate to `/audit`
4. Run audit suite
5. Review compliance score
6. Address violations
7. Generate compliance report
8. Export telemetry bundle

**Time**: 30-60 minutes

---

### Error Handling & Troubleshooting

#### When Things Go Wrong

**Pattern**:
```
Error → Explain → Diagnose → Fix → Verify
```

**Tools**:
- `aosctl explain <error-code>` - Get error explanation
- `aosctl diag --full` - Run full diagnostics
- Telemetry search - Find related events
- Logs - Review service logs

**Common Issues**:
1. **Memory Pressure**:
   - Check headroom percentage
   - Review adapter states
   - Unload unused adapters
   - Adjust headroom threshold

2. **Policy Violations**:
   - Review violation details
   - Check policy configuration
   - Adjust enforcement levels
   - Address root cause

3. **Training Failures**:
   - Check dataset format
   - Verify file size limits
   - Review training logs
   - Check resource availability

---

### Best Practices Summary

#### For All Users
- Start with dashboard to check system status
- Use UI for interactive tasks, CLI for automation
- Monitor telemetry regularly
- Review alerts promptly
- Document custom configurations

#### For Operators
- Test adapters before production
- Monitor training metrics
- Use router calibration for optimization
- Review inference traces for debugging
- Keep adapters organized by tier

#### For SREs
- Set up monitoring dashboards
- Configure alert thresholds
- Review system metrics daily
- Maintain incident runbooks
- Export telemetry bundles regularly

#### For Admins
- Configure policies on initial setup
- Review compliance regularly
- Manage user access carefully
- Monitor system health proactively
- Document system changes

---

**Last Updated**: 2025-01-15
