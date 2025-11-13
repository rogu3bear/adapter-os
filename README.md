# AdapterOS: Deterministic ML Inference Runtime

**High-performance inference runtime with K-sparse LoRA routing, Metal-optimized kernels, and comprehensive policy enforcement for production environments.**

AdapterOS (alpha-v0.04-unstable) is a Rust-based ML inference engine optimized for Apple Silicon, featuring deterministic execution, modular Metal kernels, centralized policy enforcement, and memory-efficient adapter management with zero network egress during serving.

---

## [TARGET] What is AdapterOS?

AdapterOS enables **deterministic multi-adapter inference** on Apple Silicon by:

- **K-Sparse LoRA Routing**: Dynamic gating with Q15 quantized gates and entropy floor
- **Modular Metal Kernels**: Precompiled `.metallib` kernels with deterministic compilation
- **Secure Keychain Integration**: Multi-platform hardware-backed key storage (Secure Enclave, OS keychains, encrypted fallbacks)
- **Production-Ready Streaming**: Authenticated real-time SSE streams for training, discovery, and contact events with circuit breaker protection
- **Policy Enforcement**: 21 canonical policy packs for compliance, security, and quality
- **Environment Fingerprinting**: Cryptographically signed drift detection with automatic baseline creation
- **Deterministic Execution**: Reproducible outputs with HKDF seeding and canonical JSON
- **Zero Network Egress**: Air-gapped serving with Unix domain sockets only
- **Memory Management**: Intelligent adapter eviction with ≥15% headroom maintenance

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│                   AdapterOS Runtime                     │
│                    (alpha-v0.04-unstable)                     │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────────┐    ┌──────────────┐   ┌───────────┐ │
│  │   Policy     │───▶│   Router     │──▶│  Modular  │ │
│  │  Registry   │    │ (Q15 Gates)  │   │  Kernels  │ │
│  │  (22 Packs) │    │ K-Sparse     │   │ (.metallib)│ │
│  └──────────────┘    └──────────────┘   └───────────┘ │
│         │                    │                  │       │
│         ▼                    ▼                  ▼       │
│  ┌──────────────────────────────────────────────────┐  │
│  │         LoRA Adapter Registry                    │  │
│  │  [Adapter 1] [Adapter 2] ... [Adapter N]        │  │
│  └──────────────────────────────────────────────────┘  │
│         │                                               │
│         ▼                                               │
│  ┌──────────────────────────────────────────────────┐  │
│  │         Base Model (Qwen, Llama, etc.)           │  │
│  └──────────────────────────────────────────────────┘  │
│                                                          │
│  ┌──────────────────────────────────────────────────┐  │
│  │      Secure Keychain Integration                 │  │
│  │  • Hardware Secure Enclave (macOS)              │  │
│  │  • OS Keychain Services                          │  │
│  │  • Encrypted Keystore Fallback                   │  │
│  └──────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────┐  │
│  │   🔒 Production Streaming API                    │  │
│  │  • Authenticated SSE Streams                     │  │
│  │  • Circuit Breaker Protection                    │  │
│  │  • Multi-Worker Aggregation                      │  │
│  └──────────────────────────────────────────────────┘  │
│                                                          │
└─────────────────────────────────────────────────────────┘
                         │
                         ▼
              ┌──────────────────────┐
              │   Apple Metal GPU    │
              │   (Unified Memory)   │
              │   Zero Network       │
              │   (Unix Sockets)     │
              └──────────────────────┘
```

---

## 🚀 Quick Start

### Getting Started with the UI

**New in this release:** Complete UI-driven workflow for model management!

1. **Import a Base Model** - Use the model import wizard to add models via UI
2. **Load the Model** - One-click model loading with real-time status
3. **Configure Cursor IDE** - Guided setup wizard for IDE integration
4. **Start Coding** - Use your model in Cursor with zero configuration

See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for deployment and configuration details.

### Option 1: Graphical Installer (Recommended)

**Native macOS installer with hardware validation and guided setup:**

```bash
# Build the installer
make installer

