# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

AdapterOS is an on-device LoRA adapter management system for Apple Silicon. It provides inference, training, routing, and lifecycle management for LoRA adapters via a control plane (Axum API server), worker processes (MLX/CoreML/Metal backends), a Leptos 0.7 WASM UI, and a CLI.

## Build & Test Commands

```bash
# Quick check (type-check only, fastest)
cargo c                              # alias for cargo check --workspace

# Build
cargo build --workspace              # debug build
cargo build --workspace --release    # release build (LTO, stripped)
cargo build --profile release-dev    # fast iteration with thin LTO

# Format & lint
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo cl                             # clippy with error-visibility lints

# Test single crate
cargo test -p <crate-name>

# Test server-api (serial - state isolation required)
cargo test -p adapteros-server-api -- --test-threads=1

# Determinism suite
cargo test --test determinism_core_suite

# Router determinism
cargo test -p adapteros-lora-router --test determinism

# UI tests (native only; WASM tests require wasm-pack)
cargo test -p adapteros-ui --lib

# Nextest (parallel test runner, install: cargo install cargo-nextest)
cargo nt                             # alias for nextest run --workspace

# WASM UI build (flock-protected, prevents concurrent builds)
./scripts/ui-check.sh
./scripts/build-ui.sh                # full WASM build → crates/adapteros-server/static/

# MVP smoke test (fmt + clippy + server-api tests + UI build + endpoint smoke)
bash scripts/mvp_smoke.sh

# Local CI (required checks before merge)
bash scripts/ci/local_required_checks.sh
LOCAL_REQUIRED_PROFILE=prod bash scripts/ci/local_required_checks.sh  # all-targets + prod tests

# Release gate
bash scripts/ci/local_release_gate.sh
```

## Running the System

```bash
AOS_DEV_NO_AUTH=1 ./start            # full stack: backend + worker + UI + model seeding (auth bypassed)
./scripts/service-manager.sh status  # check running services
./aosctl                             # CLI (auto-rebuilds)
```

## Architecture

```
Clients (UI @ WASM / CLI) ──→ Control Plane (Axum, :18080) ──→ SQLite (var/aos-cp.sqlite3)
                                       │
                                       ↓
                              Worker (UDS: var/run/aos/<tenant>/worker.sock)
                                       │
                                       ↓
                              Backends: MLX FFI (primary) / CoreML (ANE) / Metal
```

### Crate Layers

85 crates total. Key crates by layer (see `crates/` for full listing):

| Layer | Key Crates | Purpose |
|-------|-----------|---------|
| **Server** | `adapteros-server`, `-boot`, `-config` | Boot orchestration (phases 2-12 with sub-phases), config precedence, logging, shutdown |
| **API** | `adapteros-server-api`, `-api-health`, `-api-training`, `-api-inference`, `-api-audit`, `-api-admin`, `-api-models` | Axum handlers, middleware chain, route registration |
| **Database** | `adapteros-db` | SQLite with sqlx 0.8, migrations, atomic dual-write, audit log |
| **Inference** | `adapteros-lora-mlx-ffi`, `-lora-kernel-coreml`, `-lora-kernel-mtl`, `-lora-kernel-api`, `-lora-kernel-prof` | MLX C++ FFI (primary), CoreML ANE, Metal GPU, kernel API contract, profiling |
| **Routing** | `adapteros-lora-router` | K-sparse LoRA routing with Q15 quantization, deterministic scoring |
| **Training** | `adapteros-lora-worker`, `adapteros-training-worker`, `-orchestrator` | Worker process over UDS, training pipeline, orchestration |
| **Core** | `adapteros-core`, `-types`, `-transport-types`, `-api-types`, `-id`, `-inference-contract` | Shared types, error codes, seed derivation, UDS transport contract |
| **Config** | `adapteros-config` | Deterministic configuration with precedence enforcement |
| **Auth/Policy** | `adapteros-auth`, `adapteros-policy` | JWT/API-key auth, RBAC policy packs |
| **Crypto** | `adapteros-crypto` | Ed25519 + BLAKE3 receipts, envelope encryption |
| **Domain** | `adapteros-domain`, `-chat`, `-embeddings`, `-retrieval` | Domain adapter layer, chat, embeddings, RAG retrieval |
| **Model** | `adapteros-model-hub`, `-model-server` | Model hub, shared model server (reduces GPU memory) |
| **Storage** | `adapteros-storage`, `-single-file-adapter`, `-aos` | Adapter storage, .aos format, archive format |
| **Ops** | `adapteros-diagnostics`, `-system-metrics`, `-memory`, `-service-supervisor`, `-node` | Diagnostics, system monitoring, UMA watchdog, process management, node control |
| **Determinism** | `adapteros-deterministic-exec`, `-lint`, `-numerics` | Deterministic executor, runtime guards, numerical stability tracking |
| **Errors** | `adapteros-error-registry`, `-error-recovery` | Unified error registry, corruption detection/recovery |
| **UI** | `adapteros-ui`, `-tui` | Leptos 0.7 CSR WASM frontend (Liquid Glass design), terminal UI |
| **CLI** | `adapteros-cli` | clap 4.4 derive, 120+ commands |
| **Telemetry** | `adapteros-telemetry`, `-telemetry-types`, `-telemetry-verifier`, `-metrics-exporter`, `-trace` | OpenTelemetry, distributed tracing, metrics export |
| **Release** | `adapteros-sbom`, `-verify`, `-manifest` | SBOM generation, verification, manifest management |
| **Testing** | `adapteros-testing`, `-e2e`, `-scenarios`, `-replay` | Test harness, E2E tests, scenario playback, replay |
| **Platform** | `adapteros-platform`, `-infra-common`, `-federation` | Cross-platform filesystem, infrastructure common, federation |
| **Tools** | `sign-migrations`, `adapteros-profiler`, `-graph`, `-git` | Migration signing, profiling, tensor graph, git operations |

