# AdapterOS Technical Specification — Rectified & Consolidated (v0.14.1)

> **Document version**: 0.14.1
> **Last verified against codebase**: 2026-02-09
> **Status**: Living document. All claims verified against source code unless marked *Planned*.

---

## Table of Contents

- [1. Executive Summary](#1-executive-summary)
- [2. Core Architecture](#2-core-architecture)
  - [2.1 Process Topology](#21-process-topology)
  - [2.2 Component Map](#22-component-map)
  - [2.3 Communication Paths](#23-communication-paths)
  - [2.4 Workspace Structure](#24-workspace-structure)
- [3. Key Innovations](#3-key-innovations)
  - [3.1 Deterministic Inference Runtime (DIR)](#31-deterministic-inference-runtime-dir)
  - [3.2 K-Sparse LoRA Routing](#32-k-sparse-lora-routing)
  - [3.3 Policy Enforcement (30 Packs, Type-State Chain)](#33-policy-enforcement-30-packs-type-state-chain)
  - [3.4 Cryptographic Receipts (V7 Schema)](#34-cryptographic-receipts-v7-schema)
  - [3.5 Prefix KV Cache with Token Attribution](#35-prefix-kv-cache-with-token-attribution)
- [4. Data Flows](#4-data-flows)
  - [4.1 Inference Pipeline (11 Stages)](#41-inference-pipeline-11-stages)
  - [4.2 .aos Adapter Lifecycle](#42-aos-adapter-lifecycle)
  - [4.3 Training Pipeline](#43-training-pipeline)
  - [4.4 Hot-Swap Protocol](#44-hot-swap-protocol)
  - [4.5 Human-in-the-Loop Review Workflow](#45-human-in-the-loop-review-workflow)
- [5. API & Frontend](#5-api--frontend)
  - [5.1 REST API (300+ Endpoints)](#51-rest-api-300-endpoints)
  - [5.2 SSE Streaming Architecture](#52-sse-streaming-architecture)
  - [5.3 Leptos WASM Frontend](#53-leptos-wasm-frontend)
  - [5.4 Liquid Glass Design System](#54-liquid-glass-design-system)
- [6. Implementation Details](#6-implementation-details)
  - [6.1 Core Structs Reference](#61-core-structs-reference)
  - [6.2 .aos Format Specification](#62-aos-format-specification)
  - [6.3 Metal Kernel Suite](#63-metal-kernel-suite)
  - [6.4 Storage Architecture (SQLite + ReDB)](#64-storage-architecture-sqlite--redb)
  - [6.5 Boot Sequence (25 Phases)](#65-boot-sequence-25-phases)
- [7. Operations & Security](#7-operations--security)
  - [7.1 Deployment Models](#71-deployment-models)
  - [7.2 Security Architecture](#72-security-architecture)
  - [7.3 Observability Stack](#73-observability-stack)
  - [7.4 Performance Baselines](#74-performance-baselines)
- [8. Roadmap & Open Items](#8-roadmap--open-items)
  - [8.1 Current Limitations](#81-current-limitations)
  - [8.2 Planned for v0.15](#82-planned-for-v015)
  - [8.3 Long-Term Vision](#83-long-term-vision)
- [Appendix A: Glossary](#appendix-a-glossary)
- [Appendix B: Crate Index](#appendix-b-crate-index)
- [Appendix C: Error Redaction Patterns](#appendix-c-error-redaction-patterns)
- [Appendix D: Configuration Reference](#appendix-d-configuration-reference)

---

## 1. Executive Summary

AdapterOS is a Rust-based deterministic ML inference platform purpose-built for Apple Silicon.
It provides K-sparse LoRA adapter routing, Metal-optimized GPU kernels, and cryptographic
policy enforcement for production environments that require auditable, reproducible inference.
The system is designed from first principles for air-gapped deployments with zero network
egress during serving. Every inference run produces a signed receipt binding the exact seed
derivation, router decision, policy evaluation, and output digest into a tamper-evident chain.
The 83-crate workspace (v0.14.1) implements a multi-process architecture: an Axum 0.8 control
plane, one or more `aos-worker` inference processes communicating over Unix domain sockets, and
a Leptos 0.7 WASM client-side rendered frontend served as static assets from the control plane.

The core innovations that distinguish AdapterOS from conventional serving stacks are: (1) a
deterministic inference runtime that uses HKDF-SHA256 seed derivation with BLAKE3 global seeds
and per-step ChaCha20 reseeding to guarantee bit-identical outputs for identical inputs; (2)
a K-sparse LoRA router that selects up to K=8 adapters per request in 1-3 microseconds using
Q15 fixed-point quantized gates consumed directly by Metal, CoreML, and MLX kernels; (3) a
compile-time-enforced policy middleware chain spanning 30 canonical policy packs with type-state
progression that makes incorrect ordering a compile error; and (4) a V7 cryptographic receipt
schema with Ed25519 signing that binds prefix cache state, token attribution, and output
digests into an auditable proof chain. The platform currently targets macOS on Apple Silicon
(M2 Pro and later), with MLX as the primary inference backend, CoreML providing ANE-accelerated
operations, and Metal supplying low-level GPU compute primitives.

---

## 2. Core Architecture

### 2.1 Process Topology

```
+------------------------------------------------------------------+
|                        macOS Host (Apple Silicon)                  |
|                                                                    |
|  +-----------------------------+    +---------------------------+  |
|  |    adapteros-server         |    |    aos-worker (1..N)      |  |
|  |    (Control Plane)          |    |    (Inference Workers)    |  |
|  |                             |    |                           |  |
|  |  Axum 0.8 HTTP Server       |    |  MLX FFI Backend          |  |
|  |  Port 8080 (configurable)   |    |  CoreML ANE Layer         |  |
|  |                             |    |  Metal Kernels            |  |
|  |  +----------+ +---------+  |    |                           |  |
|  |  | REST API | | SSE Mgr |  |    |  +---------------------+  |  |
|  |  +----------+ +---------+  |    |  | UDS Server (15 rtes) |  |  |
|  |  | Middleware Chain      |  |    |  +---------------------+  |  |
|  |  | (type-state enforced) |  |    |                           |  |
|  |  +-----------------------+  |    +-------------+-------------+  |
|  |  | SQLite WAL | ReDB KV |  |                  |                 |
|  |  +------------+---------+  |                  |                 |
|  |  | Policy | Telemetry   |  |    UDS (JSON/HTTP/1.1)             |
|  |  +--------+-------------+  |    0o600 perms, 16 MB limit       |
|  +------------+----------------+                  |                 |
|               |                                   |                 |
|               +-----------------------------------+                 |
|                                                                    |
|  +-----------------------------+    +---------------------------+  |
|  |    adapteros-ui (WASM)      |    |    adapteros-secd         |  |
|  |    Leptos 0.7 CSR           |    |    Secure Enclave Daemon  |  |
|  |    Served from static/      |    |    Ed25519 key management |  |
|  +-----------------------------+    +---------------------------+  |
|                                                                    |
|  +-----------------------------+    +---------------------------+  |
|  |    adapteros-node           |    |    aosctl (CLI)           |  |
|  |    Node Agent               |    |    TUI Dashboard          |  |
|  |    Worker lifecycle mgmt    |    |    Database migrations    |  |
|  +-----------------------------+    +---------------------------+  |
+------------------------------------------------------------------+
```

**Control Plane** (`adapteros-server`): Single-process Axum 0.8 HTTP server that owns all
routing, middleware, policy enforcement, SSE streaming, database access, and telemetry
aggregation. Binds to port 8080 by default. Serves the compiled Leptos WASM frontend from
`static/`. Manages worker registration, heartbeats, and load coordination.

**Inference Workers** (`aos-worker`): Separate processes that perform actual model inference
and training. Each worker runs a UDS server with 15 routes. Workers register with the control
plane on startup and maintain heartbeat connections. The control plane dispatches inference
requests to workers over UDS using JSON over raw HTTP/1.1.

**Secure Enclave Daemon** (`adapteros-secd`): Manages Ed25519 signing keys using the macOS
Secure Enclave when available. Provides key generation, signing, and attestation services.

**Node Agent** (`adapteros-node`): Manages worker process lifecycle, including spawning with
privilege dropping (`fork` + `pre_exec` with `setgid` before `setuid`), restart supervision,
and panic reporting.

**CLI** (`aosctl`): Command-line interface for database migrations, system health diagnostics,
model management, adapter training, and interactive chat.

### 2.2 Component Map

The workspace is organized into functional layers:

| Layer | Crates | Responsibility |
|-------|--------|----------------|
| **Core** | `adapteros-core`, `adapteros-types`, `adapteros-crypto`, `adapteros-numerics` | Shared types, seed derivation, Ed25519/BLAKE3, numeric determinism |
| **Router** | `adapteros-lora-router`, `adapteros-lora-plan`, `adapteros-lora-rag` | K-sparse adapter selection, query planning, RAG retrieval |
| **Kernels** | `adapteros-lora-kernel-api`, `adapteros-lora-kernel-mtl`, `adapteros-lora-kernel-coreml` | GPU/ANE compute primitives, kernel profiling |
| **Backend** | `adapteros-lora-mlx-ffi`, `adapteros-lora-worker`, `adapteros-model-server` | MLX C++ FFI, inference execution, shared model serving |
| **Server** | `adapteros-server`, `adapteros-server-api`, `adapteros-server-api-*` | HTTP server, REST handlers, middleware chain |
| **Storage** | `adapteros-db`, `adapteros-storage`, `adapteros-registry` | SQLite WAL + ReDB dual-write, adapter registry |
| **Policy** | `adapteros-policy`, `adapteros-verify`, `adapteros-lint` | 30 policy packs, verification, code analysis |
| **Telemetry** | `adapteros-telemetry`, `adapteros-telemetry-types`, `adapteros-diagnostics` | Event pipeline, Merkle chains, diagnostic events |
| **Identity** | `adapteros-auth`, `adapteros-id` | JWT authentication, typed ID generation |
| **Lifecycle** | `adapteros-boot`, `adapteros-lora-lifecycle`, `adapteros-error-recovery` | Boot phases, adapter lifecycle, error recovery |
| **Determinism** | `adapteros-deterministic-exec`, `adapteros-replay`, `adapteros-trace` | Deterministic executor, replay engine, trace capture |
| **Format** | `adapteros-aos`, `adapteros-single-file-adapter`, `adapteros-manifest` | .aos binary/sealed formats, adapter manifests |
| **Federation** | `adapteros-federation` | Multi-node sync, tick ledger, quorum signatures |
| **Frontend** | `adapteros-ui`, `adapteros-api-types` | Leptos WASM UI, shared API types |
| **Tools** | `adapteros-cli`, `adapteros-tui`, `xtask`, `fuzz` | CLI, TUI dashboard, build tasks, fuzzing |

### 2.3 Communication Paths

```
Browser (WASM)  ----HTTPS/WSS---->  Control Plane  ----UDS (JSON/HTTP)---->  Worker(s)
                                         |
                                         +---- SQLite WAL (var/aos-cp.sqlite3)
                                         +---- ReDB KV   (var/aos-cp.redb)
                                         +---- SSE Push to Browser (15 stream types)
                                         +---- Prometheus /metrics (15s scrape)
                                         +---- Telemetry NDJSON bundles (var/telemetry/)
```

**Control Plane <-> Workers**: JSON over raw HTTP/1.1 on Unix domain sockets. Socket
permissions are set to `0o600`. Maximum body size is 16 MB. The control plane acts as HTTP
client; each worker runs a UDS HTTP server. Worker registration occurs via
`POST /v1/workers/register` carrying identity, manifest hash, and capability declarations.

**Control Plane <-> Browser**: Standard HTTPS with SSE for real-time push. The Leptos WASM
frontend communicates with the control plane via typed API calls using `adapteros-api-types`
with the `wasm` feature flag for compile-time type consistency. Streaming inference uses the
raw `fetch` API with `ReadableStream`, not `EventSource`, to support POST bodies and custom
headers.

**Control Plane <-> Database**: SQLite in WAL mode for relational data. ReDB for key-value
storage. Four storage modes control the migration path (see [6.4](#64-storage-architecture-sqlite--redb)).

**Control Plane <-> Federation Peers**: HTTP with Ed25519-signed payloads for telemetry bundle
synchronization, tick ledger entries, and quorum signatures. Clock drift rejection threshold:
5000 ms. *Partially wired in v0.14* (see [8.1](#81-current-limitations)).

### 2.4 Workspace Structure

```
adapter-os/
  Cargo.toml              # Workspace root, 83 members, feature flags
  Cargo.lock              # Locked dependencies
  rust-toolchain.toml     # Stable Rust channel
  .cargo/config.toml      # Build aliases (cargo c, cargo tb, cargo nt, etc.)
  configs/
    cp.toml               # Control plane configuration
  crates/                 # 83 Rust crates (see Appendix B)
  metal/                  # Metal shader sources (.metal files)
  migrations/             # SQLite migrations with signature tracking
  static/                 # Compiled WASM frontend (trunk build output)
  scripts/                # Build and utility scripts
  tests/                  # Workspace-level integration tests
  docs/                   # Architecture documentation
  var/                    # Runtime data (gitignored) -- see var/ Policy below
  fuzz/                   # Fuzz testing targets
  xtask/                  # Build automation tasks
```

**var/ Directory Policy**: All runtime data lives under `var/`. The system rejects
`/tmp`, `/private/tmp`, and `/var/tmp` for persistent storage (enforced by `path_security.rs`).
Canonical subdirectories: `adapters/`, `models/`, `model-cache/`, `keys/`, `logs/`, `run/`,
`telemetry/`, `manifest-cache/`, `embeddings/`, `documents/`, `datasets/`, `quarantine/`,
`analysis/`, `audit-evidence/`, `bundles/`.

---

## 3. Key Innovations

### 3.1 Deterministic Inference Runtime (DIR)

AdapterOS guarantees bit-identical inference outputs for identical inputs through a layered
determinism architecture. This property is critical for regulatory compliance, audit trails,
and replay-based debugging.

#### Seed Derivation

All randomness flows from a single BLAKE3 global seed through HKDF-SHA256 key derivation:

```
Global Seed (BLAKE3)
  |
  +-- HKDF-SHA256(global_seed, "mlx")     --> MLX backend seed
  +-- HKDF-SHA256(global_seed, "router")  --> Router seed
  +-- HKDF-SHA256(global_seed, "sample")  --> Sampling seed
  +-- HKDF-SHA256(global_seed, "train")   --> Training seed
  ...
```

Each derived seed is wrapped in a `TypedSeed` that carries:

```rust
pub struct TypedSeed {
    /// HKDF algorithm version (must match HKDF_ALGORITHM_VERSION)
    pub version: u32,
    /// 32-byte derived seed value
    bytes: [u8; 32],  // HKDF_OUTPUT_LENGTH
    /// BLAKE3 checksum of bytes for integrity validation
    pub checksum: B3Hash,
}
```

**Invariants**:
1. `version` must equal `HKDF_ALGORITHM_VERSION` at the point of use. Mismatches indicate
   schema drift and are rejected.
2. `checksum` must equal `BLAKE3(bytes)`. Validation occurs at every FFI and context boundary.
3. Seeds are never reused across backends or derivation contexts.

Source: `crates/adapteros-core/src/seed.rs`

#### Deterministic RNG

`DeterministicRng` wraps `ChaCha20Rng` with per-step HKDF reseeding:

```
Step N: ChaCha20Rng(HKDF(global_seed, step_N_context))
Step N+1: ChaCha20Rng(HKDF(global_seed, step_N+1_context))
```

This ensures that each step's randomness is independently reproducible without depending on
the execution order of prior steps.

#### Three Seed Modes

| Mode | Behavior | Use Case |
|------|----------|----------|
| **Strict** | Rejects requests missing explicit seeds. All outputs fully deterministic. | Production, audit |
| **BestEffort** | Uses provided seed if available, falls back to system entropy. Logs warnings for missing seeds. | Development |
| **NonDeterministic** | Ignores seeds entirely, uses system entropy. | Debugging, performance testing |

#### Compiler and Kernel Constraints

- **No `-ffast-math`**: IEEE 754 compliance is mandatory. Fast-math reorders floating-point
  operations, breaking determinism.
- **No `-march=native`**: Instruction set must be fixed to prevent host-dependent codegen
  differences.
- **Metal kernels**: `#pragma clang fp contract(off)` disables fused multiply-add contraction
  to ensure consistent rounding.
- **Q15 fixed-point**: Router gates use a denominator of exactly `32767.0` (not 32768.0) to
  preserve symmetric range for signed values.

#### Deterministic Async Executor

The deterministic executor (`adapteros-deterministic-exec`) provides:
- **Serial FIFO scheduling**: Tasks execute in submission order, not runtime-determined order.
- **Logical tick ledger**: Each tick is a BLAKE3-hashed event that produces an auditable
  sequence.
- **Event chain**: Every async operation is recorded as a hash chain entry, enabling exact
  replay.

Source: `crates/adapteros-deterministic-exec/src/lib.rs`

#### Training Determinism

The training pipeline uses the same HKDF seed derivation as inference. However, determinism
guards for training are currently **disabled** due to dependency issues between the training
subsystem and the deterministic executor. Training outputs may vary across runs until this is
resolved. *Planned for v0.15*.

### 3.2 K-Sparse LoRA Routing

The router selects up to K adapters (K <= 8) per inference request. The fixed upper bound of 8
is a hardware constraint driven by the `RouterRing` struct, which uses fixed-size arrays
consumed directly by Metal, CoreML, and MLX kernels.

#### RouterRing (Kernel Interface)

```rust
pub struct RouterRing {
    /// Adapter indices (fixed K=8, unused entries zero-filled)
    pub indices: [u16; 8],
    /// Q15 quantized gates (signed i16, range: -32767 to +32767)
    pub gates_q15: [i16; 8],
    /// Token position in sequence
    pub token_pos: u32,
}
```

The `RouterRing` is the wire format between the router and compute kernels. Its fixed layout
avoids dynamic allocation on the GPU path. Unused positions are zero-filled so kernels can
iterate the full array without bounds checking.

#### Decision (Router Output)

```rust
pub struct Decision {
    pub indices: SmallVec<[u16; 8]>,
    pub gates_q15: SmallVec<[i16; 8]>,
    pub entropy: f32,
    pub candidates: Vec<DecisionCandidate>,
    /// Optional decision hash for audit and reproducibility verification
    pub decision_hash: Option<DecisionHash>,
}
```

The `Decision` is the router's internal representation before packing into a `RouterRing`.
`SmallVec<[_; 8]>` avoids heap allocation for the common case. `DecisionHash` provides a
deterministic fingerprint for audit trails.

#### Routing Decision Enum

```rust
pub enum RoutingDecision {
    Selected(Decision),
    Abstain(RouterAbstainReason),
}
```

The router can explicitly abstain (returning a reason) rather than forcing a low-confidence
selection. Abstain reasons include: no adapters available, all candidates masked by policy,
confidence below threshold, and similar conditions.

#### Router Weights

```rust
pub struct RouterWeights {
    pub language_weight: f32,      // 0.30 - strong signal
    pub framework_weight: f32,     // 0.25 - strong signal (verified: 0.25 in code, not 0.20)
    pub symbol_weight: f32,        // 0.20 - moderate signal (verified: 0.20 in code, not 0.15)
    pub path_weight: f32,          // 0.10 - supporting signal
    pub verb_weight: f32,          // 0.10 - supporting signal
    pub orthogonal_weight: f32,    // 0.05 - diversity bonus
    // Note: actual weights are configurable; defaults shown
}
```

Eight weighted features produce a composite score for each candidate adapter. Tie-breaking is
deterministic: score descending, then stable_id ascending.

#### Q15 Quantization

All gate values pass through Q15 fixed-point quantization before reaching kernels:

```
q15_value = round(float_value * 32767.0)
```

The denominator is exactly `32767.0` (defined in `crates/adapteros-lora-router/src/constants.rs`).
This preserves the symmetric signed range `[-32767, +32767]` and avoids the asymmetry that
would occur with `32768.0` (since `-32768` has no positive counterpart in i16).

#### Policy Mask

```rust
pub struct PolicyMask {
    /// Per-adapter allow bit aligned with adapter ordering.
    pub allowed: Vec<bool>,
    /// Digest binding policy state to the mask bits.
    pub digest: B3Hash,
    /// Which override sources were applied when producing the mask.
    pub overrides: PolicyOverrideFlags,
}
```

Before the router scores candidates, the policy engine produces a `PolicyMask` that gates
which adapters may be selected. The `digest` field cryptographically binds the mask to the
policy evaluation state, preventing tampering between policy evaluation and routing.

### 3.3 Policy Enforcement (30 Packs, Type-State Chain)

#### 30 Canonical Policy Packs

The `PolicyId` enum defines exactly 30 policy packs:

| ID | Name | Domain |
|----|------|--------|
| 1 | Egress | Network egress control (air-gap enforcement) |
| 2 | Determinism | Seed presence and mode validation |
| 3 | Router | Routing decision constraints |
| 4 | Evidence | Proof chain integrity |
| 5 | Refusal | Content refusal policies |
| 6 | Numeric | Numeric precision requirements |
| 7 | Rag | RAG retrieval policies |
| 8 | Isolation | Tenant and resource isolation |
| 9 | Telemetry | Telemetry collection policies |
| 10 | Retention | Data retention rules |
| 11 | Performance | Latency and throughput SLOs |
| 12 | Memory | Memory usage limits (UMA thresholds) |
| 13 | Artifacts | Build artifact policies |
| 14 | Secrets | Secret detection and prevention |
| 15 | BuildRelease | Build and release gating |
| 16 | Compliance | Regulatory compliance checks |
| 17 | Incident | Incident response policies |
| 18 | Output | Output format and content policies |
| 19 | Adapters | Adapter lifecycle policies |
| 20 | DeterministicIo | I/O determinism enforcement |
| 21 | Drift | Model and configuration drift detection |
| 22 | Mplora | Multi-path LoRA specific policies |
| 23 | Naming | Naming convention enforcement |
| 24 | DependencySecurity | Dependency vulnerability scanning |
| 25 | CircuitBreaker | Circuit breaker state policies |
| 26 | Capability | Capability-based access control |
| 27 | Language | Language detection policies |
| 28 | QueryIntent | Query intent classification |
| 29 | LiveData | Live data retrieval policies |
| 30 | ProductionReadiness | Production readiness gating |

Source: `crates/adapteros-policy/src/registry.rs`

#### Type-State Middleware Chain

The middleware chain uses a compile-time type-state pattern to enforce correct ordering:

```rust
// Type-state progression (7 markers, PhantomData<S>):
NeedsAuth -> NeedsTenantGuard -> NeedsCsrf -> NeedsContext -> NeedsPolicy -> NeedsAudit -> Complete
```

Each middleware layer consumes a `ChainBuilder<CurrentState>` and returns
`ChainBuilder<NextState>`. You cannot call `.build()` until the chain reaches the `Complete`
state. Incorrect ordering or skipped steps produce a compile error, not a runtime bug.

```rust
// Correct (compiles):
ProtectedChain::new()
    .auth(auth_config)          // NeedsAuth -> NeedsTenantGuard
    .tenant_guard(guard)        // NeedsTenantGuard -> NeedsCsrf
    .csrf(csrf_config)          // NeedsCsrf -> NeedsContext
    .context(ctx_config)        // NeedsContext -> NeedsPolicy
    .policy(policy_engine)      // NeedsPolicy -> NeedsAudit
    .audit(audit_config)        // NeedsAudit -> Complete
    .build()                    // Complete -> Router layer

// Incorrect (compile error - cannot call .policy() on NeedsAuth):
ProtectedChain::new()
    .policy(policy_engine)      // ERROR: expected NeedsPolicy, found NeedsAuth
```

Source: `crates/adapteros-server-api/src/middleware/chain_builder.rs`

#### Middleware Evaluation Order (Protected Tier)

1. **Auth** -- JWT validation (Ed25519 or HMAC), API key lookup, session check
2. **Tenant Guard** -- Tenant isolation, cross-tenant access prevention
3. **CSRF** -- Cross-site request forgery token validation
4. **Context** -- Request context enrichment (trace ID, determinism seed, timing)
5. **Policy** -- Policy pack evaluation against the 30 registered packs
6. **Audit** -- Audit log entry creation with tamper-evident chain

#### Global Middleware Layers

Applied to all requests regardless of tier:

- Error-code enforcement
- Idempotency (SSE streams excluded via `Accept: text/event-stream` bypass)
- Rate limiting
- Request size limits
- Security headers
- Cache control
- API versioning
- Trace context propagation
- Request ID generation
- Seed isolation
- Lifecycle/drain gates
- Observability spans
- Response compression

#### API Route Tiers

| Tier | Middleware | Example Routes |
|------|-----------|----------------|
| Health | None (bare) | `/healthz`, `/readyz` |
| Public | Global only | `/system/ready`, `/v1/models` |
| Optional-Auth | Auth if present | Training routes (dev bypass mode) |
| Internal | Worker-to-CP auth | `/v1/workers/register`, `/v1/workers/heartbeat` |
| Protected | Full chain | All tenant-scoped CRUD, inference, admin |

### 3.4 Cryptographic Receipts (V7 Schema)

Every inference run produces a `RunReceipt` that cryptographically binds the entire execution
to a verifiable proof chain.

#### Receipt Structure

The `RunReceipt` contains 50+ fields spanning:

| Category | Fields | Purpose |
|----------|--------|---------|
| Identity | `trace_id`, `tenant_id`, `request_id` | Request correlation |
| Execution | `run_head_hash`, `output_digest`, `receipt_digest` | Deterministic chain |
| Tokens | `logical_prompt_tokens`, `logical_output_tokens`, `prefix_cached_token_count`, `billed_input_tokens`, `billed_output_tokens` | Token attribution |
| Routing | `adapter_ids`, `gate_values`, `router_entropy` | Adapter selection audit |
| Policy | `policy_evaluations`, `mask_digest` | Policy enforcement proof |
| Timing | `start_time`, `end_time`, `ttft_ms`, `inference_ms` | Performance measurement |
| Cache | `cache_scope`, `cached_prefix_digest`, `cached_prefix_len` | Prefix KV cache binding (V7) |

#### Schema Evolution

| Version | Added |
|---------|-------|
| V1 | Basic receipt: trace_id, run_head, output_digest |
| V2 | Token counts, adapter routing info |
| V3 | Policy evaluation results |
| V4 | Timing and performance metrics |
| V5 | Determinism mode and seed binding |
| V6 | Training receipts, checkpoint binding |
| V7 (current) | `cache_scope`, `cached_prefix_digest`, `cached_prefix_len` for prefix KV cache attribution |

#### Receipt Digest Input (V7)

```rust
pub struct ReceiptDigestInput {
    // V1 fields
    pub trace_id: String,
    pub run_head_hash: B3Hash,
    pub output_digest: B3Hash,
    // ... V2-V6 fields ...
    // V7 additions
    pub cache_scope: CacheScope,
    pub cached_prefix_digest: Option<B3Hash>,
    pub cached_prefix_len: u32,
}
```

The digest is computed as `BLAKE3(canonical_json(ReceiptDigestInput))` using JCS (JSON
Canonicalization Scheme) to ensure deterministic serialization.

#### Signing

Receipts are signed with Ed25519:

```
CryptographicReceipt {
    receipt: RunReceipt,
    signature: Ed25519Signature,  // 64 bytes
    public_key: Ed25519PublicKey, // 32 bytes
}
```

Keys are sourced from `AOS_SIGNING_KEY_HEX` or derived deterministically via
`BLAKE3(label + adapter_id + key_material)`.

#### Token Attribution Formula

```
Attributed Tokens (A) = Logical Tokens (L) - Cached Tokens (C)
```

The reduction is exact, not estimated. Cached token counts are cryptographically committed in
the receipt, making performance claims independently auditable. Speedup is non-linear because
memory pressure reduction compounds with compute savings.

Source: `docs/TOKEN_CACHING_ECONOMICS.md`

### 3.5 Prefix KV Cache with Token Attribution

The prefix KV cache eliminates redundant computation for shared prompt prefixes across
requests.

#### Mechanism

1. **Longest-prefix matching**: Incoming prompts are matched against cached prefix trees to
   find the longest cached prefix.
2. **Single-flight deduplication**: Concurrent requests with the same prefix share a single
   computation, avoiding thundering herd on cache misses.
3. **BLAKE3 integrity**: Cached prefixes are integrity-checked with BLAKE3 hashes to detect
   corruption or stale entries.
4. **Receipt binding**: The V7 receipt schema binds `cached_prefix_digest` and
   `cached_prefix_len` to the receipt, making cache hit/miss auditable.

#### Performance Impact

At 75% cache hit rate, throughput increases approximately 4x due to:
- Eliminated KV computation for cached prefix tokens
- Reduced UMA memory pressure (fewer active tensors)
- Compounding savings when multiple requests share system prompts

#### Cache Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_entries` | 16 | Maximum cached prefixes (LRU eviction) |
| `max_total_size` | 4 GB | Total cache memory budget |
| `max_entry_size` | 500 MB | Per-prefix size limit |
| `ttl` | 1 hour | Time-to-live before forced eviction |

Hit/miss metrics are instrumented and exposed via the Prometheus endpoint.

---

## 4. Data Flows

### 4.1 Inference Pipeline (11 Stages)

The inference pipeline is defined by the `DiagStage` enum, which provides both the execution
sequence and the diagnostic event contract:

```
Request ---> [1] RequestValidation
              |
              v
         [2] AdapterResolution
              |
              v
         [3] PolicyBeforeRouting  (OnRequestBeforeRouting hooks)
              |
              v
         [4] RagRetrieval
              |
              v
         [5] RouterDecision  (K-sparse, Q15 gates)
              |
              v
         [6] WorkerSelection  (load-aware placement)
              |
              v
         [7] PolicyBeforeInference  (OnBeforeInference hooks)
              |
              v
         [8] WorkerInference  (UDS call to aos-worker)
              |
              v
         [9] PolicyAfterInference  (OnAfterInference hooks)
              |
              v
         [10] EvidenceTelemetry  (receipt creation, chain update)
              |
              v
         [11] ResponseAssembly  (streaming or batch)
```

Source: `crates/adapteros-telemetry/src/diagnostics/mod.rs`,
`crates/adapteros-diagnostics/src/lib.rs`

#### Stage Details

**Stage 1 -- RequestValidation**: Validates tenant isolation, sampling parameters (temperature,
top_p, top_k), seed presence (per seed mode), request size limits, and schema conformance.

**Stage 2 -- AdapterResolution**: Resolves adapter IDs from the database. Loads adapter
manifests, verifies signatures, checks lifecycle state (must be `active` or `warm`).

**Stage 3 -- PolicyBeforeRouting**: Evaluates pre-routing policy hooks. Produces the
`PolicyMask` that constrains which adapters the router may select. Can reject the request
entirely if policy violations are critical.

**Stage 4 -- RagRetrieval**: Retrieves relevant context from the RAG subsystem if configured.
Context is injected into the prompt before routing to ensure the router sees the full input.

**Stage 5 -- RouterDecision**: The K-sparse router scores all unmasked adapters, selects up to
K=8, quantizes gates to Q15, and packs the result into a `RouterRing`. Deterministic
tie-breaking: score DESC, stable_id ASC. Returns `RoutingDecision::Abstain` if no adapter meets
the confidence threshold.

**Stage 6 -- WorkerSelection**: Selects a worker process based on load coordination. Considers
worker health, current request count, GPU memory pressure, and adapter affinity (workers with
warm adapters are preferred).

**Stage 7 -- PolicyBeforeInference**: Final policy check before dispatching to the worker.
Evaluates content policies, rate limits, and any tenant-specific restrictions. Can still reject.

**Stage 8 -- WorkerInference**: Dispatches the inference request to the selected worker over
UDS. The worker executes using the MLX backend, applying the selected LoRA adapters with the
provided `RouterRing`. Streaming responses are proxied back via SSE.

**Stage 9 -- PolicyAfterInference**: Evaluates post-inference policy hooks on the generated
output. Checks content safety, numeric precision, output format compliance, and similar
policies. Can redact or reject output.

**Stage 10 -- EvidenceTelemetry**: Creates the `RunReceipt`, updates the trace chain
(`run_head`), logs diagnostic events, and appends to the telemetry pipeline.

**Stage 11 -- ResponseAssembly**: Assembles the final response. For streaming, this is the SSE
frame containing the final receipt. For batch, the complete response body with receipt attached.

### 4.2 .aos Adapter Lifecycle

```
                       +--------+
                       | Author |
                       +---+----+
                           |
                    Train / Import
                           |
                           v
                   +-------+-------+
                   | SingleFile    |  (JSON, format_version=2)
                   | Adapter       |  Draft state
                   +-------+-------+
                           |
                     Validate & Sign
                           |
                           v
                   +-------+-------+
                   | Binary Archive|  (AOS\0 magic, 64-byte header)
                   | .aos          |  safetensors payloads
                   +-------+-------+
                           |
                   Seal (optional)
                           |
                           v
                   +-------+-------+
                   | Sealed        |  (SEAL magic, 144-byte header)
                   | Container     |  Ed25519 signed, BLAKE3 integrity
                   +-------+-------+
                           |
                   Content-address
                           |
                           v
                   +-------+-------+
                   | objects/      |  Content-addressed store
                   |  {h[0:2]}/    |  Symlink refs: draft/current/previous
                   |   {h[2:10]}/  |
                   |    {hash}.aos |
                   +-------+-------+
                           |
                   Register in DB
                           |
                           v
                   +-------+-------+
                   | Adapter       |  Lifecycle: draft -> active -> retired
                   | Registry      |  Hot-swap compatible
                   +-------+-------+
```

#### Lifecycle States

| State | Description | Can Serve |
|-------|-------------|-----------|
| `draft` | Newly created or imported, not yet validated | No |
| `validating` | Undergoing signature and integrity checks | No |
| `active` | Validated, registered, available for routing | Yes |
| `warm` | Loaded in worker GPU memory, ready for immediate use | Yes |
| `cooling` | Marked for eviction but still serving in-flight requests | Yes (in-flight only) |
| `retired` | Removed from routing, weights may still be cached | No |
| `quarantined` | Policy violation detected, under review | No |

#### Promotion Flow

Promotion from `draft` to `active` requires:
1. Manifest signature verification (Ed25519)
2. Weight integrity check (BLAKE3 hash of safetensors)
3. Policy pack evaluation (all 30 packs must pass or be waived)
4. Human-in-the-loop approval (if configured)

### 4.3 Training Pipeline

```
Documents/Data --> Ingestion --> Dataset Preparation --> LoRA Training --> Checkpoint --> Validation --> Registration

       |               |                |                    |              |              |
  adapteros-       adapteros-       adapteros-           adapteros-     JSON ckpt      adapteros-
  ingest-docs      lora-plan       lora-worker          lora-mlx-ffi   format         verify
```

#### Ingestion

The `adapteros-ingest-docs` crate handles document ingestion with OCR mode support.
Documents are processed into training-ready datasets with configurable strategies:
- **QA** (Question-Answer): Generates question-answer pairs from document content
- **Completion**: Uses raw document text as completion targets

#### Training Configuration

```bash
./aosctl train-docs --docs-dir ./my-docs \
  --training-strategy qa \
  --epochs 5 \
  --register \
  --tenant-id <tenant> \
  --base-model-id Llama-3.2-3B-Instruct-4bit
```

#### Checkpointing

Checkpoints are JSON files with the naming convention:
- Epoch checkpoints: `{adapter_id}_epoch_{N:04}.ckpt`
- Latest checkpoint: `{adapter_id}_latest.ckpt`

Each checkpoint contains: epoch, step, loss, learning_rate, config, weights (lora_a/lora_b
matrices), best_loss, and ISO 8601 timestamp.

Resume is supported via `--resume` flag, which loads from the latest checkpoint in the output
directory.

#### Determinism Note

Training uses the same HKDF seed pipeline as inference but determinism guards are currently
**disabled**. Training outputs may vary across runs. See [3.1](#31-deterministic-inference-runtime-dir).

### 4.4 Hot-Swap Protocol

Hot-swap enables replacing loaded adapters without service interruption.

#### Two-Phase with RCU

```
Phase 1: PRELOAD
  1. Validate all add_ids exist in the adapter registry
  2. Load adapter weights into staging buffers (background)
  3. Compute BLAKE3 stack hash over staged adapter set
  4. Compute GPU buffer fingerprint for integrity
  5. ALL-or-nothing: if any adapter fails to load, abort entire preload

Phase 2: SWAP
  1. Acquire write lock on adapter set (RCU pattern)
  2. Atomic pointer flip: old set -> new set
  3. Old readers continue on old set (RCU grace period)
  4. Verify stack hash matches preload hash
  5. Verify GPU buffer fingerprints
  6. Release old set after grace period
```

#### Rollback

If the swap fails (hash mismatch, GPU buffer corruption, policy violation), the system
rolls back to the previous adapter set. The rollback path:
1. Atomic pointer flip back to old set
2. Release staged buffers
3. Log rollback event with evidence
4. Emit alert via SSE Alerts stream

#### Lock Ordering

Documented lock ordering prevents deadlocks during hot-swap:
1. Adapter registry lock (read or write)
2. Worker state lock
3. GPU buffer lock
4. Telemetry buffer lock

Acquiring locks out of order is a programming error caught by debug assertions.

### 4.5 Human-in-the-Loop Review Workflow

AdapterOS supports a *pause → review → resume* protocol for cases where inference must not proceed
without human input. This mechanism is intentionally **gate-only** in v0.14.1: the control plane
forwards a review payload to the worker to resume execution, but **does not persist** the review as
a first-class database record.

Sources:
- Shared protocol types: `crates/adapteros-api-types/src/review.rs`
- Control plane HTTP handlers: `crates/adapteros-server-api/src/handlers/review.rs`
- Pause queue + worker forwarding: `crates/adapteros-server-api/src/pause_tracker.rs`
- Worker-side pause registry: `crates/adapteros-lora-worker/src/inference_pause.rs`
- UI pause event plumbing: `crates/adapteros-ui/src/signals/chat.rs`
- CLI workflows: `crates/adapteros-cli/src/commands/review.rs`

#### Architecture

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│                                  Control Plane                               │
│                                                                              │
│  Axum Routes (protected)                                                     │
│  - GET  /v1/infer/{inference_id}/state                                       │
│  - POST /v1/infer/{inference_id}/review                                      │
│  - GET  /v1/infer/paused                                                     │
│  - GET  /v1/reviews/paused (alias)                                           │
│  - GET  /v1/reviews/{pause_id}                                               │
│  - GET  /v1/reviews/{pause_id}/context                                       │
│  - POST /v1/reviews/submit (alias)                                           │
│                                                                              │
│  ServerPauseTracker (in-memory queue)                                        │
│  - keyed by pause_id                                                         │
│  - tagged with tenant_id                                                     │
│  - stores worker UDS path for resume forwarding                              │
└───────────────┬──────────────────────────────────────────────────────────────┘
                │
                │ UDS (HTTP/JSON) forward: POST /inference/resume/{pause_id}
                │
┌───────────────▼──────────────────────────────────────────────────────────────┐
│                                   Worker                                     │
│                                                                              │
│  InferencePauseRegistry (oneshot resume)                                     │
│  - register(pause) -> Receiver<Review>                                       │
│  - submit_review(...) -> Sender<Review>                                      │
│                                                                              │
│  Inference task blocks until Review is received                              │
└───────────────┬──────────────────────────────────────────────────────────────┘
                │
                │ Paused event during streaming
                │ (piggybacks on inference streaming channel)
                │
┌───────────────▼──────────────────────────────────────────────────────────────┐
│                                 Clients                                      │
│                                                                              │
│  Leptos UI                                                                    │
│  - receives InferenceEvent::Paused { pause_id, inference_id, ... }           │
│  - stores pause_id for navigation                                             │
│  - provides /reviews queue + /reviews/:pause_id detail page                  │
│                                                                              │
│  aosctl CLI                                                                   │
│  - review list/get/export/submit/import/tui                                  │
└──────────────────────────────────────────────────────────────────────────────┘
```

#### Shared Protocol Types

All public request/response payloads and state machine enums live in
`crates/adapteros-api-types/src/review.rs` and are consumed by both server and UI.

**State Machine**

| Type | Variants | Notes |
|------|----------|-------|
| `InferenceState` | `Running`, `Paused(PauseReason)`, `Complete`, `Failed`, `Cancelled` | `Paused` carries the correlation + context payload |
| `PauseKind` | `ReviewNeeded`, `PolicyApproval`, `ResourceWait`, `UserRequested`, `ThreatEscalation` | Control plane maps worker trigger strings into this enum |

**Context & Review Payloads**

| Type | Key Fields | Notes |
|------|------------|-------|
| `PauseReason` | `kind`, `pause_id`, `context: ReviewContext`, `created_at` | `pause_id` is the canonical join key across UI/CLI/API |
| `ReviewContext` | `code`, `question`, `scope: Vec<ReviewScope>`, `metadata` | `metadata` is arbitrary JSON; keep it small and redact secrets |
| `ReviewScope` | `Logic`, `EdgeCases`, `Security`, `Performance`, `Style`, `ApiDesign`, `Testing`, `Documentation` | Hinting for external reviewers (UI + CLI export) |
| `SubmitReviewRequest` | `pause_id`, `review: Review`, `reviewer` | Submitted to `/v1/reviews/submit` (alias) or `/v1/infer/{id}/review` |
| `Review` | `assessment`, `issues`, `suggestions`, `comments`, `confidence` | `confidence` is optional float (0.0 - 1.0) |
| `ReviewIssue` | `severity`, `category`, `description`, `location`, `suggested_fix` | `location` is free-form; prefer `path:line` |
| `IssueSeverity` | `Info`, `Low`, `Medium`, `High`, `Critical` | Sorting is severity-first when presented |

**API Responses**

| Type | Purpose |
|------|---------|
| `InferenceStateResponse` | Response for `/v1/infer/{inference_id}/state` and `/v1/reviews/{pause_id}` |
| `ListPausedResponse` | Response for `/v1/infer/paused` and `/v1/reviews/paused` |
| `SubmitReviewResponse` | Response for `/v1/infer/{id}/review` and `/v1/reviews/submit` |
| `ReviewContextExport` | Export bundle for external reviewers (`GET /v1/reviews/{pause_id}/context`) |

#### Pause Lifecycle (End-to-End)

1. **Trigger**
   - Worker-side inference logic decides that progress must halt without review.
   - A pause token is registered in the worker `InferencePauseRegistry` (oneshot pattern).
2. **Emit `Paused`**
   - During streaming inference, the worker emits a `Paused` event carrying:
     `pause_id`, `inference_id`, `trigger_kind`, `context`, `text_so_far`, `token_count`
     (`crates/adapteros-ui/src/signals/chat.rs` expects this shape).
3. **Control Plane Registers Pause**
   - `ServerPauseTracker::register_pause(tenant_id, event, uds_path)` stores:
     `tenant_id`, `pause_id`, `inference_id`, trigger info, and worker UDS path
     (`crates/adapteros-server-api/src/pause_tracker.rs`).
4. **UI/CLI Surfaces Queue**
   - UI:
     - Chat renders a “Paused” banner and stores `pause_id` for navigation.
     - Review queue: `/reviews` (polling fallback; no dedicated review SSE stream).
     - Review detail: `/reviews/:pause_id` (structured form).
   - CLI: `aosctl review list|get|export|submit|import|tui`
5. **Submit Review**
   - Client POSTs `SubmitReviewRequest` to:
     - `/v1/reviews/submit` (pause_id-only), or
     - `/v1/infer/{inference_id}/review` (includes additional pause_id↔inference_id binding check).
6. **Forward To Worker**
   - Control plane forwards the review over UDS to the worker resume endpoint:
     `POST /inference/resume/{pause_id}` (`crates/adapteros-server-api/src/pause_tracker.rs`).
7. **Resume**
   - Worker receives the review, sends it on the oneshot Sender, and continues inference.
   - Control plane removes the pause entry only after worker confirms `"status": "resumed"`.

#### Trigger Taxonomy (Worker → PauseKind)

The wire-level `trigger_kind` is a string emitted by the worker and mapped to `PauseKind`
in `crates/adapteros-server-api/src/pause_tracker.rs` (see `parse_trigger_kind`).

Common trigger kinds:
- `uncertainty` → `PauseKind::ReviewNeeded`
- `policy_violation` / `policy_approval` → `PauseKind::PolicyApproval`
- `safety_gate` / `threat_escalation` → `PauseKind::ThreatEscalation`
- `resource_wait` → `PauseKind::ResourceWait`
- `user_requested` → `PauseKind::UserRequested`

#### API Surface (Review Protocol)

All routes are registered under protected routing in
`crates/adapteros-server-api/src/routes/mod.rs` and implemented in
`crates/adapteros-server-api/src/handlers/review.rs`.

| Method | Route | Request | Response | Notes |
|--------|-------|---------|----------|------|
| GET | `/v1/infer/{inference_id}/state` | (path) | `InferenceStateResponse` | Returns `Paused(PauseReason)` when paused |
| POST | `/v1/infer/{inference_id}/review` | `SubmitReviewRequest` | `SubmitReviewResponse` | Validates `pause_id` belongs to the path inference_id |
| GET | `/v1/infer/paused` | `kind?` | `ListPausedResponse` | `kind` is optional string filter |
| GET | `/v1/reviews/paused` | `kind?` | `ListPausedResponse` | Alias for CLI/UI compatibility |
| GET | `/v1/reviews/{pause_id}` | (path) | `InferenceStateResponse` | Pause lookup by pause_id |
| GET | `/v1/reviews/{pause_id}/context` | (path) | `ReviewContextExport` | Bundled export for external review tools |
| POST | `/v1/reviews/submit` | `SubmitReviewRequest` | `SubmitReviewResponse` | Alias for CLI/UI compatibility |

#### CLI Examples

From `crates/adapteros-cli/src/commands/review.rs`:

```bash
# List paused items
aosctl review list
aosctl review list --kind review-needed

# Inspect a pause
aosctl review get pause-abc123

# Export a context bundle for an external reviewer
aosctl review export pause-abc123 -o context.json

# Submit an inline review
aosctl review submit pause-abc123 --approve
aosctl review submit pause-abc123 --needs-changes --issue "Missing validation" --suggestion "Add tests"

# Import a JSON response from an external reviewer
aosctl review import pause-abc123 -f response.json --reviewer external
```

#### Integration With The 11-Stage Inference Pipeline

Review pauses occur inside **Stage 8: Worker Inference (UDS)** of `InferenceCore::route_and_infer()`
(`crates/adapteros-server-api/src/inference_core/core.rs`).

- The worker may emit `Paused` mid-stream.
- The control plane registers the pause and keeps the streaming task alive while the worker blocks.
- After review submission, the worker resumes, tokens continue, and the pipeline proceeds through:
  - Stage 9 (post-inference policy hooks)
  - Stage 10 (evidence/telemetry)
  - Stage 11 (response assembly)

#### Tenant Isolation & Auth

In v0.14.1, review queue operations enforce tenant isolation at the API boundary:
- Pause entries are tagged with a `tenant_id` at registration time (`ServerPauseTracker`).
- List and detail endpoints filter/validate access using the standard tenant isolation engine
  (`crates/adapteros-server-api/src/handlers/review.rs`, `crates/adapteros-server-api/src/security/mod.rs`).

This keeps the pause queue in-memory for fast routing while preventing cross-tenant access.

#### Webhook (Stub)

If `server.review_webhook_url` is configured, the control plane emits a best-effort
`review_submitted` HTTP POST after successful submission (`crates/adapteros-server-api/src/handlers/review.rs`).
This is intended for lightweight notifications and is not a durable delivery mechanism.

Outbound webhook requests are protected by SSRF guards by default. If you need to send webhooks
to private-network targets, set `server.ssrf_protection = false`.

#### Field-Level Reference (Review Protocol Types)

This subsection is an *authoritative field reference* for the review protocol payloads as they
exist in code at v0.14.1. All types below are defined in `crates/adapteros-api-types/src/review.rs`
and are shared between control plane, UI, and CLI.

##### `PauseReason`

| Field | Type | Notes |
|------|------|------|
| `kind` | `PauseKind` | Why the inference paused |
| `pause_id` | `String` | Canonical correlation ID (UI/CLI/API join key) |
| `context` | `ReviewContext` | Content + question presented to the reviewer |
| `created_at` | `Option<String>` | ISO 8601 timestamp (present in server responses) |

##### `ReviewContext`

| Field | Type | Notes |
|------|------|------|
| `code` | `Option<String>` | Content to review (may be generated text so far, or code snippet) |
| `question` | `Option<String>` | The reviewer-facing question/prompt |
| `scope` | `Vec<ReviewScope>` | Focus areas; empty means “general review” |
| `metadata` | `Option<serde_json::Value>` | Extra context (paths, ids, token_count) |

##### `SubmitReviewRequest`

| Field | Type | Notes |
|------|------|------|
| `pause_id` | `String` | Must match a currently registered pause |
| `review` | `Review` | The structured review payload |
| `reviewer` | `String` | Defaults to `"human"` if omitted |

##### `Review`

| Field | Type | Notes |
|------|------|------|
| `assessment` | `ReviewAssessment` | Overall disposition |
| `issues` | `Vec<ReviewIssue>` | Structured findings; empty allowed |
| `suggestions` | `Vec<String>` | Actionable suggestions; empty allowed |
| `comments` | `Option<String>` | Free-form human notes |
| `confidence` | `Option<f32>` | Intended range: `0.0..=1.0` (not currently enforced server-side) |

##### `ReviewIssue`

| Field | Type | Notes |
|------|------|------|
| `severity` | `IssueSeverity` | `Info`..`Critical` |
| `category` | `ReviewScope` | Scope bucket (Logic/Security/etc.) |
| `description` | `String` | Human-readable issue text |
| `location` | `Option<String>` | Free-form location; prefer `path:line` |
| `suggested_fix` | `Option<String>` | Concrete remediation hint |

##### `InferenceStateResponse`

| Field | Type | Notes |
|------|------|------|
| `schema_version` | `String` | API schema version marker |
| `inference_id` | `String` | Stable inference request ID |
| `state` | `InferenceState` | Includes `Paused(PauseReason)` when paused |
| `paused_at` | `Option<String>` | RFC3339 timestamp (server-only) |
| `paused_duration_secs` | `Option<u64>` | Duration in seconds (server-only) |

##### `SubmitReviewResponse`

| Field | Type | Notes |
|------|------|------|
| `schema_version` | `String` | API schema version marker |
| `accepted` | `bool` | True when forwarded and worker confirmed resume |
| `new_state` | `InferenceState` | New state (typically `Running`) |
| `message` | `Option<String>` | Human-readable status message |

##### `ListPausedResponse` / `PausedInferenceInfo`

| Field | Type | Notes |
|------|------|------|
| `ListPausedResponse.paused` | `Vec<PausedInferenceInfo>` | The current queue snapshot |
| `PausedInferenceInfo.inference_id` | `String` | Inference request ID |
| `PausedInferenceInfo.pause_id` | `String` | Pause correlation ID |
| `PausedInferenceInfo.kind` | `PauseKind` | Pause reason classification |
| `PausedInferenceInfo.paused_at` | `String` | RFC3339 timestamp |
| `PausedInferenceInfo.duration_secs` | `u64` | Time since pause registered (seconds) |
| `PausedInferenceInfo.context_preview` | `Option<String>` | Optional truncated preview string |

##### `ReviewContextExport`

This export bundle is intended for tooling workflows (external reviewers) and is returned by
`GET /v1/reviews/{pause_id}/context` (`crates/adapteros-server-api/src/handlers/review.rs`).

| Field | Type | Notes |
|------|------|------|
| `pause_id` | `String` | Correlation |
| `inference_id` | `String` | Inference id |
| `kind` | `String` | Stringified `PauseKind` |
| `paused_at` | `String` | RFC3339 |
| `duration_secs` | `u64` | Seconds |
| `code` | `Option<String>` | Content to review |
| `question` | `Option<String>` | Prompt to reviewer |
| `scope` | `Vec<String>` | Stringified `ReviewScope` |
| `metadata` | `Option<serde_json::Value>` | Arbitrary structured context |
| `instructions` | `String` | Tool-facing instructions for response formatting |

#### Review State Transitions (Formal)

The review protocol is a thin state machine embedded into the broader inference lifecycle:

| From | Trigger | To | Notes |
|------|---------|----|------|
| `Running` | Worker emits pause | `Paused(PauseReason)` | Control plane registers in `ServerPauseTracker` |
| `Paused(..)` | Successful submit + worker confirms `"resumed"` | `Running` | Pause entry removed only after confirmation |
| `Paused(..)` | Worker rejects payload | `Paused(..)` | Entry remains for retry (error returned to client) |
| `Paused(..)` | Control plane restart | *untracked* | Queue is in-memory; pause is lost unless re-emitted |

**Invariants**:
- `pause_id` is the stable correlation key across UI/CLI/API; all submissions must target a
  specific `pause_id`.
- `/v1/infer/{inference_id}/review` must validate that the submitted `pause_id` is associated
  with the path `inference_id` (`crates/adapteros-server-api/src/handlers/review.rs`).
- Tenant isolation is enforced at API boundaries by validating the pause entry tenant id against
  the authenticated claims (`crates/adapteros-server-api/src/security/mod.rs`).

#### UI Integration Notes

Review-related UI surfaces live under `crates/adapteros-ui/src/pages/`:
- `/reviews` queue: polling-based refresh (10s) until a dedicated review SSE stream exists.
- `/reviews/:pause_id` detail: pause context display + structured submit form.
- Chat: pause banner rendered from `InferenceEvent::Paused` and includes deep links to review
  pages (`crates/adapteros-ui/src/signals/chat.rs`, `crates/adapteros-ui/src/pages/chat.rs`).

There is **no** `/v1/stream/reviews` SSE stream in v0.14.1; review events piggyback on inference
streaming via `InferenceEvent::Paused` and queue updates are surfaced by polling.

#### Failure Modes & Recovery

The pause queue is intentionally minimal in v0.14.1 and has the following operational properties:

- **Control plane restart**: all queued pauses are lost (in-memory). The worker may still be
  paused, but the operator must re-establish correlation (typically by re-running inference or
  re-emitting pause from worker logic).
- **Worker restart**: resume channels (oneshot) are lost. Submitting a review will fail until
  the worker re-registers a new pause and provides a fresh `pause_id`.
- **Webhook delivery**: best-effort only; integrations must tolerate drops and implement their
  own reconciliation strategy if they require reliable delivery.

---

## 5. API & Frontend

### 5.1 REST API (300+ Endpoints)

The API surface spans 300+ REST/JSON endpoints organized by the route tier system.

#### OpenAI-Compatible Shim

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/chat/completions` | POST | Chat completion (streaming and non-streaming) |
| `/v1/completions` | POST | Text completion |
| `/v1/embeddings` | POST | Embedding generation |
| `/v1/models` | GET | List available models |

These endpoints accept standard OpenAI API request formats and return compatible response
structures, enabling drop-in replacement in existing toolchains.

#### Core Endpoints

| Category | Example Routes | Auth Tier |
|----------|---------------|-----------|
| Health | `/healthz`, `/readyz`, `/system/ready` | None / Public |
| Models | `/v1/models`, `/v1/models/{id}` | Public / Protected |
| Adapters | `/v1/adapters`, `/v1/adapters/{id}` | Protected |
| Stacks | `/v1/stacks`, `/v1/stacks/{id}` | Protected |
| Training | `/v1/training/jobs`, `/v1/training/checkpoints` | Protected / Optional-Auth |
| Inference | `/v1/inference`, `/v1/chat/completions` | Protected |
| Policy | `/v1/policies`, `/v1/policies/{id}/evaluate` | Protected |
| Telemetry | `/v1/telemetry/events`, `/v1/telemetry/bundles` | Protected |
| Audit | `/v1/audit/log`, `/v1/audit/receipts` | Protected |
| Admin | `/v1/admin/lifecycle`, `/v1/services/*` | Protected (admin role) |
| Workers | `/v1/workers/register`, `/v1/workers/heartbeat` | Internal |
| SSE | `/v1/events/{stream_type}` | Protected |
| Diagnostics | `/v1/diagnostics`, `/v1/replay` | Protected |
| Boot | `/v1/boot/attestation`, `/v1/boot/phases` | Public |
| Federation | `/v1/federation/*` | Protected |
| Discovery | `/v1/discovery/*` | Protected |
| Git | `/v1/git/*` | Protected |
| Datasets | `/v1/datasets/*` | Protected |

#### Authentication

| Mode | Mechanism | Use Case |
|------|-----------|----------|
| JWT (Ed25519) | `Authorization: Bearer <token>` | Production |
| JWT (HMAC) | `Authorization: Bearer <token>` | Development |
| API Key | `X-API-Key: <key>` | Automation |
| TOTP MFA | Second factor after JWT | High-security operations |
| Dev Bypass | `AOS_DEV_NO_AUTH=1` or `security.dev_bypass = true` | Local development |

#### RBAC

Three roles with 57 permissions:

| Role | Permissions | Description |
|------|------------|-------------|
| `admin` | All 57 | Full system control |
| `operator` | ~40 | Inference, training, adapter management |
| `viewer` | ~15 | Read-only access to status, metrics, logs |

#### OpenAPI Documentation

Full OpenAPI 3.0 specification generated via `utoipa`. Swagger UI available at `/swagger-ui`
when the server is running.

#### Error Codes

All errors return structured responses with machine-readable error codes. The error code
registry is in `crates/adapteros-error-registry/`. Error codes can be explained via:

```bash
./aosctl explain <error-code>
```

### 5.2 SSE Streaming Architecture

The SSE manager supports 15 stream types with per-type buffer capacities:

| Stream Type | Capacity | Description |
|-------------|----------|-------------|
| `SystemMetrics` | 1000 | CPU, memory, disk, GPU metrics |
| `Telemetry` | 1500 | Telemetry events from all components |
| `AdapterState` | 1000 | Adapter lifecycle state transitions |
| `Workers` | 500 | Worker status updates |
| `Training` | 500 | Training progress and signals |
| `Alerts` | 200 | System alerts and notifications |
| `Anomalies` | 200 | Anomaly detection events |
| `Dashboard` | 1000 | Dashboard widget metrics |
| `Inference` | 2000 | Token-by-token inference streaming |
| `Discovery` | 1000 | Model discovery events |
| `Activity` | 1000 | Workspace activity events |
| `BootProgress` | 1000 | Boot progress events |
| `DatasetProgress` | 1000 | Dataset processing progress |
| `GitProgress` | 1000 | Git operations progress |
| `TraceReceipts` | 1000 | Inference trace receipts |

Source: `crates/adapteros-server-api/src/sse/types.rs`

#### SSE Exclusions

SSE stream requests are excluded from the idempotency middleware. Detection is via the
`Accept: text/event-stream` header.

#### Client-Side SSE (Leptos)

The Leptos frontend implements a robust SSE client with:

**State Machine**:
```
Disconnected -> Connecting -> Connected -> Error -> CircuitOpen
                    |              |          |
                    +--> Connected +          |
                                   +-> Error +
                                        |
                                        v
                                   CircuitOpen -> (auth probe) -> Connecting
```

**Circuit Breaker Configuration**:

Defaults come from `CircuitBreakerConfig::default()` in `crates/adapteros-ui/src/api/sse.rs`.

| Parameter | Default | Notes |
|-----------|---------|-------|
| `failure_threshold` | 3 | Circuit opens after N consecutive failures |
| `retry_delay_ms` | 1000 | Base backoff |
| `max_retry_delay_ms` | 30000 | Backoff cap |
| `reset_timeout_ms` | 60000 | Circuit transitions back toward connecting after this timeout |
| `idle_timeout_ms` | 120_000 | Watchdog recycles the connection if no events arrive |
| `with_credentials` | true | Sends cookies for same-origin SSE auth |
| `auth_query_param` | None | Optional `(key,value)` for environments without cookie auth |

Backoff uses exponential growth (`compute_backoff_ms`) and is applied from the failure handler
path (`handle_failure`) in `crates/adapteros-ui/src/api/sse.rs`.

**Idle Watchdog**: If no events are received for `idle_timeout_ms` (default 120s), the client
automatically reconnects (`crates/adapteros-ui/src/api/sse.rs`).

**Auth Probe**: When the circuit breaker opens, the client probes `/v1/auth/me` before
attempting reconnection. This prevents reconnection storms when the auth token has expired.
See `probe_auth_and_stop_on_unauthorized` in `crates/adapteros-ui/src/api/sse.rs`.

**JSON Parse-Failure Recycling (EventSource Streams)**:

The typed SSE helper `use_sse_json_events()` maintains a *consecutive* JSON parse-failure counter
and **reconnects after 3 failures**, resetting the counter on the next successful parse
(`crates/adapteros-ui/src/api/sse.rs`).

**Streaming Inference (POST + ReadableStream)**: Uses the raw `fetch` API with `ReadableStream`, not `EventSource`,
because `EventSource` does not support POST bodies or custom headers needed for authenticated
inference requests (`crates/adapteros-ui/src/signals/chat.rs`).

Parser behavior (as of v0.14.1):
- The client attempts to parse each SSE `data:` payload as `InferenceEvent` first, then as an
  OpenAI-compatible chunk. If both parses fail, the payload is **ignored** with **no parse-failure
  counter** (`crates/adapteros-ui/src/signals/chat.rs`).
- `InferenceEvent::Paused { pause_id, inference_id, trigger_kind, ... }` is recognized and used
  to populate the chat pause banner and navigation into the review flow
  (`crates/adapteros-ui/src/signals/chat.rs`, review pages under `crates/adapteros-ui/src/pages/`).

### 5.3 Leptos WASM Frontend

The UI is a Leptos 0.7 client-side rendered (CSR) WASM application.

#### Build Toolchain

```bash
# Development
cd crates/adapteros-ui && trunk serve   # Hot reload on port 8081

# Production (outputs to static/)
trunk build --release                   # wasm-opt with --enable-bulk-memory
```

The `Trunk.toml` configures output to `../adapteros-server/static/`, meaning the compiled
WASM frontend is served directly by the control plane server. No separate frontend server
is needed.

#### Directory Structure

```
crates/adapteros-ui/
  src/
    api/         # Typed API client, error handling, SSE streaming
    components/  # Reusable Leptos components (Button, Card, Table, etc.)
    pages/       # Route pages (Dashboard, Adapters, Chat, etc.)
    hooks/       # Custom hooks (use_api_resource, use_polling, etc.)
    contexts/    # Context providers (AuthProvider)
    signals/     # Reactive signal definitions
    validation.rs  # Form validation rules
    lib.rs       # App root and router
  dist/
    glass.css    # Liquid Glass design system
  Trunk.toml     # WASM build configuration
  index.html     # HTML shell
```

#### Shared API Types

The UI and server share type definitions through `adapteros-api-types`:

```toml
# In crates/adapteros-ui/Cargo.toml
adapteros-api-types = { path = "../adapteros-api-types", features = ["wasm"] }
```

This ensures compile-time type consistency between the Rust server and WASM client. The `wasm`
feature gate removes server-only dependencies (sqlx, tokio) from the WASM build.

#### Error Handling

UI errors must call `report_error_with_toast()`, not just `console::error_1()`. This surfaces
errors visually and logs them to the telemetry pipeline.

### 5.4 Liquid Glass Design System

The UI uses a three-tier glass morphism design system implemented in pure CSS.

#### Tier Definitions

| Tier | Usage | Blur | Background Alpha | Border |
|------|-------|------|-----------------|--------|
| **1** | Headers, nav bars, inputs | 9.6px | 70% | 1px hsla(0, 0%, 100%, 0.30) |
| **2** | Cards, panels, content areas | 12px | 78% | 1px hsla(0, 0%, 100%, 0.30) |
| **3** | Dialogs, popovers, modals | 15.6px | 85% | 1px hsla(0, 0%, 100%, 0.30) |

#### Dark Mode

| Property | Light | Dark |
|----------|-------|------|
| Background alpha range | 70-85% | 65-80% |
| Blur intensity | Standard | +33% (e.g., 16px vs 12px for Tier 2) |
| Base color | White translucency | Deep blue translucency |

#### Design Tokens

All values are CSS custom properties:

```css
--glass-blur-t1: 9.6px;
--glass-blur-t2: 12px;
--glass-blur-t3: 15.6px;
--glass-alpha-t1: 0.70;
--glass-alpha-t2: 0.78;
--glass-alpha-t3: 0.85;
--glass-border: 1px solid hsla(0, 0%, 100%, 0.30);
```

#### Constraints

- **Borders required**: Every glass element must have the 1px white border at 30% opacity.
- **Noise overlay**: 2% opacity fractalNoise SVG pattern applied as pseudo-element.
- **Motion policy**: State-change animations only. No idle animations, no decorative motion.
  Transitions are reserved for user-initiated state changes (hover, focus, expand/collapse).

Source: `crates/adapteros-ui/dist/glass.css`

---

## 6. Implementation Details

### 6.1 Core Structs Reference

#### RouterRing

```rust
// crates/adapteros-lora-kernel-api/src/lib.rs
pub struct RouterRing {
    pub indices: [u16; 8],       // Adapter indices, zero-filled for unused
    pub gates_q15: [i16; 8],    // Q15 quantized gates (-32767 to +32767)
    pub token_pos: u32,         // Token position in sequence
}
```

**Consumed by**: Metal kernels, CoreML ANE operations, MLX FFI backend.
**Invariant**: Unused positions must be zero (both index and gate).

#### Decision

```rust
// crates/adapteros-lora-router/src/types.rs
pub struct Decision {
    pub indices: SmallVec<[u16; 8]>,
    pub gates_q15: SmallVec<[i16; 8]>,
    pub entropy: f32,
    pub candidates: Vec<DecisionCandidate>,
    pub decision_hash: Option<DecisionHash>,
}
```

**Produced by**: K-sparse router after scoring and selection.
**Consumed by**: `RouterRing` packing, receipt creation, audit logging.

#### RoutingDecision

```rust
// crates/adapteros-lora-router/src/types.rs
pub enum RoutingDecision {
    Selected(Decision),
    Abstain(RouterAbstainReason),
}
```

**Semantics**: The router either selects adapters or explicitly abstains with a reason.
Abstaining is preferred over forcing a low-confidence selection.

#### PolicyMask

```rust
// crates/adapteros-lora-router/src/policy_mask.rs
pub struct PolicyMask {
    pub allowed: Vec<bool>,              // Per-adapter allow bits
    pub digest: B3Hash,                  // Cryptographic binding
    pub overrides: PolicyOverrideFlags,  // Override sources applied
}
```

**Produced by**: Policy engine (Stage 3).
**Consumed by**: Router (Stage 5) to mask forbidden adapters.

#### ExecutionContext

```rust
// crates/adapteros-lora-worker/src/execution.rs
pub struct ExecutionContext {
    context_digest: B3Hash,   // BLAKE3(prompt + metadata)
    run_head: B3Hash,         // Running hash chain accumulator
    token_pos: u32,           // Current token position
    backend_id: BackendId,    // Which backend is executing
    // ... additional fields for verified context (type-state)
}
```

**Invariant**: The `run_head` chain is updated at each step, creating a Merkle-like hash chain
over the entire execution.

#### TypedSeed

```rust
// crates/adapteros-core/src/seed.rs
pub struct TypedSeed {
    pub version: u32,         // Must match HKDF_ALGORITHM_VERSION
    bytes: [u8; 32],          // HKDF-derived seed value
    pub checksum: B3Hash,     // BLAKE3(bytes) for integrity
}
```

**Validated at**: Every FFI boundary, every context transition, every backend entry point.

#### RunReceipt

```rust
// crates/adapteros-types/src/inference.rs
pub struct RunReceipt<Hash = String> {
    pub trace_id: String,
    pub run_head_hash: Hash,
    pub output_digest: Hash,
    pub receipt_digest: Hash,
    pub logical_prompt_tokens: u32,
    pub prefix_cached_token_count: u32,
    pub billed_input_tokens: u32,
    pub logical_output_tokens: u32,
    pub billed_output_tokens: u32,
    // ... 40+ additional fields covering routing, policy, timing, cache
}
```

**Schema version**: V7 (current). See [3.4](#34-cryptographic-receipts-v7-schema) for full
schema evolution history.

#### DeviceFingerprint

```rust
// crates/adapteros-verify/src/metadata.rs
pub struct DeviceFingerprint {
    pub schema_version: u8,
    pub device_model: String,         // e.g., "MacBookPro18,3"
    pub soc_id: String,               // e.g., "Apple M1 Pro"
    pub gpu_pci_id: String,           // Metal device registry ID
    pub os_version: String,           // e.g., "14.0"
    pub os_build: String,             // e.g., "23A344"
    pub metal_family: String,         // e.g., "Apple9"
    pub gpu_driver_version: String,
    pub path_hash: B3Hash,            // BLAKE3(PATH env var)
    pub env_hash: B3Hash,             // BLAKE3(sorted env vars)
    pub cpu_features: Vec<String>,
    pub firmware_hash: Option<B3Hash>,
    pub bootloader_hash: Option<B3Hash>,
    // ... additional fields (17 total hardware/software attributes)
}
```

**Used for**: Drift detection with Critical/Warning/Info severity levels. A fingerprint change
at Critical severity (e.g., SoC change) invalidates all cached adapter bindings.

#### BootAttestation

```rust
// crates/adapteros-boot/src/evidence.rs
pub struct BootAttestation {
    pub schema_version: u8,
    pub boot_id: String,              // Unique boot identifier
    pub merkle_root: B3Hash,          // Merkle root over all phase evidence
    pub phase_count: u32,             // Number of phases completed
    pub total_boot_time_ms: u64,
    // ... additional fields for signing, device binding
}
```

**Signed with**: Ed25519 via JCS (JSON Canonicalization Scheme) serialization.

### 6.2 .aos Format Specification

The `.aos` family supports multiple packaging forms serving different lifecycle stages.

- **Binary Archive** (`AOS\0` magic): compact, segment-indexed container for multi-backend
  artifacts. Source: `crates/adapteros-aos/src/writer.rs`.
- **Binary Sealed Container** (`SEAL` magic): integrity hash + Ed25519 signature wrapper over
  a manifest + payload. Source: `crates/adapteros-aos/src/sealed.rs`.
- **SingleFileAdapter (JSON)**: training/portability oriented format with embedded manifest,
  weights, and provenance. Source: `crates/adapteros-aos/src/single_file/format.rs`.

#### Variant 1: Binary Archive (AOS\0)

The writer uses a fixed 64-byte header and a fixed 80-byte index entry layout:
`HEADER_SIZE = 64`, `INDEX_ENTRY_SIZE = 80` (`crates/adapteros-aos/src/writer.rs`).

**Header** (64 bytes, little-endian):

```text
| Offset | Size | Field                         |
|--------|------|-------------------------------|
| 0      | 4    | Magic: "AOS\0"                |
| 4      | 4    | Flags (u32 LE)                |
| 8      | 8    | Index offset (u64 LE)         |
| 16     | 8    | Index size (u64 LE, bytes)    |
| 24     | 8    | Manifest offset (u64 LE)      |
| 32     | 8    | Manifest size (u64 LE, bytes) |
| 40     | 24   | Reserved (zero)               |
```

**Index Entry** (80 bytes each, little-endian):

```text
| Offset | Size | Field                                     |
|--------|------|-------------------------------------------|
| 0      | 4    | segment_id (u32 LE)                       |
| 4      | 2    | backend_tag (u16 LE)                      |
| 6      | 2    | reserved                                  |
| 8      | 8    | offset (u64 LE)                            |
| 16     | 8    | length (u64 LE)                            |
| 24     | 16   | scope_hash (BLAKE3(scope_path)[0..16])    |
| 40     | 32   | content_hash (BLAKE3(payload_bytes))      |
| 72     | 8    | reserved                                  |
```

**Scope Hash**:

`compute_scope_hash(scope_path)` is defined as `BLAKE3(scope_path)[0..16]`
(`crates/adapteros-aos/src/writer.rs`).

**Backend Tags** (`BackendTag::as_u16()` in `crates/adapteros-aos/src/writer.rs`):

| Value | Backend |
|-------|---------|
| 0 | Canonical |
| 1 | MLX |
| 2 | Metal |
| 3 | CoreML |

**Payloads**:

The binary archive stores backend-specific segment payload bytes plus a JSON manifest blob.
The writer enforces content hash matching and scope hash consistency before writing.

#### Variant 2: Binary Sealed Container (SEAL)

The binary sealed container header is fixed-size and 64-byte aligned
(`SEALED_HEADER_SIZE = 144` in `crates/adapteros-aos/src/sealed.rs`).

**Header** (144 bytes):

```text
| Offset | Size | Field                                   |
|--------|------|-----------------------------------------|
| 0      | 4    | Magic: "SEAL"                           |
| 4      | 1    | Version (u8)                            |
| 5      | 3    | Reserved                                |
| 8      | 32   | integrity_hash (BLAKE3 of contents)     |
| 40     | 8    | payload_offset (u64 LE)                 |
| 48     | 8    | payload_size (u64 LE)                   |
| 56     | 8    | manifest_offset (u64 LE)                |
| 64     | 8    | manifest_size (u64 LE)                  |
| 72     | 64   | signature (Ed25519 over integrity_hash) |
| 136    | 8    | Reserved                                |
```

**Integrity Hash**:

`integrity_hash = BLAKE3(version_byte || manifest_bytes || payload_bytes)` and is verified
before parsing the manifest/payload (`crates/adapteros-aos/src/sealed.rs`).

**Signer Identity**:

The header does not embed the signer public key. During load, the verifier tests the signature
against the configured trusted public keys and records the verifying key used.

#### Variant 3: SingleFileAdapter (JSON)

`SingleFileAdapter` is a JSON container with `AOS_FORMAT_VERSION = 2` and an embedded manifest
(`crates/adapteros-aos/src/single_file/format.rs`).

At a high level:
- `manifest`: `AdapterManifest` (schema-versioned metadata)
- `weights`: `AdapterWeights` with positive/negative groups and optional combined group
- `training_data`, `config`, `lineage`
- `signature`: optional signature metadata

**Content Hash (Identity)**:

`compute_content_hash()` hashes the manifest (with `content_hash = None` to avoid circularity)
plus the serialized weights: `BLAKE3(manifest_bytes || weights_bytes)` (`crates/adapteros-aos/src/single_file/format.rs`).

#### Content-Addressed Storage

Adapters are stored in a content-addressed filesystem:

```
objects/
  {hash[0:2]}/
    {hash[2:10]}/
      {hash}.aos
```

**Symlink-based refs** provide named access:

```
refs/
  draft/     -> objects/.../hash.aos
  current/   -> objects/.../hash.aos
  previous/  -> objects/.../hash.aos
```

#### Signing Flow

1. Compute BLAKE3 hash of critical fields (manifest bytes, archive bytes, or sealed container)
2. Sign hash with Ed25519 private key
3. Embed 64-byte signature in sealed container header or attach as sidecar
4. Verification: `Ed25519_verify(public_key, hash, signature)`

Key sources:
- `AOS_SIGNING_KEY_HEX` environment variable
- Deterministic derivation: `BLAKE3(label + adapter_id + key_material)`
- Secure Enclave via `adapteros-secd`

### 6.3 Metal Kernel Suite

Metal shaders are in `metal/` and compiled via `metal/build.sh`.

#### Kernel Categories

| Category | Kernels | Purpose |
|----------|---------|---------|
| LoRA forward | `lora_forward_f16`, `lora_forward_f32` | LoRA weight application |
| Quantized matmul | `qmatmul_q4`, `qmatmul_q8` | Quantized matrix multiplication |
| Activation | `silu`, `gelu`, `relu` | Activation functions |
| Normalization | `rms_norm`, `layer_norm` | Layer normalization |
| Attention | `flash_attention_f16` | Flash attention (Metal-optimized) |
| Reduction | `sum_reduce`, `max_reduce` | Tensor reductions |
| K-sparse routing | `k_sparse_gate_apply` | Apply RouterRing gates to adapter weights |

#### Determinism Guarantees

All Metal kernels enforce:

```metal
#pragma clang fp contract(off)  // Disable FMA contraction
```

This prevents the compiler from fusing `a * b + c` into a single fused multiply-add (FMA)
instruction, which would produce a differently-rounded result than the separate multiply and
add operations. IEEE 754 compliance is mandatory for deterministic output.

#### Thread Group Sizing

Kernels use fixed thread group sizes rather than runtime-optimal sizes to ensure deterministic
execution order across different GPU configurations.

### 6.4 Storage Architecture (SQLite + ReDB)

#### Dual-Write Architecture

The storage layer implements atomic dual-write across SQLite WAL and ReDB key-value store.
Code is complete across all 14 domains with 19 KV modules.

#### Storage Modes

```rust
// crates/adapteros-db/src/lib.rs
pub enum StorageMode {
    /// SQL backend only (default, current production mode)
    SqlOnly,
    /// Both backends written atomically; SQL is authoritative for reads
    DualWrite,
    /// Both written; KV is authoritative for reads (migration validation)
    KvPrimary,
    /// KV backend only (future target)
    KvOnly,
}
```

**Migration path**: `SqlOnly` (current default) -> `DualWrite` -> `KvPrimary` -> `KvOnly`.

The dual-write pattern is intentional, not accidental complexity. It enables:
1. **Gradual migration**: Switch read source without data loss
2. **Consistency verification**: Compare SQL and KV results during migration
3. **Rollback safety**: Fall back to SQL if KV shows issues
4. **Deterministic replay**: Both backends maintain the same audit trail

#### 14 Storage Domains

| Domain | SQL Table(s) | KV Module(s) | Description |
|--------|-------------|--------------|-------------|
| Adapters | `adapters`, `adapter_versions` | `adapters_kv` | Adapter registry and versions |
| Models | `models`, `model_manifests` | `models_kv` | Base model registry |
| Training | `training_jobs`, `checkpoints` | `training_kv` | Training job tracking |
| Telemetry | `telemetry_events`, `bundles` | `telemetry_kv` | Event storage |
| Policy | `policy_evaluations` | `policy_kv` | Policy evaluation history |
| Audit | `audit_log` | `audit_kv` | Audit trail |
| Tenants | `tenants`, `tenant_config` | `tenants_kv` | Tenant management |
| Auth | `users`, `api_keys`, `sessions` | `auth_kv` | Authentication data |
| Receipts | `run_receipts` | `receipts_kv` | Cryptographic receipts |
| Workers | `workers`, `worker_health` | `workers_kv` | Worker registry |
| Datasets | `datasets`, `dataset_items` | `datasets_kv` | Dataset management |
| Federation | `federation_peers`, `tick_entries` | `federation_kv` | Federation state |
| Config | `config_overrides` | `config_kv` | Runtime configuration |
| Diagnostics | `diagnostic_events` | `diagnostics_kv` | Diagnostic history |

#### SQLite Configuration

- **WAL mode**: Write-ahead logging for concurrent read/write
- **Busy timeout**: 5000 ms
- **Journal size limit**: 67108864 bytes (64 MB)
- **Cache size**: -64000 (64 MB, negative means KB)
- **Foreign keys**: Enabled
- **Migrations**: Tracked in `migrations/` with signatures in `migrations/signatures.json`

### 6.5 Boot Sequence (25 Phases)

The boot sequence is defined by the `BootPhase` enum with 25 variants: 20 ordered boot phases
and 5 runtime/shutdown states.

#### Boot Phase Progression

```
Stopped
  |
  v
Starting              [PID lock, config load]
  |
  v
SecurityInit          [Security subsystem initialization]
  |
  v
ExecutorInit          [Deterministic executor setup]
  |
  v
Preflight             [Security preflight checks]
  |
  v
BootInvariants        [Pre-database invariant validation]
  |
  v
DbConnecting          [Establishing database connection]
  |
  v
Migrating             [Running database migrations]
  |
  v
PostDbInvariants      [Post-database invariant validation]
  |
  v
StartupRecovery       [Orphaned resource recovery]
  |
  v
Seeding               [Initial data seeding, dev fixtures, model cache]
  |
  v
LoadingPolicies       [Policy verification, hash watcher, baseline load]
  |
  v
StartingBackend       [MLX/CoreML/Metal initialization]
  |
  v
LoadingBaseModels     [Manifest validation, executor seeding]
  |
  v
LoadingAdapters       [Lifecycle manager, heartbeat recovery]
  |
  v
WorkerDiscovery       [Discovering and registering worker processes]
  |
  v
RouterBuild           [API router construction]
  |
  v
Finalize              [Final boot preparation]
  |
  v
Bind                  [Server socket binding]
  |
  v
Ready                 [Accepting requests, models may still be loading]
  |
  v
FullyReady            [All priority models loaded and health-checked]
```

Source: `crates/adapteros-boot/src/phase.rs`

#### Runtime States

| State | Reachable From | Recovery |
|-------|---------------|----------|
| **FullyReady** | Ready | N/A (target state) |
| **Degraded** | Ready, FullyReady | Can recover to Ready |
| **Failed** | Any non-terminal | Terminal (no recovery) |
| **Maintenance** | Ready, FullyReady | Can recover to Ready |
| **Draining** | Ready, FullyReady, Degraded, Maintenance | Forward to Stopping |
| **Stopping** | Draining | Terminal |

#### Transition Rules

1. Boot phases progress forward monotonically (no backward transitions during boot).
2. `Failed` can be reached from any non-terminal state (critical failure).
3. `Degraded` can only be reached from `Ready` or `FullyReady`.
4. Recovery from `Degraded` to `Ready` is allowed.
5. Terminal states (`Failed`, `Stopping`) prevent further transitions.
6. Backward compatibility shortcuts exist: `Starting` -> `DbConnecting`,
   `Migrating` -> `Seeding`, `LoadingAdapters`/`WorkerDiscovery` -> `Ready`.

#### Boot Attestation

Upon reaching `Ready`, a `BootAttestation` is generated:
1. Collect evidence from each completed phase (timing, hashes, validation results)
2. Build Merkle tree over all phase evidence
3. Compute Merkle root (BLAKE3)
4. Sign attestation with Ed25519 via JCS serialization
5. Store in boot report, expose via `/v1/boot/attestation`

---

## 7. Operations & Security

### 7.1 Deployment Models

| Model | Database | Workers | Use Case |
|-------|----------|---------|----------|
| **Single-node** | SQLite WAL | Local UDS | Development, small deployments |
| **Air-gap bundle** | SQLite WAL | Local UDS | Classified environments, zero egress |
| **macOS installer** | SQLite WAL | Local UDS | Enterprise Mac fleet |
| **Multi-node** | PostgreSQL | Network | Large-scale deployments (*Planned*) |
| **Kubernetes** | PostgreSQL | Pod-per-worker | Cloud/hybrid (*Planned*) |

#### Air-Gap Deployment

The system is designed for zero network egress during serving:
1. All models and adapters are pre-loaded before deployment
2. The `Egress` policy pack (PolicyId=1) enforces network isolation
3. Telemetry is written to local NDJSON bundles (no external collectors)
4. Federation is optional and requires explicit configuration

#### Service Management

```bash
# Start all services
./start

# Start individual services
./start backend
./start worker
./start secd
./start node

# Service manager (explicit per-service control)
scripts/service-manager.sh start <backend|worker|secd|node|ui>
```

The `ui` service is a no-op in the service manager; the static WASM frontend is served
directly by the backend process.

#### Supervisor Integration

External supervisors (see `deploy/supervisor.yaml`) should be configured to restart processes
when they exit. The admin safe-restart endpoint triggers an in-process shutdown after draining;
the supervisor is responsible for restarting.

Configuration via `SUPERVISOR_API_URL` or `AOS_PANEL_PORT`.

### 7.2 Security Architecture

#### Authentication Chain

```
Request -> JWT Validation (Ed25519/HMAC)
              |
              +--> API Key Lookup (X-API-Key header)
              |
              +--> Session Check (cookie-based)
              |
              +--> TOTP MFA (second factor, if required)
              |
              +--> Dev Bypass (AOS_DEV_NO_AUTH=1, debug builds only)
```

#### Cryptographic Primitives

| Primitive | Algorithm | Use |
|-----------|-----------|-----|
| Signing | Ed25519 | Receipts, attestations, sealed adapters |
| Hashing | BLAKE3 | Content addressing, integrity, fingerprints |
| Key derivation | HKDF-SHA256 | Seed derivation, key material expansion |
| RNG | ChaCha20 | Deterministic random number generation |
| Token signing | Ed25519 or HMAC-SHA256 | JWT tokens |

#### Error Redaction

The `SecretString` type auto-redacts sensitive values on `Display` and `Debug`.

The `redact_sensitive()` function applies 14 ordered regex patterns:

| # | Pattern Target | Example |
|---|---------------|---------|
| 1 | Bearer tokens | `Authorization: Bearer eyJ...` |
| 2 | JWT tokens | `eyJhbGciOi...` |
| 3 | API keys | `X-API-Key: sk-...` |
| 4 | Password fields | `password=...` |
| 5 | Private keys | `-----BEGIN PRIVATE KEY-----` |
| 6 | AWS credentials | `AKIA...` |
| 7 | Generic secrets | `secret_key=...` |
| 8 | SSN patterns | `\d{3}-\d{2}-\d{4}` |
| 9 | Credit card numbers | `\d{4}[- ]\d{4}[- ]\d{4}[- ]\d{4}` |
| 10 | Database connection strings | `postgres://...`, `sqlite://...` |
| 11 | Socket paths | `/var/run/...` |
| 12 | Temp file paths | `/tmp/...` |
| 13 | Source file paths | `<repo-root>/**/*.rs` |
| 14 | Hex secrets | `[0-9a-f]{32,}` (long hex strings) |

**Defense in depth**: Redaction is applied both at error construction and at serialization.
Applying only at one point would miss cases where errors are created in one context and
serialized in another.

**Kill switch**: `ADAPTEROS_DISABLE_ERROR_REDACTION=1` disables all redaction for debugging.
This should never be set in production.

#### Worker Process Security

The Node Agent spawns workers with privilege dropping:

```
fork()
  |
  pre_exec:
    setgid(worker_gid)   // Drop group first
    setuid(worker_uid)   // Then drop user
  |
  exec(aos-worker)
```

`setgid` before `setuid` is mandatory. Reversing the order leaves a window where the process
has the target UID but the original GID.

#### UDS Security

- Socket permissions: `0o600` (owner read/write only)
- Body size limit: 16 MB
- No TLS (local-only communication)
- Worker identity verified via registration handshake

### 7.3 Observability Stack

#### Metrics

- **Prometheus**: 15-second scrape interval at `/metrics`
- **Grafana**: Dashboard templates in `deploy/grafana/`
- **AlertManager**: Slack and PagerDuty integration

#### Logging

Four log profiles via `tracing-subscriber`:

| Profile | Output | Verbosity |
|---------|--------|-----------|
| `Json` | Structured JSON | INFO and above |
| `Plain` | Human-readable | INFO and above |
| `Debug` | Verbose with spans | DEBUG and above |
| `Trace` | Full trace | TRACE and above |

Outputs: console + rolling file (`var/logs/`) + OpenTelemetry (optional).

#### Telemetry Pipeline

```
Event Source -> 50K bounded channel -> Aggregator -> NDJSON bundles
                                                      |
                                                      +-> BLAKE3 hash per bundle
                                                      +-> Merkle chain (append-only)
                                                      +-> Ed25519 signature per chain
                                                      +-> var/telemetry/
```

Bundle format: NDJSON (newline-delimited JSON). Each bundle is BLAKE3-hashed. Bundles form a
Merkle chain with Ed25519 signatures for tamper evidence. The 50K bounded channel prevents
unbounded memory growth under telemetry storms.

#### Diagnostics

The 11-stage `DiagStage` enum provides the diagnostic event contract. Each stage emits
structured diagnostic events that include timing, hashes, and stage-specific metadata. These
events are consumed by the telemetry pipeline and exposed via the `/v1/diagnostics` endpoint.

### 7.4 Performance Baselines

#### Inference Latency

| Percentile | Target | Notes |
|------------|--------|-------|
| P50 | < 50 ms | Single adapter, warm cache |
| P95 | < 150 ms | K=3 adapters |
| P99 | < 300 ms | K=5 adapters, cold cache |

#### Router Latency

| Metric | Value |
|--------|-------|
| Typical | 1-3 microseconds |
| Target ceiling | < 100 microseconds |

#### Throughput (M3 Max, 128 GB UMA)

| Configuration | Tokens/second |
|---------------|---------------|
| Base model (no adapters) | 45 tok/s |
| K=3 adapters | 42 tok/s |
| K=5 adapters | 38 tok/s |

**Note**: Only M2 Pro benchmarks exist for the shared model server. M3 Max and M4 benchmarks
are not yet available. The numbers above are extrapolated from M2 Pro results.

#### UMA Memory Thresholds

| Level | Utilization | Action |
|-------|-------------|--------|
| Normal | < 60% | No action |
| Moderate | 60-75% | Log warning, consider adapter eviction |
| High | 75-90% | Evict cold adapters, reduce cache |
| Critical | > 90% | Emergency eviction, reject new loads |

#### Adapter Cache

| Parameter | Value |
|-----------|-------|
| Eviction policy | LRU |
| Max entries | 16 adapters |
| Max total size | 4 GB |
| Max per-adapter | 500 MB |
| TTL | 1 hour |
| Instrumentation | Hit/miss counters, eviction events |

#### Prefix KV Cache

| Metric | Value |
|--------|-------|
| Matching strategy | Longest-prefix |
| Deduplication | Single-flight (concurrent requests share computation) |
| Integrity | BLAKE3 per cached entry |
| Throughput at 75% hit | ~4x baseline |

### 7.5 Worker Management

#### Heartbeat Protocol

Workers maintain heartbeat connections to the control plane on a dedicated blocking thread.
Heartbeat failure triggers the circuit breaker.

#### Retry Policy

| Parameter | Value |
|-----------|-------|
| Base delay | 1 second |
| Multiplier | 2x (exponential backoff) |
| Max delay | 5 minutes |
| Total deadline | 10 minutes |
| Circuit breaker threshold | 10 consecutive failures |

#### UDS Accept Loop Circuit Breaker

The worker UDS server has its own circuit breaker: 5 consecutive accept failures trigger a
graceful shutdown. This prevents a worker from consuming resources when its socket is
persistently failing.

#### Panic Reporting

Workers report panics to the control plane via `POST /fatal`. The control plane logs the panic,
updates the worker's health status, and triggers supervisor restart if configured.

#### Worker UDS Routes (15 Routes)

| Route | Method | Description |
|-------|--------|-------------|
| `/v1/workers/register` | POST | Worker registration |
| `/v1/workers/heartbeat` | POST | Heartbeat update |
| `/v1/workers/status` | GET | Worker status |
| `/v1/inference` | POST | Run inference |
| `/v1/inference/stream` | POST | Streaming inference |
| `/v1/adapters/load` | POST | Load adapter into GPU memory |
| `/v1/adapters/unload` | POST | Unload adapter from GPU memory |
| `/v1/adapters/swap` | POST | Hot-swap adapter set |
| `/v1/adapters/status` | GET | Adapter load status |
| `/v1/training/start` | POST | Start training job |
| `/v1/training/status` | GET | Training job status |
| `/v1/training/cancel` | POST | Cancel training job |
| `/v1/health` | GET | Worker health check |
| `/v1/metrics` | GET | Worker metrics |
| `/fatal` | POST | Report fatal error/panic |

---

## 8. Roadmap & Open Items

### 8.1 Current Limitations

| Area | Limitation | Impact |
|------|-----------|--------|
| **Training determinism** | Determinism guards disabled due to dependency issues between training subsystem and deterministic executor | Training outputs may vary across runs |
| **StorageMode** | Only `SqlOnly` is production-tested. `DualWrite`, `KvPrimary`, and `KvOnly` are code-complete but not validated in production. | ReDB migration path untested under load |
| **Model server benchmarks** | Only M2 Pro benchmarks exist. No M3 Max or M4 data. | Performance claims extrapolated, not measured |
| **PostgreSQL** | Multi-node deployment with PostgreSQL is designed but not implemented | Single-node SQLite only |
| **Kubernetes** | K8s deployment manifests exist but are untested | Not production-ready |
| **TOTP MFA** | Implemented but not exposed in the Leptos UI | CLI/API only |
| **CoreML ANE** | ANE acceleration layer exists but integration testing is limited | May not deliver expected speedup on all models |

#### Human Review Workflow

Review pauses are intentionally minimal and in-memory in v0.14.1. Key limitations:

| Gap | Description | Impact |
|-----|-------------|--------|
| **Tenant isolation (review queue)** | Tenant isolation is enforced at the API boundary, but the pause queue is an in-memory map and is not persisted or partitioned by a durable store. | Restarts lose queued pauses; multi-node review coordination requires external control plane discipline |
| **Review persistence** | Submitted `Review` payloads are forwarded to the worker to resume inference but are not stored as first-class records. | Post-hoc audit of human decisions is limited to logs/telemetry; no long-term review history |
| **Webhook delivery** | Review webhook is best-effort (single URL, no durable retry queue). | Integrations must tolerate drops and implement their own reconciliation |
| **Review signing** | Review submissions are not Ed25519-signed and are not bound into receipts. | Review content is not cryptographically attributable (only indirectly observable via diagnostics/logs) |

### 8.2 Planned for v0.15

| Feature | Description | Priority |
|---------|-------------|----------|
| **Training determinism guards** | Re-enable determinism enforcement for training pipeline | High |
| **DualWrite validation** | Production testing of SQLite + ReDB dual-write mode | Medium |
| **M3/M4 benchmarks** | Official benchmark suite on M3 Max and M4 hardware | Medium |
| **MFA in UI** | Expose TOTP MFA setup and verification in Leptos frontend | Medium |
| **PostgreSQL backend** | Implement PostgreSQL storage backend for multi-node | Low |
| **Adapter hot-reload** | File-watcher based automatic adapter reload | Low |

### 8.3 Long-Term Vision

**v0.16+**: Full multi-node federation with quorum-based consensus. PostgreSQL as primary
storage for multi-node deployments. Kubernetes operator for automated scaling.

**v0.18+**: Hardware residency enforcement via Secure Enclave attestation. Adapters are
cryptographically bound to specific hardware, preventing unauthorized copying or execution
on different devices.

**v1.0**: Production-certified air-gap deployment with full audit trail, regulatory compliance
package (SOC 2 Type II evidence generation), and formal verification of determinism properties
for the core inference path.

**Beyond v1.0**: Cross-platform support (Linux + NVIDIA via CUDA backend). Federated learning
across air-gapped nodes using sealed telemetry bundles. Formal verification of the policy
engine using property-based testing and model checking.

---

## Appendix A: Glossary

| Term | Definition |
|------|-----------|
| **Adapter** | A LoRA (Low-Rank Adaptation) weight set that modifies base model behavior |
| **AOS** | AdapterOS, the platform |
| **B3Hash** | A BLAKE3 hash value (32 bytes) |
| **CSR** | Client-Side Rendering (Leptos WASM frontend) |
| **DIR** | Deterministic Inference Runtime |
| **Drain** | Graceful shutdown phase where new requests are rejected but in-flight requests complete |
| **Gate** | A scalar weight applied to a LoRA adapter's contribution |
| **HKDF** | HMAC-based Key Derivation Function |
| **Hot-swap** | Replacing loaded adapters without service interruption |
| **JCS** | JSON Canonicalization Scheme (RFC 8785) |
| **K-sparse** | Selecting exactly K adapters from a larger pool |
| **KV cache** | Key-Value cache storing attention state for prefix reuse |
| **LoRA** | Low-Rank Adaptation, a parameter-efficient fine-tuning technique |
| **MLX** | Apple's machine learning framework for Apple Silicon |
| **PolicyMask** | Boolean vector gating which adapters the router may select |
| **Q15** | 15-bit signed fixed-point quantization (range: -32767 to +32767) |
| **RCU** | Read-Copy-Update, a synchronization pattern |
| **Receipt** | Cryptographic proof of an inference run's execution |
| **RouterRing** | Fixed-size array struct consumed by GPU kernels |
| **Safetensors** | A safe and fast file format for storing tensors |
| **SFA** | SingleFileAdapter, JSON-based adapter format |
| **SSE** | Server-Sent Events |
| **Tick** | A logical time unit in the deterministic executor |
| **TypedSeed** | Versioned seed with integrity checksum |
| **UDS** | Unix Domain Socket |
| **UMA** | Unified Memory Architecture (Apple Silicon shared CPU/GPU memory) |

## Appendix B: Crate Index

83 workspace members organized by function:

### Core & Types
| Crate | Description |
|-------|-------------|
| `adapteros-core` | Shared types, error handling, seed derivation (HKDF-SHA256) |
| `adapteros-types` | Inference types (RunReceipt, etc.) |
| `adapteros-crypto` | Ed25519 signing, BLAKE3 hashing |
| `adapteros-numerics` | Numeric determinism utilities |
| `adapteros-id` | Typed ID generation with word aliases |
| `adapteros-platform` | Platform utilities and path resolution |

### Router & Inference
| Crate | Description |
|-------|-------------|
| `adapteros-lora-router` | K-sparse adapter routing with Q15 gates |
| `adapteros-lora-plan` | Query planning for adapter selection |
| `adapteros-lora-rag` | RAG retrieval integration |
| `adapteros-lora-lifecycle` | Adapter lifecycle state machine |
| `adapteros-lora-quant` | Quantization utilities |

### Kernel & Backend
| Crate | Description |
|-------|-------------|
| `adapteros-lora-kernel-api` | Kernel interface types (RouterRing) |
| `adapteros-lora-kernel-mtl` | Metal GPU kernels |
| `adapteros-lora-kernel-coreml` | CoreML ANE acceleration |
| `adapteros-lora-kernel-prof` | Kernel profiling |
| `adapteros-lora-mlx-ffi` | MLX C++ FFI backend (primary) |
| `adapteros-lora-worker` | Inference worker process |
| `adapteros-model-server` | Shared model server |
| `adapteros-base-llm` | Base LLM abstractions |

### Server & API
| Crate | Description |
|-------|-------------|
| `adapteros-server` | Control plane server (Axum 0.8) |
| `adapteros-server-api` | REST handlers, routes, middleware |
| `adapteros-server-api-health` | Health check endpoints |
| `adapteros-server-api-training` | Training endpoint handlers |
| `adapteros-server-api-inference` | Inference and streaming endpoints |
| `adapteros-server-api-audit` | Audit logging endpoints |
| `adapteros-server-api-admin` | Admin and policy endpoints |
| `adapteros-server-api-models` | Model management endpoints |
| `adapteros-server-api-types` | Shared types for server-api crates |
| `adapteros-api` | Public API client library |
| `adapteros-api-types` | Shared API types (server + WASM) |

### Storage & Data
| Crate | Description |
|-------|-------------|
| `adapteros-db` | SQLite WAL + ReDB dual-write |
| `adapteros-storage` | Storage management, versioned records |
| `adapteros-registry` | Adapter registry |
| `adapteros-embeddings` | Deterministic embedding generation |
| `adapteros-retrieval` | Deterministic retrieval with receipts |

### Policy & Security
| Crate | Description |
|-------|-------------|
| `adapteros-policy` | 30 canonical policy packs |
| `adapteros-verify` | Verification, device fingerprinting |
| `adapteros-lint` | Code analysis and linting |
| `adapteros-auth` | Centralized authentication |
| `adapteros-secd` | Secure Enclave Daemon |

### Telemetry & Diagnostics
| Crate | Description |
|-------|-------------|
| `adapteros-telemetry` | Event pipeline, Merkle chains |
| `adapteros-telemetry-types` | Shared telemetry metric types |
| `adapteros-diagnostics` | Diagnostic event contract |
| `adapteros-metrics-exporter` | Prometheus metrics export |
| `adapteros-profiler` | Performance profiling |
| `adapteros-system-metrics` | System resource monitoring |

### Determinism & Replay
| Crate | Description |
|-------|-------------|
| `adapteros-deterministic-exec` | Deterministic async executor |
| `adapteros-replay` | Replay engine |
| `adapteros-trace` | Trace capture |

### Adapter Formats
| Crate | Description |
|-------|-------------|
| `adapteros-aos` | .aos binary/sealed format |
| `adapteros-single-file-adapter` | SingleFileAdapter JSON format |
| `adapteros-manifest` | Adapter manifest handling |
| `adapteros-artifacts` | Build artifact management |

### Lifecycle & Boot
| Crate | Description |
|-------|-------------|
| `adapteros-boot` | Boot phase management, attestation |
| `adapteros-config` | Deterministic configuration |
| `adapteros-error-recovery` | Error recovery strategies |
| `adapteros-error-registry` | Unified error code registry |
| `adapteros-service-supervisor` | Service supervisor integration |

### Networking & Federation
| Crate | Description |
|-------|-------------|
| `adapteros-client` | UDS client for worker communication |
| `adapteros-node` | Node Agent (worker lifecycle) |
| `adapteros-federation` | Multi-node federation |
| `adapteros-infra-common` | Shared infrastructure utilities |

### Frontend
| Crate | Description |
|-------|-------------|
| `adapteros-ui` | Leptos 0.7 WASM frontend |

### Tools & Testing
| Crate | Description |
|-------|-------------|
| `adapteros-cli` | Command-line tool (aosctl) |
| `adapteros-tui` | Terminal UI dashboard |
| `adapteros-chat` | Interactive chat client |
| `adapteros-scenarios` | Test scenario definitions |
| `adapteros-testing` | Test utilities and fixtures |
| `adapteros-e2e` | End-to-end test suite |
| `adapteros-domain` | Domain model definitions |
| `adapteros-git` | Git integration |
| `adapteros-web-browse` | Web browsing for live data |
| `adapteros-ingest-docs` | Document ingestion |
| `adapteros-model-hub` | Model discovery and download |
| `adapteros-codegraph` | Code graph analysis |
| `adapteros-orchestrator` | Task orchestration |
| `adapteros-agent-spawn` | Multi-agent spawn system |
| `adapteros-memory` | Memory management |
| `adapteros-plugin-advanced-metrics` | Advanced metrics plugin |
| `sign-migrations` | Migration signing tool |
| `xtask` | Build automation tasks |
| `fuzz` | Fuzz testing targets |
| `benchmark` (tests/) | Performance benchmark suite |

## Appendix C: Error Redaction Patterns

The 14 ordered regex patterns in `redact_sensitive()`:

| Order | Category | Pattern (simplified) | Replacement |
|-------|----------|---------------------|-------------|
| 1 | Bearer tokens | `Bearer\s+\S+` | `Bearer [REDACTED]` |
| 2 | JWT tokens | `eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+` | `[JWT_REDACTED]` |
| 3 | API keys | `(api[_-]?key|x-api-key)[=:]\s*\S+` | `\1=[REDACTED]` |
| 4 | Passwords | `(password|passwd|pwd)[=:]\s*\S+` | `\1=[REDACTED]` |
| 5 | Private keys | `-----BEGIN\s+\w+\s+PRIVATE\s+KEY-----[\s\S]*?-----END` | `[PRIVATE_KEY_REDACTED]` |
| 6 | AWS credentials | `AKIA[0-9A-Z]{16}` | `[AWS_KEY_REDACTED]` |
| 7 | Generic secrets | `(secret|token|credential)[_-]?\w*[=:]\s*\S+` | `\1=[REDACTED]` |
| 8 | SSN | `\d{3}-\d{2}-\d{4}` | `[SSN_REDACTED]` |
| 9 | Credit cards | `\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}` | `[CC_REDACTED]` |
| 10 | DB connection strings | `(postgres|mysql|sqlite|mongodb)://\S+` | `\1://[REDACTED]` |
| 11 | Socket paths | `/var/run/\S+\.sock` | `[SOCKET_REDACTED]` |
| 12 | Temp paths | `/tmp/\S+` | `[TEMP_PATH_REDACTED]` |
| 13 | Source paths | `/\S+\.(rs|py|js|ts)` | `[SOURCE_PATH_REDACTED]` |
| 14 | Hex secrets | `[0-9a-f]{32,}` | `[HEX_REDACTED]` |

**Ordering matters**: Patterns are applied sequentially. Early patterns (bearer tokens, JWTs)
take priority over later generic patterns (hex secrets) to ensure the most specific redaction
applies.

**Disable switch**: Set `ADAPTEROS_DISABLE_ERROR_REDACTION=1` to disable all redaction. **Never
use in production.**

## Appendix D: Configuration Reference

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `AOS_VAR_DIR` | Runtime data directory | `./var` |
| `AOS_MODEL_PATH` | Base model directory | `./var/models` |
| `AOS_TOKENIZER_PATH` | Explicit tokenizer.json path | Auto-discovered |
| `AOS_SIGNING_KEY_HEX` | Ed25519 signing key (hex) | Generated |
| `AOS_DEV_NO_AUTH=1` | Disable authentication (dev only) | Disabled |
| `AOS_DEBUG_DETERMINISM=1` | Log seed inputs and router details | Disabled |
| `ADAPTEROS_DISABLE_ERROR_REDACTION=1` | Disable error redaction | Disabled |
| `SUPERVISOR_API_URL` | Supervisor API endpoint | None |
| `AOS_PANEL_PORT` | Panel port override | None |

### Configuration File (configs/cp.toml)

```toml
[server]
bind = "0.0.0.0"
port = 8080
production_mode = false
# Optional: invoked (best-effort) after successful review submission.
# See: `crates/adapteros-server-api/src/state.rs` and `crates/adapteros-server-api/src/handlers/review.rs`
review_webhook_url = ""
# Optional: SSRF protection for outbound HTTP requests (default: true).
# Set to false only if you need webhook targets on a private network.
ssrf_protection = true

[security]
dev_bypass = false  # Enable auth bypass (debug builds only)

[database]
path = "var/aos-cp.sqlite3"
storage_mode = "sql_only"  # sql_only | dual_write | kv_primary | kv_only

[determinism]
seed_mode = "strict"  # strict | best_effort | non_deterministic
global_seed = "..."   # BLAKE3 global seed (hex)

[cache]
prefix_kv_max_entries = 16
prefix_kv_max_total_size = "4GB"
prefix_kv_ttl = "1h"
adapter_cache_max = 16
adapter_cache_max_size = "4GB"

[telemetry]
log_profile = "json"  # json | plain | debug | trace
channel_capacity = 50000
bundle_dir = "var/telemetry"

[worker]
heartbeat_interval = "5s"
retry_base_delay = "1s"
retry_max_delay = "5m"
retry_deadline = "10m"
circuit_breaker_threshold = 10
```

### Feature Flags

```toml
# Root Cargo.toml [features]
default = ["multi-backend", "coreml-backend"]

# Backend selection
multi-backend = []          # MLX primary backend (C++ FFI)
coreml-backend = []         # CoreML ANE acceleration layer
production-macos = []       # Full Apple Silicon stack (MLX + CoreML)

# Testing
extended-tests = []         # Extended test suite
hardware-residency = []     # Hardware residency integration tests
loom = []                   # Concurrency testing with loom
```

**Note**: Metal always compiles on macOS via `target_os` gate (no feature flag needed).

### Rust Toolchain

Defined in `rust-toolchain.toml`: **stable channel**. No nightly features required.

### Build Aliases (.cargo/config.toml)

| Alias | Command | Description |
|-------|---------|-------------|
| `cargo c` | `cargo check --workspace` | Quick type check |
| `cargo tb` | `cargo build --timings` | Timed build with HTML report |
| `cargo tbr` | `cargo build --release --timings` | Timed release build |
| `cargo bv` | `cargo build -v` | Verbose build (shows each crate) |
| `cargo nt` | `cargo nextest run` | Tests with progress bar |
| `cargo ntf` | `cargo nextest run --failure-output=immediate` | Tests with immediate failure output |

---

*End of specification. This document is maintained alongside the codebase and should be updated
when architectural changes are merged. All claims are verified against source code at the
commit noted in the document header unless explicitly marked as Planned.*