# Or open in Xcode
make installer-open
```

The graphical installer provides:
- **Hardware Pre-Checks**: Validates Apple Silicon (M1+), RAM (≥16GB), and disk space
- **Installation Modes**: Full (with model download) or Minimal (binaries only)
- **Air-Gapped Support**: Skip all network operations for offline installations
- **Checkpoint Recovery**: Resume interrupted installations automatically
- **Determinism Education**: Learn about cryptographic verification after install

See [installer/README.md](installer/README.md) for details.

### Option 2: Manual Installation

### Prerequisites

- **macOS 13.0+** with Apple Silicon (M1/M2/M3/M4)
- **Rust 1.75+**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **MLX** (optional): Install via Homebrew for MLX backend support: `brew install mlx`

### Build

```bash
# Clone the repository
git clone https://github.com/rogu3bear/adapter-os.git
cd adapter-os

# Build the workspace (default: Metal backend only)
cargo build --release

# Build with MLX backend support (C++ FFI, no Python required)
cargo build --release --features mlx-ffi-backend

# Build with telemetry support (metrics, monitoring, observability)
cargo build --release --features telemetry

# Note: Metal backend is the primary production backend (default)
# MLX backend is available via --features mlx-ffi-backend (uses C++ FFI, no PyO3)
# Telemetry is optional via --features telemetry (includes metrics, monitoring, observability)

# Initialize the database
./target/release/aosctl init-tenant --id default --uid 1000 --gid 1000

### Import a Model

```bash
# Import Qwen 2.5 7B for Metal backend (default)
./target/release/aosctl import-model \
  --name qwen2.5-7b \
  --weights models/qwen2.5-7b-mlx/weights.safetensors \
  --config models/qwen2.5-7b-mlx/config.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --tokenizer-cfg models/qwen2.5-7b-mlx/tokenizer_config.json \
  --license models/qwen2.5-7b-mlx/LICENSE

# Import MLX model (requires --features mlx-ffi-backend)
./target/release/aosctl import-model \
  --name qwen2.5-7b-mlx \
  --weights models/qwen2.5-7b-mlx/weights.safetensors \
  --config models/qwen2.5-7b-mlx/config.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --tokenizer-cfg models/qwen2.5-7b-mlx/tokenizer_config.json \
  --license models/qwen2.5-7b-mlx/LICENSE
```

### Register LoRA Adapters

```bash
# Register your LoRA adapters
./target/release/aosctl register-adapter \
  --id my-lora \
  --hash <adapter-hash> \
  --tier 1 \
  --rank 16
```

### Start Serving

```bash
# Build and serve a plan with Metal backend (default)
./target/release/aosctl build-plan --tenant-id default --manifest configs/cp.toml
./target/release/aosctl serve --plan <plan-id> --backend metal

# Serve with MLX backend (requires --features mlx-ffi-backend and model path)
export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx
./target/release/aosctl serve --plan <plan-id> --backend mlx --model-path ./models/qwen2.5-7b-mlx

# Or use the integrated server
./target/release/adapteros-server --config configs/cp.toml
```

Note: If your policy requires open-book (evidence) serving, the server refuses to start unless a RAG index is available (pgvector or local index). See docs/rag-pgvector.md for setup.

---

## 📦 Components

### Core Crates

| Crate | Purpose |
|-------|---------|
| `adapteros-worker` | Inference engine with policy enforcement |
| `adapteros-router` | K-sparse LoRA routing with Q15 quantized gates |
| `adapteros-kernel-mtl` | Modular Metal kernels with deterministic compilation |
| `adapteros-plan` | Plan builder and loader |
| `adapteros-chat` | Chat template processor (ChatML, etc.) |
| `adapteros-rag` | Evidence retrieval with HNSW vector search |
| `adapteros-service-supervisor` | Service management and health monitoring |

### Management

| Crate | Purpose |
|-------|---------|
| `adapteros-server` | Control plane API server |
| `adapteros-server-api` | REST API handlers |
| `adapteros-cli` | Command-line tool (`aosctl`) |
| `adapteros-db` | SQLite database layer with migrations |
| `adapteros-system-metrics` | System metrics collection and alerting |

### Infrastructure

| Crate | Purpose |
|-------|---------|
| `adapteros-policy` | 20-pack policy registry with enforcement |
| `adapteros-telemetry` | Canonical JSON event logging with Merkle trees |
| `adapteros-crypto` | Ed25519 signing, BLAKE3 hashing, HKDF |
| `adapteros-artifacts` | Content-addressed artifact store with SBOM |
| `adapteros-config` | Deterministic configuration with precedence rules |