### Middleware Chain (type-state enforced)

**Global (all routes):** api_prefix_compat → drain → request_tracking → client_ip → seed_isolation → trace_context → versioning → security_headers → request_size_limit → rate_limiting → cors → idempotency → ErrorCodeEnforcement

**Protected route group (innermost):** auth → tenant_guard → csrf → context → policy → audit

### API Route Tiers

- `health`: /healthz, /readyz, /version (no middleware)
- `public`: /v1/auth/login, /v1/status, /metrics
- `internal`: /v1/workers/register, /v1/workers/heartbeat (worker UID)
- `protected`: everything else (full middleware chain)

## Key Patterns

### Error Handling
- `Result<T, AosError>` everywhere; canonical codes in `crates/adapteros-core/src/error_codes.rs`
- Normalization: `crates/adapteros-server-api/src/error_code_normalization.rs`
- Never `unwrap()` in library code; propagate with `?`
- Never swallow results with `let _ =` without a comment justifying fire-and-forget
- Use `tracing` macros, never `println!`

### Determinism
- Seed: HKDF-SHA256 with BLAKE3 global seed (`adapteros-core/src/seed.rs`)
- Router tie-break: score DESC, stable_id ASC; Q15 denominator = 32767.0
- No `-ffast-math` (CI scans). Debug: `AOS_DEBUG_DETERMINISM=1`
- No `HashMap` iteration in hot paths (use `BTreeMap` or sorted `Vec`)

### Database
- SQLite at `var/aos-cp.sqlite3`; sqlx 0.8 with `SQLX_OFFLINE=1` for CI
- Migrations in `migrations/` (top-level, 333 files), signed via `scripts/sign_migrations.sh`
- Test isolation: `TestDb::new()` creates temp DB with auto-cleanup
- Dual-write pattern: primary + audit log atomically

### SSE Event Contract
- Server events in `streaming_infer.rs` and UI `InferenceEvent` in `sse.rs` / `signals/chat.rs` must stay in sync
- New UI event fields: `#[serde(default)]` for backward compatibility

## UI (Leptos 0.7 WASM)

- Target: `wasm32-unknown-unknown`, CSR only
- CSS: pure CSS, no Tailwind. Liquid Glass design system with 3 tiers (blur 9.6/12/15.6px)
- No `println!` (use `leptos::logging::log!`), no `std::time` (use `gloo_timers`)
- Use `use_api_resource` hook for data fetching (custom wrapper in `src/hooks/mod.rs`), not `spawn_local`
- Signal disposal: always use `try_get()` / `try_set()` in reactive contexts, never `.get()` / `.set()` (signals dispose on component unmount)
- Use `<Show>` instead of `{move || if ...}` for conditional rendering (avoids teardown/rebuild)

## MLX FFI

- Requires MLX headers: `brew install ml-explore/mlx/mlx`
- Two implementations: `mlx_cpp_wrapper_real.cpp` (real) and `mlx_cpp_wrapper.cpp` (stub/CI). Feature flag `mlx` controls linkage
- Unified memory on Apple Silicon; no explicit GPU transfers

## Runtime Paths (var/, gitignored)

- Database: `var/aos-cp.sqlite3`
- Worker socket: `var/run/aos/<tenant>/worker.sock`
- Logs: `var/logs/`
- Models: `var/models/`
- PIDs: `var/backend.pid`, `var/worker.pid`
- Never use `/tmp` or create `var/` inside crates

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `AOS_DEV_NO_AUTH=1` | Bypass auth for development |
| `AOS_MODEL_PATH` | Path to model weights |
| `AOS_MODEL_BACKEND` | `mlx\|coreml\|metal\|auto` |
| `AOS_SERVER_PORT` | API server port (default: 18080) |
| `AOS_WORKER_UID` | Expected worker UID for UCred validation |
| `AOS_DEBUG_DETERMINISM=1` | Log seed/router details |
| `AOS_VAR_DIR` | Runtime data directory (default: `var`) |
| `AOS_LOG_PROFILE` | Log output format (`json\|pretty`) |
| `AOS_CONFIG` | Path to config TOML override |
| `AOS_QUICK_BOOT` | Skip non-essential startup phases |
| `SQLX_OFFLINE=1` | Offline sqlx for CI builds |
| `DATABASE_URL` | SQLite connection string |

## Feature Flags

```bash
cargo build                                    # default: MLX + CoreML
cargo build --features production-macos        # explicit full Apple Silicon stack
cargo build --features extended-tests          # intensive test suite
cargo build --features hardware-residency      # hardware integration tests
```

## Contract Checks

```bash
bash scripts/contracts/check_all.sh                           # all contract checks
bash scripts/check_error_code_drift.sh                        # error code consistency
bash scripts/contracts/check_determinism_contract.sh           # determinism invariants
bash scripts/contracts/check_startup_contract.sh               # startup negative paths
bash scripts/contracts/check_release_security_assertions.sh    # security posture
```
