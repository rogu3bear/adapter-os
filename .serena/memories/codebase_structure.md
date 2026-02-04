# AdapterOS Codebase Structure

## Top-Level Directories

```
adapter-os/
├── crates/              # 83 Rust workspace crates
├── migrations/          # SQLite migrations + signatures.json
├── metal/               # Metal shader sources
├── configs/             # Runtime configuration (cp.toml)
├── tests/               # Workspace-level integration tests
├── scripts/             # Build and utility scripts
├── docs/                # Architecture documentation
└── var/                 # Runtime data (gitignored)
```

## Core Crates

| Crate | Purpose |
|-------|---------|
| `adapteros-core` | Shared types, error handling, seed derivation (HKDF-SHA256) |
| `adapteros-lora-router` | K-sparse adapter routing with Q15 quantized gates |
| `adapteros-lora-worker` | Inference engine with policy enforcement |
| `adapteros-lora-kernel-mtl` | Metal GPU kernels (low-level compute) |
| `adapteros-lora-kernel-coreml` | CoreML ANE acceleration layer |
| `adapteros-lora-mlx-ffi` | MLX FFI backend (primary inference/training) |

## Server/API Crates

| Crate | Purpose |
|-------|---------|
| `adapteros-server` | Control plane API server (Axum), serves UI from static/ |
| `adapteros-server-api` | REST handlers, routes, middleware, AppState |
| `adapteros-db` | SQLite with migrations, adapter registry |
| `adapteros-cli` | Command-line tool (aosctl) |

## Supporting Crates

| Crate | Purpose |
|-------|---------|
| `adapteros-policy` | 31 canonical policy packs |
| `adapteros-telemetry` | Event logging with Merkle trees |
| `adapteros-crypto` | Ed25519 signing, BLAKE3 hashing |
| `adapteros-config` | Deterministic configuration with precedence |
| `adapteros-api-types` | Shared API types (server + WASM) |

## UI Crate Structure (`crates/adapteros-ui/`)

```
src/
├── api/           # Typed API client, error handling, SSE streaming
├── components/    # Leptos components (Button, Card, Table, etc.)
├── pages/         # Route pages (Dashboard, Adapters, Chat, etc.)
├── hooks/         # Custom hooks (use_api_resource, use_polling, etc.)
├── contexts/      # Context providers (AuthProvider)
└── validation.rs  # Form validation rules
```

## var/ Directory (Runtime Data)

```
var/
├── aos-cp.sqlite3      # Control plane database (REQUIRED)
├── adapters/           # Trained LoRA adapters
├── models/             # Base model files (~16 GB)
├── keys/               # Signing keys (sensitive)
├── logs/               # Application logs (rotated)
├── run/                # Runtime sockets and status
├── telemetry/          # Telemetry events
└── ...                 # See CLAUDE.md for full list
```

## Backend Hierarchy

1. **MLX** (primary): Native macOS inference and training with unified memory
2. **CoreML** (ANE layer): Provides ANE-accelerated ops that MLX calls into
3. **Metal** (kernels): Low-level GPU compute primitives used by MLX

## Why 83 Crates?

Fine-grained modularity enables:
- Selective compilation for air-gapped deployments
- Single responsibility per crate
- Explicit dependencies
- Minimal runtime overhead via feature flags
