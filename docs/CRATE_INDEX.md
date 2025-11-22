# AdapterOS Crate Index

**Purpose:** Comprehensive mapping of all workspace crates to architectural layers
**Last Updated:** 2025-11-22
**Total Crates:** 57

---

## Quick Reference Table

| Crate | Layer | Purpose |
|-------|-------|---------|
| `adapteros-cli` | Client | CLI tools (`aosctl` command-line interface) |
| `adapteros-client` | Client | API client library for external integrations |
| `adapteros-chat` | Client | Chat interface client |
| `adapteros-server` | Gateway | HTTP/UDS server with static file serving |
| `adapteros-server-api` | Gateway | REST API handlers for control plane operations |
| `adapteros-api` | Gateway | API types and shared API utilities |
| `adapteros-core` | Runtime (Core) | Foundational types, error handling, hashing, CPID |
| `adapteros-lora-router` | Runtime (Core) | Top-K sparse router with Q15 gate quantization |
| `adapteros-policy` | Runtime (Core) | Policy enforcement engine (22 policy packs) |
| `adapteros-deterministic-exec` | Runtime (Core) | Deterministic async executor with serial task execution |
| `adapteros-base-llm` | Runtime (Inference) | Base LLM integration (Qwen2.5-7B-Instruct) |
| `adapteros-lora-lifecycle` | Runtime (Inference) | LoRA adapter loader and lifecycle management |
| `adapteros-lora-kernel-api` | Runtime (Inference) | Kernel trait definitions for compute backends |
| `adapteros-lora-kernel-mtl` | Runtime (Inference) | Metal-optimized fused kernels (attention + MLP + LoRA) |
| `adapteros-lora-kernel-prof` | Runtime (Inference) | Kernel profiling and performance measurement |
| `adapteros-lora-quant` | Runtime (Inference) | Quantization API for LoRA adapters |
| `adapteros-lora-mlx-ffi` | Runtime (Inference) | MLX backend FFI (experimental, PyO3) |
| `adapteros-lora-rag` | Runtime (Data) | RAG engine with vector search for evidence retrieval |
| `adapteros-memory` | Runtime (Data) | Unified Memory Watchdog - monitors page-outs and OS memory |
| `adapteros-telemetry` | Runtime (Observability) | Telemetry system with bundle store and canonical JSON |
| `adapteros-telemetry-types` | Runtime (Observability) | Shared telemetry metric types |
| `adapteros-trace` | Runtime (Observability) | Trace builder for audit trail and deterministic replay |
| `adapteros-replay` | Runtime (Observability) | Replay system for deterministic execution verification |
| `adapteros-metrics-collector` | Runtime (Observability) | Metrics collection system |
| `adapteros-metrics-exporter` | Runtime (Observability) | Metrics export (Prometheus, etc.) |
| `adapteros-profiler` | Runtime (Observability) | Performance profiling tools |
| `adapteros-system-metrics` | Runtime (Observability) | System resource monitoring and metrics collection |
| `adapteros-db` | Storage | Database layer (SQLite/PostgreSQL) with migrations |
| `adapteros-artifacts` | Storage | Content-addressed artifact store with BLAKE3 hashing |
| `adapteros-storage` | Storage | Storage management and disk space enforcement |
| `adapteros-registry` | Storage | Adapter registry management and metadata |
| `adapteros-orchestrator` | Control Plane | Orchestration service for code jobs and training |
| `adapteros-lora-plan` | Control Plane | Plan manager for CPID lifecycle management |
| `adapteros-cdp` | Control Plane | Commit Delta Pack generation and processing |
| `adapteros-crypto` | Shared | Cryptographic operations (Ed25519, BLAKE3, envelope encryption) |
| `adapteros-config` | Shared | Deterministic configuration system with precedence enforcement |
| `adapteros-manifest` | Shared | Plan manifest parsing and validation |
| `adapteros-platform` | Shared | Cross-platform filesystem operations |
| `adapteros-git` | Shared | Git repository integration and analysis |
| `adapteros-codegraph` | Shared | CodeGraph building, tree-sitter parsing, symbol extraction |
| `adapteros-patch` | Shared | Deterministic, policy-compliant code patching with verification |
| `adapteros-verify` | Shared | Golden-run archive system for audit reproducibility |
| `adapteros-verification` | Shared | Verification utilities and validation |
| `adapteros-lint` | Shared | Runtime guards for determinism enforcement |
| `adapteros-testing` | Shared | Testing utilities and test harnesses |
| `adapteros-numerics` | Shared | Numerical stability tracking and quantization noise measurement |
| `adapteros-graph` | Shared | Tensor metadata canonicalization and hash graph |
| `adapteros-secure-fs` | Shared | Secure filesystem operations with cap-std integration |
| `adapteros-concurrent-fs` | Shared | Concurrent filesystem operations with locking |
| `adapteros-temp` | Shared | Temporary file management with guaranteed cleanup |
| `adapteros-error-recovery` | Shared | Error recovery and corruption detection |
| `adapteros-federation` | Shared | Cross-host federation signatures for telemetry bundles |
| `adapteros-sbom` | Shared | SBOM (Software Bill of Materials) generation and validation |
| `adapteros-aos` | Shared | Memory-mapped .aos file loading with LRU caching |
| `adapteros-api-types` | Shared | API type definitions and schemas |
| `adapteros-node` | Shared | Node.js integration utilities |
| `adapteros-autograd` | Shared | Rust autograd system for tensor operations |
| `adapteros-service-supervisor` | Shared | Production-ready service supervisor with process management |
| `adapteros-domain` | Shared | Domain-specific adapter layer (text, vision, telemetry) |

