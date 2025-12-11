# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is AdapterOS?

AdapterOS is an ML inference platform powered by **LORAX (Low Rank Adapter Exchange)** — an offline-capable, UMA-optimized orchestration layer for multi-LoRA systems on Apple Silicon. Key components:

- **Control Plane** (`adapteros-server`): HTTP API (port 8080) with SQLite, JWT auth, policy enforcement
- **Worker Processes** (`aos-worker`): LoRA inference/training over Unix Domain Sockets (UDS)
- **K-Sparse Router**: Multi-adapter mixing with Q15 quantization
- **Multi-Backend**: CoreML/ANE (primary), Metal, MLX

**Characteristics**: Single-node, multi-tenant, zero network egress during serving, deterministic replay, hot-swap adapters.

---

## Build & Development Commands

```bash
# Development
make dev                    # Start control plane (port 8080), NO_AUTH=1 disables auth
make dev-no-auth            # Start with auth disabled (debug builds only)
make ui-dev                 # Start UI dev server (port 3200)
make cli                    # Build CLI with TUI, symlink to ./aosctl
./start                     # Canonical boot: backend + UI via service-manager.sh

# Building
make build                  # Fresh build with cleanup (stops services, cleans ports)
make metal                  # Build Metal shaders
cargo build --release       # Standard Rust build

# Testing
cargo test -p <crate>                    # Single crate
cargo test --workspace                   # All tests (excludes MLX)
cargo test -- --test-threads=1           # Sequential (for debugging)
cargo test -- --nocapture                # With output
make determinism-check                   # Determinism test suite
make test                                # All tests + Miri for worker

# Code Quality
cargo fmt --all
cargo clippy --workspace
make check                               # fmt + clippy + test + determinism-check

# Database
cargo sqlx migrate run                   # Apply migrations
./aosctl db migrate                      # Via CLI

# UI (in ui/ directory)
pnpm dev                                 # Dev server
pnpm build                               # Production build
pnpm test                                # Vitest tests

# Worker startup e2e
make e2e-worker-test                     # MLX Qwen 4-bit defaults from .env/.env.local; auto adds multi-backend when backend=mlx
```

---

## Dev Authentication (Debug Builds Only)

Frontend runs on 3200.
Backend runs on 8080.


# Option 2: No-Auth Bypass
AOS_DEV_NO_AUTH=1 cargo run --bin adapteros-server

# Option 3: Custom JWT Secret
AOS_DEV_JWT_SECRET="my-test-secret" cargo run --bin adapteros-server
```

---

## Critical Invariants (DO NOT BREAK)

| Invariant | Location | Notes |
|-----------|----------|-------|
| All inference routes through `InferenceCore` | `inference_core.rs` | Bypassing breaks auditability |
| Q15 denominator is 32767.0 | `lora-router/src/lib.rs` | NOT 32768 - precision-critical |
| `tenant_id` in all queries | All handlers | FK triggers in migration 0131 |
| No `-ffast-math` compiler flags | Cargo.toml | Breaks determinism |
| FK constraints enabled | `db/lib.rs` | `foreign_keys=true` required |

---

## Repository Structure

### Binaries
- `adapteros-server` — Control plane HTTP API (`crates/adapteros-server/`)
- `aos-worker` — Inference worker (`crates/adapteros-lora-worker/src/bin/aos_worker.rs`)
- `aosctl` — CLI tool (`crates/adapteros-cli/`)

### Key Crates
| Layer | Crates |
|-------|--------|
| Control Plane | `adapteros-server`, `adapteros-server-api`, `adapteros-orchestrator` |
| Worker/Data | `adapteros-lora-worker`, `adapteros-lora-router`, `adapteros-lora-kernel-mtl`, `adapteros-lora-kernel-coreml` |
| Core | `adapteros-core` (domain types, BLAKE3, HKDF), `adapteros-db` (SQLite, 160+ migrations), `adapteros-config`, `adapteros-policy` |
| UI | `ui/` — React 18 + TypeScript + Vite + TanStack Query |

---

## Key Workflows

### Inference Flow
```
HTTP Request → InferenceCore → Worker Selection → UDS Request
                    ↓
              Policy Hooks (OnBeforeInference)
                    ↓
              Router Decision (K-sparse, Q15 gates)
                    ↓
              Kernel Execution (CoreML/Metal/MLX)
                    ↓
              Response + Evidence + Replay Metadata
