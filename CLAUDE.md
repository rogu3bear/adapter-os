# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

AdapterOS is a Rust-based deterministic ML inference platform for Apple Silicon. It provides K-sparse LoRA routing, Metal-optimized kernels, and policy enforcement for production environments. The system is designed for air-gapped deployments with zero network egress during serving.

## Build Commands

```bash
# Build
cargo build --release --workspace

# Build CLI and symlink
cargo build --release -p adapteros-cli --features tui
ln -sf target/release/aosctl ./aosctl

# Start dev server (port 8080)
cargo run -p adapteros-server -- --config configs/cp.toml

# Start dev server with auth disabled (env var)
AOS_DEV_NO_AUTH=1 cargo run -p adapteros-server -- --config configs/cp.toml

# Or set in config file: security.dev_bypass = true (debug builds only)

# Boot (backend + worker, serves UI from static/)
./start

# Build Metal shaders
cd metal && bash build.sh
```

## Feature Flags

Defined at workspace level in root `Cargo.toml` and propagate to crates:

```bash
# Backend flags (macOS only)
--features coreml-backend    # CoreML ANE acceleration layer
--features metal-backend     # Metal GPU kernels
--features mlx               # MLX C++ FFI (Homebrew MLX)

# Combined profiles
--features production-macos  # Full Apple Silicon stack (MLX + CoreML + Metal)
--features multi-backend     # MLX primary backend (C++ FFI)

# Testing flags
--features extended-tests    # Extended test suite
--features hardware-residency # Hardware residency integration tests
--features loom              # Concurrency testing with loom
```

Default features: `deterministic-only`, `multi-backend`, `coreml-backend`

## Testing

```bash
# Run all workspace tests
cargo test --workspace

# Single crate tests
cargo test -p adapteros-lora-router
cargo test -p adapteros-db --test atomic_dual_write_tests

# Run specific test
cargo test test_name -- --nocapture

# Run ignored tests (infrastructure/hardware dependent)
cargo test --workspace -- --ignored

# Determinism verification
cargo test --test determinism_core_suite -- --test-threads=8
cargo test -p adapteros-lora-router --test determinism

# Production gate tests (pre-deployment validation)
cargo test -p adapteros-e2e --features prod-gate
```

**Test organization**: Workspace-level integration tests are in `tests/` at the repo root. Per-crate unit tests are in each crate's `src/` or `tests/` directory.

## CLI (aosctl)

```bash
# Common commands
./aosctl db migrate              # Run database migrations
./aosctl status                  # Show system status
./aosctl chat                    # Interactive chat with streaming
./aosctl serve                   # Start serving
./aosctl doctor                  # Run system health diagnostics
./aosctl preflight               # Pre-flight readiness check
./aosctl adapter list            # List adapters
./aosctl stack list              # List adapter stacks
./aosctl policy list             # List policy packs
./aosctl explain <error-code>    # Explain an error code

# Train adapter on markdown documentation (end-to-end pipeline)
./aosctl train-docs --docs-dir ./my-docs --dry-run              # Preview what will be trained
./aosctl train-docs --docs-dir ./my-docs --register \
  --tenant-id <tenant> --base-model-id Qwen2.5-7B-Instruct      # Train and register
./aosctl train-docs --training-strategy qa --epochs 5           # Custom training params
./aosctl train-docs --docs-dir ./my-docs --resume               # Resume from checkpoint

# Model management
./aosctl models seed                           # Seed models from model directory
./aosctl models seed --model-path /path/to/models --force  # Force re-seed from custom path
./aosctl models seed --db-path ./custom.sqlite3            # Use custom database
./aosctl models list                           # List registered models
./aosctl models list --json                    # Output in JSON format
./aosctl models list --db-path ./custom.sqlite3            # Use custom database

# Tokenizer validation
./aosctl models check-tokenizer ./var/models/Qwen2.5-7B-Instruct/tokenizer.json  # Validate tokenizer

# Serve with additional options
./aosctl serve --capture-events ./events/              # Capture telemetry events to directory
./aosctl serve --insecure-skip-egress-check            # Skip PF egress preflight (dev only)

# Cancellation receipt verification
./aosctl verify-cancellation-receipt <receipt-file>    # Verify a cancellation receipt signature
```

### Stubbed/Partial CLI Commands

The following CLI paths are intentionally stubbed or only partially implemented:

- `aosctl agent spawn|worker|list|status|cancel` (orchestrator integration pending; all operations are mock/placeholder)
- `aosctl policy quarantine-clear|quarantine-rollback` (requires runtime policy manager connection; prints guidance only)
- `aosctl metrics config --key --value` (config persistence not implemented; displays placeholder message)

## UI (Leptos WASM)

Located in `crates/adapteros-ui/`. Leptos 0.7 + Pure CSS + WASM (Client-Side Rendering).

```bash
# Install trunk (WASM bundler)
cargo install trunk

# Development build
cd crates/adapteros-ui
trunk serve                    # Dev server with hot reload

# Production build (outputs to ../adapteros-server/static/)
trunk build --release

# Check WASM compilation
cargo check -p adapteros-ui --target wasm32-unknown-unknown

# Run unit tests (native, not WASM)
cargo test -p adapteros-ui --lib
```

### Leptos UI Structure

- `src/api/` - Typed API client, error handling, SSE streaming
- `src/components/` - Leptos components (Button, Card, Table, etc.)
- `src/pages/` - Route pages (Dashboard, Adapters, Chat, etc.)
- `src/hooks/` - Custom hooks (use_api_resource, use_polling, etc.)
- `src/contexts/` - Context providers (AuthProvider)
- `src/validation.rs` - Form validation rules

### Shared API Types

The Leptos UI uses `adapteros-api-types` crate with the `wasm` feature for type-safe API communication:

```toml
# In Cargo.toml
adapteros-api-types = { path = "../adapteros-api-types", features = ["wasm"] }
```

This ensures compile-time type consistency between the Rust server and WASM client.

### Build Configuration

The `Trunk.toml` configures the WASM build:
- Output directory: `../adapteros-server/static/`
- Pure CSS (migrated from Tailwind, no Node.js required)
- wasm-opt with `--enable-bulk-memory` for size optimization

### Liquid Glass Design System

See `dist/glass.css` header for full spec. Key rules:

| Tier | Usage | Blur | Alpha |
|------|-------|------|-------|
| 1 | headers, nav, inputs | 9.6px | 70% |
| 2 | cards, panels | 12px | 78% |
| 3 | dialogs, popovers | 15.6px | 85% |

- Borders required: 1px, hsla white 0.30
- Noise: 2% opacity, fractalNoise pattern
- Motion: state-change only, no idle animations

## Architecture

### Core Layer Structure
- **adapteros-core**: Shared types, error handling, seed derivation (HKDF-SHA256)
- **adapteros-lora-router**: K-sparse adapter routing with Q15 quantized gates
- **adapteros-lora-worker**: Inference engine with policy enforcement
- **adapteros-lora-kernel-mtl**: Metal GPU kernels (low-level compute)
- **adapteros-lora-kernel-coreml**: CoreML ANE acceleration layer
- **adapteros-lora-mlx-ffi**: MLX FFI backend (primary inference/training)

### Server/API Layer
- **adapteros-server**: Control plane API server (Axum-based)
- **adapteros-server-api**: REST API handlers, routes, middleware
- **adapteros-db**: SQLite with migrations, adapter registry
- **adapteros-cli**: Command-line tool (`aosctl`)

### Supporting Crates
- **adapteros-policy**: 25+ canonical policy packs
- **adapteros-telemetry**: Event logging with Merkle trees
- **adapteros-crypto**: Ed25519 signing, BLAKE3 hashing
- **adapteros-config**: Deterministic configuration with precedence

### Backend Architecture
1. **MLX** (primary): Native macOS inference and training with unified memory
2. **CoreML** (ANE layer): Provides ANE-accelerated ops that MLX calls into
3. **Metal** (kernels): Low-level GPU compute primitives used by MLX

### Intentional Complexity

The architecture's scale is deliberate, not accidental:

- **~70 crates**: Fine-grained modularity enables selective compilation for air-gapped deployments where only specific capabilities ship. Each crate has a single responsibility with explicit dependencies.
- **15+ feature flags**: Required to target different hardware backends (CoreML/Metal/MLX), deployment modes (production/development), and testing scenarios (loom/hardware-residency) without runtime overhead.
- **Dual-write patterns**: The database layer uses atomic dual-write (see `adapteros-db`) to maintain consistency guarantees required for deterministic replay and audit trails.

This complexity serves the core constraint: **deterministic, auditable inference in air-gapped environments**. Do not simplify by merging crates or removing feature gates without understanding the deployment implications.

## Training Checkpoints

