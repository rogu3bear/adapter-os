# AdapterOS CLI Inventory

This document tracks all command-line entrypoints and scripts that interact with AdapterOS. It is intended as the single source of truth for CLI ownership and lifecycle state (KEEP / MERGE / DELETE).

## Canonical Rust CLIs

- `aos`  
  - Purpose: Local adapter / service runner (single Rust binary).  
  - Owner crate: `crates/adapteros-aos` (`aos` binary).  
  - Status: **KEEP** (local services/adapters only; no DB, migration, or cluster operations).  
  - Notes: Should control only local services/adapters on the current node, with no direct database writes or cluster coordination logic.  
  - Sources: `[source: crates/adapteros-aos/src/bin/aos.rs L1-L260]`

- `aosctl`  
  - Purpose: System / ops / admin CLI for DB operations, cluster management, determinism checks, maintenance, and deploy flows.  
  - Owner crate: `crates/adapteros-cli` (`aosctl` binary).  
  - Status: **KEEP** (becomes the single system control-plane CLI).  
  - Notes: All DB migrations, deploy flows, determinism checks, and maintenance commands should eventually live here.  
  - Sources: `[source: crates/adapteros-cli/Cargo.toml L1-L40]`, `[source: crates/adapteros-cli/src/main.rs L1-L80]`

- `cargo xtask`  
  - Purpose: Developer automation only (build orchestration, SBOM generation, determinism-report, training helpers, packaging, etc.).  
  - Owner crate: `xtask`.  
  - Status: **KEEP** (dev-only; must not be used for production operations).  
  - Notes: All dev-only flows currently implemented in shell should converge here over time.  
  - Sources: `[source: xtask/Cargo.toml L1-L40]`, `[source: xtask/src/main.rs L1-L120]`

## Legacy Shell Entry Points (Root)

These existed previously but are now either removed or shims pointing to the Rust CLIs.

- `scripts/aos.sh`  
  - Purpose: Backwards-compatibility shim that fails fast with a deprecation message and directs users to the Rust `aos` binary.  
  - Owner: Shell script.  
  - Status: **DEPRECATED** (use `aos` Rust binary instead).  
  - Notes: Exists only to catch legacy `./scripts/aos.sh` invocations.  
  - Sources: `[source: scripts/aos.sh L1-L20]`

- `./aos-launch`
  - Purpose: Canonical orchestration script that performs comprehensive pre-flight checks, port management, and coordinates backend/UI/menu-bar app startup.
  - Owner: Root shell script.
  - Status: **KEEP** (canonical launch method for local development and testing).
  - Notes: Provides important orchestration features including database initialization, port conflict resolution, health monitoring, and graceful shutdown. Battle-tested and comprehensive. See docs/cli/aos-launch.md for full documentation.
  - Sources: `[source: aos-launch L1-L403]`

## Scripts Under `scripts/` (High-Level Inventory)

The following `.sh` scripts currently exist under `scripts/`. Each new script must be explicitly listed here or in `DEPRECATIONS.md`.

> NOTE: Status here is provisional; later phases will migrate behavior into `aos`, `aosctl`, or `cargo xtask`.

- `scripts/migrate.sh`  
  - Purpose: Database migration helper.  
  - Owner: Shell script.  
  - Status: **DEPRECATED** (migrate to `aosctl db migrate`).  
  - Sources: `[source: scripts/migrate.sh L1-L20]`

- `scripts/deploy_adapters.sh`  
  - Purpose: Adapter deployment helper.  
  - Owner: Shell script.  
  - Status: **DEPRECATED** (migrate to `aosctl deploy adapters`).  
  - Sources: `[source: scripts/deploy_adapters.sh L1-L20]`

- `scripts/verify-determinism-loop.sh`  
  - Purpose: Determinism verification loop runner.  
  - Owner: Shell script.  
  - Status: **DEPRECATED** (migrate to `aosctl verify determinism-loop`).  
  - Sources: `[source: scripts/verify-determinism-loop.sh L1-L20]`

- `scripts/gc_bundles.sh`  
  - Purpose: Artifact/bundle garbage collection.  
  - Owner: Shell script.  
  - Status: **DEPRECATED** (migrate to `aosctl maintenance gc-bundles`).  
  - Sources: `[source: scripts/gc_bundles.sh L1-L20]`

## Active Scripts (KEEP)

The following scripts are actively used and should be maintained:

### Build & Development

- `scripts/build_metadata.sh`
  - Purpose: Collect build metadata for reproducibility (rustc, Metal SDK, system info).
  - Status: **KEEP** (used in `make build` pipeline).
  - Sources: `[source: scripts/build_metadata.sh L1-L71]`

- `scripts/build-mlx.sh`
  - Purpose: Build MLX backend with auto-detection of MLX paths.
  - Status: **KEEP** (active via `make build-mlx/test-mlx/bench-mlx`).
  - Sources: `[source: scripts/build-mlx.sh L1-L111]`