---

## 1. Client Layer

Client-facing tools and interfaces for interacting with AdapterOS.

### `adapteros-cli`
**Purpose:** Command-line interface (`aosctl`) for AdapterOS operations  
**Key Dependencies:** `adapteros-core`, `adapteros-api-types`, `adapteros-client`  
**README:** None

### `adapteros-client`
**Purpose:** API client library for external integrations  
**Key Dependencies:** `adapteros-api-types`, `adapteros-core`  
**README:** None

### `adapteros-chat`
**Purpose:** Chat interface client  
**Key Dependencies:** `adapteros-core`, `adapteros-api-types`  
**README:** None

---

## 2. API Gateway Layer

Transport, authentication, and API routing layer.

### `adapteros-server`
**Purpose:** HTTP/UDS server with static file serving and OpenAPI support  
**Key Dependencies:** `adapteros-server-api`, `adapteros-core`, `adapteros-config`, `adapteros-telemetry`  
**README:** None

### `adapteros-server-api`
**Purpose:** REST API handlers for control plane operations (CAB workflow, adapter management)  
**Key Dependencies:** `adapteros-db`, `adapteros-core`, `adapteros-crypto`, `adapteros-policy`  
**README:** `crates/adapteros-server-api/README.md`

### `adapteros-api`
**Purpose:** API types and shared API utilities  
**Key Dependencies:** `adapteros-core`  
**README:** None

---

## 3. AdapterOS Runtime Layer

### 3.1 Core Services

Core runtime services for routing, policy, and execution.

#### `adapteros-core`
**Purpose:** Foundational types and utilities (error handling, BLAKE3 hashing, CPID, deterministic seed derivation)  
**Key Dependencies:** `blake3`, `thiserror`, `serde`, `hkdf`  
**README:** None  
**Note:** Used by virtually all other crates

#### `adapteros-lora-router`
**Purpose:** Top-K sparse router with Q15 quantized gates for adapter selection  
**Key Dependencies:** `adapteros-core`, `adapteros-telemetry`, `adapteros-memory`  
**README:** None

#### `adapteros-policy`
**Purpose:** Policy enforcement engine implementing 22 policy packs  
**Key Dependencies:** `adapteros-core`, `adapteros-manifest`  
**README:** None

#### `adapteros-deterministic-exec`
**Purpose:** Deterministic async executor with serial task execution and event logging  
**Key Dependencies:** `adapteros-core`, `tokio`  
**README:** None

### 3.2 Inference Engine

Components for LLM inference and adapter execution.

#### `adapteros-base-llm`
**Purpose:** Base LLM integration for AdapterOS Layer 1 (Qwen2.5-7B-Instruct)  
**Key Dependencies:** `adapteros-core`, `tokenizers`, `safetensors`  
**README:** None

#### `adapteros-lora-lifecycle`
**Purpose:** LoRA adapter loader and lifecycle management
**Key Dependencies:** `adapteros-core`, `adapteros-aos`
**README:** None