---

## 🎛️ Key Features

**Database Migrations**
- SQLite uses `migrations/` (default for local/dev; tests reference this path).
- PostgreSQL uses `migrations_postgres/` (production/cluster deployments).
- System metrics support both backends with automatic schema selection.
- Set `DATABASE_URL=postgresql://...` to use the Postgres backend and apply PG migrations.

### RAG PgVector Backend (feature)

- Default builds use an in-memory per-tenant index with a synchronous `RagSystem` API.
- Enable the PostgreSQL backend with `--features rag-pgvector` (CLI forwards the feature to the RAG crate).
- At startup, the CLI will connect to Postgres, run migrations, and initialize `PgVectorIndex` with `RAG_EMBED_DIM` (default `3584`).
- Deterministic retrieval is guaranteed by ordering `(score DESC, doc_id ASC)`.
- See docs/rag-pgvector.md for setup and docker-compose.

### 1. **K-Sparse LoRA Routing**

AdapterOS uses learned gates to select the top-K most relevant LoRA adapters per token:

```rust
// Router selects K=3 adapters with highest gate values
let selected = router.route(hidden_states, k=3);
// Gates are quantized to Q15 for efficiency
// Entropy floor prevents single-adapter collapse
// Deterministic tie-breaking: (score desc, doc_id asc)
```

Runtime adapter selection integrates feature-driven scoring and priors:
- Features: language one-hot, framework priors, symbol hits, path tokens, prompt verb, attention entropy (22-dim vector)
- Priors: framework hint boosts and lifecycle activation percentage
- Deterministic ordering with entropy floor and Q15 quantization

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

20 canonical policy packs ensure compliance:
- **Egress Ruleset**: Zero network during serving, PF enforcement
- **Determinism Ruleset**: Precompiled kernels, HKDF seeding
- **Router Ruleset**: K bounds, entropy floor, Q15 gates
- **Evidence Ruleset**: Mandatory open-book grounding
- **Refusal Ruleset**: Abstain on low confidence
- **And 15 more** for security, compliance, and quality

### 4. **Deterministic Execution**

Reproducible inference with:
- Fixed random seeds (HKDF-derived)
- Quantized gates (Q15)
- Deterministic tie-breaking in retrieval
- Embedded `.metallib` kernels (no runtime compilation)
- Canonical JSON serialization (JCS)
- Configuration freeze with BLAKE3 hashing

---

## 🧪 Development

### Run Tests

```bash
cargo test --workspace
```

### Build Documentation

## 🎓 Training Adapters (Micro-LoRA)

Use the CLI to train with a JSON dataset (pre-tokenized). Optionally initialize Metal kernels with a plan. After training, you can package the adapter and register it in the local registry DB for runtime use.

```bash
# Train only (writes legacy outputs under --output)
aosctl train \
  --data data/small.json \
  --output out/train1 \
  --plan plan/qwen7b/PLAN_ID \
  --rank 16 --epochs 1 \
  --base-model qwen2.5-7b

# Train, then package + register
export AOS_ADAPTERS_ROOT=$PWD/adapters
aosctl train \
  --data data/small.json \
  --output out/train1 \
  --plan plan/qwen7b/PLAN_ID \
  --rank 16 --epochs 1 \
  --base-model qwen2.5-7b \
  --pack --adapters-root "$AOS_ADAPTERS_ROOT" \
  --register --adapter-id demo_adapter --tier ephemeral --reg-rank 16
```

Artifacts are placed under `<adapters_root>/<adapter_id>/` (weights.safetensors, manifest.json, signature.sig, public_key.pem). The control plane loads artifacts from the adapters root and stores metadata (ID, B3 hash, rank, tier, etc.) in the registry DB.

Adapters root:
- Default: `./adapters`
- Override: set `AOS_ADAPTERS_ROOT=/path/to/adapters`

Set `--deterministic` to derive a repeatable seed from the dataset/config tuple, or provide an explicit seed with `--seed <u64>` when you need bit-for-bit reproducibility across hosts. When registering adapters, `--pack` must be supplied so manifests and signatures are present.

### Orchestrated Training (Server API)

You can kick off training via the control plane:

