# AdapterOS Quick Start Guide

Get AdapterOS up and running in under 10 minutes. This guide covers both backend integration and web UI setup, including recent enhancements for service supervision and improved monitoring.

**Last Updated: November 13, 2025**

## Prerequisites

### System Requirements
- **macOS 13.0+** with Apple Silicon (M1/M2/M3/M4)
- **Rust 1.75+**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **MLX**: `brew install mlx` (required for MLX backend; `pip install mlx-lm` for model conversion)

### Additional Tools
- **Trunk** (for Leptos UI): `cargo install trunk`

## Quick Start: Backend

### 1. Configure Environment

Create `.env` file in project root:

```bash
# Database
DATABASE_URL=sqlite:var/aos.db

# Server
SERVER_HOST=127.0.0.1
SERVER_PORT=9443

# Security
JWT_SECRET=$(openssl rand -base64 32)

# Logging
RUST_LOG=info,adapteros=debug
```

### 2. Build the Project

```bash
# Build CLI (creates ./aosctl symlink in project root)
make cli

# Or build all components
cargo build --release

# Or build specific packages
cargo build --release --package adapteros-lora-worker
cargo build --release --package adapteros-server
cargo build --release --package adapteros-cli
```

## Understanding the Database

AdapterOS uses SQLite for tenant management, adapter registry, and telemetry. For complete database schema documentation:

- [Database Schema Overview](database-schema/README.md) - Complete database structure and workflow animations
- [Schema Diagram](database-schema/SCHEMA-DIAGRAM.md) - ER diagram with 30+ tables
- [Basic Workflows](database-schema/examples/BASIC-WORKFLOWS.md) - Common database operations and examples

The following sections show basic operations with the database.

### 3. Initialize Database & Import Model

```bash
# Initialize tenant
./aosctl init-tenant --id default --uid 1000 --gid 1000

# Import model (using included Qwen 2.5 7B)
./aosctl import-model \
  --name qwen2.5-7b \
  --weights models/qwen2.5-7b-mlx/weights.safetensors \
  --config models/qwen2.5-7b-mlx/config.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json
```

### 4. Register LoRA Adapters

```bash
# Register your LoRA adapters
./aosctl register-adapter \
  --id my-lora \
  --hash <adapter-hash> \
  --tier 1 \
  --rank 16
```

### 5. Start Server with Service Supervisor

```bash
# Start dev server
make dev

# Or build and serve a plan
./aosctl build-plan --tenant-id default --manifest configs/cp.toml
./aosctl serve --plan <plan-id>
```

## Quick Start: Web UI

### Development Mode (Hot Reload)

```bash
# Terminal 1: UI dev server (Leptos + Trunk)
cd crates/adapteros-ui
trunk serve

# Terminal 2: Backend server
cargo run --release --bin adapteros-server -- --config configs/cp.toml
```

Visit http://127.0.0.1:8080 (UI served from static/) and http://127.0.0.1:8080/api (API)

### Production Build (Single Binary)

```bash
# 1. Build UI
make ui

# 2. Build control plane with embedded UI
cargo build --release --bin adapteros-server

# 3. Run single binary
./target/release/adapteros-server --config configs/cp.toml
```

Visit http://127.0.0.1:8080

## What You'll See in the UI

- **Dashboard**: System health, user info, quick links
- **Tenants**: Create and manage tenants (Admin role)
- **Nodes**: View compute nodes
- **Plans**: View build plans and kernels
- **Adapters**: Manage LoRA adapters and registry
- **Promotion**: Gate-checked CP promotions
- **Telemetry**: View telemetry bundles and metrics
- **Policies**: Edit and validate policy configurations
- **Code Intelligence**: Repository analysis and patch management

## Key Features

### 1. K-Sparse LoRA Routing

Dynamic adapter selection per token with quantized gates:

```rust
// Router selects K=3 adapters with highest gate values
let selected = router.route(hidden_states, k=3);
// Gates are quantized to Q15 for efficiency
// Entropy floor prevents single-adapter collapse
```

### 2. Metal-Optimized Kernels

