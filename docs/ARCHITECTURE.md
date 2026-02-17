# adapterOS Architecture

**Canonical reference for adapterOS system architecture, concepts, and workflows**

**Last Updated:** 2026-01-13

---

## Table of Contents

1. [System Overview](#system-overview)
2. [Core Concepts](#core-concepts)
3. [Architecture Components](#architecture-components)
4. [Cryptographic Receipts & Sealed Adapters](#cryptographic-receipts--sealed-adapters)
5. [Inference Flow](#inference-flow)
6. [Adapter Lifecycle](#adapter-lifecycle)
7. [User Flows](#user-flows)
8. [Glossary](#glossary)

---

## System Overview

adapterOS is an ML inference platform with an offline-capable, UMA-optimized orchestration layer for multi-LoRA systems on Apple Silicon.

### Core Technologies

- **DIR (Deterministic Inference Runtime)**: The core execution engine that ensures reproducible, auditable inference with token-level determinism
- **TAS (Token Artifact System)**: Transforms inference outputs into persistent, reusable artifacts that can be referenced and composed

### Key Characteristics

- **Single-node, multi-tenant** deployment model
- **Zero network egress** during serving
- **Deterministic replay** for compliance and debugging
- **Hot-swap adapters** without service interruption
- **Multi-backend support**: MLX (primary), CoreML/ANE (acceleration layer), Metal (kernels)

### Backend Execution Modes

adapterOS supports two backend execution modes with different adapter handling:

- **MLX (Primary)**: Native macOS inference and training with unified memory. Supports **hot-swap adapters** with live replacement during inference. Uses atomic pointer updates for zero-downtime updates and rollbacks. HKDF-seeded determinism for reproducible results.

- **CoreML/ANE (Acceleration Layer)**: Provides ANE-accelerated ops for specific layers (e.g., K-sparse gate routing). Uses **frozen CoreML packages** for deterministic execution. Power-efficient (50% savings) when offloading to Neural Engine.

The backend selection is deterministic based on capabilities and configuration. See [BACKEND_SELECTION.md](BACKEND_SELECTION.md) for details.

### System Architecture Diagram

```mermaid
graph TB
    subgraph "Client Layer"
        UI[Web UI<br/>Leptos + WASM]
        CLI[CLI Tool<br/>aosctl]
    end

    subgraph "Control Plane (Port 8080)"
        API[HTTP API Server<br/>adapteros-server]
        Auth[Authentication<br/>JWT + RBAC]
        Policy[Policy Manager<br/>25 Policy Packs]
        Orchestrator[Training Orchestrator]
        DB[(SQLite Database<br/>220+ migrations)]
    end

    subgraph "Worker Processes"
        Worker1[aos-worker<br/>Tenant A]
        Worker2[aos-worker<br/>Tenant B]
        Router[K-Sparse Router<br/>Q15 Quantization]
        Lifecycle[Lifecycle Manager<br/>Adapter States]
    end

    subgraph "Execution Kernels"
        CoreML[CoreML/ANE<br/>Acceleration Layer]
        Metal[Metal Kernels<br/>GPU Compute]
        MLX[MLX Backend<br/>Apple ML Framework]
    end

    subgraph "Storage Layer"
        Adapters[Adapter Storage<br/>.aos format]
        Models[Base Models<br/>SafeTensors]
        Telemetry[Telemetry Bundles<br/>NDJSON + Signatures]
    end

    UI -->|HTTPS| API
    CLI -->|HTTP| API
    API --> Auth
    API --> Policy
    API --> Orchestrator
    API --> DB

    API -->|UDS| Worker1
    API -->|UDS| Worker2

    Worker1 --> Router
    Worker1 --> Lifecycle
    Worker2 --> Router
    Worker2 --> Lifecycle

    Router --> CoreML
    Router --> Metal
    Router --> MLX

    Worker1 --> Adapters
    Worker1 --> Models
    Worker2 --> Adapters
    Worker2 --> Models

    API --> Telemetry
    Worker1 --> Telemetry
    Worker2 --> Telemetry
```

### Technology Stack

| Layer | Technologies |
|-------|-------------|
| **Frontend** | Leptos 0.7, Tailwind CSS, WASM (Client-Side Rendering), Trunk |
| **Backend** | Rust (nightly), Axum, SQLite with WAL mode |
| **ML Frameworks** | CoreML, Metal Performance Shaders, MLX |
| **Security** | JWT (HMAC-SHA256 or Ed25519), Argon2id password hashing |
| **Observability** | Tracing, Prometheus metrics, NDJSON telemetry bundles |

---

## Core Concepts

### 1. Tenant

**Definition:** A tenant is the top-level isolation unit in adapterOS, representing a user, organization, or environment.

**Purpose:** Enforce security boundaries, resource quotas, and access control.

**Properties:**
- `tenant_id` - Unique identifier
- `uid` / `gid` - Unix user/group for OS-level isolation
- Resource limits (memory, adapters, stacks)

**Examples:**
- `tenant-dev` - Development environment
- `tenant-prod` - Production environment
- `acme-corp` - Customer organization

**CLI Usage:**
```bash
aosctl init-tenant --id tenant-prod --uid 5000 --gid 5000
```

**Database Schema:**
```sql
CREATE TABLE tenants (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    uid INTEGER,
    gid INTEGER,
    created_at TEXT DEFAULT (datetime('now'))
);
```

---

### 2. Adapter

**Definition:** A LoRA (Low-Rank Adaptation) module that specializes a base model for a specific task, domain, or style.

**Purpose:** Efficient fine-tuning without modifying base model weights.

**Naming Convention:** `{tenant}/{domain}/{purpose}/{revision}`
- Example: `tenant-a/engineering/code-review/r001`

**Properties:**
- `adapter_id` - Unique identifier
- `hash` - BLAKE3 content hash
- `rank` - LoRA rank (e.g., 8, 16, 32)
- `current_state` - Lifecycle state (unloaded, cold, warm, hot, resident)
- `activation_pct` - % of requests where router selected this adapter
- `memory_bytes` - VRAM footprint
- `expires_at` - TTL for ephemeral adapters
- `pinned` - Protection from eviction

**Lifecycle States:**
```
Unloaded → Cold → Warm → Hot → Resident
    ↑                              ↓
    └──────── (eviction) ──────────┘
```

**State Definitions:**
- **Unloaded**: Not in memory (0 MB VRAM, ~500ms load latency)
- **Cold**: In memory, not compiled (~100 MB VRAM, ~50ms activation)
- **Warm**: Compiled and cached (~150 MB VRAM, ~5ms activation)
- **Hot**: Highly optimized (~200 MB VRAM, ~1ms activation)
- **Resident**: Pinned, protected from eviction (~200 MB VRAM)

**File Format (.aos):**
```
+--------+--------+------------------------------------------+
| Offset | Size   | Field                                    |
+--------+--------+------------------------------------------+
| 0      | 8      | Magic bytes: "AOS3\x00\x00\x00\x00"      |
| 8      | 4      | Format version (u32 LE) = 3              |
| 12     | 4      | Flags (reserved)                         |
| 16     | 8      | Total file size (u64 LE)                 |
| 24     | 8      | Weights offset (u64 LE)                  |
| 32     | 8      | Weights size (u64 LE)                    |
| 40     | 8      | Manifest offset (u64 LE)                 |
| 48     | 8      | Manifest size (u64 LE)                   |
| 56     | 8      | Reserved                                 |
+--------+--------+------------------------------------------+
| 64     | N      | Weights (SafeTensors or Q15)             |
| 64+N   | M      | Manifest (JSON metadata)                 |
+--------+--------+------------------------------------------+
```

#### Adapter Domains

Adapters are classified into three domains based on their purpose and lifecycle constraints:

| Domain | Description | Stream Bound | Auto-Version | Requires Base |
|--------|-------------|--------------|--------------|---------------|
| **Core** | Baseline adapters (e.g., `adapteros.aos`). Stable reference points that serve as delta bases for codebase adapters. | No | No | No |
| **Codebase** | Stream-scoped adapters combining repo state with session context. Tied exclusively to a single inference stream. | Yes | Yes | Yes |
| **Standard** (Portable) | General-purpose `.aos` adapters. Can be freely shared and loaded across sessions. Default type. | No | No | No |

**Codebase Adapter Rules:**

1. **Exclusive Stream Binding**: One codebase adapter per inference stream (enforced via database unique index)
2. **Base Adapter Requirement**: Must declare `base_adapter_id` pointing to a core adapter
3. **Auto-Versioning**: When `activation_count >= versioning_threshold`, system triggers versioning
4. **Deployment Verification**: Requires repo clean state and manifest hash match before activation
5. **Frozen Before CoreML**: Codebase adapters must be frozen (versioned and unbound) before CoreML export

**Session Start Flow:**

```
Session Start
     │
     ├─► Backend Selection (CoreML/ANE vs MLX/Metal)
     │
     ├─► Adapter Domain Selection
     │       ├─► Core adapter as base (if codebase required)
     │       ├─► Codebase adapter binding (exclusive to stream)
     │       └─► Portable adapters (freely loaded)
     │
     └─► Stack Activation
```

**Code References:**
- Adapter type enum: `crates/adapteros-core/src/adapter_type.rs`
- Database schema: `migrations/0261_codebase_adapter_type.sql`
- Session binding: `migrations/0262_session_codebase_binding.sql`

---

### 3. Stack

**Definition:** A tenant-scoped set of adapters with execution rules (workflow type, policies) used for inference.

**Purpose:** Reusable adapter combinations with consistent behavior.

**Workflow Types:**
- **Sequential**: Apply adapters in order
- **Parallel**: Apply adapters concurrently, merge results
- **UpstreamDownstream**: Two-phase (analysis → generation)

**Properties:**
- `stack_id` - Unique identifier
- `name` - Human-readable name
- `adapter_ids` - Ordered list of adapters
- `workflow_type` - Execution strategy
- `tenant_id` - Owner tenant

**Examples:**
```yaml
code-review-stack:
  adapters: [syntax-analyzer, style-checker]
  workflow: Sequential

multilingual-stack:
  adapters: [en-adapter, fr-adapter, es-adapter]
  workflow: Parallel

reasoning-stack:
  adapters: [fact-checker, reasoner]
  workflow: UpstreamDownstream
```

---

### 4. Router

**Definition:** The K-sparse gating mechanism that selects the top-K most relevant adapters for each inference request.

**Purpose:** Dynamic adapter selection based on input features, not static rules.

**Algorithm:**
1. Compute gate scores for all adapters (based on hidden states)
2. Select top-K adapters (e.g., K=3)
3. Deterministic tie-breaking: `(score desc, adapter_id asc)`
4. Quantize gates to Q15 for efficiency

**Critical Invariant:** Q15 denominator is **32767.0** (NOT 32768) - precision-critical

**Q15 Quantization:**
```rust
// Quantize gate score to Q15
let gate_q15 = (gate_f32 * 32767.0).round() as i16;

// Dequantize Q15 to float
let gate_f32 = gate_q15 as f32 / 32767.0;
```

**Key Parameters:**
- `k_sparse` - Number of adapters to select (default: 3)
- `entropy_floor` - Minimum entropy to prevent collapse (default: 0.02)
- `gate_quant` - Quantization mode (Q15, Q8)

**Location:** `crates/adapteros-lora-router/src/lib.rs`

---

### 5. Kernel

**Definition:** Precompiled Metal compute shaders that execute LoRA operations on the GPU.

**Purpose:** Deterministic, reproducible computation with zero runtime compilation.

**Types:**
- Attention kernels (Q, K, V with LoRA)
- MLP kernels (FFN with LoRA)
- Fused kernels (attention + LoRA in one pass)

**Properties:**
- `.metallib` files embedded in binary
- Deterministic rounding modes
- Parameter structs for modularity
- BLAKE3 hashes for verification

**Critical Invariant:** No `-ffast-math` compiler flags (breaks determinism)

**Location:** `crates/adapteros-lora-kernel-mtl/`

### Backend Cache Implementation

**Backend selection** is deterministic based on capabilities/config, but **cache eviction behavior and UI/telemetry exposure are unverified**.

**✅ IMPLEMENTED:**
- `BackendStrategy.select_backend()` selects deterministically
- `ModelHandleCache` keys on `(backend_type, manifest_hash)`

**⚠️ UNVERIFIED:**
- Cache eviction predictability
- UI/telemetry exposure of cache state

**Citation:** `plan/drift-findings.json` backend-01 rule validation

---

### 6. Telemetry

**Definition:** Structured event logging system that creates an immutable audit trail of all system operations.

**Purpose:** Compliance, debugging, replay verification, incident response.

**Event Types:**
- Inference events (request, response, router decisions)
- Lifecycle events (adapter load/unload, eviction)
- Policy events (violations, enforcement)
- System events (memory pressure, crashes)

**Storage Format:**
- Canonical JSON (JCS-serialized)
- Merkle chain (each event references previous hash)
- Bundles (compressed, signed archives)

**Event Structure:**
```json
{
  "event_id": "evt_abc123",
  "event_type": "adapter.lifecycle.promoted",
  "timestamp": "2025-01-15T12:00:00Z",
  "tenant_id": "default",
  "component": "adapteros-server-api",
  "metadata": {
    "adapter_id": "my-adapter",
    "old_state": "cold",
    "new_state": "warm",
    "actor": "user@example.com",
    "reason": "inference request",
    "duration_ms": 12.5
  },
  "signature": "ed25519_signature_here"
}
```

**Location:** `crates/adapteros-telemetry/`

---

### 7. Golden Run & Replay

**Definition:** A golden run is a verified, deterministic inference execution whose telemetry bundle serves as a reference for future replay.

**Purpose:** Verify determinism by re-executing the same request and comparing outputs.

**Workflow:**
1. **Golden Run**: Execute inference, record telemetry bundle
2. **Store Bundle**: Save bundle with signature
3. **Replay**: Re-execute same request using bundle metadata
4. **Compare**: Verify outputs match byte-for-byte
5. **Report**: Emit divergence events if mismatch detected

**Replay Metadata:**
- `manifest_hash` - Adapter manifest hash
- `router_seed` - Seed for audit (routing is deterministic by algorithm)
- `sampling_params_json` - Temperature, top_k, top_p, seed
- `rag_snapshot_hash` - RAG context hash (if applicable)
- `adapter_ids_json` - List of adapters used

**CLI Usage:**
```bash
# Create golden run
aosctl infer --prompt "Test" --golden-run ./golden-runs/test-001.json

# Replay
aosctl replay --bundle ./golden-runs/test-001.json

# Verify determinism
aosctl verify determinism-loop --json
```

---

## Architecture Components

### Control Plane

The control plane (`adapteros-server`) is the orchestration hub for adapterOS.

**Responsibilities:**
- HTTP API server (port 8080)
- Authentication and authorization (JWT + RBAC)
- Policy enforcement (30 policy packs)
- Training orchestration
- Worker management
- Telemetry indexing
- Database management (SQLite with WAL)

**Component Diagram:**

```mermaid
graph TB
    subgraph "Control Plane Components"
        API[Axum HTTP API]
        Auth[JWT Authentication]
        RBAC[Role-Based Access Control]
        Policy[Policy Engine]
        Training[Training Orchestrator]
        Telemetry[Telemetry Indexer]
        DB[(SQLite + WAL)]
    end

    subgraph "External Interfaces"
        UI[Web UI]
        CLI[CLI Tool]
    end

    subgraph "Worker Layer"
        Worker[aos-worker]
    end

    UI --> API
    CLI --> API
    API --> Auth
    Auth --> RBAC
    API --> Policy
    API --> Training
    API --> Telemetry
    API --> DB
    Training --> DB
    Telemetry --> DB
    API -->|UDS| Worker
```

**Key API Endpoints:**

| Category | Endpoint | Method | Description |
|----------|----------|--------|-------------|
| **Health** | `/healthz` | GET | Health check |
| **Auth** | `/v1/auth/login` | POST | User login |
| **Auth** | `/v1/auth/me` | GET | Get current user |
| **Adapters** | `/v1/adapters` | GET | List adapters |
| **Adapters** | `/v1/adapters/register` | POST | Register adapter |
| **Training** | `/v1/training/start` | POST | Start training job |
| **Training** | `/v1/training/jobs/:id` | GET | Get job status |
| **Inference** | `/v1/infer` | POST | Perform inference |
| **Inference** | `/v1/infer/stream` | POST | Streaming inference |
| **Telemetry** | `/v1/telemetry/stream` | GET | SSE event stream |

**RBAC Roles:**
- **admin**: Full access to all operations
- **operator**: Manage workers, plans, and promotions
- **sre**: Worker management and node operations
- **compliance**: Audit access, policy management
- **auditor**: Read-only audit and telemetry access
- **viewer**: Read-only access to status and reports

**Location:** `crates/adapteros-server-api/`

---

### Worker Processes

Workers (`aos-worker`) are the execution engines that perform inference and training.

**Responsibilities:**
- Load base models into memory
- Manage adapter lifecycle (load/unload/hot-swap)
- Execute inference requests
- Run training jobs
- Emit telemetry events
- Enforce memory pressure policies

**Communication:**
- **Unix Domain Sockets (UDS)** for control plane ↔ worker communication
- Path format: `/var/run/aos/{tenant_id}/aos.sock`
- HTTP over UDS protocol
- 30-second timeout default

**Worker State Machine:**

```mermaid
stateDiagram-v2
    [*] --> Starting: Worker spawned
    Starting --> Serving: Model loaded
    Serving --> Draining: Shutdown signal
    Draining --> Stopped: Requests completed
    Serving --> Crashed: Unhandled error
    Crashed --> [*]: Exit
    Stopped --> [*]: Clean shutdown
```

**Location:** `crates/adapteros-lora-worker/`

**⚠️ CURRENT GAPS (Documentation Drift):**
- **Worker lifecycle tenant scoping** not validated in storage layer
- Mapping of `WorkerStatus` to database schema and telemetry events unverified

**Citation:** `plan/drift-findings.json` lifecycle-01 rule validation

---

### Router (K-Sparse Gating)

The router is the core intelligence that selects which adapters to use for each request.

**Algorithm Details:**

```rust
pub struct Decision {
    pub adapter_ids: Vec<usize>,
    pub gates_q15: Vec<i16>,
    pub entropy: f32,
}

impl Router {
    pub fn route(&self, hidden_states: &[f32], k: usize) -> Decision {
        // 1. Compute gate scores for all adapters
        let scores: Vec<f32> = self.compute_scores(hidden_states);

        // 2. Select top-K adapters
        let mut indexed_scores: Vec<(usize, f32)> =
            scores.iter().enumerate()
                .map(|(i, &s)| (i, s))
                .collect();

        // 3. Sort: score DESC, then stable_id ASC (deterministic tie-breaking)
        indexed_scores.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then_with(|| self.adapter_info[a.0].stable_id.cmp(&self.adapter_info[b.0].stable_id))
        });

        // 4. Take top K
        let top_k = &indexed_scores[..k.min(indexed_scores.len())];

        // 5. Quantize to Q15
        let gates_q15: Vec<i16> = top_k.iter()
            .map(|(_, score)| (score * 32767.0).round() as i16)
            .collect();

        Decision {
            adapter_ids: top_k.iter().map(|(idx, _)| *idx).collect(),
            gates_q15,
            entropy: self.compute_entropy(&scores),
        }
    }
}
```

**Entropy Floor:** Prevents router collapse (all weight on one adapter)
- Minimum entropy: 0.02
- If entropy < floor, reject decision and fall back to uniform distribution

**Location:** `crates/adapteros-lora-router/src/lib.rs`

---

### Lifecycle Manager

Manages adapter state transitions and memory pressure.

**State Transitions:**

```mermaid
stateDiagram-v2
    [*] --> Unloaded: Adapter registered

    Unloaded --> Cold: First load
    Cold --> Warm: Activation % > 10%
    Warm --> Hot: Activation % > 50%
    Hot --> Resident: Manual pin

    Resident --> Hot: Unpin
    Hot --> Warm: Inactivity (1 hour)
    Warm --> Cold: Activation % < 10%
    Cold --> Unloaded: Memory pressure

    Hot --> Unloaded: Critical memory
    Warm --> Unloaded: High memory pressure
    Resident --> Unloaded: Admin override only

    Unloaded --> [*]: Adapter deleted
```

**Memory Pressure Levels:**
- **Low**: <30% usage
- **Medium**: 20-30% usage
- **High**: 15-20% usage (evict Cold adapters)
- **Critical**: <15% headroom (evict Warm/Hot adapters)

**Eviction Priority (lowest tier evicted first):**
1. **Cold** - Low priority, minimal impact
2. **Warm** - Moderate priority
3. **Hot** - High priority (only under critical pressure)
4. **Resident** - Protected (admin override required)

**Heartbeat Mechanism:**
- Adapters send periodic heartbeats (every 60s)
- Stale adapters (no heartbeat for 5 minutes) automatically recovered to `unloaded`
- Background task runs every 5 minutes

**TOCTOU Protection:**
- Compare-And-Swap (CAS) for state transitions
- Prevents race conditions between concurrent state updates

```rust
// Use CAS to prevent TOCTOU races
let updated = db.update_adapter_state_cas(
    adapter_id,
    "cold",      // Expected current state
    "warm",      // New state
    "warming up for inference"
).await?;

if !updated {
    // State changed between read and write - retry
    return Err(AosError::Validation("State conflict"));
}
```

**Location:** `crates/adapteros-lora-lifecycle/src/lib.rs`

---

## Cryptographic Receipts & Sealed Adapters

adapterOS implements **cryptographic receipts** and **sealed adapters** for verifiable, tamper-proof ML operations.

### Cryptographic Receipts

**Purpose:** Create verifiable proofs that specific inferences were executed under specific conditions, enabling third-party verification without system access.

**Architecture:**

```mermaid
graph TD
    subgraph "Receipt Generation"
        A[Input Tokens] --> B[BLAKE3 Input Digest]
        C[Model + Adapters] --> D[Context ID Digest]
        E[Routing Decisions] --> F[Routing Digest]
        G[Output Tokens] --> H[Output Digest]
        I[Hardware Profile] --> J[Equipment Digest]

        B --> K[Final Receipt]
        D --> K
        F --> K
        H --> K
        J --> K
        K --> L[BLAKE3 Final Digest]
    end

    subgraph "Verification"
        M[Receipt Digest] --> N[Third-Party Verification]
        O[Input Tokens] --> N
        P[Expected Output] --> N
    end
```

**Key Properties:**
- **Deterministic**: Same inputs → same digest (across identical hardware)
- **Tamper-proof**: Any modification → completely different digest
- **Third-party verifiable**: No access to inference system needed
- **Cryptographic binding**: All execution parameters are bound

**Integration Points:**
- Generated during inference in `ReceiptGenerator`
- Stored in database alongside inference traces
- Exposed via API for verification
- Used for compliance and audit trails

### Sealed Adapters

**Purpose:** Cryptographically secure adapter distribution with integrity guarantees and tamper detection.

**Architecture:**

```mermaid
graph TD
    subgraph "Sealing Process"
        A[Adapter Bundle] --> B[Extract Weights Hash]
        A --> C[Extract Config Hash]
        B --> D[Create Signed Manifest]
        C --> D
        D --> E[Ed25519 Signature]
        E --> F[Build Container]
        F --> G[BLAKE3 Integrity Hash]
        G --> H[.sealed.aos File]
    end

    subgraph "Loading Process"
        I[.sealed.aos File] --> J[Verify Integrity Hash]
        J --> K[Verify Ed25519 Signature]
        K --> L[Extract Weights]
        L --> M[Load Adapter]
        M --> N[Bind to Receipt Context]
    end
```

**Security Properties:**
- **Integrity**: BLAKE3 hash covers entire container
- **Authenticity**: Ed25519 signatures from trusted authorities
- **Tamper detection**: Any modification invalidates signatures
- **Receipt binding**: Weights hash included in cryptographic receipts

**File Format:**
- **Magic bytes**: "SEAL" identifier
- **Version**: Format version for compatibility
- **Integrity hash**: BLAKE3 of entire file
- **Signed manifest**: Metadata with Ed25519 signature
- **Adapter payload**: Weights and configuration data

**Integration Points:**
- Created using `aosctl adapter seal`
- Loaded via `aosctl adapter load-sealed`
- Verified against trusted public keys
- Weights hash bound to inference receipts

### Combined Security Architecture

```mermaid
graph TD
    subgraph "Adapter Supply Chain"
        A[Adapter Author] --> B[Seal with Private Key]
        B --> C[Distribute .sealed.aos]
        C --> D[Recipient Loads]
        D --> E[Verify Signature]
        E --> F[Extract Weights Hash]
    end

    subgraph "Inference Execution"
        F --> G[Include in Receipt Context]
        H[Execute Inference] --> I[Generate Cryptographic Receipt]
        I --> J[Receipt proves adapter legitimacy]
    end

    subgraph "Verification"
        K[Third Party] --> L[Verify Receipt Digest]
        L --> M[Confirms authentic adapter + correct execution]
    end
```

**End-to-End Guarantees:**
1. **Adapter integrity**: Sealed containers prove adapters haven't been tampered with
2. **Execution authenticity**: Receipts prove specific adapters were used
3. **Third-party verifiability**: Anyone can verify without system access
4. **Chain of custody**: From adapter creation to inference result

---

## Inference Flow

### End-to-End Request Flow

```mermaid
sequenceDiagram
    participant Client
    participant API as Control Plane API
    participant Auth as Auth Middleware
    participant Policy as Policy Manager
    participant Router as K-Sparse Router
    participant Lifecycle as Lifecycle Manager
    participant Worker as aos-worker
    participant Kernel as Metal Kernel
    participant Telemetry as Telemetry

    Client->>API: POST /v1/infer
    API->>Auth: Validate JWT token
    Auth-->>API: Claims (user, tenant, role)

    API->>Policy: enforce_policy(request)
    Policy-->>API: OK / Violation

    alt Policy Violation
        API-->>Client: 403 Forbidden
    end

    API->>Router: select_adapters(prompt, k=3)
    Router-->>API: Decision {adapter_ids, gates_q15}

    API->>Lifecycle: ensure_loaded(adapter_ids)
    Lifecycle-->>API: Adapters ready

    API->>Worker: UDS request
    Worker->>Worker: Tokenize prompt

    loop For each token
        Worker->>Router: route(hidden_states)
        Router-->>Worker: top-K adapters
        Worker->>Lifecycle: load_if_needed(adapters)
        Worker->>Kernel: forward_pass(base + lora)
        Kernel-->>Worker: logits
        Worker->>Worker: sample(logits)
    end

    Worker->>Telemetry: emit inference event
    Worker-->>API: InferResponse
    API-->>Client: 200 OK + response
```

### Model Load Status Gate

**Canonical Statuses:**
- `no-model` - No model loaded
- `loading` - Model loading in progress
- `ready` - Model ready for inference
- `unloading` - Model being unloaded
- `error` - Load/unload error
- `checking` - Health check in progress

**Aggregation (cluster-level per model):**
```
Any worker ready → ready
Else any loading → loading
Else any checking → checking
Else any unloading → unloading
Else any error → error
Else no-model
```

**Router Guard:**
- Inference allowed **only** when aggregated status is `ready`
- Otherwise, requests fail fast with `MODEL_NOT_READY` (503)

**Error Response:**
```json
{
  "code": "MODEL_NOT_READY",
  "message": "Base model not ready for inference",
  "request_id": "req_abc123"
}
```

**Metrics:**
- `adapteros_model_load_success_total` (counter)
- `adapteros_model_load_failure_total` (counter)
- `adapteros_model_unload_success_total` (counter)
- `adapteros_model_unload_failure_total` (counter)
- `adapteros_model_loaded{model_id,tenant_id}` (gauge: 1=ready, 0=not ready)

---

### Data Flow Diagram

```
User Prompt (text)
    ↓
[1] Tokenizer → [token_ids]
    ↓
[2] InferencePipeline.infer() → Autoregressive Loop:
    ├─ [3] Router.route() → Decision { adapter_ids, gates_q15 }
    ├─ [4] HotSwap → Ensure adapters loaded
    ├─ [5] MetalKernels.run_step() → Apply LoRA deltas
    └─ [6] Generator.next_token() → Sample from logits
    ↓
[7] Tokenizer.decode() → Generated Text
    ↓
[8] Build InferenceResponse with trace
```

---

### Token Accounting and Cache Credits

adapterOS distinguishes **logical tokens** (total processed) from **billed tokens** (charged to user), crediting cache-reused computation.

```mermaid
flowchart TD
    subgraph "Token Accounting Flow"
        subgraph "Input Side"
            A["User sends prompt"]
            B["Tokenized: logical_prompt_tokens"]
            C["Prefix cache lookup"]
            D{"Cache hit?"}
            E["prefix_cached_token_count"]
            F["billed_input = logical - cached"]
        end
        
        subgraph "Output Side"
            G["Generation loop runs"]
            H["logical_output_tokens"]
            I["billed_output_tokens"]
        end
        
        subgraph "Receipt"
            J["All 5 values committed"]
            K["BLAKE3 hash binding"]
            L["Verifiable proof"]
        end
    end
    
    A --> B --> C --> D
    D -->|Yes| E --> F
    D -->|No| F
    F --> G --> H --> I
    I --> J --> K --> L
```

**Token Accounting Fields (per inference):**

| Field | Description | Formula |
|-------|-------------|---------|
| `logical_prompt_tokens` | Total input tokens in prompt | Count of tokenized input |
| `prefix_cached_token_count` | Tokens satisfied from cache | From PrefixKvCache lookup |
| `billed_input_tokens` | Tokens charged for input | `logical - cached` (floor 0) |
| `logical_output_tokens` | Tokens generated | Count of output tokens |
| `billed_output_tokens` | Tokens charged for output | Currently equals logical |

**Example - Multi-Turn Conversation:**

| Turn | Logical In | Cached | Billed In | Output | Total Billed |
|------|------------|--------|-----------|--------|--------------|
| 1 | 50 | 0 | 50 | 95 | 145 |
| 2 | 145 | 50 | 95 | 78 | 173 |
| 3 | 223 | 145 | 78 | 62 | 140 |
| **Total** | — | **195** | **223** | **235** | **458** |

Without cache credits, the same conversation would bill 653 tokens (30% more).

**Location:** `crates/adapteros-core/src/evidence_envelope.rs`, `crates/adapteros-db/src/inference_trace.rs`

---

### Inference Error Codes

| Code | HTTP Status | Description | Handler Location |
|------|-------------|-------------|------------------|
| `MODEL_NOT_READY` | 503 | Base model not loaded | `InferenceCore::route_and_infer()` |
| `NO_COMPATIBLE_WORKER` | 503 | No workers available | Handler layer |
| `BACKPRESSURE` | 503 | System overloaded | Worker layer |
| `PERMISSION_DENIED` | 403 | Authorization failure | Auth middleware |
| `RAG_ERROR` | 500 | Evidence retrieval failed | Worker pipeline |
| `ROUTING_BYPASS` | 400 | Invalid routing params | Router |
| `REQUEST_TIMEOUT` | 504 | Request timed out | Worker/UDS client |
| `SERVICE_UNAVAILABLE` | 503 | Service temporarily down | Various |
| `ADAPTER_NOT_FOUND` | 404 | Adapter doesn't exist | Database layer |
| `POLICY_HOOK_VIOLATION` | 403 | Policy blocked request | Policy manager |
| `VALIDATION_ERROR` | 400 | Request validation failed | Handler layer |
| `DATABASE_ERROR` | 500 | Database operation failed | Database layer |
| `SERIALIZATION_ERROR` | 500 | JSON serialization failed | Handler layer |
| `ACCESS_DENIED` | 403 | Tenant isolation violation | Security layer |
| `ADAPTER_NOT_LOADABLE` | 500 | Adapter load failed | Lifecycle manager |
| `APPROXIMATE_REPLAY_REQUIRED` | 400 | Exact replay impossible | Replay handler |

**All errors wrapped in:**
```json
{
  "code": "ERROR_CODE",
  "message": "Human-readable message",
  "detail": "Optional detailed error info",
  "request_id": "req_abc123"
}
```

---

## Adapter Lifecycle

### State Machine Detail

The adapter lifecycle is a state machine with 5 states and automatic transitions based on usage patterns and memory pressure.

**Complete State Diagram:**

```mermaid
stateDiagram-v2
    [*] --> Unloaded: Adapter registered

    note right of Unloaded
        State: On disk
        Memory: 0 MB
        Load latency: ~500ms
    end note

    Unloaded --> Cold: First load (activation)

    note right of Cold
        State: In memory, not compiled
        Memory: ~100 MB
        Activation: ~50ms
    end note

    Cold --> Warm: Activation % threshold crossed

    note right of Warm
        State: Compiled, cached
        Memory: ~150 MB
        Activation: ~5ms
    end note

    Warm --> Hot: Frequent use

    note right of Hot
        State: Hot cache, optimized
        Memory: ~200 MB
        Activation: ~1ms
    end note

    Hot --> Resident: Pinned (critical adapter)

    note right of Resident
        State: Pinned, protected
        Memory: ~200 MB
        Eviction: Blocked
    end note

    Resident --> Hot: Unpinned
    Hot --> Warm: Inactivity timeout (1 hour)
    Warm --> Cold: Demotion (activation % < 10%)
    Cold --> Unloaded: Eviction (memory pressure)

    Hot --> Unloaded: Force eviction (critical memory)
    Warm --> Unloaded: Eviction (memory pressure)
    Resident --> Unloaded: Admin override only

    Unloaded --> [*]: Adapter deleted
```

---

### Transition Triggers

**Promotion Triggers:**

| From | To | Trigger | Threshold |
|------|-----|---------|-----------|
| Unloaded | Cold | First load request | N/A |
| Cold | Warm | Activation % increased | >10% |
| Warm | Hot | High activation rate | >50% |
| Hot | Resident | Manual pinning | Admin action |

**Demotion Triggers:**

| From | To | Trigger | Threshold |
|------|-----|---------|-----------|
| Resident | Hot | Manual unpinning | Admin action |
| Hot | Warm | Inactivity timeout | 1 hour no use |
| Warm | Cold | Low activation % | <10% |
| Cold | Unloaded | Memory pressure | System-wide threshold |

---

### Lifecycle API

```rust
use adapteros_lora_lifecycle::LifecycleManager;

let manager = LifecycleManager::new_with_db(
    adapter_names,
    &policies,
    path,
    telemetry,
    k,
    db
);

// Auto-promote on router decision
manager.record_router_decision(&selected).await?;

// Auto-evict on memory pressure
manager.check_memory_pressure(total_mem, MemoryPressureLevel::High).await?;

// Manual state transitions
manager.load_adapter("adapter-id").await?;
manager.evict_adapter("adapter-id").await?;

// Heartbeat mechanism
manager.heartbeat_adapter(&adapter_id).await?;
let stale_ids = manager.check_stale_adapters(300).await?;
let recovered = manager.recover_stale_adapters(300).await?;
```

---

### Telemetry Events

| Event Type | When Emitted | Metadata |
|------------|--------------|----------|
| `adapter_promoted` | State tier increased | adapter_id, old_tier, new_tier, activation_pct |
| `adapter_demoted` | State tier decreased | adapter_id, old_tier, new_tier, inactivity_duration_s |
| `adapter_evicted` | Adapter removed from memory | adapter_id, tier, memory_freed_mb, reason |
| `adapter_crash_detected` | Stale adapter recovered | adapter_id, last_seen, recovery_timestamp |

**Location:** `crates/adapteros-lora-lifecycle/src/lib.rs:213-346`

---

### Critical Field Conventions

**CRITICAL:** The `adapters` table has TWO distinct state-related fields:

| Field | Purpose | Valid Values |
|-------|---------|--------------|
| `current_state` | Runtime lifecycle state | `unloaded`, `cold`, `warm`, `hot`, `resident` |
| `lifecycle_state` | Metadata/registration status | `draft`, `active`, `deprecated`, `retired` |

**Always use `current_state` for runtime state checks. Using `lifecycle_state` is a bug.**

**State Check Methods:**

```rust
use adapteros_lora_lifecycle::AdapterHeatState;

let state: AdapterHeatState = adapter.current_state.parse()?;

// Check if adapter can serve inference
if state.is_available() {  // warm, hot, or resident
    // OK to infer
}

// Check if adapter is loaded at all
if state.is_loaded() {  // cold, warm, hot, or resident
    // In memory
}

// Check if adapter is protected
if state.is_pinned() {  // resident
    // Protected from eviction
}
```

---

## User Flows

### Flow 1: Authentication & Login

```mermaid
sequenceDiagram
    participant User
    participant UI as Login Form
    participant API as Control Plane
    participant DB as Database
    participant Auth as Auth Service

    User->>UI: Enter credentials
    UI->>UI: Validate email format
    UI->>API: POST /v1/auth/login
    API->>DB: SELECT FROM users WHERE email=?
    DB-->>API: User record
    API->>Auth: Verify password (Argon2)
    Auth-->>API: Valid
    API->>Auth: Generate JWT token
    Auth-->>API: Token (8h expiry)
    API-->>UI: LoginResponse {token, user_id, role}
    UI->>UI: Store token (httpOnly cookie)
    UI->>API: GET /v1/auth/me
    API-->>UI: UserInfoResponse
    UI-->>User: Redirect to Dashboard
```

**Key Components:**
- **Password Hashing:** Argon2id with `m_cost=19456, t_cost=2, p_cost=1`
- **JWT Mode:** HMAC-SHA256 or Ed25519 (configurable)
- **Token Expiry:** 8 hours default (configurable)
- **Cookie:** `auth_token`; HttpOnly, Secure, SameSite=Strict

**Location:** `crates/adapteros-server-api/src/handlers.rs:1145-1280`

---

### Flow 2: Adapter Training

```mermaid
sequenceDiagram
    participant User
    participant UI as Training Wizard
    participant API as Control Plane
    participant Orchestrator
    participant Worker
    participant Storage as Adapter Storage

    User->>UI: Configure training job
    UI->>UI: Step 1: Category (code/framework/etc)
    UI->>UI: Step 2: Basic info (name, desc)
    UI->>UI: Step 3: Data source (repo/template/custom)
    UI->>UI: Step 4: Category-specific config
    UI->>UI: Step 5: Training params (rank, alpha, epochs)
    UI->>UI: Step 6: Packaging (package, register)
    UI->>API: POST /v1/training/start
    API->>Orchestrator: start_training(params)
    Orchestrator->>Worker: UDS training request
    Worker->>Worker: Load dataset
    Worker->>Worker: Tokenize examples

    loop Training epochs
        Worker->>Worker: Forward pass
        Worker->>Worker: Compute loss
        Worker->>Worker: Backward pass
        Worker->>Worker: Update weights
        Worker->>API: Progress update
        API->>UI: Poll GET /v1/training/jobs/:id
        UI-->>User: Show progress
    end

    Worker->>Worker: Save weights
    Worker->>Storage: Package .aos file
    Worker->>API: Job completed

    opt Register adapter
        API->>API: Compute BLAKE3 hash
        API->>API: Insert into adapters table
        API-->>User: Adapter registered
    end

    API-->>UI: Job complete
    UI-->>User: Training finished
```

**Training Parameters:**
- `rank` - LoRA rank (default: 8)
- `alpha` - LoRA alpha (default: 16)
- `targets` - Target layers (e.g., `['q_proj', 'v_proj']`)
- `epochs` - Number of epochs (default: 3)
- `learning_rate` - Learning rate (default: 3e-4)
- `batch_size` - Batch size (default: 4)

**Location:**
- Frontend: `crates/adapteros-ui/src/pages/training.rs`
- Backend: `crates/adapteros-server-api/src/handlers.rs:10599-10756`
- Orchestrator: `crates/adapteros-orchestrator/src/training.rs`

---

### Flow 3: Model Inference

```mermaid
sequenceDiagram
    participant User
    participant UI as Inference Playground
    participant API as Control Plane
    participant Router
    participant Lifecycle
    participant Worker
    participant Kernel

    User->>UI: Enter prompt
    UI->>UI: Configure parameters (max_tokens, temp, etc)
    UI->>API: POST /v1/infer
    API->>API: Policy check

    alt No adapters specified
        API->>Router: select_adapters(prompt, k=3)
        Router-->>API: top-K adapter IDs
    end

    API->>Lifecycle: ensure_loaded(adapter_ids)
    Lifecycle-->>API: Adapters ready

    API->>Worker: UDS inference request
    Worker->>Worker: Tokenize prompt

    loop Generate tokens
        Worker->>Router: route(hidden_states)
        Router-->>Worker: Decision {adapters, gates}
        Worker->>Kernel: forward_pass(base + lora)
        Kernel-->>Worker: logits
        Worker->>Worker: sample(logits, temp, top_k, top_p)
        Worker->>Worker: Append token
    end

    Worker->>Worker: Decode tokens → text
    Worker-->>API: InferResponse
    API-->>UI: Response with trace
    UI-->>User: Display generated text
```

**Inference Parameters:**
- `prompt` - Input text
- `max_tokens` - Maximum tokens to generate (default: 100)
- `temperature` - Sampling temperature (default: 0.7)
- `top_k` - Top-k sampling (default: 50)
- `top_p` - Nucleus sampling (default: 0.9)
- `seed` - Random seed for determinism (optional)
- `require_evidence` - Require citations/evidence (default: false)
- `adapters` - Explicit adapter IDs (optional, router auto-selects if empty)

**Location:**
- Frontend: `crates/adapteros-ui/src/pages/inference.rs`
- Backend: `crates/adapteros-server-api/src/handlers.rs:4736+`
- Worker: `crates/adapteros-lora-worker/src/inference_pipeline.rs`

---

### Flow 4: Memory Pressure → Eviction

```mermaid
sequenceDiagram
    participant Monitor as UMA Pressure Monitor
    participant Lifecycle as Lifecycle Manager
    participant DB as Database
    participant Telemetry

    loop Every 5 seconds
        Monitor->>Monitor: Poll UMA stats
        Monitor->>Monitor: Compute headroom %

        alt Headroom < 15% (Critical)
            Monitor->>Lifecycle: check_memory_pressure(Critical)
            Lifecycle->>DB: Find expired adapters
            DB-->>Lifecycle: Expired list

            loop For each expired
                Lifecycle->>Lifecycle: evict_adapter()
                Lifecycle->>Telemetry: emit adapter_evicted
            end

            Lifecycle->>DB: Query adapters ORDER BY activation_pct ASC
            DB-->>Lifecycle: Coldest adapters (Cold, Warm, Hot)

            loop Until headroom >= 15%
                Lifecycle->>Lifecycle: evict_coldest()
                Lifecycle->>DB: UPDATE current_state='unloaded'
                Lifecycle->>Telemetry: emit adapter_evicted
                Lifecycle->>Monitor: Check headroom
            end
        else Headroom < 20% (High)
            Monitor->>Lifecycle: check_memory_pressure(High)
            Lifecycle->>DB: Find Cold adapters

            loop Until headroom >= 20%
                Lifecycle->>Lifecycle: evict_adapter(cold)
                Lifecycle->>Telemetry: emit adapter_evicted
            end
        end
    end
```

**Pressure Levels:**
- **Low**: <30% usage (no action)
- **Medium**: 20-30% usage (monitor only)
- **High**: 15-20% usage (evict Cold)
- **Critical**: <15% headroom (evict Cold, Warm, Hot)

**Eviction Protection:**
- **Resident** adapters are protected from automatic eviction
- Only admin override can unload Resident adapters

**Location:** `crates/adapteros-lora-lifecycle/src/lib.rs:1068-1128`

---

### Flow 5: Telemetry → Golden Run → Replay

```mermaid
sequenceDiagram
    participant User
    participant API as Control Plane
    participant Worker
    participant Telemetry as Telemetry Store
    participant Replay as Replay Engine

    User->>API: POST /v1/infer (with golden-run flag)
    API->>Worker: Execute inference
    Worker->>Telemetry: Emit inference events
    Worker->>Telemetry: Emit router decisions
    Worker->>Telemetry: Emit adapter loads
    Worker-->>API: InferResponse

    API->>Telemetry: Create bundle
    Telemetry->>Telemetry: Build Merkle chain
    Telemetry->>Telemetry: Sign bundle (Ed25519)
    Telemetry->>Telemetry: Save as golden run
    API-->>User: Bundle ID + response

    User->>API: POST /v1/replay (bundle_id)
    API->>Telemetry: Load golden run bundle
    Telemetry-->>API: Bundle metadata
    API->>Replay: route_and_infer_replay(bundle)
    Replay->>Worker: Re-execute with same params
    Worker-->>Replay: New response
    Replay->>Replay: Compare outputs byte-for-byte

    alt Outputs match
        Replay->>Telemetry: Emit determinism_verified
        Replay-->>User: Verification passed
    else Outputs differ
        Replay->>Telemetry: Emit divergence_detected
        Replay->>Telemetry: Store divergence report
        Replay-->>User: Divergence details
    end
```

**Replay Metadata Stored:**
- `manifest_hash` - Adapter manifest hash
- `router_seed` - Seed for audit trail (routing is deterministic)
- `sampling_params_json` - Temperature, top_k, top_p, seed
- `rag_snapshot_hash` - RAG context hash (if applicable)
- `adapter_ids_json` - List of adapters used

**Determinism Requirements:**
- Same base model version
- Same adapter versions (hash-verified)
- Same sampling parameters
- Same router algorithm (Q15 gates)
- No `-ffast-math` compiler flags

**Location:**
- Replay: `crates/adapteros-server-api/src/inference_core.rs:route_and_infer_replay()`
- Telemetry: `crates/adapteros-telemetry/`

---

## Glossary

| Term | Definition |
|------|------------|
| **Adapter** | LoRA module that specializes a base model for a specific task |
| **Adapter Stack** | Tenant-scoped set of adapters with execution rules |
| **Activation %** | Percentage of requests where router selected this adapter |
| **Base Model** | Foundation model (e.g., Qwen, Llama) that adapters modify |
| **Bundle** | Compressed, signed telemetry archive for replay |
| **Divergence** | Mismatch between golden run and replay execution |
| **Eviction** | Removal of adapter from memory due to pressure |
| **Golden Run** | Verified, deterministic execution used as reference |
| **K-Sparse** | Router algorithm that selects top-K adapters per request |
| **Kernel** | Precompiled Metal compute shader for LoRA operations |
| **Lifecycle** | State machine for adapter memory management (Unloaded → Resident) |
| **Merkle Chain** | Linked sequence of hashed telemetry events |
| **Pinning** | Protection mechanism to prevent adapter eviction |
| **Policy Pack** | Set of rules enforced across tenants, adapters, and execution |
| **Q15** | 15-bit fixed-point quantization format (denominator: 32767.0) |
| **Replay** | Re-execution of golden run to verify determinism |
| **Router** | K-sparse gating mechanism for adapter selection |
| **Stack** | See "Adapter Stack" |
| **Telemetry** | Structured event logging for audit trail |
| **Tenant** | Top-level isolation unit (user, org, environment) |
| **Tier** | Lifecycle state (unloaded, cold, warm, hot, resident) |
| **TTL** | Time-to-live for ephemeral adapters (auto-delete) |
| **UDS** | Unix Domain Socket (IPC mechanism for worker communication) |
| **UMA** | Unified Memory Architecture (shared CPU/GPU memory on Apple Silicon) |
| **Workflow Type** | Execution strategy (Sequential, Parallel, UpstreamDownstream) |

---

## Related Documentation

- **[AGENTS.md](../AGENTS.md)** - Developer quick reference
- **[POLICIES.md](POLICIES.md)** - Policy enforcement details
- **[DATABASE.md](DATABASE.md)** - Database schema reference
- **[DETERMINISM.md](DETERMINISM.md)** - Determinism and replay guarantees
- **[VISUAL_GUIDES.md](VISUAL_GUIDES.md)** - Visual guides: comparisons, token flows, diagrams
- **[replay_spec.md](replay_spec.md)** - Replay harness and verification
- **[LIFECYCLE.md](LIFECYCLE.md)** - Adapter package format specification
- **[SECURITY.md](SECURITY.md)** - Event catalog and telemetry
- **[AUTHENTICATION.md](AUTHENTICATION.md)** - Authentication details
- **[API_REFERENCE.md](API_REFERENCE.md)** - Frontend integration guide

---

**Copyright:** © 2025 MLNavigator Inc / James KC Auchterlonie. All rights reserved.

**Maintained by:** adapterOS Team

**Last Updated:** 2025-12-11