- `scripts/build_with_stable_sdk.sh`
  - Purpose: macOS 14.4 SDK workaround for Sequoia linker issues.
  - Status: **KEEP** (conditional workaround for macOS SDK compatibility).
  - Sources: `[source: scripts/build_with_stable_sdk.sh L1-L57]`

- `scripts/fresh-build.sh`
  - Purpose: Pre-build cleanup (stops services, kills orphaned processes).
  - Status: **KEEP** (used in `make build` and `make prepare`).
  - Sources: `[source: scripts/fresh-build.sh L1-L300]`

### Infrastructure & Deployment

- `scripts/service-manager.sh`
  - Purpose: Service lifecycle management (start/stop/restart backend, UI).
  - Status: **KEEP** (core boot system component, used by `./start`).
  - Sources: `[source: scripts/service-manager.sh L1-L500]`

- `scripts/graceful-shutdown.sh`
  - Purpose: Phased shutdown of AdapterOS services with timeout fallbacks.
  - Status: **KEEP** (used by fresh-build.sh, essential for dev workflow).
  - Sources: `[source: scripts/graceful-shutdown.sh L1-L100]`

- `scripts/port-guard.sh`
  - Purpose: Shared helpers for graceful port cleanup and conflict detection.
  - Status: **KEEP** (used by service-manager.sh, fresh-build.sh).
  - Sources: `[source: scripts/port-guard.sh L1-L80]`

- `scripts/lib/freeze-guard.sh`
  - Purpose: Port conflict detection library, never auto-kills external processes.
  - Status: **KEEP** (core library for `./start` boot system).
  - Sources: `[source: scripts/lib/freeze-guard.sh L1-L200]`

- `scripts/deploy-production.sh`
  - Purpose: Production deployment with UDS, JWT, PF validation.
  - Status: **KEEP** (critical for production deployments).
  - Sources: `[source: scripts/deploy-production.sh L1-L168]`

- `scripts/deploy-uds-metrics.sh`
  - Purpose: Deploy UDS metrics bridge and systemd service.
  - Status: **KEEP** (required for telemetry pipeline).
  - Sources: `[source: scripts/deploy-uds-metrics.sh L1-L119]`

- `scripts/prevent_infrastructure_issues.sh`
  - Purpose: CI workspace validator for tokio, dependencies, compilation.
  - Status: **KEEP** (active in infrastructure-health.yml workflow).
  - Sources: `[source: scripts/prevent_infrastructure_issues.sh L1-L100]`

### Database & Migrations

- `scripts/sign_migrations.sh`
  - Purpose: Sign database migrations with Ed25519 signatures.
  - Status: **KEEP** (mandatory for production deployment).
  - Sources: `[source: scripts/sign_migrations.sh L1-L100]`

- `scripts/check_migration_conflicts.sh`
  - Purpose: Detect migration conflicts (duplicates, table conflicts, signatures).
  - Status: **KEEP** (active in migration-testing.yml CI workflow).
  - Sources: `[source: scripts/check_migration_conflicts.sh L1-L80]`

### Quality & Testing

- `scripts/run_jscpd.sh`
  - Purpose: Code duplication detection via jscpd.
  - Status: **KEEP** (active in duplication.yml CI workflow, `make dup`).
  - Sources: `[source: scripts/run_jscpd.sh L1-L100]`

- `scripts/security_audit.sh`
  - Purpose: Comprehensive security audit (cargo-audit, SBOM, licenses).
  - Status: **KEEP** (used via `make security-audit`).
  - Sources: `[source: scripts/security_audit.sh L1-L80]`

- `scripts/validate-docs.sh`
  - Purpose: Documentation cross-reference and consistency validation.
  - Status: **KEEP** (documentation QA).
  - Sources: `[source: scripts/validate-docs.sh L1-L100]`

- `scripts/validate_env.sh`
  - Purpose: Environment validation (tools, ports, config).
  - Status: **KEEP** (critical for developer onboarding).
  - Sources: `[source: scripts/validate_env.sh L1-L150]`

- `scripts/validate_openapi_docs.sh`
  - Purpose: OpenAPI specification validation.
  - Status: **KEEP** (used via `make validate-openapi`).
  - Sources: `[source: scripts/validate_openapi_docs.sh L1-L50]`

- `scripts/test_api_endpoints.sh`
  - Purpose: Comprehensive API endpoint testing (250+ endpoints).
  - Status: **KEEP** (documented in API_ENDPOINT_INVENTORY.md).
  - Sources: `[source: scripts/test_api_endpoints.sh L1-L300]`

- `scripts/bootstrap_integration_test.sh`
  - Purpose: Boot system validation (8 scenarios).
  - Status: **KEEP** (validates `./start` unified boot system).
  - Sources: `[source: scripts/bootstrap_integration_test.sh L1-L200]`

- `scripts/generate_test_metrics.sh`
  - Purpose: Generate test metrics report.
  - Status: **KEEP** (active in integration-tests.yml CI workflow).
  - Sources: `[source: scripts/generate_test_metrics.sh L1-L80]`

