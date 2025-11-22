# AdapterOS Glossary

**Purpose:** Centralized definitions for technical terms used throughout AdapterOS documentation.  
**Last Updated:** 2025-01-15

---

## A

### Adapter
A small AI module specialized for a specific task (like Python coding or Django debugging). Adapters are LoRA (Low-Rank Adaptation) modules that modify base model behavior. They are loaded dynamically based on routing decisions, memory-efficient with shared base model, and tiered by importance and activation frequency.

**See also:** [LoRA](#lora), [Router](#router)

---

## C

### Circuit Breaker
Safety mechanism that stops processing when too many errors occur. Prevents cascading failures and protects system stability.

### Control Plane
The management system that coordinates everything in AdapterOS. Includes the API server, job orchestration, telemetry indexing, and policy enforcement.

**See also:** [Control Plane Documentation](CONTROL-PLANE.md)

### Control Point (CP)
Versioned configuration snapshot for promotion and rollback. Contains deterministic execution settings, gate-checked promotion rules, and audit trail with telemetry.

---

## D

### Deterministic
Always produces the same output for the same input. In AdapterOS, this is achieved through:
- Fixed random seeds (HKDF-derived)
- Quantized gates (Q15)
- Deterministic tie-breaking in retrieval
- Embedded `.metallib` kernels (no runtime compilation)
- Canonical JSON serialization (JCS)
- Configuration freeze with BLAKE3 hashing

**See also:** [Determinism Policy](POLICIES.md#2-determinism)

---

## E

### Eviction
Removing something from memory to free up space. AdapterOS uses intelligent eviction policies to maintain ≥15% memory headroom, evicting adapters in order: ephemeral_ttl → cold_lru → warm_lru.

**See also:** [Memory Management](architecture.md#memory-management)

---

## G

### Gateway
Entry point that checks security before allowing access. Validates authentication, authorization, and policy compliance before forwarding requests.

---

## I

### Inference
Running the AI model to generate an answer. AdapterOS performs inference using K-sparse LoRA routing with Metal-optimized kernels.

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
Apple's GPU programming framework for Mac. AdapterOS uses Metal for high-performance GPU computation on Apple Silicon.

**See also:** [Metal Kernels](metal/PHASE4-METAL-KERNELS.md)

### MPLoRA
**Multi-Path LoRA** - The technical/internal name for the AdapterOS inference engine. MPLoRA refers to the core routing and adapter management system. "AdapterOS" is the user-facing name for the complete system.

**Note:** In user-facing documentation, use "AdapterOS". Use "MPLoRA" only when referring to the specific routing algorithm or internal implementation.

**See also:** [Naming Conventions](README.md#naming-conventions)

---

## P

### Policy Pack
A set of rules the system must follow. AdapterOS enforces 22 canonical policy packs covering:
- Security (Egress, Isolation, Secrets)
- Quality (Determinism, Performance, Memory)
- Compliance (Evidence, Refusal, Compliance)
- Operations (Telemetry, Retention, Incident)

**See also:** [Policy Packs](POLICIES.md)

---

## Q

### Q15
A number format using 16 bits (0 to 32767). AdapterOS uses Q15 quantization for router gates to improve efficiency:
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

## T

### Telemetry
Recording what the system does for monitoring and debugging. AdapterOS uses canonical JSON format with BLAKE3 hashing for integrity, bundle rotation, and signing.

**See also:** [Telemetry Policy](POLICIES.md#9-telemetry)

### Tokenization
Breaking text into pieces the AI can understand. The tokenizer converts human-readable text into tokens that the model processes.

---

## U

### UDS
**Unix Domain Socket** - A way for programs to communicate securely on the same machine. AdapterOS uses UDS for worker communication instead of TCP for better security and isolation.

**See also:** [Isolation Policy](POLICIES.md#8-isolation)

---

## W

### Worker
A process that does the actual AI inference work. Workers run in isolated processes with unique UID/GID per tenant, communicating via UDS sockets.

**See also:** [Control Plane](CONTROL-PLANE.md)

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
| **MPLoRA** | Multi-Path LoRA | Core routing algorithm name |
| **PF** | Packet Filter | macOS firewall system |
| **Q15** | 16-bit fixed-point format | Quantization format |
| **RAG** | Retrieval-Augmented Generation | Evidence retrieval system |
| **RBAC** | Role-Based Access Control | Authorization system |
| **SBOM** | Software Bill of Materials | Artifact metadata |
| **UDS** | Unix Domain Socket | Inter-process communication |
| **UID/GID** | User ID / Group ID | Unix user/group identifiers |

---

## Related Documentation

- [Getting Started Guide](GETTING_STARTED_WITH_DIAGRAMS.md) - Plain-language explanations
- [Policy Packs](POLICIES.md) - Complete policy documentation
- [Architecture Overview](ARCHITECTURE.md) - System design and components
- [Control Plane](CONTROL-PLANE.md) - Management system documentation

---

**Last Updated:** 2025-01-15  
**Maintained by:** AdapterOS Documentation Team