```

### Training Flow
```
API Request → TrainingService → Orchestrator → Worker (MicroLoRATrainer)
                                                    ↓
                                              Adapter Packaging (Q15)
                                                    ↓
                                              Registry → Stack Creation
```

---

## Determinism & Replay

- **Seed derivation**: HKDF-SHA256 with BLAKE3 global seed (`crates/adapteros-core/src/seed.rs`)
- **Router determinism**: Sorted by score DESC, then index ASC for tie-breaking
- **Q15 quantization**: `gate_f32 = q15 / 32767.0`
- **Replay metadata stored**: `manifest_hash`, `router_seed`, `sampling_params_json`, `rag_snapshot_hash`, `adapter_ids_json`

**Note**: `router_seed` is for AUDIT only — routing is deterministic by algorithm, not RNG.

---

## Tenant Isolation

**Handler-Level**: `validate_tenant_isolation(claims, resource_tenant_id)` in `security/mod.rs`
- Same tenant: always allowed
- Admin cross-tenant: requires `admin_tenants` claim
- Wildcard `"*"`: dev mode only, grants all-tenant access

**Database-Level**: Composite FKs and 15+ triggers prevent cross-tenant references (migration 0131)

---

## Policy & Audit

**24 Policy Packs** — Core (always enabled): Egress, Determinism, Isolation, Evidence

**Hook Points**: `OnRequestBeforeRouting`, `OnBeforeInference`, `OnAfterInference`

**Merkle Chain Audit**: BLAKE3 hash chaining with Ed25519 signatures (`crates/adapteros-db/src/policy_audit.rs`)

---

## Critical File References

| Area | File | Key Functions |
|------|------|---------------|
| Inference | `server-api/src/inference_core.rs` | `route_and_infer`, `route_and_infer_replay` |
| Router | `lora-router/src/lib.rs` | `Router`, `route_with_adapter_info`, `Decision` |
| Chat context | `server-api/src/chat_context.rs` | `build_chat_prompt` (multi-turn) |
| Tenant isolation | `server-api/src/security/mod.rs` | `validate_tenant_isolation` |
| Seed derivation | `adapteros-core/src/seed.rs` | `derive_seed`, `derive_seed_typed` |
| Backend factory | `lora-worker/src/backend_factory.rs` | `create_backend_with_model` |

---

## Common Misconceptions

### Naming
- Handlers use `infer()`, NOT `handle_infer()`
- `GlobalTickLedger`, NOT `GlobalLedger`
- No `StackService` — use `LifecycleManager::activate_stack()`

### Architecture
- Chat is **multi-turn**, not stateless (`build_chat_prompt` retrieves full history)
- Replay goes through `InferenceCore`, NOT around it
- Workers have zero network egress (UDS only)

---

## Environment

- **Platform**: macOS with Apple Silicon (M1/M2/M3/M4)
- **Database**: SQLite at `var/aos-cp.sqlite3`
- **Default Model**: Qwen2.5-7B
- **Rust**: Nightly (see `rust-toolchain.toml`)
- **UI**: pnpm 9+, React 18

---

## Style Conventions

```rust
// Error handling: thiserror + Result alias
use thiserror::Error;
pub type Result<T> = std::result::Result<T, AosError>;

// Logging: tracing, not println!
use tracing::{info, warn, error};
info!(request_id = %id, latency_ms = ms, "message");
```

Feature flags affect backends — check `Cargo.toml` features before assuming capabilities:
- `default = ["deterministic-only", "coreml-backend"]`
- `mlx-backend`, `metal-backend` for alternatives

---

## Troubleshooting

### Determinism Issues
1. Check seed derivation (same inputs → same seeds)
2. Verify router sorting (score DESC, index ASC tie-break)
3. Confirm Q15 denominator = 32767.0
4. Run `make determinism-check`

### Build Issues
1. `cargo clean && cargo build`
2. Check feature flags
3. `cargo sqlx prepare` for offline mode
4. Verify migration signatures: `migrations/signatures.json`

### Port Conflicts
```bash
make prepare                 # Stops services, cleans ports
lsof -ti:8080 | xargs kill   # Manual port cleanup
```

### Env loading
- `.envrc` auto-exports `.env` + `.env.local` (run `direnv allow` once). Without direnv: `set -a; source .env; source .env.local; set +a`.

---

MLNavigator Inc 2025-12-09
