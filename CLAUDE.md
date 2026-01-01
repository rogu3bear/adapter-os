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

# Unified boot (backend + UI with health waits)
./start

# Build Metal shaders
cd metal && bash build.sh
```

## Feature Flags

Defined at workspace level in root `Cargo.toml` and propagate to crates:

```bash
# Backend flags (macOS only)
--features coreml-backend    # CoreML + ANE (production primary)
--features metal-backend     # Metal GPU (fallback)
--features mlx-backend       # MLX FFI stubs
--features mlx               # Real MLX library (requires C++ FFI)

# Combined profiles
--features production-macos  # CoreML + Metal + MLX (full production)
--features multi-backend     # MLX development backends

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
```

## UI (React Frontend)

Located in `ui/`. React 18 + Vite + Tailwind v4 + TypeScript (strict mode).

```bash
cd ui
pnpm install          # Install dependencies
pnpm dev              # Dev server (port 3200, proxies /api to :8080)
pnpm build            # Production build → ../crates/adapteros-server/static/
pnpm test             # Vitest unit tests
pnpm test:e2e         # Cypress E2E tests
pnpm lint             # ESLint
pnpm format           # Biome formatter
```

### API Type Generation

Types are generated from OpenAPI spec to ensure frontend/backend sync:

```bash
pnpm run gen:types    # Generate from OpenAPI → src/api/generated.ts
pnpm run check:drift  # CI check: fails if generated types differ
```

The generation flow:
1. `cargo run -p adapteros-server-api --bin export-openapi` exports OpenAPI spec
2. `openapi-typescript` generates TypeScript types to `src/api/generated.ts`

### UI Structure

- `src/api/` - API client, generated types, domain-specific type files
- `src/api/services/` - Domain-specific API methods (adapters, chat, training, etc.)
- `src/components/` - React components (uses Radix UI primitives)
- `src/hooks/` - Custom React hooks
- `src/pages/` - Route pages
- `src/contexts/` - React contexts
- `src/stores/` - State stores

### Build Modes

```bash
pnpm dev              # Default: full UI
pnpm dev:demo         # Demo mode
VITE_BUILD_MODE=minimal pnpm dev    # Minimal build
VITE_BUILD_MODE=service-panel pnpm dev  # Service panel (port 3300)
```

## Architecture

### Core Layer Structure
- **adapteros-core**: Shared types, error handling, seed derivation (HKDF-SHA256)
- **adapteros-lora-router**: K-sparse adapter routing with Q15 quantized gates
- **adapteros-lora-worker**: Inference engine with policy enforcement
- **adapteros-lora-kernel-mtl**: Metal GPU kernels
- **adapteros-lora-kernel-coreml**: CoreML/ANE acceleration (primary backend)
- **adapteros-lora-mlx-ffi**: MLX FFI backend (development/training)

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

### Backend Priority
1. **CoreML** (primary): ANE acceleration for production
2. **MLX** (secondary): GPU inference and training
3. **Metal** (fallback): Legacy/incomplete

### Intentional Complexity

The architecture's scale is deliberate, not accidental:

- **~70 crates**: Fine-grained modularity enables selective compilation for air-gapped deployments where only specific capabilities ship. Each crate has a single responsibility with explicit dependencies.
- **15+ feature flags**: Required to target different hardware backends (CoreML/Metal/MLX), deployment modes (production/development), and testing scenarios (loom/hardware-residency) without runtime overhead.
- **Dual-write patterns**: The database layer uses atomic dual-write (see `adapteros-db`) to maintain consistency guarantees required for deterministic replay and audit trails.

This complexity serves the core constraint: **deterministic, auditable inference in air-gapped environments**. Do not simplify by merging crates or removing feature gates without understanding the deployment implications.

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
- `ui/`: React frontend
- `migrations/`: SQLite migrations (signatures in `signatures.json`)
- `metal/`: Metal shader sources
- `configs/`: Runtime configuration
- `tests/`: Workspace-level integration tests
- `scripts/`: Build, test, and utility scripts
