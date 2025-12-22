# AGENTS.md

Guidance for coding agents (ChatGPT/Copilot/etc.) working in this repository. Follow these guardrails to avoid hallucinations, preserve determinism, and respect tenant boundaries.

## What is AdapterOS?

AdapterOS is an ML inference platform with an offline-capable, UMA-optimized orchestration layer for multi-LoRA systems on Apple Silicon. Key components:
- **Control Plane** (`adapteros-server`): HTTP API (port 8080) with SQLite, JWT auth, policy enforcement
- **Worker Processes** (`aos-worker`): LoRA inference/training over Unix Domain Sockets (UDS)
- **K-Sparse Router**: Multi-adapter mixing with Q15 quantization
- **Multi-Backend**: CoreML/ANE (primary), Metal, MLX

**Characteristics**: Single-node, multi-tenant, zero network egress during serving, deterministic replay, hot-swap adapters.

### Technology Names
- **DIR (Deterministic Inference Runtime)**: Core inference engine
- **TAS (Token Artifact System)**: Artifact system for reusable inference outputs
- **AdapterOS**: Full platform (user-facing name)

## Build & Development Commands

```bash
# Development
make dev                    # Start control plane (port 8080), NO_AUTH=1 disables auth
make dev-no-auth            # Start with auth disabled (debug builds only)
make ui-dev                 # Start UI dev server (port 3200)
make cli                    # Build CLI with TUI, symlink to ./aosctl
./start                     # Canonical boot: backend + UI via service-manager.sh

# Quick Setup
make mvp-demo               # One-command MVP demo setup (deps, model, build, db, demo data)
make download-model         # Download Qwen 2.5 7B Instruct model (~3.8GB)
make check-system           # Check system readiness before launch (preflight checks)

# Building
make build                  # Fresh build with cleanup (stops services, cleans ports)
make prepare                # Prepare build environment (stop services, clean ports)
make metal                  # Build Metal shaders
cargo build --release       # Standard Rust build

# Testing
cargo test -p <crate>                    # Single crate
cargo test --workspace                   # All tests (excludes MLX)
cargo test -- --test-threads=1           # Sequential (for debugging)
cargo test -- --nocapture                # With output
make test                                # All tests + Miri for worker
make test-rust                           # Rust unit/integration tests only
make test-ui                             # UI tests only
make test-ignored                        # Run ignored tests (non-blocking suite)
make test-hw                             # Hardware-dependent tests (macOS Metal GPU)
make determinism-check                   # Determinism test suite

# Code Quality
cargo fmt --all
cargo clippy --workspace
make check                               # fmt + clippy + test + determinism-check
make fmt                                 # Format code
make fmt-check                           # Check formatting
make clippy                              # Run clippy
make lint-fix                            # Auto-fix common linting issues
make infra-check                         # Infrastructure health checks
make security-audit                      # Security audit (vulnerabilities, licenses, SBOM)
make license-check                       # License compliance check
make dup                                 # Check for code duplication

# Database
cargo sqlx migrate run                   # Apply migrations
./aosctl db migrate                      # Via CLI
make kv-verify                           # SQL↔KV drift verification

# UI (in ui/ directory)
pnpm dev                                 # Dev server
pnpm build                               # Production build
pnpm test                                # Vitest tests

# Apps & Tools
make installer                           # Build SwiftUI macOS installer
make menu-bar                            # Build menu bar status monitor
make menu-bar-dev                        # Build and run menu bar app (debug)
make menu-bar-install                    # Install menu bar app to /usr/local/bin
make codegraph-viewer                    # Build CodeGraph Viewer (Tauri desktop app)
make codegraph-viewer-dev                # Start CodeGraph Viewer in dev mode

# API & SDKs
make openapi-docs                        # Generate OpenAPI documentation
make validate-openapi                     # Validate OpenAPI documentation
make gen-types                           # Generate TypeScript types from OpenAPI spec
make gen-sdk-python                      # Generate Python SDK from OpenAPI spec
make gen-sdks                            # Generate all SDKs (TypeScript + Python)
make check-types-drift                   # Check if TypeScript types are in sync

# Artifacts & Verification
make sbom                                # Generate SBOM
make determinism-report                  # Generate determinism report
make verify-artifacts                    # Verify and sign artifacts
make ignored-tests-audit                 # Audit ignored tests
make ignored-tests-check                 # Strict ignored tests check

# Docker & Infrastructure
make docker-build                        # Build production Docker image
make docker-dev                          # Start development environment
make docker-test                         # Start test environment
make docker-monitoring                   # Start monitoring stack
make docker-down                         # Stop all docker services
make docker-clean                        # Remove all Docker images and volumes

# Terraform & Deployment
make terraform-init                      # Initialize Terraform
make terraform-plan                      # Plan Terraform changes
make terraform-apply                     # Apply Terraform changes
make deploy-staging                      # Deploy to staging
make deploy-prod                         # Deploy to production (requires approval)

# Worker & MLX
make e2e-worker-test                     # Worker startup lifecycle test
make verify-mlx-env                      # Verify MLX environment
make build-mlx                           # Build with real MLX backend
make bench-mlx                           # Run MLX benchmarks
make verify-mlx-metal                    # Verify MLX + Metal access

# Setup & Maintenance
make setup-git-hooks                     # Setup git hooks for code quality
make stability-check                     # Must-pass stabilization gate
make stability-ci                        # Feature matrix build
```

