# adapterOS Glossary

**Purpose:** Centralized definitions for technical terms used throughout adapterOS documentation.  
**Last Updated:** 2025-01-15

---

## A

### Adapter
A small AI module specialized for a specific task (like Python coding or Django debugging). Adapters are LoRA (Low-Rank Adaptation) modules that modify base model behavior. They are classified into domains: **core** (baseline), **codebase** (stream-scoped), or **portable/standard** (general-purpose). Adapters are loaded dynamically based on routing decisions, memory-efficient with shared base model, and tiered by importance and activation frequency.

**See also:** [LoRA](#lora), [Router](#router), [Core Adapter](#core-adapter), [Codebase Adapter](#codebase-adapter), [Portable Adapter](#portable-adapter)

---

## C

### Circuit Breaker
Safety mechanism that stops processing when too many errors occur. Prevents cascading failures and protects system stability.

### Codebase Adapter
A stream-scoped adapter representing repository state combined with session context. Codebase adapters require a base adapter ID (core adapter), support auto-versioning, and are exclusively bound to a single inference stream. **One codebase adapter per stream.** They evolve over time and must be versioned when context grows.

**See also:** [Adapter](#adapter), [Core Adapter](#core-adapter), [Portable Adapter](#portable-adapter), [Stream-Bound Adapter](#stream-bound-adapter)

### Core Adapter
A baseline adapter (like `adapteros.aos`) that serves as the delta base for codebase adapters. Core adapters are stable reference points, rarely modified directly, and versioned independently of their dependents.

**See also:** [Adapter](#adapter), [Codebase Adapter](#codebase-adapter)

### Control Plane
The management system that coordinates everything in adapterOS. Includes the API server, job orchestration, telemetry indexing, and policy enforcement.

**See also:** [ARCHITECTURE.md](ARCHITECTURE.md) for control plane details

### Control Point (CP)
Versioned configuration snapshot for promotion and rollback. Contains deterministic execution settings, gate-checked promotion rules, and audit trail with telemetry.

---

## D

### Deterministic
Always produces the same output for the same input. In adapterOS, this is achieved through:
- Fixed random seeds (HKDF-derived)
- Quantized gates (Q15)
- Deterministic tie-breaking in retrieval
- Embedded `.metallib` kernels (no runtime compilation)
- Canonical JSON serialization (JCS)
- Configuration freeze with BLAKE3 hashing

**See also:** [Determinism Policy](POLICIES.md#2-determinism)

### DIR (Deterministic Inference Runtime)
The core execution engine that ensures reproducible, auditable inference with token-level determinism. DIR transforms ephemeral inference outputs into persistent, deterministic artifacts that can be reused across sessions.

**See also:** [TAS](#tas-token-artifact-system)

---

## E

### Eviction
Removing something from memory to free up space. adapterOS uses intelligent eviction policies to maintain ≥15% memory headroom, evicting adapters in order: ephemeral_ttl → cold_lru → warm_lru.

**See also:** [Memory Management](architecture.md#memory-management)

---

## F

### Frozen Adapter
An adapter whose state has been locked for deterministic CoreML export. Freezing captures weights, manifest, and configuration at a point in time. Frozen is a **property**, not a lifecycle state—an Active adapter can be frozen without changing its business state. Codebase adapters must be frozen before CoreML export.

**See also:** [Frozen CoreML Package](#frozen-coreml-package), [Codebase Adapter](#codebase-adapter)

### Frozen CoreML Package
The output of exporting a frozen adapter to CoreML format. Contains fused weights and `adapteros_coreml_fusion.json` metadata with BLAKE3 hashes for integrity verification of base manifest, fused manifest, and adapter payload.

**See also:** [Frozen Adapter](#frozen-adapter), [Deterministic](#deterministic)

---

## G

### Gateway
Entry point that checks security before allowing access. Validates authentication, authorization, and policy compliance before forwarding requests.

---

## H

### Hot-Swap Adapter
An adapter that supports live replacement without service interruption. Uses atomic pointer updates (CAS) and memory pooling so that in-flight requests complete with the old version while new requests use the new version. CoreML uses fused packages and does not support per-token hot-swap; use MLX/Metal for live adapters.

**See also:** [Adapter](#adapter), [Frozen Adapter](#frozen-adapter)

---

## I

### Inference
Running the AI model to generate an answer. adapterOS performs inference using K-sparse LoRA routing with Metal-optimized kernels.

---

## K

### K-sparse
Using only K adapters at a time (K=3 by default). Instead of using all available adapters, the router:
1. Scores each adapter based on the question
2. Picks the top K (typically 3)
3. Uses only those K adapters

**Why?** Speed + accuracy. Using too many adapters slows things down and can confuse the AI.

**See also:** [Router](#router), [Router Policy](POLICIES.md#3-router)

---

## L

### LoRA
**Low-Rank Adaptation** - A way to add specialized skills to AI models. LoRA adapters are small, efficient modules that modify base model behavior without requiring full model retraining.

**See also:** [Adapter](#adapter)

---

## M

### Metal
Apple's GPU programming framework for Mac. adapterOS uses Metal for high-performance GPU computation on Apple Silicon.

**See also:** [METAL_BACKEND.md](METAL_BACKEND.md) for Metal kernel details

### Deterministic Inference Runtime (DIR)
**DIR** - The core inference engine that enables deterministic execution and token artifact reusability. DIR refers to the routing, adapter management, and execution system that treats inference outputs as persistent, reusable artifacts.

**Note:** Use "adapterOS" for the complete system. Use "DIR" when referring to the inference runtime specifically.

**See also:** [Token Artifact System](#token-artifact-system-tas), [Naming Conventions](README.md#naming-conventions)

### Token Artifact System (TAS)
**TAS** - The artifact management layer that enables inference outputs to be stored, referenced, and reused as deterministic tokens. TAS transforms ephemeral inference results into persistent, composable artifacts.

**Note:** TAS is enabled by the Deterministic Inference Runtime and represents the core innovation of treating tokens as reusable building blocks.

**See also:** [Deterministic Inference Runtime](#deterministic-inference-runtime-dir)

---

## P

### Policy Pack
A set of rules the system must follow. adapterOS enforces 25 canonical policy packs covering:
- Security (Egress, Isolation, Secrets)
- Quality (Determinism, Performance, Memory)
- Compliance (Evidence, Refusal, Compliance)
- Operations (Telemetry, Retention, Incident)

**See also:** [Policy Packs](POLICIES.md)

### Portable Adapter
The default adapter type (also called "standard"). Portable adapters are `.aos` files that can be freely shared, loaded across streams/sessions, and registered from external sources without stream binding constraints.

**See also:** [Adapter](#adapter), [Codebase Adapter](#codebase-adapter), [Core Adapter](#core-adapter)

---

## Q

### Q15
A number format using 16 bits (0 to 32767). adapterOS uses Q15 quantization for router gates to improve efficiency:
- **Before**: Gate values stored as 32-bit floats (0.0 to 1.0)
- **After**: Gate values stored as 16-bit integers (0 to 32767)

**Why?** Faster computation, less memory, no accuracy loss for this use case.

**See also:** [Router Policy](POLICIES.md#3-router)

---

## R

### Router
Decides which adapters to use for each question. The router:
- Analyzes the question (language, framework, symbols, path, action)
- Scores each adapter based on relevance
- Selects top K adapters (K=3 by default)
- Uses Q15 quantized gates for efficiency
- Enforces entropy floor to prevent single-adapter collapse

**See also:** [K-sparse](#k-sparse), [Router Policy](POLICIES.md#3-router)

---

## S

### Stream-Bound Adapter
An adapter tied to a specific inference stream, unable to serve requests in other streams simultaneously. Currently only codebase adapters support stream binding. Enforced via database unique constraint on `(stream_session_id, adapter_type='codebase', active=1)`.

**See also:** [Codebase Adapter](#codebase-adapter), [Inference](#inference)

---

## T

### Telemetry
Recording what the system does for monitoring and debugging. adapterOS uses canonical JSON format with BLAKE3 hashing for integrity, bundle rotation, and signing.

**See also:** [Telemetry Policy](POLICIES.md#9-telemetry)

### Tokenization
Breaking text into pieces the AI can understand. The tokenizer converts human-readable text into tokens that the model processes.

### TAS (Token Artifact System)
Transforms inference outputs into persistent, reusable artifacts that can be referenced, verified, and composed into larger workflows. Unlike traditional ML systems where outputs are ephemeral, TAS treats tokens as immutable artifacts with audit trails and composition capabilities.

**See also:** [DIR](#dir-deterministic-inference-runtime)

---

## U

### UDS
**Unix Domain Socket** - A way for programs to communicate securely on the same machine. adapterOS uses UDS for worker communication instead of TCP for better security and isolation.

**See also:** [Isolation Policy](POLICIES.md#8-isolation)

---

## W

### Worker
A process that does the actual AI inference work. Workers run in isolated processes with unique UID/GID per tenant, communicating via UDS sockets.

**See also:** [ARCHITECTURE.md](ARCHITECTURE.md) for control plane details

---

## Acronyms

| Acronym | Full Form | Context |
|---------|-----------|---------|
| **API** | Application Programming Interface | How programs talk to each other |
| **CAS** | Content-Addressed Storage | Artifact storage system |
| **CP** | Control Point | Versioned configuration snapshot |
| **CPID** | Control Point ID | Unique identifier for a control point |
| **GPU** | Graphics Processing Unit | Hardware for parallel computation |
| **HKDF** | HMAC-based Key Derivation Function | Cryptographic key derivation |
| **HNSW** | Hierarchical Navigable Small World | Vector search algorithm |
| **JCS** | JSON Canonicalization Scheme | Deterministic JSON serialization |
| **JWT** | JSON Web Token | Authentication token format |
| **LoRA** | Low-Rank Adaptation | Efficient model fine-tuning technique |
| **DIR** | Deterministic Inference Runtime | Core inference engine |
| **PF** | Packet Filter | macOS firewall system |
| **Q15** | 16-bit fixed-point format | Quantization format |
| **RAG** | Retrieval-Augmented Generation | Evidence retrieval system |
| **RBAC** | Role-Based Access Control | Authorization system |
| **SBOM** | Software Bill of Materials | Artifact metadata |
| **UDS** | Unix Domain Socket | Inter-process communication |
| **UID/GID** | User ID / Group ID | Unix user/group identifiers |

---

## Related Documentation

- [Getting Started Guide](getting-started.md) - Plain-language explanations
- [Policy Packs](POLICIES.md) - Complete policy documentation
- [Architecture Overview](ARCHITECTURE.md) - System design and components
- [ARCHITECTURE.md](ARCHITECTURE.md) - Management system documentation

---

**Last Updated:** 2025-01-15  
**Maintained by:** adapterOS Documentation Team