- `scripts/collect_stress_results.sh`
  - Purpose: Collect and format stress test results.
  - Status: **KEEP** (active in stress-tests.yml CI workflow).
  - Sources: `[source: scripts/collect_stress_results.sh L1-L60]`

### Setup & Environment

- `scripts/setup_env.sh`
  - Purpose: Interactive environment setup wizard.
  - Status: **KEEP** (core onboarding tool).
  - Sources: `[source: scripts/setup_env.sh L1-L200]`

- `scripts/switch_env_profile.sh`
  - Purpose: Quick profile switching (dev/training/production).
  - Status: **KEEP** (complements setup_env.sh).
  - Sources: `[source: scripts/switch_env_profile.sh L1-L100]`

- `scripts/install_git_hooks.sh`
  - Purpose: Configure git to use .githooks directory.
  - Status: **KEEP** (modern git hook configuration).
  - Sources: `[source: scripts/install_git_hooks.sh L1-L30]`

- `scripts/install-metal-toolchain.sh`
  - Purpose: Verify/install Metal compiler toolchain.
  - Status: **KEEP** (active in metal-build.yml CI workflow).
  - Sources: `[source: scripts/install-metal-toolchain.sh L1-L100]`

- `scripts/check-system.sh`
  - Purpose: Run preflight system readiness checks.
  - Status: **KEEP** (used via `make check-system`).
  - Sources: `[source: scripts/check-system.sh L1-L50]`

### Utility

- `scripts/record_env.sh`
  - Purpose: Record build environment variables for reproducibility.
  - Status: **KEEP** (used in `make build` pipeline).
  - Sources: `[source: scripts/record_env.sh L1-L60]`

- `scripts/strip_timestamps.sh`
  - Purpose: Normalize binary timestamps for reproducible builds.
  - Status: **KEEP** (used in `make build` pipeline).
  - Sources: `[source: scripts/strip_timestamps.sh L1-L80]`

- `scripts/download-model.sh`
  - Purpose: Download Qwen2.5 models from HuggingFace.
  - Status: **KEEP** (used via `make download-model`, documented in QUICKSTART).
  - Sources: `[source: scripts/download-model.sh L1-L150]`

- `scripts/bootstrap_with_checkpoints.sh`
  - Purpose: Installer with checkpoint recovery for resume capability.
  - Status: **KEEP** (active integration with `aosctl bootstrap`).
  - Sources: `[source: scripts/bootstrap_with_checkpoints.sh L1-L200]`

- `scripts/verify_metal_access.sh`
  - Purpose: macOS Metal device access verification.
  - Status: **KEEP** (documented in MLX troubleshooting, CI integration).
  - Sources: `[source: scripts/verify_metal_access.sh L1-L100]`

## Scripts Under Review

The following scripts require further evaluation:

- `scripts/consolidate_deps.sh` - Dependency consolidation (verify if already applied)
- `scripts/audit_api_endpoints.sh` - API audit (convert to cargo xtask)
- `scripts/verify-deployment.sh` - Deployment verification (clarify determinism components)
- `scripts/validate_mermaid.sh` - Mermaid diagram validation (integrate into CI or deprecate)
- `scripts/run_benchmarks.sh` - Benchmark runner (integrate into CI or document as manual)
- `scripts/run_jscpd_batched.sh` - Batched duplication detection (consider replacing run_jscpd.sh)
- `scripts/ui_smoke.sh` - UI smoke tests (integrate into CI or deprecate)
- `scripts/metrics-bridge.sh` - UDS-to-Prometheus bridge (verify production usage)
- `scripts/colima-start.sh` / `scripts/colima-stop.sh` - Colima Docker tools (verify Docker workflow usage)
- `scripts/demo_inference.sh` - Inference demo (fix missing dependency or deprecate)
- `scripts/list_root_docs.sh` / `scripts/execute_doc_audit.sh` - Doc audit tools (verify audit completion)

## Deprecated Scripts

All deprecated scripts are now documented in `docs/DEPRECATIONS.md` with:
- Clear deprecation status
- Replacement commands
- Migration guidance

Key categories of deprecated scripts:
- **Migration scripts** (rename_imports.sh, rename_packages.sh, etc.) - One-time use completed
- **Build scripts** (build_ui.sh, build_web_ui.sh) - Superseded by pnpm
- **Infrastructure scripts** (init_cp.sh, start_server.sh) - Superseded by `./start`
- **Utility scripts** (unload_model.sh, generate_openapi_docs.sh) - Superseded by aosctl/xtask

## Policy

- All new CLI entrypoints and `.sh` scripts MUST be added to this inventory or to `DEPRECATIONS.md`.  
- The CI guardrail (`script_inventory_guard` test) enforces that every `scripts/*.sh` file name appears in at least one of:
  - `docs/internal/cli-inventory.md`
  - `DEPRECATIONS.md`
