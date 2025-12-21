# AdapterOS: Deterministic ML Inference Platform

[![Rust](https://img.shields.io/badge/rust-nightly-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-Apache%202.0%2FMIT-blue.svg)](LICENSE)
[![CI](https://github.com/rogu3bear/adapter-os/workflows/CI/badge.svg)](https://github.com/rogu3bear/adapter-os/actions)
[![Security](https://github.com/rogu3bear/adapter-os/workflows/Security%20Regression%20Tests/badge.svg)](https://github.com/rogu3bear/adapter-os/actions)
[![Stars](https://img.shields.io/github/stars/rogu3bear/adapter-os.svg)](https://github.com/rogu3bear/adapter-os/stargazers)
[![Forks](https://img.shields.io/github/forks/rogu3bear/adapter-os.svg)](https://github.com/rogu3bear/adapter-os/network/members)

**Deterministic ML Inference Platform** — High-performance inference with K-sparse LoRA routing, Metal-optimized kernels, and comprehensive policy enforcement for production environments.

AdapterOS (alpha-v0.11-unstable-pre-release) is a Rust-based ML inference platform optimized for Apple Silicon. Features deterministic execution across multiple backends (CoreML, MLX, Metal), modular Metal kernels, centralized policy enforcement, and memory-efficient adapter management with zero network egress during serving.

---

## [TARGET] What is AdapterOS?

AdapterOS is an ML inference platform that enables **deterministic multi-adapter inference** on Apple Silicon by:

### Core Technologies

- **Deterministic Inference Runtime (DIR)**: The core execution engine that ensures reproducible, auditable inference with token-level determinism
- **Token Artifact System (TAS)**: Transforms inference outputs into persistent, reusable artifacts that can be referenced and composed
- **K-Sparse LoRA Routing**: Dynamic gating with Q15 quantized gates and entropy floor
- **Modular Metal Kernels**: Precompiled `.metallib` kernels with deterministic compilation
- **Policy Enforcement**: 25 canonical policy packs for compliance, security, and quality

- **K-Sparse LoRA Routing**: Dynamic gating with Q15 quantized gates and entropy floor
- **Modular Metal Kernels**: Precompiled `.metallib` kernels with deterministic compilation
- **Policy Enforcement**: 25 canonical policy packs for compliance, security, and quality
- **Environment Fingerprinting**: Cryptographically signed drift detection with automatic baseline creation
- **Deterministic Execution**: Reproducible outputs with HKDF seeding and canonical JSON
- **Zero Network Egress**: Air-gapped serving with Unix domain sockets only
- **Memory Management**: Intelligent adapter eviction with ≥15% headroom maintenance

---

## Table of Contents

- [🚀 Quick Start](#-quick-start)
- [🏗️ Architecture](#️-architecture)
- [📦 Components](#-components)
- [🎛️ Key Features](#️-key-features)
- [📊 Current Status](#-current-status)
- [🖼️ Visual Diagrams](#️-visual-diagrams)
- [🧪 Development](#-development)
- [📈 Performance](#-performance)
- [⚙️ Configuration](#️-configuration)
- [📚 Documentation](#-documentation)
- [🤝 Contributing](#-contributing)
- [📄 License](#-license)
- [🙏 Acknowledgments](#-acknowledgments)

## Architecture

<details>
<summary>AdapterOS Architecture</summary>

```mermaid
graph TB
    subgraph Runtime[AdapterOS Runtime v0.11.0]
        subgraph Control[Control Layer]
            Policy[Policy Registry<br/>25 Canonical Packs]
            Router[K-Sparse Router<br/>Q15 Quantized Gates]
            Kernels[Modular Metal Kernels<br/>.metallib]
        end

        subgraph Registry[LoRA Adapter Registry]
            A1[Adapter 1]
            A2[Adapter 2]
            A3[...]
            AN[Adapter N]
        end

        subgraph Model[Base Model Layer]
            BaseModel[Base Model<br/>Qwen, Llama, etc.]
        end

        Policy --> Router
        Router --> Kernels

        Policy --> Registry
        Router --> Registry
        Kernels --> Registry

        Registry --> BaseModel
    end

    subgraph Hardware[Apple Silicon]
        GPU[Apple Metal GPU<br/>Unified Memory Architecture]
        Network[Zero Network Egress<br/>Unix Domain Sockets Only]
    end

    BaseModel --> GPU
    Runtime -.->|Air-gapped| Network

    style Policy fill:#e8a87c,stroke:#333,stroke-width:2px
    style Router fill:#e27d60,stroke:#333,stroke-width:2px
    style Kernels fill:#c38d9e,stroke:#333,stroke-width:2px
    style Registry fill:#e1f5ff,stroke:#333,stroke-width:2px
    style BaseModel fill:#4a90e2,stroke:#333,stroke-width:3px
    style GPU fill:#27ae60,stroke:#333,stroke-width:2px
    style Network fill:#ffe1e1,stroke:#333,stroke-width:2px
```

**Key Components:**
- **Policy Registry**: 25 canonical policy packs (egress, determinism, router, evidence, etc.)
- **K-Sparse Router**: Top-K adapter selection with Q15 quantized gates
- **Modular Kernels**: Precompiled `.metallib` kernels for deterministic execution
- **Adapter Registry**: Content-addressed LoRA adapter storage
- **Zero Network**: Air-gapped serving via Unix domain sockets only

</details>

---

## Quick Start

### 🚀 **Getting Started**

For complete setup instructions, see **[QUICKSTART.md](QUICKSTART.md)** which provides:

- **Hardware Requirements**: Apple Silicon (M1/M2/M3/M4), macOS 13.0+
- **Installation Options**: Graphical installer or manual setup
- **Model Setup**: Download and configure MLX models
- **First Inference**: Run your first adapter inference
- **GPU Training**: Set up LoRA fine-tuning pipeline

### 📦 **Quick Manual Installation**

```bash
# Clone and build
git clone https://github.com/rogu3bear/adapter-os.git
cd adapter-os
make cli  # Build CLI and create ./aosctl symlink

# Initialize database
./aosctl db migrate

# Download a model (optional)
huggingface-cli download mlx-community/Qwen2.5-7B-Instruct \
    --include "*.safetensors" "*.json" \
    --local-dir models/qwen2.5-7b-mlx

# Start the server
make dev
```

### Prerequisites

- **macOS 13.0+** with Apple Silicon (M1/M2/M3/M4)
- **Rust (nightly toolchain)**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` (see `rust-toolchain.toml` for exact channel)
- **pnpm 8+**: Required for UI development (`npm install -g pnpm`)
- **MLX**: `pip install mlx` (Optional - for MLX backend development only)

### Build

```bash
# Clone the repository
git clone https://github.com/rogu3bear/adapter-os.git
cd adapter-os

# Build with fresh cleanup (recommended - stops services, cleans ports)
make build

# Or just prepare environment without building
make prepare

# Manual build without cleanup (not recommended)
cargo build --release
```

**Fresh Build System**: The `fresh-build` target ensures clean rebuilds by:
- Stopping any running AdapterOS services
- Freeing occupied ports (8080, 3200)
- Cleaning orphaned processes
- Removing stale build artifacts

Use `make fresh-build` before rebuilding to prevent port conflicts and build errors.

**Note**: CoreML backend is the primary production backend (ANE acceleration), MLX backend is secondary for production inference and training workloads. Metal backend is an incomplete fallback (model loading issues) for legacy scenarios only.

### Database Initialization

```bash
# Run database migrations
cargo run -p adapteros-orchestrator -- db migrate

# Initialize the default tenant
cargo run -p adapteros-orchestrator -- init-tenant --id default --uid 1000 --gid 1000
```

### Migration Hygiene (dev)

- `bash scripts/check_migrations.sh` to catch duplicate numbers, gaps, and filename collisions before opening a PR.
- `python -m pip install --quiet blake3 && python scripts/verify_migration_signatures.py` to ensure `migrations/signatures.json` matches on-disk migrations.
- Regenerate signatures after editing migrations: `./scripts/sign_migrations.sh`.

### Import a Model

> **Note**: The `import-model` command requires the MLX backend which is currently
> disabled in the default build. To set up a model for inference, download an
> MLX-format model and set the `AOS_MLX_FFI_MODEL` environment variable:

```bash
# Download Qwen 2.5 7B MLX format (~3.8GB)
huggingface-cli download mlx-community/Qwen2.5-7B-Instruct \
    --include "*.safetensors" "*.json" \
    --local-dir var/models/qwen2.5-7b-mlx

# Set the model path for the server
export AOS_MLX_FFI_MODEL=var/models/qwen2.5-7b-mlx

# See QUICKSTART.md for complete setup instructions
```

### Runtime var layout (canonical)

- `var/adapters/repo/<tenant_id>/<adapter_name>/<version>.aos`: canonical, write-once bundles (default `AOS_ADAPTERS_DIR`).
- `var/adapters/cache/<hash-prefix>/<manifest_hash>.aos`: worker/system cache; evictable except pinned hashes.
- `var/manifests/<tenant_id>/<adapter_name>-<version>.json`: resolved manifest snapshots.
- `var/models/<model-name>/...`: MLX model roots (point `AOS_MLX_FFI_MODEL` here).
### Register LoRA Adapters

```bash
# Register your LoRA adapters with semantic names
# Format: {tenant}/{domain}/{purpose}/{revision}
./aosctl register-adapter tenant-a/engineering/code-review/r001 b3:abc123... --tier persistent --rank 16

# See docs/ADAPTER_TAXONOMY.md for naming conventions
```

### Start Serving

```bash
# Start dev server (port 8080)
make dev

# Or use aosctl to serve with a specific tenant and plan
./aosctl serve --tenant default --plan <plan-id>

# See QUICKSTART.md for complete setup and configuration
```
---

## 📦 Components

### Core Crates

| Crate | Purpose |
|-------|---------|
| `adapteros-lora-worker` | Inference engine with policy enforcement |
| `adapteros-lora-router` | K-sparse LoRA routing with Q15 quantized gates |
| `adapteros-lora-kernel-mtl` | Modular Metal kernels with deterministic compilation |
| `adapteros-lora-plan` | Plan builder and loader |
| `adapteros-chat` | Chat template processor (ChatML, etc.) |
| `adapteros-lora-rag` | Evidence retrieval with HNSW vector search |

### Management

| Crate | Purpose |
|-------|---------|
| `adapteros-server` | Control plane API server |
| `adapteros-server-api` | REST API handlers |
| `adapteros-cli` | Command-line tool (`aosctl`) |
| `adapteros-db` | SQLite database layer with migrations |

### Infrastructure

| Crate | Purpose |
|-------|---------|
| `adapteros-policy` | 28-pack policy registry with enforcement |
| `adapteros-telemetry` | Canonical JSON event logging with Merkle trees |
| `adapteros-crypto` | Ed25519 signing, BLAKE3 hashing, HKDF |
| `adapteros-artifacts` | Content-addressed artifact store with SBOM |
| `adapteros-config` | Deterministic configuration with precedence rules |

---

## 🎛️ Key Features

### 1. **K-Sparse LoRA Routing**

AdapterOS uses learned gates to select the top-K most relevant LoRA adapters per token:

```rust
// Router selects K=3 adapters with highest gate values
let selected = router.route(hidden_states, k=3);
// Gates are quantized to Q15 for efficiency
// Entropy floor prevents single-adapter collapse
// Deterministic tie-breaking: (score desc, doc_id asc)
```

### 2. **Modular Metal Kernels**

Precompiled kernels with deterministic compilation:

```metal
// Modular kernel with parameter structs
kernel void fused_attention_lora(
    constant AttentionParams& params,
    device float* Q,
    device float* K,
    device float* V,
    device float* lora_A,
    device float* lora_B
) {
    // Deterministic execution with fixed rounding
    // Precompiled to .metallib for reproducibility
}
```

### 3. **Policy Enforcement**

25 canonical policy packs ensure compliance:
- **Egress Ruleset**: Zero network during serving, PF enforcement
- **Determinism Ruleset**: Precompiled kernels, HKDF seeding
- **Router Ruleset**: K bounds, entropy floor, Q15 gates
- **Evidence Ruleset**: Mandatory open-book grounding
- **Refusal Ruleset**: Abstain on low confidence
- **Naming Ruleset**: Semantic adapter naming with lineage tracking
- **And 19 more** for security, compliance, and quality

### 4. **Deterministic Execution**

Reproducible inference with:
- Fixed random seeds (HKDF-derived)
- Quantized gates (Q15)
- Deterministic tie-breaking in retrieval
- Embedded `.metallib` kernels (no runtime compilation)
- Canonical JSON serialization (JCS)
- Configuration freeze with BLAKE3 hashing

### 5. **Adapter Lifecycle Management**

Adapters transition through lifecycle states for efficient memory management:

```
Unloaded -> Cold -> Warm -> Hot -> Resident
    ^                              |
    +------ (eviction) -----------+
```

- **Promotion**: Adapters move to higher states based on activation frequency
- **Demotion/Eviction**: Inactive adapters are demoted or evicted under memory pressure
- **Pinning**: Critical adapters can be pinned to prevent eviction

See [docs/LIFECYCLE.md](docs/LIFECYCLE.md) for detailed state machine documentation.

## 📊 Current Status (alpha-v0.11-unstable-pre-release)

### ✅ **Implemented Features**
- **Multi-Backend Support**: CoreML (ANE, primary), MLX (GPU, secondary), Metal (incomplete fallback) backends
- **K-Sparse LoRA Routing**: Dynamic adapter selection with Q15 quantization
- **Deterministic Execution**: HKDF seeding, reproducible results
- **Policy Enforcement**: 25 canonical policy packs with runtime validation
- **Adapter Lifecycle**: Hot-swap, pinning, TTL management, memory optimization
- **REST API**: Complete inference endpoints with streaming support
- **Database**: SQLite with migrations, adapter registry, telemetry
- **CLI Tools**: Comprehensive `aosctl` command suite
- **Security**: Ed25519 signing, audit trails, secure FFI boundaries

### 🔄 **In Development**
- **Training Pipeline**: Dataset management and LoRA fine-tuning
- **Federation**: Cross-node adapter synchronization
- **Advanced UI**: Web dashboard with real-time monitoring
- **Production Deployment**: Kubernetes operators, service mesh integration

**📋 Completion Roadmap**: See project board for the comprehensive plan to complete all features.

### 🎯 **Architecture Highlights**
- **Zero Network Egress**: Air-gapped inference with Unix domain sockets
- **Memory Management**: Intelligent eviction with ≥15% headroom maintenance
- **Content Addressing**: BLAKE3 hashing for all artifacts and configurations
- **Hot-Swap**: Live adapter replacement without service interruption

## 🖼️ Visual Diagrams

AdapterOS includes comprehensive visual documentation. Here are key diagrams:

### **System Architecture**
- **[System Architecture](docs/ARCHITECTURE.md)**: Architecture documentation
- **[Precision Diagrams](docs/architecture/precision-diagrams.md)**: Code-verified architecture diagrams
- **[Multi-Backend Strategy](docs/ADR_MULTI_BACKEND_STRATEGY.md)**: Backend selection rationale

### **Data Flow & Processing**
- **[Inference Flow](docs/INFERENCE_FLOW.md)**: Complete token processing pipeline
- **[Hot Swap Scenarios](docs/HOT_SWAP_SCENARIOS.md)**: Live adapter replacement flows
- **[Runtime Diagrams](docs/RUNTIME_DIAGRAMS.md)**: Execution state machines

### **Database & Schema**
- **[Database Schema](docs/database-schema.md)**: Complete entity relationships
- **[Workflow Diagrams](docs/database-schema/workflows/)**: 10+ workflow visualizations
- **[Route Map](docs/ROUTE_MAP_DIAGRAM.md)**: API endpoint relationships

### **Component Details**
- **[Lifecycle States](docs/LIFECYCLE.md)**: Adapter state machine visualization
- **[MLX Integration](docs/MLX_INTEGRATION.md)**: Backend architecture diagrams
- **[CoreML Integration](docs/COREML_INTEGRATION.md)**: ANE acceleration flows
- **[Metal Kernels](docs/metal/)**: GPU compute pipeline diagrams

### **Quick Diagram Reference**

| Component | Diagram | Location |
|-----------|---------|----------|
| **System Overview** | Architecture Flow | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| **Inference Pipeline** | Token Processing | [docs/INFERENCE_FLOW.md](docs/INFERENCE_FLOW.md) |
| **Database Schema** | Entity Relationships | [docs/database-schema.md](docs/database-schema.md) |
| **Adapter Lifecycle** | State Machine | [docs/LIFECYCLE.md](docs/LIFECYCLE.md) |
| **Multi-Backend** | Selection Logic | [docs/ADR_MULTI_BACKEND_STRATEGY.md](docs/ADR_MULTI_BACKEND_STRATEGY.md) |
| **Hot Swap** | Runtime Updates | [docs/HOT_SWAP.md](docs/HOT_SWAP.md) |
| **API Routes** | Endpoint Map | [docs/ROUTE_MAP_DIAGRAM.md](docs/ROUTE_MAP_DIAGRAM.md) |

**All diagrams are interactive Mermaid charts** - click any link above to explore the visual documentation!

---

## 🧪 Development

### Run Tests

```bash
cargo test --workspace
```

### Build Documentation

```bash
cargo doc --no-deps --open
```

### Format Code

```bash
cargo fmt --all
```

### Lint

```bash
cargo clippy --workspace -- -D warnings
```

### Duplication Monitoring

- Run a local scan: `make dup` (writes reports under `var/reports/jscpd/<timestamp>`)

---

## Performance

Benchmarked on **M3 Max (128GB unified memory)** with alpha-v0.11:

| Configuration | Tokens/sec | Latency (p95) | Memory | Determinism |
|--------------|-----------|---------------|---------|-------------|
| Base model only | 45 tok/s | 22ms | 14GB | ✓ |
| K=3, 5 adapters | 42 tok/s | 24ms | 16GB | ✓ |
| K=5, 10 adapters | 38 tok/s | 28ms | 18GB | ✓ |

*Router overhead: ~8% at K=3, Policy enforcement: <1%*

---

## Configuration

Example `configs/cp.toml`:

```toml
[server]
port = 8080

[db]
path = "var/aos-cp.sqlite3"

[security]
jwt_secret = "your-secret-key"
require_pf_deny = false

[paths]
plan_dir = "plan"
artifact_dir = "artifacts"

[router]
k_sparse = 3
entropy_floor = 0.02
gate_quant = "q15"

[memory]
min_headroom_pct = 15
evict_order = ["ephemeral_ttl", "cold_lru", "warm_lru"]
```

---

## 🛠️ Alpha Release Features

AdapterOS alpha-v0.11 includes:

### Completed Features
- ✅ **Naming Unification**: All crates renamed to `adapteros-*` with compatibility shims
- ✅ **Policy Registry**: 25 canonical policy packs with CLI commands
- ✅ **Adapter Taxonomy**: Semantic naming with lineage tracking and fork semantics
- ✅ **Metal Kernel Refactor**: Modular kernels with parameter structs
- ✅ **Deterministic Config**: Precedence rules with freeze mechanism
- ✅ **Database Schema**: Versioned migrations with rollback support

### In Progress
- 🔄 **Server API Refactor**: Structural improvements for production readiness
- 🔄 **Integration Tests**: End-to-end testing with policy enforcement
- 🔄 **Documentation**: Complete API reference and deployment guides

### Planned
- 📋 **Performance Optimization**: Router calibration and kernel tuning
- 📋 **Security Hardening**: Advanced threat detection and response
- 📋 **Monitoring**: Comprehensive observability and alerting

---

## 📚 Documentation

### Quick Links
- **[Quick Start Guide](docs/QUICKSTART.md)** - Get running in 10 minutes
- **[Documentation Index](docs/README.md)** - Complete documentation navigation
- **[System Architecture](docs/ARCHITECTURE.md)** - High-level design and components
- **[Policy Registry](docs/POLICIES.md)** - 25 canonical policy packs
- **[Security Guide](docs/SECURITY.md)** - Security architecture and practices
- **[Stability Log (Last Two Weeks)](docs/stability/RECENT_ISSUES.md)** - Point-in-time risk tracking

### Key Topics
- **Environment Setup**: [docs/ENVIRONMENT_SETUP.md](docs/ENVIRONMENT_SETUP.md) - Configuration profiles and variable reference
- **API Reference**: [docs/API_REFERENCE.md](docs/API_REFERENCE.md) - REST API documentation
- **Configuration**: [docs/CONFIG_PRECEDENCE.md](docs/CONFIG_PRECEDENCE.md)
- **Metal Kernels**: [docs/metal/phase4-metal-kernels.md](docs/metal/phase4-metal-kernels.md)
- **Safety Features**: [docs/runaway-prevention.md](docs/runaway-prevention.md)
- **Database Schema**: [docs/database-schema/](docs/database-schema/)

### API Reference
- **Rust API**: Run `cargo doc --open`
- **REST API**: See [docs/API_REFERENCE.md](docs/API_REFERENCE.md) - includes hot-swap endpoints like `POST /v1/adapter-stacks/{id}/activate` for zero-downtime stack swaps
- **CLI Commands**: See [crates/adapteros-cli/docs/aosctl_manual.md](crates/adapteros-cli/docs/aosctl_manual.md)

### Web UI Ports / Boot Entrypoint

- Canonical boot: `./start` (backend + UI via `scripts/service-manager.sh`), UI on port 3200 by default with health waits and drift checks.
- Legacy launchers (`launch.sh`, `scripts/run_complete_system.sh`, `scripts/start.sh`) are deprecated; they prompt/redirect or should be avoided. Menu bar helper is optional and currently not maintained.

---

## 🤝 Contributing

Contributions welcome! Please see `CONTRIBUTING.md` for guidelines.

### Development Setup

#### Pre-commit Hooks

Architectural pattern validation is enforced via pre-commit hooks:

```bash
# Install pre-commit hook
cp .githooks/pre-commit-architectural .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

The hook checks:
- Citation format validation (for extracted code)
- Architectural pattern compliance (lifecycle manager usage, database access)
- Error type validation (AosError vs generic errors)

To skip hooks (not recommended): `git commit --no-verify`

```bash
# Install development dependencies
cargo install cargo-watch cargo-nextest

# Run tests in watch mode
cargo watch -x test

# Run benchmarks
cargo bench
```

---

## 🗺️ Roadmap & Vision

### **Current Development Focus** (Updated 2025-12-06)
- Roadmap below is superseded; current priorities are tracked in the project board and docs runtime guides.
- Active themes: unified `./start` boot path and guardrails, determinism/regression coverage, policy enforcement wiring, production hardening (egress/tenant isolation), and training pipeline/registry cleanup.

### **Long-term Vision** (Aspirational)
- Multi-modal support (vision/audio), distributed training, auto-scaling, decentralized adapter sharing/marketplace.

### **Community & Ecosystem** (Aspirational)
- Plugin system, integration APIs, richer docs/tutorials, IDE/tooling integrations.

---

## 📄 License

Dual-licensed under Apache 2.0 or MIT at your option.

See [LICENSE](LICENSE) for the complete license text.

---

## 🙏 Acknowledgments

- **Apple Metal Team** for the excellent GPU compute framework
- **Rust Community** for amazing tooling and ecosystem
- **LoRA Authors** for the efficient fine-tuning technique
- **BLAKE3 Team** for fast cryptographic hashing
- **Ed25519 Implementers** for secure digital signatures

---

## 📞 Contact

- **GitHub**: [@rogu3bear](https://github.com/rogu3bear)
- **Email**: vats-springs0m@icloud.com

---

**AdapterOS alpha-v0.11-unstable-pre-release - Built with ❤️ for Apple Silicon**

*Deterministic ML inference with policy enforcement and zero network egress*

## Plugins

AdapterOS supports pluggable extensions via the PluginRegistry.

### Registering Custom Plugins

1. Implement the `Plugin` trait in your crate.
2. Register in main.rs with `registry.register(name, plugin_instance, config).await?`
3. Use API to enable/disable per tenant: POST /v1/plugins/:name/enable {tenant_id}

### API Examples

To enable the Git plugin (tenant is determined from JWT claims):

```bash
curl -X POST http://localhost:8080/v1/plugins/git/enable \
  -H "Authorization: Bearer $JWT"
```

To disable the Git plugin (tenant is determined from JWT claims):

```bash
curl -X POST http://localhost:8080/v1/plugins/git/disable \
  -H "Authorization: Bearer $JWT"
```

To list all plugins and their status:

```bash
curl -X GET http://localhost:8080/v1/plugins \
  -H "Authorization: Bearer $JWT"
```

---

## See Also

- [QUICKSTART.md](QUICKSTART.md) - Quick start guide for macOS
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) - Full architecture documentation
- [docs/ARCHITECTURE.md#architecture-components](docs/ARCHITECTURE.md#architecture-components) - Detailed architectural patterns
- [AGENTS.md](AGENTS.md) - Developer quick reference guide
- [docs/MLX_INTEGRATION.md](docs/MLX_INTEGRATION.md) - MLX backend integration
- [docs/COREML_INTEGRATION.md](docs/COREML_INTEGRATION.md) - CoreML backend with ANE acceleration

MLNavigator Inc Thursday Dec 11, 2025.