Training checkpoints enable resuming interrupted training sessions. Checkpoints are saved to `{output_dir}/` and contain weights, optimizer state, and loss history.

### Checkpoint Format

Checkpoints are saved as JSON files with the following naming convention:
- Epoch checkpoints: `{adapter_id}_epoch_{N:04}.ckpt`
- Latest checkpoint: `{adapter_id}_latest.ckpt`

Each checkpoint contains:
- `epoch`: Current epoch number (0-indexed)
- `step`: Current step within epoch
- `loss`: Current loss value
- `learning_rate`: Learning rate at checkpoint
- `config`: Training configuration
- `weights`: LoRA weights (lora_a, lora_b matrices)
- `best_loss`: Best loss seen so far
- `timestamp`: ISO 8601 timestamp

### Resume Training

```bash
# Resume training from latest checkpoint
./aosctl train --resume --data ./data.json --output ./adapter-out

# Resume docs training
./aosctl train-docs --docs-dir ./my-docs --resume
```

When `--resume` is specified:
1. Checks for existing checkpoint in the output directory
2. If found, loads weights and resumes from that epoch
3. If not found, starts fresh training
4. Logs checkpoint availability and resumed epoch in output

### Configuration

Checkpointing is automatically enabled when using the CLI training commands. Default settings:
- Save frequency: Every epoch
- Max checkpoints: 5 (oldest are deleted)

## Determinism Rules

These rules ensure reproducible inference:

- Seed derivation: HKDF-SHA256 with BLAKE3 global seed (`crates/adapteros-core/src/seed.rs`)
- Router tie-breaking: score DESC, index ASC
- Q15 quantization denominator: 32767.0 (`crates/adapteros-lora-router/src/constants.rs`)
- No `-ffast-math` compiler flags
- Set `AOS_DEBUG_DETERMINISM=1` to log seed inputs and router details

## Database

```bash
# Run migrations
./aosctl db migrate

# SQLx offline mode
cargo sqlx prepare --workspace
```

Migrations are in `migrations/` with signatures tracked in `migrations/signatures.json`.

## Key Configuration

- Main config: `configs/cp.toml`
- Environment: `.env` (copy from `.env.example`)
- Rust toolchain: `rust-toolchain.toml` (stable channel)

## Tokenizer Configuration

Tokenizers are required for inference and training. The system discovers tokenizers in this order:

1. **AOS_TOKENIZER_PATH**: Explicit path to `tokenizer.json`
2. **Model directory**: `tokenizer.json` in `AOS_MODEL_PATH`

```bash
# Set explicit tokenizer path
export AOS_TOKENIZER_PATH=./var/models/Qwen2.5-7B-Instruct/tokenizer.json

# Validate a tokenizer file
./aosctl models check-tokenizer ./path/to/tokenizer.json
```

Known working tokenizers:
- Qwen2.5-7B-Instruct: `./var/models/Qwen2.5-7B-Instruct/tokenizer.json`
- Llama-3-8B: `./var/models/Llama-3-8B/tokenizer.json`

## Code Quality

```bash
cargo fmt --all           # Format
cargo fmt --all --check   # Check formatting
cargo clippy --workspace -- -D warnings
```

## API Endpoints

- Liveness: `/healthz`
- Readiness: `/readyz`
- System status: `/system/ready`
- Swagger UI: available when server running

## Key Directories

- `crates/`: All Rust workspace crates (~70 crates)
  - `crates/adapteros-ui/`: Leptos WASM frontend
  - `crates/adapteros-api-types/`: Shared API types (server + WASM)
  - `crates/adapteros-server/`: Control plane server (serves UI from `static/`)
- `migrations/`: SQLite migrations
- `metal/`: Metal shader sources
- `configs/`: Runtime configuration
- `tests/`: Workspace-level integration tests
- `scripts/`: Build and utility scripts
- `docs/`: Architecture documentation

## Human-in-the-Loop Review

AdapterOS supports a review workflow where the system surfaces items needing human review (inference pauses, dataset safety gates, promotion approvals). External reviewers—including AI assistants like Claude Code—can provide structured feedback.

See `docs/REVIEW_WORKFLOW.md` for the full architecture.

Key entry points:
- **Review types**: `crates/adapteros-api-types/src/review.rs`
- **Promotion workflow**: `crates/adapteros-db/src/promotions.rs`
- **Quarantine system**: `crates/adapteros-policy/src/quarantine.rs`