```bash
curl -X POST http://127.0.0.1:8080/api/v1/training/start \
  -H 'Authorization: Bearer adapteros-local' \
  -H 'Content-Type: application/json' \
  -d '{
    "adapter_name": "demo_adapter",
    "config": { "rank": 8, "alpha": 16, "targets": ["q_proj","v_proj"], "epochs": 1, "learning_rate": 0.0003, "batch_size": 4 },
    "dataset_path": "data/code_to_db_training.json",
    "adapters_root": "./adapters",
    "package": true,
    "register": true,
    "adapter_id": "demo_adapter",
    "tier": 8
  }'
```

This will train, package, and register the adapter when the job completes.

### Train From a Directory (Server API)

Build a dataset directly from a code directory using the codegraph analyzer (no pre-tokenized JSON required):

```bash
curl -X POST http://127.0.0.1:8080/api/v1/training/start \
  -H 'Authorization: Bearer adapteros-local' \
  -H 'Content-Type: application/json' \
  -d '{
    "adapter_name": "dir_adapter",
    "config": { "rank": 8, "alpha": 16, "targets": ["q_proj","v_proj"], "epochs": 1, "learning_rate": 0.0003, "batch_size": 4 },
    "directory_root": "/absolute/path/to/repo",
    "directory_path": "src",
    "adapters_root": "./adapters",
    "package": true,
    "register": true,
    "adapter_id": "dir_adapter",
    "tier": 8
  }'
```

### Dataset Schema

The CLI supports two data formats:

**Text-based format** (auto-detected, recommended):
```json
{
  "name": "my_dataset",
  "examples": [
    {
      "input": { "Text": "Write a function" },
      "target": { "Text": "def func():\n    pass" },
      "weight": 1.0
    }
  ]
}
```

**Pre-tokenized format** (backward compatible):
```json
{
  "examples": [
    { "input": [1,2,3], "target": [4,5,6] },
    { "input": [7,8,9], "target": [10,11,12] }
  ]
}
```

The CLI automatically detects the format. For text-based data, specify `--tokenizer` (defaults to `models/qwen2.5-7b-mlx/tokenizer.json`). Ensure your tokenization matches the tokenizer used at inference time (e.g., Qwen tokenizer). Smoke test by encoding/decoding a few snippets and running a tiny training (N=2, epochs=1) to observe loss decrease.

### CodeGraph Analysis (Repository Intelligence)

AdapterOS includes comprehensive repository analysis capabilities for framework detection, language analysis, and security scanning:

```bash
# Detect frameworks in a project directory
curl -X POST http://127.0.0.1:8080/api/v1/codegraph/frameworks/detect \
  -H 'Authorization: Bearer adapteros-local' \
  -H 'Content-Type: application/json' \
  -d '{
    "path": "/path/to/project",
    "framework_types": ["React", "Django"]
  }'

# Get comprehensive repository metadata
curl -X POST http://127.0.0.1:8080/api/v1/codegraph/repository/metadata \
  -H 'Authorization: Bearer adapteros-local' \
  -H 'Content-Type: application/json' \
  -d '{
    "path": "/path/to/repository",
    "include_frameworks": true,
    "include_languages": true,
    "include_security": true
  }'
```

**Features:**
- **15+ Framework Detection**: React, Next.js, Vue, Angular, Django, FastAPI, Flask, Rails, Laravel, Spring Boot, Quarkus, Actix Web, Axum, Express
- **Language Analysis**: File counts, line counts, percentage distributions
- **Security Scanning**: Entropy-based secret detection with configurable severity
- **Git Integration**: Efficient repository statistics without expensive operations
- **Intelligent Caching**: 5-minute TTL caching for performance
- **Production Security**: Path validation, traversal protection, rate limiting

Results feed into the K-sparse LoRA router for context-aware adapter selection.

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
- See `docs/DUPLICATION_MONITORING.md` for CI integration and enforcement options.

---

## 📊 Performance

Benchmarked on **M3 Max (128GB unified memory)** with alpha-v0.04-unstable:

| Configuration | Tokens/sec | Latency (p95) | Memory | Determinism |
|--------------|-----------|---------------|---------|-------------|
| Base model only | 45 tok/s | 22ms | 14GB | ✓ |
| K=3, 5 adapters | 42 tok/s | 24ms | 16GB | ✓ |
| K=5, 10 adapters | 38 tok/s | 28ms | 18GB | ✓ |

