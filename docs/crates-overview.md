# Rust Crates Outline

## Core Engine (Inference & Execution)
- **adapteros-lora-worker** (`src/lib.rs`, `src/inference_pipeline.rs`): Handles UDS requests → adapter selection → kernel execution.
  - Entry: `UdsServer::new()`.
  - Flow: Request → Policy check → Router → Fused kernels (MTL/MLX) → Response.
  - Dependencies: lora-router, lora-kernel-mtl, policy.
  - Distinguish: Services (e.g., `determinism_policy.rs`) vs. pipeline stages.
- **adapteros-lora-router** (`src/lib.rs`, `src/features.rs`): K-sparse selection with Q15 gates.
  - Entry: `Router::select_adapters()`.
  - Components: Feature extraction → Scoring → Top-K output.
  - Distinguish: Calibration utils vs. runtime routing.

## Policy & Security Layer
- **adapteros-policy** (`src/policy_packs.rs`, `src/packs/`): Enforces 20+ packs (e.g., determinism, egress).
  - Entry: `PolicyEngine::enforce()`.
  - Structure: Packs (modular) → Evidence tracker → Violation logging.
  - Dependencies: core (AosError), telemetry.
  - Distinguish: Runtime checks vs. static validation.

## Storage & Data
- **adapteros-db** (`src/lib.rs`, `migrations/`): SQLite ops for adapters, tenants, metrics.
  - Entry: `Db::new()`.
  - Modules: Operations (e.g., `domain_adapters.rs`) → Queries (parameterized SQLx).
  - Distinguish: Migrations vs. runtime pooling.
- **adapteros-secure-fs** (`src/lib.rs`): Content-addressed storage with BLAKE3/Ed25519.
  - Entry: `SecureFs::open()`.
  - Flow: Write → Hash → Sign → Store.

## Telemetry & Monitoring
- **adapteros-telemetry** (`src/lib.rs`, `src/unified_events.rs`): Structured logging with JCS.
  - Entry: `telemetry::log_event()`.
  - Components: Metrics collector → Exporter (Prometheus).
  - Distinguish: Event types vs. sampling policies.

## Cross-Cutting Concerns
- **adapteros-core** (`src/lib.rs`): Shared types (AosError, Result).
- **Dependencies Graph**: Core → All; Use `cargo tree` for visuals.

[source: crates/adapteros-lora-worker/src/lib.rs L1-L100]
[source: crates/adapteros-policy/src/policy_packs.rs L1-L50]
[source: docs/CRATE_INDEX.md L1-L200]
