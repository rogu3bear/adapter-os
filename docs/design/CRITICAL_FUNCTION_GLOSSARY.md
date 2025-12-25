# Critical Function Glossary

> **Validated**: All locations and signatures verified against source code.

## Quick Reference

| Entity | Purpose | Location (Verified) |
|--------|---------|---------------------|
| **Worker** | Inference/training execution | `lora-worker/src/lib.rs:1109` |
| **Tenant** | Multi-tenancy isolation | `core/src/tenant.rs:65` |
| **Adapter** | LoRA adapter lifecycle | `types/src/adapters/metadata.rs:53` |
| **Model** | Base model management | `db/src/models.rs` |
| **Backend** | Execution backend | `core/src/backend.rs:15` |
| **WorkerStatus** | Worker lifecycle states | `core/src/worker_status.rs:55` |

---

## 1. WORKERS

### Worker Struct
**Location:** `crates/adapteros-lora-worker/src/lib.rs:1109`

```rust
pub struct Worker<K: FusedKernels + StrictnessControl + Send + Sync> {
    manifest: ManifestV3,
    policy: PolicyEngine,
    router: Router,
    kernels: Arc<Mutex<K>>,
    kv_cache: Arc<Mutex<KvCache>>,
    hotswap: Arc<HotSwapManager<K>>,
    worker_id: u32,
    // ... safety mechanisms
}
```

### Worker Lifecycle State Machine
**Location:** `crates/adapteros-core/src/worker_status.rs:55`

```mermaid
stateDiagram-v2
    [*] --> Created: Process launched
    Created --> Registered: CP accepts registration
    Created --> Error: Config/init failure
    Registered --> Healthy: UDS listening
    Registered --> Error: Bind failure
    Healthy --> Draining: SIGINT/shutdown
    Healthy --> Error: Health check failure
    Draining --> Stopped: Clean shutdown
    Draining --> Error: Drain timeout
    Stopped --> [*]
    Error --> [*]
```

**Valid Transitions (from source):**
```rust
WorkerStatus::Created    => [Registered, Error]
WorkerStatus::Registered => [Healthy, Error]
WorkerStatus::Healthy    => [Draining, Error]
WorkerStatus::Draining   => [Stopped, Error]
WorkerStatus::Stopped    => []  // terminal
WorkerStatus::Error      => []  // terminal
```

### Worker Communication Flow

```mermaid
sequenceDiagram
    participant CP as Control Plane
    participant W as Worker
    participant UDS as UDS Server
    participant K as Kernels

    W->>CP: POST /workers/register
    CP-->>W: {heartbeat_interval, kv_quota}
    W->>CP: POST /workers/status (Registered)
    W->>UDS: Bind socket
    W->>CP: POST /workers/status (Healthy)

    loop Inference
        UDS->>W: InferRequest (JSON)
        W->>K: forward()
        K-->>W: tokens
        W-->>UDS: InferResponse
    end

    W->>CP: POST /workers/status (Draining)
    W->>CP: POST /workers/status (Stopped)
```

### Kernel Types

| Type | Location | Purpose |
|------|----------|---------|
| `DirectKernels` | lib.rs:335 | Single backend, no fallback |
| `CoordinatedKernels` | lib.rs:351 | Primary + fallback |
| `KernelWrapper` | lib.rs:380 | Unified enum |

---

## 2. BACKENDS

### BackendKind Enum
**Location:** `crates/adapteros-core/src/backend.rs:15`

```rust
pub enum BackendKind {
    Auto,      // Deterministic auto-selection
    CoreML,    // CoreML/ANE (macOS)
    Mlx,       // MLX FFI
    MlxBridge, // MLX subprocess (MoE)
    Metal,     // Metal GPU
    CPU,       // CPU-only
}
```

### Backend Fallback Chain

```mermaid
flowchart LR
    Auto --> CoreML
    CoreML -->|unavailable| Mlx
    Mlx -->|unavailable| MlxBridge
    MlxBridge -->|unavailable| Metal
    Metal -->|unavailable| CPU

    style CoreML fill:#4a9
    style CPU fill:#a44
```

**From source** (`backend.rs:64-74`):
```rust
pub fn inference_priority() -> &'static [BackendKind] {
    static ORDER: [BackendKind; 5] = [
        BackendKind::CoreML,
        BackendKind::Mlx,
        BackendKind::MlxBridge,
        BackendKind::Metal,
        BackendKind::CPU,
    ];
    &ORDER
}
```

---

## 3. TENANTS

### TenantId
**Location:** `crates/adapteros-core/src/tenant.rs:65`

```rust
pub struct TenantId(String);  // 1-64 chars, alphanumeric
```

