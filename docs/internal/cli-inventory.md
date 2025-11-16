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
  - Purpose: Launch panel that performs pre-flight checks and then starts backend, UI, and menu bar app via the `aos` CLI.  
  - Owner: Root shell script.  
  - Status: **DELETE** after behavior is covered by `aos`/`aosctl` and documentation.  
  - Notes: High UX surface area; should be replaced by `aos` and `aosctl` commands rather than a bespoke launcher.  
  - Sources: `[source: aos-launch L1-L220]`

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

- Other existing `.sh` scripts (current state)  
  - Examples: `archive_audit.sh`, `bootstrap_with_checkpoints.sh`, `build_metadata.sh`, `build_ui.sh`, `build_web_ui.sh`, `build_with_stable_sdk.sh`, `check_sqlx_cache.sh`, `colima-start.sh`, `colima-stop.sh`, `deploy-production.sh`, `deploy-uds-metrics.sh`, `download_model.sh`, `generate_openapi_docs.sh`, `generate_openapi_simple.sh`, `graceful-shutdown.sh`, `init_cp.sh`, `install_git_hooks.sh`, `metrics-bridge.sh`, `record_env.sh`, `rename_imports.sh`, `rename_packages.sh`, `run_benchmarks.sh`, `run_jscpd.sh`, `setup_sqlx_offline.sh`, `sign_migrations.sh`, `smoke_test.sh`, `strip_timestamps.sh`, `ui_smoke.sh`, `unload_model.sh`, `unload_model_db.sh`, `update_cargo_names.sh`, `update_rust_imports.sh`, `validate-docs.sh`, `validate_mermaid.sh`, `validate_openapi_docs.sh`, `verify-deployment.sh`, `verify_artifacts.sh`, `verify_compiler_lockbox.sh`, `verify_deterministic_ui.sh`, `verify_directory_adapter_perf.sh`, `verify_prometheus_metrics.sh`, `aos.sh`.  
  - Status: **REVIEW** (to be categorized into KEEP / MERGE / DELETE as they are migrated into `aos`, `aosctl`, or `cargo xtask`).  
  - Sources: `[source: scripts/archive_audit.sh L1-L40]`, `[source: scripts/bootstrap_with_checkpoints.sh L1-L40]`, `[source: scripts/build_metadata.sh L1-L40]`, `[source: scripts/build_ui.sh L1-L40]`, `[source: scripts/build_web_ui.sh L1-L40]`, `[source: scripts/build_with_stable_sdk.sh L1-L40]`

## Policy

- All new CLI entrypoints and `.sh` scripts MUST be added to this inventory or to `DEPRECATIONS.md`.  
- The CI guardrail (`script_inventory_guard` test) enforces that every `scripts/*.sh` file name appears in at least one of:
  - `docs/internal/cli-inventory.md`
  - `DEPRECATIONS.md`
