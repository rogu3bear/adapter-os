# adapterOS Complete Setup Guide

Get adapterOS fully up and running in under 10 minutes. This guide covers both backend integration and web UI setup.

> **For basic backend-only setup**, see [getting-started.md](getting-started.md) (5 minutes).  
> **Canonical source:** `./start`, `./aosctl`, `scripts/build-ui.sh` — code is authoritative.

**Last Updated:** 2026-02-18

## Prerequisites

### System Requirements
- **macOS 13.0+** with Apple Silicon (M1/M2/M3/M4)
- **Rust stable**: See `rust-toolchain.toml` for exact version
- **MLX**: `brew install mlx` (required for MLX backend)
- **Trunk** (for Leptos UI): `cargo install trunk`

### Clone & Setup

```bash
git clone https://github.com/rogu3bear/adapter-os.git
cd adapter-os
```

## Quick Start: Backend

### 1. Build the Project

```bash
# Build everything (release mode)
cargo build --release --workspace

# Build CLI and create symlink
cargo build --release -p adapteros-cli --features tui
ln -sf target/release/aosctl ./aosctl
```

### 2. Run Database Migrations

```bash
./aosctl db migrate
```

### 3. Seed Models

```bash
# Seed models from the default model directory
./aosctl models seed

# Or seed from a custom path
./aosctl models seed --model-path /path/to/models

# Verify models are seeded
./aosctl models list
```

### 4. Start the Server

```bash
# Option 1: Full stack (backend + worker, recommended)
./start

# Option 2: Backend only (canonical helper)
./start backend

# Option 3: Backend only via cargo (advanced/manual)
AOS_DEV_NO_AUTH=1 cargo run -p adapteros-server -- --config configs/cp.toml
```

### 5. Verify Setup

```bash
# Check system status
./aosctl status

# Run system health diagnostics
./aosctl doctor

# Run pre-flight readiness check
./aosctl preflight
```

## Understanding the Database

adapterOS uses SQLite for tenant management, adapter registry, and telemetry. For complete database schema documentation:

- [Database Documentation](DATABASE.md) - Complete database structure and schema

## Working with Adapters

```bash
# List adapters
./aosctl adapter list

# List adapter stacks
./aosctl stack list

# List policy packs
./aosctl policy list
```

## Quick Start: Web UI

### Development Mode (Hot Reload)

```bash
# Terminal 1: UI dev server (Leptos + Trunk)
cd crates/adapteros-ui
trunk serve

# Terminal 2: Backend server
AOS_DEV_NO_AUTH=1 ./start backend
```

Visit http://127.0.0.1:3200 for the hot-reload UI. API calls proxy to backend at http://127.0.0.1:8080.

### Production Build (Single Binary)

```bash
# 1. Build backend + static UI assets
./scripts/build-ui.sh

# 2. Start backend with embedded static UI
./start backend
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
./aosctl metrics show <cpid>

# Via API
curl http://localhost:8080/v1/code/metrics/<cpid> \
  -H "Authorization: Bearer $TOKEN"
```

### Manage Adapters

```bash
# List adapters
./aosctl list-adapters

# Pin adapter
./aosctl pin-adapter --id <adapter-id> --rank 16

# Evict adapter
./aosctl evict-adapter --id <adapter-id>
```

## Troubleshooting

### "UI not built. Run: trunk build"

Build the UI first:
```bash
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
