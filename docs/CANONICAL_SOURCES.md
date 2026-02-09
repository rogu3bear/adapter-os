# Canonical Sources

This document declares code-level sources of truth for behavior claims in documentation.

Last verified: 2026-02-09

## Runtime and Startup

| Domain | Canonical Source | Notes |
|---|---|---|
| Boot orchestration | `start` | Primary local startup contract (`./start`) |
| Service primitives | `scripts/service-manager.sh` | Lower-level lifecycle controls used by `./start` |
| Control-plane boot lifecycle | `crates/adapteros-server/src/main.rs` | Boot phase orchestration and timeout envelope |
| Boot modules | `crates/adapteros-server/src/boot/` | Phase-specific implementation details |
| Runtime config defaults | `configs/cp.toml` | Default server/database/security/runtime knobs |

## API and Security Tiers

| Domain | Canonical Source | Notes |
|---|---|---|
| Route registry and tier wiring | `crates/adapteros-server-api/src/routes/mod.rs` | `health/public/optional_auth/internal/protected` |
| API middleware implementations | `crates/adapteros-server-api/src/middleware/` | Auth, tenant guard, CSRF, policy, audit, etc. |
| Security middleware layers | `crates/adapteros-server-api/src/middleware_security.rs` | Rate limits, headers, lifecycle/drain, etc. |
| Public API surface | `docs/api/openapi.json` | Generated spec for externally documented HTTP contract |

## Worker and Determinism

| Domain | Canonical Source | Notes |
|---|---|---|
| Worker entrypoint | `crates/adapteros-lora-worker/src/bin/aos_worker.rs` | Process init and telemetry/error exit semantics |
| Worker startup/runtime | `crates/adapteros-lora-worker/src/bin/worker_modules/` | CLI/config/backend/registration wiring |
| Seed derivation contract | `crates/adapteros-core/src/seed.rs` | HKDF derivation constants and versioning |
| Router quantization contract | `crates/adapteros-lora-router/src/quantization.rs` | Q15 denominator and encode/decode invariants |
| Persistent path security | `crates/adapteros-core/src/path_security.rs` | `/tmp` and scheme-prefixed tmp-path rejection |

## UI Surface

| Domain | Canonical Source | Notes |
|---|---|---|
| Route map and shell boundaries | `crates/adapteros-ui/src/lib.rs` | Public vs protected routes, shell wrapping |
| UI architecture boundary | `crates/adapteros-ui/UI_CONTRACT.md` | Browser-light vs server-heavy responsibility split |
| Trunk build/proxy | `crates/adapteros-ui/Trunk.toml` | Dev server routing and static output target |
| UI static build integration | `scripts/build-ui.sh` | Build + versioning into `crates/adapteros-server/static` |

## Generated Contract Artifacts

These files are generated from canonical source and must not be manually edited.

- `docs/generated/api-route-inventory.json`
- `docs/generated/ui-route-inventory.json`
- `docs/generated/middleware-chain.json`

Generator:

- `scripts/contracts/generate_contract_artifacts.py`

Validation:

- `scripts/contracts/check_contract_artifacts.sh`
- `scripts/contracts/check_startup_contract.sh`
- `scripts/contracts/check_determinism_contract.sh`
- `scripts/contracts/check_docs_claims.sh`

## Policy

1. If behavior docs conflict with canonical source, source wins.
2. Any behavior claim in high-level docs must be traceable to one canonical source above.
3. Contract artifacts must be regenerated in the same change that modifies their source logic.
4. Determinism and path-security constants are treated as policy invariants; changes require explicit review context.