## Dev Authentication (Debug Builds Only)

Frontend runs on 3200. Backend runs on 8080.

```bash
# No-auth bypass (debug only)
AOS_DEV_NO_AUTH=1 cargo run --bin adapteros-server

# Custom JWT secret
AOS_DEV_JWT_SECRET="my-test-secret" cargo run --bin adapteros-server
```

## Critical Invariants (Current Status)

| Invariant | Location | Status | Notes |
|-----------|----------|--------|-------|
| All inference routes through `InferenceCore` | `crates/adapteros-server-api/src/inference_core.rs` | ✅ IMPLEMENTED | Keep audit trail intact for live + replay |
| Q15 denominator is 32767.0 | `crates/adapteros-lora-router/src/constants.rs` | ✅ IMPLEMENTED | Precision-critical; do not change to 32768 |
| `tenant_id` required in adapter/base-model queries | `crates/adapteros-server-api/src/security/mod.rs`, `crates/adapteros-db/src/adapters.rs` | ✅ IMPLEMENTED | Handler validation exists; tenant-scoped methods (`list_adapters_for_tenant`) enforce isolation; old `list_adapters()` deprecated (PRD-RECT-001/004) |
| No `-ffast-math` compiler flags | `Cargo.toml` | ✅ IMPLEMENTED | Determinism would break |
| FK constraints enabled | `crates/adapteros-db/src/lib.rs` | ✅ IMPLEMENTED | `foreign_keys=true` |
| Runtime paths reject `/tmp` and `/private/tmp` | `crates/adapteros-config/src/path_resolver.rs` | ✅ IMPLEMENTED | Telemetry, manifest-cache, adapters, database, index-root, model-cache, status + dataset/document roots; unit tests cover `/tmp` rejection |
| Model cache budget required (no panics) | `crates/adapteros-lora-worker/src/backend_factory.rs` | ✅ IMPLEMENTED | `get_model_cache()` returns Result; `validate_model_cache_budget()` fails fast on missing/zero budget |
| GQA config validation is fatal | `crates/adapteros-lora-worker/src/backend_factory.rs` | ✅ IMPLEMENTED | `load_and_validate_model_config()` rejects num_kv_heads > num_heads |
| Sharded model completeness checked | `crates/adapteros-lora-worker/src/backend_factory.rs` | ✅ IMPLEMENTED | `detect_sharded_model()` errors on missing shards |
| Policy packs registry = 25 | `crates/adapteros-policy/src/registry.rs` | ✅ IMPLEMENTED | `PolicyId::all()` enumerates 25 packs incl. MPLoRA and Circuit Breaker |

### Active Gaps to Watch
- Backend cache eviction predictability/observability not yet validated (PRD-RECT-003).
- Worker lifecycle tenant scoping and storage/telemetry validation pending (PRD-RECT-002).

## Multi-Agent Guardrails

**Pre-write checks**
- Read files before editing; never assume contents.
- Verify symbols exist before use (`rg "fn name" crates/`, `rg "struct Type" crates/`).
- Check for local edits that may conflict (`git diff --name-only`).

**Post-write checks**
- Run `cargo check -p <crate>` for touched crates.
- Run crate/unit tests for logic changes (`cargo test -p <crate>`).
- Run clippy on touched crates (`cargo clippy -p <crate> -- -D warnings`).