#### `adapteros-lora-kernel-api`
**Purpose:** Kernel trait definitions for compute backends  
**Key Dependencies:** `adapteros-core`  
**README:** None

#### `adapteros-lora-kernel-mtl`
**Purpose:** Metal-optimized fused kernels (attention + MLP + LoRA) for Apple Silicon  
**Key Dependencies:** `adapteros-lora-kernel-api`, `metal`, `mach`  
**README:** None

#### `adapteros-lora-kernel-prof`
**Purpose:** Kernel profiling and performance measurement  
**Key Dependencies:** `adapteros-lora-kernel-api`  
**README:** None

#### `adapteros-lora-quant`
**Purpose:** Quantization API for LoRA adapters  
**Key Dependencies:** `adapteros-core`  
**README:** None

#### `adapteros-lora-mlx-ffi`
**Purpose:** MLX backend FFI (experimental, requires PyO3)  
**Key Dependencies:** `adapteros-lora-kernel-api`, `pyo3`  
**README:** `crates/adapteros-lora-mlx-ffi/README.md`  
**Note:** Experimental backend, excluded from tests by default

### 3.3 Data Services

Data retrieval, caching, and memory management.

#### `adapteros-lora-rag`
**Purpose:** RAG engine with vector search for evidence retrieval  
**Key Dependencies:** `adapteros-core`, `adapteros-db`, `adapteros-codegraph`  
**README:** `crates/adapteros-lora-rag/README.md`

#### `adapteros-memory`
**Purpose:** Unified Memory Watchdog - monitors page-outs and OS memory tricks  
**Key Dependencies:** `adapteros-core`  
**README:** None

### 3.4 Observability

Telemetry, tracing, metrics, and replay systems.

#### `adapteros-telemetry`
**Purpose:** Telemetry system with bundle store, canonical JSON, and Merkle tree signing  
**Key Dependencies:** `adapteros-core`, `adapteros-crypto`, `adapteros-db`  
**README:** `crates/adapteros-telemetry/README.md`

#### `adapteros-telemetry-types`
**Purpose:** Shared telemetry metric types  
**Key Dependencies:** `adapteros-core`, `serde`  
**README:** None

#### `adapteros-trace`
**Purpose:** Trace builder for audit trail and deterministic replay  
**Key Dependencies:** `adapteros-core`  
**README:** None

#### `adapteros-replay`
**Purpose:** Replay system for deterministic execution verification  
**Key Dependencies:** `adapteros-core`, `adapteros-telemetry`  
**README:** None

#### `adapteros-metrics-collector`
**Purpose:** Metrics collection system  
**Key Dependencies:** `adapteros-core`, `adapteros-telemetry-types`  
**README:** None

#### `adapteros-metrics-exporter`
**Purpose:** Metrics export (Prometheus, etc.)  
**Key Dependencies:** `adapteros-core`, `adapteros-metrics-collector`  
**README:** None

#### `adapteros-profiler`
**Purpose:** Performance profiling tools  
**Key Dependencies:** `adapteros-core`  
**README:** None

#### `adapteros-system-metrics`
**Purpose:** System resource monitoring and metrics collection  
**Key Dependencies:** `adapteros-core`, `adapteros-db`  
**README:** None

---

## 4. Storage Layer

Database, artifact storage, and registry management.

### `adapteros-db`
**Purpose:** Database layer (SQLite/PostgreSQL) with migrations and schema management  
**Key Dependencies:** `adapteros-core`, `sqlx`, `rusqlite`  
**README:** None

### `adapteros-artifacts`
**Purpose:** Content-addressed artifact store with BLAKE3 hashing  
**Key Dependencies:** `adapteros-core`, `adapteros-crypto`  
**README:** None

### `adapteros-storage`
**Purpose:** Storage management and disk space enforcement  
**Key Dependencies:** `adapteros-core`  
**README:** None

### `adapteros-registry`
**Purpose:** Adapter registry management and metadata  
**Key Dependencies:** `adapteros-core`, `adapteros-db`  
**README:** None

---

## 5. Control Plane Layer

Orchestration, planning, and deployment management.

### `adapteros-orchestrator`
**Purpose:** Orchestration service for code jobs, training, and CDP processing  
**Key Dependencies:** `adapteros-core`, `adapteros-db`, `adapteros-lora-worker`, `adapteros-codegraph`  
**README:** None

