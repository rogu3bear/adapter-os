# AdapterOS Quick Start Guide

Get MPLoRA up and running in under 10 minutes. This guide covers both backend integration and web UI setup.

## Prerequisites

### System Requirements
- **macOS 13.0+** with Apple Silicon (M1/M2/M3/M4)
- **Rust 1.75+**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **MLX**: `pip install mlx` (for model operations)

### Additional Tools
- **pnpm** (for web UI): `npm install -g pnpm`
- **Node.js 20+**: For React UI development

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

# Optional: OpenAI for patch generation
OPENAI_API_KEY=sk-your-api-key-here
```

### 2. Build the Project

```bash
# Build all components
cargo build --release

# Or build specific packages
cargo build --release --package adapteros-lora-worker
cargo build --release --package adapteros-server
cargo build --release --package adapteros-cli
```

## Understanding the Database

AdapterOS uses SQLite for tenant management, adapter registry, and telemetry. For complete database schema documentation:

- [Database Schema Overview](database-schema/README.md) - Complete database structure and workflow animations
- [Schema Diagram](database-schema/schema-diagram.md) - ER diagram with 30+ tables
- [Basic Workflows](database-schema/examples/basic-workflows.md) - Common database operations and examples

The following sections show basic operations with the database.

### 3. Initialize Database & Import Model

```bash
# Initialize tenant
./target/release/aosctl init-tenant --id default --uid 1000 --gid 1000

# Import model (using included Qwen 2.5 7B)
./target/release/aosctl import-model \
  --name qwen2.5-7b \
  --weights models/qwen2.5-7b-mlx/weights.safetensors \
  --config models/qwen2.5-7b-mlx/config.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json
```

### 4. Register LoRA Adapters

```bash
# Register your LoRA adapters
./target/release/aosctl register-adapter \
  --id my-lora \
  --hash <adapter-hash> \
  --tier 1 \
  --rank 16
```

### 5. Start Server

```bash
# Build and serve a plan
./target/release/aosctl build-plan --tenant-id default --manifest configs/cp.toml
./target/release/aosctl serve --plan <plan-id>

# Or use the integrated server
./target/release/adapteros-server --config configs/cp.toml
```

## Quick Start: Web UI

### Development Mode (Hot Reload)

```bash
# Terminal 1: UI dev server
cd ui
pnpm install
pnpm dev

# Terminal 2: Backend server
cargo run --release --bin adapteros-server -- --config configs/cp.toml
```

Visit http://127.0.0.1:3200 (UI) and http://127.0.0.1:8080 (API)

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

### 2. OpenAI Patch Generation (Optional)

Generate code patches using GPT-4:

```bash
curl -X POST http://localhost:9443/api/v1/patch/propose \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "repo_id": "owner/repo",
    "commit_sha": "abc123",
    "target_files": ["src/main.rs"],
    "description": "Fix the timeout issue"
  }'
```

### 3. Metal-Optimized Kernels

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
cd ui && pnpm install && pnpm build
```

### "Connection refused"

Make sure the server is running:
```bash
cargo run --release --bin adapteros-server -- --config configs/cp.toml
```

### "OpenAI API error: Unauthorized"

Set your API key:
```bash
export OPENAI_API_KEY=sk-your-key-here
# Or add to .env file
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
- [ ] OpenAI API key stored securely (if using)
- [ ] HTTPS/TLS configured (if exposing publicly)
- [ ] Rate limiting configured (future)
- [ ] Audit logging enabled (future)

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    MPLoRA Runtime                        │
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
  - Architecture: `docs/architecture.md`
  - Code Intelligence: `docs/code-intelligence/`
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