**Rectification PRD boundaries** (see `docs/prds/rectification/README.md`):
- PRD-RECT-001: Tenant isolation for adapter lifecycle — `crates/adapteros-db/src/adapters.rs`, selected server-api files, new tests only.
- PRD-RECT-002: Worker lifecycle scoping — `crates/adapteros-db/src/workers.rs`, `crates/adapteros-server-api/src/handlers/workers.rs`, new tests.
- PRD-RECT-003: Backend cache eviction/metrics — worker cache/key/metrics files, new tests.
- PRD-RECT-004: Tenant DB trigger revalidation — migrations + DB tests only.
- PRD-RECT-005: Model loading integrity — worker backend factory/cache/worker bin.

## Policy Studio (Tenant-Safe Policy Customizations)

- Tenants author policy overrides stored in `tenant_policy_customizations` with history in `tenant_policy_customization_history`.
- Status pipeline: `draft → pending_review → approved/rejected → active`; activation deactivates prior active entry per tenant/policy type.
- Validation uses `adapteros-policy/src/validation.rs` against canonical bounds for egress/determinism/router/evidence/refusal/numeric/rag/isolation/telemetry/retention/performance/memory/artifacts/secrets/build_release/compliance/incident/output/adapters.
- Review workflows surface in UI pages `Security/PolicyStudio` and `PolicyReviewQueue`; hooks in `ui/src/hooks/security/useTenantPolicies.ts`.
- Tenant scoping enforced in DB queries (`list_tenant_customizations`, `get_active_customization`); review queue lists pending items across tenants for approvers.

## Determinism & Replay

- Seed derivation: HKDF-SHA256 with BLAKE3 global seed (`crates/adapteros-core/src/seed.rs`).
- Router determinism: score DESC, index ASC tie-break; Q15 gates use 32767.0 denominator.
- Replay metadata stored: `manifest_hash`, `router_seed`, `sampling_params_json`, `rag_snapshot_hash`, `adapter_ids_json`.
- `router_seed` is audit-only; routing is deterministic without RNG.

## Tenant Isolation

- Handler-level: `validate_tenant_isolation(claims, resource_tenant_id)` in `server-api/src/security/mod.rs` (admin cross-tenant via `admin_tenants`, wildcard `"*"` dev only).
- DB-level: composite FKs + triggers (migration 0131) enforce tenant references.
- Gaps: adapter/base-model lifecycle DB queries still need tenant predicates and cross-tenant denial tests; triggers need revalidation for those paths.

## Path Security

- Runtime state must live under `var/`; `/tmp` (and macOS `/private/tmp`) is rejected for telemetry, manifest cache, adapters, database, index-root, model-cache, status, dataset/document roots, and worker sockets.
- Unit tests cover `/tmp` rejection in `crates/adapteros-config/src/path_resolver.rs`.

## Policy & Audit

- **25 Policy Packs**: Egress, Determinism, Router, Evidence, Refusal, Numeric, RAG, Isolation, Telemetry, Retention, Performance, Memory, Artifacts, Secrets, Build/Release, Compliance, Incident, Output, Adapters, Deterministic I/O, Drift, DIR/MPLoRA, Naming, Dependency Security, Circuit Breaker.
- Hook points: `OnRequestBeforeRouting`, `OnBeforeInference`, `OnAfterInference`.
- Merkle chain audit: BLAKE3 chaining with Ed25519 signatures (`crates/adapteros-db/src/policy_audit.rs`).

## Repository Structure (High Level)