Fused operations for maximum GPU utilization:
- Embedded `.metallib` blobs (no runtime compilation)
- Deterministic execution
- Unified memory optimization

### 4. Policy Enforcement

Configurable policies for inference rules:
- Evidence requirements
- Refusal thresholds
- Numeric validation
- Router constraints

## Common Tasks

### Run Tests

```bash
# Unit tests
cargo test --workspace

# Integration tests (requires running server)
cargo test --test integration_tests -- --ignored

# Specific test
cargo test test_patch_lifecycle
```

### View Metrics

```bash
# Via CLI
./target/release/aosctl metrics show <cpid>

# Via API
curl http://localhost:9443/api/v1/code/metrics/<cpid> \
  -H "Authorization: Bearer $TOKEN"
```

### Manage Adapters

```bash
# List adapters
./target/release/aosctl list-adapters

# Pin adapter
./target/release/aosctl pin-adapter --id <adapter-id> --rank 16

# Evict adapter
./target/release/aosctl evict-adapter --id <adapter-id>
```

## Troubleshooting

### "UI not built. Run: make ui"

Build the UI first:
```bash
make ui
# Or manually:
cd crates/adapteros-ui && trunk build --release
```

### "Connection refused"

Make sure the server is running:
```bash
cargo run --release --bin adapteros-server -- --config configs/cp.toml
```

### Compilation Errors

Clean and rebuild:
```bash
cargo clean
cargo build --release
```

### Privilege Dropping Fails

Either run as root or skip (dev mode will log warning but continue):
```bash
# As root
sudo ./target/release/adapteros-server --config configs/cp.toml

# Or in dev mode (no privilege dropping)
./target/release/adapteros-server --config configs/cp.toml
```

## Performance Tips

### 1. Optimize Router Configuration

```toml
# In configs/cp.toml
[router]
k_sparse = 3              # Lower K for faster inference
entropy_floor = 0.02      # Prevent collapse
gate_quant = "q15"        # Quantized gates
```

### 2. Memory Management

```toml
[memory]
min_headroom_pct = 15                              # Reserve 15% headroom
evict_order = ["ephemeral_ttl", "cold_lru", "warm_lru"]  # Eviction order
```

### 3. Parallel Testing

```bash
cargo test --workspace --jobs 4
```

## Security Checklist

- [x] JWT secret generated and secured
- [x] Input validation enabled on all endpoints
- [x] Database migrations applied
- [x] Privilege dropping configured for production
- [ ] HTTPS/TLS configured (if exposing publicly)
- [ ] Rate limiting configured (future)
- [ ] Audit logging enabled (future)

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                DIR (Deterministic Inference Runtime)     │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────────┐    ┌──────────────┐   ┌───────────┐ │
│  │   Router     │───▶│  K-Sparse    │──▶│  Fused    │ │
│  │  (Q15 Gates) │    │   Selector   │   │  Kernels  │ │
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
└─────────────────────────────────────────────────────────┘
                         │
                         ▼
              ┌──────────────────────┐
              │   Apple Metal GPU    │
              │   (Unified Memory)   │
              └──────────────────────┘
```

## Next Steps

1. **Explore Documentation**: See `/docs` for detailed architecture and API docs
2. **Try Code Intelligence**: Use patch generation and analysis features
3. **Configure Policies**: Customize inference behavior in `configs/cp.toml`
4. **Add Adapters**: Register and route to your LoRA adapters
5. **Deploy to Production**: Follow deployment guide for production setup

## Getting Help

- **Documentation**: See `/docs` directory
  - Architecture: `docs/ARCHITECTURE.md`
  - Metal Kernels: `docs/metal/`
- **Examples**: Check `examples/` directory
- **API Reference**: Run `cargo doc --open`
- **Integration Tests**: See `tests/` for usage examples

## Summary

You now have:
- ✅ High-performance inference runtime
- ✅ K-sparse LoRA routing
- ✅ Metal-optimized kernels
- ✅ Web UI for management
- ✅ Code intelligence features
- ✅ Policy enforcement system
- ✅ Production-ready backend

Start exploring with the web UI or dive into the examples!

**Built with ❤️ for Apple Silicon**