**Validation Rules:**
- 1-64 characters
- Start/end with alphanumeric
- May contain `-` and `_`
- No path traversal (`..`, `/`, `\`)

**Special Values:**
- `"primary"` - Single-tenant default
- `"system"` - System operations

### Tenant Isolation Flow

```mermaid
flowchart TD
    REQ[Request with JWT] --> EXT[Extract claims.tenant_id]
    EXT --> VAL{validate_tenant_isolation}

    VAL --> R1{Same tenant?}
    R1 -->|Yes| ALLOW[✓ Allow]
    R1 -->|No| R2{Admin + dev mode?}

    R2 -->|Yes| ALLOW
    R2 -->|No| R3{Admin wildcard *?}

    R3 -->|Yes| ALLOW
    R3 -->|No| R4{Explicit grant?}

    R4 -->|Yes| ALLOW
    R4 -->|No| DENY[✗ Deny 403]

    style ALLOW fill:#4a9
    style DENY fill:#a44
```

### Critical Isolation Functions

| Function | Location | Signature |
|----------|----------|-----------|
| `validate_tenant_isolation` | `server-api/src/security/mod.rs:125` | `(claims: &Claims, resource_tenant_id: &str) -> Result<()>` |
| `check_tenant_access` | `server-api/src/security/mod.rs` | `(claims: &Claims, resource_tenant_id: &str) -> bool` |

### Tenant-Scoped DB Queries (All include `WHERE tenant_id = ?`)

| Function | Location |
|----------|----------|
| `list_adapters_for_tenant()` | `db/src/adapters.rs` |
| `get_adapter_for_tenant()` | `db/src/adapters.rs` |
| `get_training_jobs_for_tenant()` | `db/src/training_jobs.rs` |
| `list_workers_by_tenant()` | `db/src/workers.rs` |

---

## 4. ADAPTERS

### AdapterMetadata
**Location:** `crates/adapteros-types/src/adapters/metadata.rs:53`

```rust
pub struct AdapterMetadata {
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,      // BLAKE3 content hash
    pub rank: i32,            // LoRA rank
    pub tier: i32,            // Memory tier
    pub languages: Vec<String>,
    pub framework: Option<String>,
    pub domain: Option<String>,
    pub scope_path: Option<String>,
    pub lora_tier: Option<LoraTier>,
    pub lora_strength: Option<f32>,
}
```

### Adapter Lifecycle States
**Location:** `crates/adapteros-types/src/adapters/metadata.rs:227`

```mermaid
stateDiagram-v2
    [*] --> Registered: register_adapter()
    Registered --> Loading: load request
    Loading --> Active: load complete
    Loading --> Error: load failure
    Active --> Inactive: deactivate
    Active --> Unloading: unload request
    Inactive --> Active: reactivate
    Inactive --> Unloading: unload
    Unloading --> Unloaded: unload complete
    Unloaded --> Loading: reload
    Unloaded --> Expired: TTL expired
    Expired --> [*]: cleanup
    Error --> [*]: cleanup
```

**From source** (`metadata.rs:227-251`):
```rust
pub enum LifecycleState {
    Registered,  // Registered but not loaded
    Loading,     // Currently loading
    Active,      // Ready for inference
    Inactive,    // Temporarily inactive
    Unloading,   // Being unloaded
    Unloaded,    // Unloaded from memory
    Expired,     // Marked for deletion
    Error,       // Error state
}
```

### Memory Tiers

| Tier | Value | Location | Latency |
|------|-------|----------|---------|
| Metal GPU | 0 | VRAM | ~1ms |
| System RAM | 1 | RAM | ~5ms |
| Disk | 2 | SSD/NVMe | ~50ms |

---

## 5. ENTITY RELATIONSHIPS

```mermaid
erDiagram
    TENANT ||--o{ WORKER : spawns
    TENANT ||--o{ ADAPTER : owns
    TENANT ||--o{ TRAINING_JOB : owns
    TENANT ||--o{ CHAT_SESSION : owns
    TENANT ||--o{ INFERENCE_TRACE : owns

    WORKER ||--|| KERNELS : uses
    WORKER ||--|| ROUTER : uses
    WORKER ||--|| KV_CACHE : manages
    WORKER ||--|| HOTSWAP_MGR : uses

    ADAPTER }o--|| MODEL : references
    ADAPTER ||--o| TRAINING_JOB : produced_by

    TRAINING_JOB }o--|| MODEL : trains_on
    TRAINING_JOB }o--o{ DATASET : uses

    INFER_REQUEST ||--o{ ADAPTER : routes_to
    INFER_REQUEST ||--|| INFER_RESPONSE : produces
    INFER_RESPONSE ||--o| INFERENCE_TRACE : contains

    CHAT_SESSION ||--o{ CHAT_MESSAGE : contains
```

---

## 6. REQUEST/RESPONSE FLOW

### Inference Pipeline

```mermaid
sequenceDiagram
    participant C as Client
    participant H as Handler
    participant S as Security
    participant W as Worker
    participant R as Router
    participant K as Kernel

    C->>H: POST /infer {prompt, stack_id}
    H->>S: validate_tenant_isolation()
    S-->>H: OK
    H->>W: infer(request)
    W->>R: route(prompt, adapters)
    R-->>W: selected_adapters, gates
    W->>K: forward(tokens, adapters)
    K-->>W: output_tokens
    W-->>H: InferResponse
    H-->>C: {text, adapters_used, receipt}
```

### Training Pipeline

```mermaid
sequenceDiagram
    participant C as Client
    participant H as Handler
    participant DB as Database
    participant W as Worker
    participant T as Trainer

    C->>H: POST /training/jobs
    H->>DB: create_training_job(Pending)
    DB-->>H: job_id
    H-->>C: {job_id, status: Pending}

    W->>DB: poll for pending jobs
    DB-->>W: job
    W->>DB: update_status(Running)
    W->>T: train(config, dataset)

    loop Epochs
        T->>T: forward/backward
        T->>DB: update_progress()
    end

    T-->>W: adapter_weights
    W->>DB: register_adapter()
    W->>DB: update_status(Completed)
```

---

## 7. CRITICAL FUNCTIONS BY IMPACT

### Changes to Worker affect:

| Function | Location | Impact |
|----------|----------|--------|
| `Worker::new()` | lib.rs:1160 | Initialization, kernel setup |
| `Worker::infer()` | lib.rs:1678 | All inference requests |
| `UdsServer::handle_request()` | uds_server.rs | IPC protocol |
| `StrictnessControl` impls | lib.rs:265 | Backend selection |

### Changes to Tenant affect:

| Function | Location | Impact |
|----------|----------|--------|
| `validate_tenant_isolation()` | security/mod.rs:125 | Every endpoint |
| `list_*_for_tenant()` | db/*.rs | All list queries |
| `TenantKvQuotaManager` | kv_quota.rs | Cache allocation |
| `NodeAgent::spawn_worker()` | node/agent.rs | Worker isolation |

### Changes to Adapter affect:

| Function | Location | Impact |
|----------|----------|--------|
| `Router::route()` | router/mod.rs | Adapter selection |
| `HotSwapManager::swap()` | hot_swap.rs | Runtime loading |
| `register_adapter()` | db/adapters.rs | Adapter creation |
| Training packager | training/packager.rs | Adapter output |

### Changes to Backend affect:

| Function | Location | Impact |
|----------|----------|--------|
| `BackendKind::inference_priority()` | backend.rs:64 | Fallback order |
| Kernel creation | aos_worker.rs | Worker init |
| `CoordinatedKernels` | lib.rs:351 | Fallback logic |
| `FusedKernels` impls | kernel crates | All inference |

---

## 8. CONTENT ADDRESSING

All major entities use BLAKE3 hashing for integrity:

```mermaid
flowchart LR
    subgraph Adapter
        AW[Weights] --> AH[hash_b3]
    end

    subgraph Model
        MW[Weights] --> MH[hash_b3]
        MC[Config] --> MCH[config_hash_b3]
        MT[Tokenizer] --> MTH[tokenizer_hash_b3]
    end

    subgraph Inference
        SP[StopPolicy] --> SPH[stop_policy_digest_b3]
        PR[Prompt+Params] --> PRH[prompt_system_params_digest_b3]
    end
```

---

## 9. DETERMINISTIC EXECUTION

```mermaid
flowchart TD
    subgraph Request
        SEED[seed: u64]
        MODE[routing_determinism_mode]
        STOP[stop_policy]
    end

    subgraph Execution
        RNG[Seeded RNG]
        ROUTER[Deterministic Router]
        SAMPLER[Deterministic Sampler]
    end

    subgraph Output
        RECEIPT[DeterministicReceipt]
        TRACE[InferenceTrace]
    end

    SEED --> RNG
    MODE --> ROUTER
    STOP --> SAMPLER

    RNG --> ROUTER
    ROUTER --> SAMPLER
    SAMPLER --> RECEIPT
    SAMPLER --> TRACE
```

**Key fields for reproducibility:**
- `InferRequest.seed` - Random seed
- `InferRequest.routing_determinism_mode` - Router behavior
- `InferRequest.stop_policy` - Deterministic stop
- `DeterministicReceipt` - Audit trail

---

## 10. QUICK LOOKUP TABLE

| What | Where | Line |
|------|-------|------|
| Worker struct | `lora-worker/src/lib.rs` | 1109 |
| Worker::new() | `lora-worker/src/lib.rs` | 1160 |
| Worker::infer() | `lora-worker/src/lib.rs` | 1678 |
| WorkerStatus enum | `core/src/worker_status.rs` | 55 |
| BackendKind enum | `core/src/backend.rs` | 15 |
| inference_priority() | `core/src/backend.rs` | 64 |
| TenantId struct | `core/src/tenant.rs` | 65 |
| validate_tenant_isolation() | `server-api/src/security/mod.rs` | 125 |
| AdapterMetadata struct | `types/src/adapters/metadata.rs` | 53 |
| LifecycleState enum | `types/src/adapters/metadata.rs` | 227 |
| UdsServer struct | `lora-worker/src/uds_server.rs` | 63 |
| Router struct | `lora-router/src/lib.rs` | - |
| HotSwapManager | `aos/src/hot_swap.rs` | - |