- **Binaries**: `adapteros-server`, `aos-worker`, `aosctl`, `adapteros-tui`.
- **Control Plane**: `adapteros-server`, `adapteros-server-api`, `adapteros-orchestrator`, `adapteros-service-supervisor`.
- **Worker/Data**: `adapteros-lora-worker`, `adapteros-lora-router`, `adapteros-lora-kernel-mtl`, `adapteros-lora-kernel-coreml`, `adapteros-lora-kernel-api`, `adapteros-lora-kernel-prof`.
- **Core**: `adapteros-core`, `adapteros-db`, `adapteros-config`, `adapteros-policy`, `adapteros-crypto`, `adapteros-platform`.
- **AI/ML**: `adapteros-chat`, `adapteros-base-llm`, `adapteros-lora-plan`, `adapteros-lora-rag`, `adapteros-lora-quant`, `adapteros-lora-lifecycle`, `adapteros-model-hub`, `adapteros-ingest-docs`.
- **Analysis/Verification**: `adapteros-codegraph`, `adapteros-verify`, `adapteros-verification`, `adapteros-lint`, `adapteros-deterministic-exec`, `adapteros-replay`, `adapteros-trace`, `adapteros-profiler`, `adapteros-telemetry-verifier`.
- **Utilities**: `adapteros-api`, `adapteros-client`, `adapteros-cli`, `adapteros-tui`, `adapteros-node`, `adapteros-git`, `adapteros-storage`, `adapteros-memory`, `adapteros-graph`, `adapteros-numerics`, `adapteros-autograd`, `adapteros-error-recovery`.
- **Infrastructure**: `adapteros-federation`, `adapteros-secd`, `adapteros-boot`, `adapteros-testing`, `adapteros-scenarios`, `adapteros-domain`, `adapteros-registry`, `adapteros-artifacts`, `adapteros-manifest`, `adapteros-aos`, `adapteros-sbom`.
- **Monitoring**: `adapteros-telemetry`, `adapteros-metrics-exporter`, `adapteros-metrics-collector`, `adapteros-system-metrics`, `adapteros-plugin-advanced-metrics`.
- **Types/Config**: `adapteros-types`, `adapteros-api-types`, `adapteros-config-types`, `adapteros-telemetry-types`.
- **Legacy/Experimental**: `adapteros-temp`, `adapteros-single-file-adapter`, `adapteros-secure-fs`, `adapteros-concurrent-fs`, `adapteros-cdp`, `adapteros-cp-client`, `adapteros-patch`.
- **UI**: `ui/` (React 18 + TypeScript + Vite + TanStack Query).
- **Apps**: `installer/` (SwiftUI macOS installer), `menu-bar-app/` (SwiftUI menu bar status monitor), `monitoring/` (Grafana dashboards).

## Common Misconceptions

- Handlers are `infer()`, not `handle_infer()`.
- Use `GlobalTickLedger`, not `GlobalLedger`.
- Chat is multi-turn (`build_chat_prompt` pulls full history).
- Replay flows through `InferenceCore`, not around it.
- Workers have zero network egress (UDS only).

## Environment

- Platform: macOS on Apple Silicon (M1/M2/M3/M4).
- Database: SQLite at `var/aos-cp.sqlite3`.
- Default model: Qwen2.5-7B.
- Rust: nightly (see `rust-toolchain.toml`).
- UI: pnpm 9+, React 18.

## Additional Tooling

### Native Apps
- **Graphical Installer** (`installer/`): SwiftUI macOS installer with hardware pre-checks, checkpoint recovery, and air-gapped support.
- **Menu Bar Monitor** (`menu-bar-app/`): SwiftUI status monitor providing real-time AdapterOS service monitoring from the macOS menu bar.
- **CodeGraph Viewer** (`codegraph-viewer/`): Tauri desktop app for visualizing code relationships and analysis.

### Infrastructure & Deployment
- **Docker**: Complete containerized development and production environments.
- **Terraform**: Infrastructure-as-code for cloud deployments.
- **Grafana Monitoring**: Pre-built dashboards for system and inference performance monitoring.
- **Service Supervisor**: Process management and orchestration layer.

### SDK Generation
- **OpenAPI**: Automatic API documentation and specification generation.
- **TypeScript Types**: Auto-generated TypeScript types from OpenAPI specs.
- **Python SDK**: Auto-generated Python client library.

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
- Production profiles: `production-macos`, `dev-macos-real`
- Combined features: `stub-backends`, `multi-backend`, `full`
- Testing: `extended-tests`, `hardware-residency`, `integration_tests`, `security_tests`, `loom`
- Experimental: `domain-experimental`, `lint-experimental`, `numerics-experimental`
- Advanced: `federation`, `rag-pgvector`, `secure-enclave`, `cdp`

## Troubleshooting

**Determinism**
1. Check seed derivation inputs.
2. Verify router sorting (score DESC, index ASC tie-break).
3. Confirm Q15 denominator = 32767.0.
4. Run `make determinism-check`.

**Build**
1. `cargo clean && cargo build`
2. Check feature flags
3. `cargo sqlx prepare` for offline mode
4. Verify migration signatures: `migrations/signatures.json`

**Ports**
```bash
make prepare                 # Stops services, cleans ports
lsof -ti:8080 | xargs kill   # Manual port cleanup
```

**Env loading**
- `.envrc` auto-exports `.env` + `.env.local` (run `direnv allow` once).
- Without direnv: `set -a; source .env; source .env.local; set +a`.

MLNavigator Inc 2025-12-21

## Session Continuity

**At turn start**: Read `.agents/active-work.json`. Update `session` block when goal/decisions change.

- `session.goal` — Current objective (null if none)
- `session.decisions` — Key choices made
- `session.open_questions` — Unresolved items
- `work_items[].status` — Your agent's progress

Use TodoWrite for execution steps. This file is the only continuity state.