### `adapteros-lora-plan`
**Purpose:** Plan manager for CPID lifecycle management  
**Key Dependencies:** `adapteros-core`, `adapteros-manifest`  
**README:** None

### `adapteros-cdp`
**Purpose:** Commit Delta Pack generation and processing  
**Key Dependencies:** `adapteros-core`, `adapteros-codegraph`, `adapteros-git`  
**README:** None

---

## 6. Shared Infrastructure

Cross-cutting utilities and shared components used across layers.

### Cryptography & Security
- **`adapteros-crypto`**: Cryptographic operations (Ed25519, BLAKE3, envelope encryption)
- **`adapteros-secure-fs`**: Secure filesystem operations with cap-std integration
- **`adapteros-federation`**: Cross-host federation signatures for telemetry bundles  
  - **README:** `crates/adapteros-federation/README.md`

### Configuration & Manifest
- **`adapteros-config`**: Deterministic configuration system with precedence enforcement
- **`adapteros-manifest`**: Plan manifest parsing and validation
- **`adapteros-sbom`**: SBOM (Software Bill of Materials) generation and validation

### Code Intelligence
- **`adapteros-codegraph`**: CodeGraph building, tree-sitter parsing, symbol extraction
- **`adapteros-git`**: Git repository integration and analysis
- **`adapteros-patch`**: Deterministic, policy-compliant code patching with verification

### Verification & Testing
- **`adapteros-verify`**: Golden-run archive system for audit reproducibility
- **`adapteros-verification`**: Verification utilities and validation
- **`adapteros-lint`**: Runtime guards for determinism enforcement
- **`adapteros-testing`**: Testing utilities and test harnesses

### Filesystem & Platform
- **`adapteros-platform`**: Cross-platform filesystem operations
- **`adapteros-concurrent-fs`**: Concurrent filesystem operations with locking
- **`adapteros-temp`**: Temporary file management with guaranteed cleanup

### Adapter Format & Loading
- **`adapteros-aos`**: Memory-mapped .aos file loading with LRU caching and hot-swap support (primary .aos implementation)
  - **README:** `crates/adapteros-aos/README.md`

### Domain & Numerics
- **`adapteros-domain`**: Domain-specific adapter layer (text, vision, telemetry) with deterministic execution
- **`adapteros-numerics`**: Numerical stability tracking and quantization noise measurement
- **`adapteros-graph`**: Tensor metadata canonicalization and hash graph
- **`adapteros-autograd`**: Rust autograd system for tensor operations

### Error Handling & Recovery
- **`adapteros-error-recovery`**: Error recovery and corruption detection

### Service Management
- **`adapteros-service-supervisor`**: Production-ready service supervisor with process management and security

### API & Integration
- **`adapteros-api-types`**: API type definitions and schemas
- **`adapteros-node`**: Node.js integration utilities

---

## Architecture Reference

For detailed architecture documentation, see:
- **[MasterPlan.md](architecture/MASTERPLAN.md)** - Complete system design and layer definitions
- **[ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md)** - Architecture documentation index
- **[Precision Diagrams](architecture/PRECISION-DIAGRAMS.md)** - Code-verified architecture diagrams

---

## Usage

### Finding a Crate

1. **By Layer**: Use the layer sections above to find crates by architectural responsibility
2. **By Name**: Use the Quick Reference Table at the top
3. **By Purpose**: Search this document for keywords

### Understanding Dependencies

Each crate lists its key dependencies. For complete dependency information, see:
```bash
cargo tree -p <crate-name>
```

### Contributing

When adding a new crate:
1. Add it to the appropriate layer section above
2. Update the Quick Reference Table
3. Add workspace metadata to `Cargo.toml` (see `[workspace.metadata.categories]`)
4. Update this index

When modifying an existing crate:
- If the crate's purpose or architectural layer changes, update this index
- If dependencies change significantly, update the "Key Dependencies" section
- If a README is added/removed, update the README links

**Maintenance Reminder:** This index should be kept in sync with the actual crate structure. See `Cargo.toml` `[workspace.metadata]` section for maintenance notes.

---

**Last Updated:** 2025-01-15  
**Maintained By:** AdapterOS Team