*Router overhead: ~8% at K=3, Policy enforcement: <1%*

---

## 🔄 Production Streaming API

AdapterOS provides **enterprise-grade real-time streaming** with cryptographic authentication, circuit breaker protection, and multi-worker aggregation.

### Streaming Endpoints

| Endpoint | Purpose | Authentication | Real-time Source |
|----------|---------|----------------|------------------|
| `GET /v1/streams/training` | Training progress & metrics | JWT + tenant filter | `TrainingService` events |
| `GET /v1/streams/discovery` | Repository scanning & analysis | JWT + tenant/repo filter | `DiscoverySignalBridge` |
| `GET /v1/streams/contacts` | Contact discovery & interactions | JWT + tenant filter | `ContactDiscoveryHandler` |

### Key Features

- **🔐 Cryptographic Authentication**: Ed25519 signatures prevent signal tampering
- **🛡️ Circuit Breaker Protection**: Automatic failure detection and recovery
- **🔄 Multi-Worker Aggregation**: Load balancing across worker pools
- **📊 Real-time Metrics**: Signal processing health monitoring
- **🏢 Enterprise Reliability**: Exponential backoff, graceful shutdown, tenant isolation

### Configuration

```toml
[signals]
auth_required = true                    # Require authentication (production default)
channel_capacity = 256                  # Broadcast buffer size
retry_delay_secs = 5                    # Initial retry delay
max_retry_delay_secs = 300              # Max backoff delay (5min)
circuit_breaker_threshold = 5           # Failure threshold
circuit_breaker_reset_secs = 60         # Recovery timeout
connection_timeout_secs = 30            # UDS connection timeout
multi_worker_enabled = true             # Worker pool aggregation
```

### Example Usage

```bash
# Stream training progress for tenant "acme"
curl -H "Authorization: Bearer <jwt>" \
     "http://localhost:8080/api/v1/streams/training?tenant=acme"

# Stream repository discovery events
curl -H "Authorization: Bearer <jwt>" \
     "http://localhost:8080/api/v1/streams/discovery?tenant=acme&repo=github.com/acme/payments"
```

---

## 📊 Observability

AdapterOS provides comprehensive monitoring and observability features for production deployments:

### Buffer Tuning Options

Configure buffer sizes based on your workload and memory constraints:

```toml
[telemetry]
enabled = true
# In-memory buffer for recent telemetry events (default: 1024)
# Increase for high-throughput deployments to prevent event loss
telemetry_buffer_capacity = 2048

# Channel capacity for telemetry broadcasting (default: 256)
# Increase if you have many concurrent telemetry consumers
telemetry_channel_capacity = 512

# In-memory buffer for recent trace events (default: 512)
# Increase for detailed tracing in high-traffic environments
trace_buffer_capacity = 1024
```

### Monitoring Features

- **Prometheus Integration**: Exposes `/metrics` endpoint for scraping
- **Structured Telemetry**: JSON-formatted event logging with metadata
- **Health Checks**: `/health` endpoint for load balancer monitoring
- **Performance Metrics**: Inference latency, throughput, and memory usage
- **Policy Enforcement Tracking**: Audit trail for policy decisions
- **Alert Integration**: Configurable thresholds for automated alerts

See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for complete monitoring setup and alerting configuration.

---

## 🔧 Configuration

Example `configs/cp.toml`:

```toml
[server]
port = 8080

[db]
path = "var/aos.db"

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

### 🛠️ Alpha Release Features

AdapterOS alpha-v0.04-unstable includes:

### Completed Features
- ✅ **Naming Unification**: All crates renamed to `adapteros-*` with compatibility shims
- ✅ **Policy Registry**: 20 canonical policy packs with CLI commands
- ✅ **Metal Kernel Refactor**: Modular kernels with parameter structs
- ✅ **Deterministic Config**: Precedence rules with freeze mechanism
- ✅ **Database Schema**: Versioned migrations with rollback support
- ✅ **Server API Refactor**: Rate limiting (100/min), RBAC enhancements for admin routes【@crates/adapteros-server-api/src/routes.rs §694】
- ✅ **Integration Tests**: E2E flows for policy, routing, determinism, memory, tenants【@tests/integration_tests.rs §new tests】
- ✅ **Service Supervisor**: Process management, health checks, and metrics integration in adapteros-service-supervisor

### In Progress
- 🔄 **Server Compilation**: Fix async lifetime issues in adapteros-server
- 🔄 **MLX Backend UI Integration**: Add MLX backend selection to web dashboard
- 🔄 **Observability**: Prometheus hooks, threat detection【@README.md §431】
- 🔄 **Menu Bar App Enhancements**: Updated status views and service panel client in Swift

### MVP Status
- ✅ **Core Components**: Inference pipeline, router, CLI all functional
- ✅ **Security**: Keychain implementations secure (no hardcoded keys)
- ✅ **Testing**: Integration tests enabled, unit tests passing
- ✅ **UI Updates**: Root layout improvements in React
- ⚠️ **Server**: Compilation errors prevent full E2E testing
- See [MVP Quick Start Guide](docs/MVP_QUICKSTART.md) for details

### Planned for v0.02
- 📋 **MLX Backend Default Build**: Consider including MLX in default build (currently opt-in)
- 📋 **Observability Hardening**: Alerting, advanced detection【@crates/adapteros-telemetry/src/】
- 📋 **Deployment Guides**: Multi-node, scaling【@docs/DEPLOYMENT.md】
- 📋 **Database Schema Updates**: Reflect recent changes in docs/database-schema/README.md

### API Reference
- **Rust API**: Run `cargo doc --no-deps --open`【@README.md §451】
- **REST API**: Swagger UI at `/swagger-ui` post-server start【@crates/adapteros-server-api/src/routes.rs §690】
- **Deployment**: See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for prod setup (Postgres, RAG, monitoring)【@docs/DEPLOYMENT.md §1】

---

## 📚 Documentation

### Quick Links
- **[MVP Quick Start Guide](docs/MVP_QUICKSTART.md)** - MVP status and quick start (NEW)
- **[Quick Start Guide](docs/QUICKSTART.md)** - Get running in 10 minutes
- **[Documentation Index](docs/README.md)** - Complete documentation navigation
- **[System Architecture](docs/architecture.md)** - High-level design and components
- **[Policy Registry](docs/POLICIES.md)** - 20 canonical policy packs

### Key Topics
- **Control Plane**: [docs/control-plane.md](docs/control-plane.md)
- **Configuration**: [docs/CONFIG_PRECEDENCE.md](docs/CONFIG_PRECEDENCE.md)
- **Metal Kernels**: [docs/metal/phase4-metal-kernels.md](docs/metal/phase4-metal-kernels.md)
- **Safety Features**: [docs/runaway-prevention.md](docs/runaway-prevention.md)
- **Database Schema**: [docs/database-schema/](docs/database-schema/)

### API Reference
- **Rust API**: Run `cargo doc --open`
- **REST API**: See [docs/control-plane.md](docs/control-plane.md)
- **Authentication API**: See [docs/AUTHENTICATION.md](docs/AUTHENTICATION.md)
- **CLI Commands**: See [crates/adapteros-cli/docs/aosctl_manual.md](crates/adapteros-cli/docs/aosctl_manual.md)

### Performance & Quality
- **Performance Characteristics**: See [docs/AUTH_PERFORMANCE.md](docs/AUTH_PERFORMANCE.md)
- **Benchmark Suite**: Run `cargo test --test kernel_regression`
- **Code Quality**: 21 policy packs, comprehensive linting, security audit

---

## 🤝 Contributing

Contributions welcome! Please see `CONTRIBUTING.md` for guidelines.

### Development Setup

```bash
# Install development dependencies
cargo install cargo-watch cargo-nextest

# Run tests in watch mode
cargo watch -x test

# Run benchmarks
cargo bench

# Populate demo monitoring data (uses var/cp.db)
python3 scripts/seed_demo_data.py
```

---

## 📄 License

Dual-licensed under Apache 2.0 or MIT at your option.

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

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

**AdapterOS alpha-v0.04-unstable - Built with ❤️ for Apple Silicon**

*Deterministic ML inference with policy enforcement and zero network egress*

## Boot
./bootstrap.sh

## Shutdown
./shutdown.sh

## Services
launchctl list | grep adapteros

## Simplify
Native launchd + scripts for macOS.
