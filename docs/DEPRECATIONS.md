# adapterOS CLI & Script Deprecations

This document tracks deprecated CLIs and scripts, along with their supported replacements. Adding a new script under `scripts/` requires either:

- An entry here (with a clear deprecation plan), or  
- An entry in `docs/internal/cli-inventory.md` (for active, non-deprecated scripts).

The CI guardrail fails the build if any `scripts/*.sh` file is not referenced by at least one of these documents.

## Shell Scripts (Root)

- `service-manager.sh`  
  - Status: **DEPRECATED for direct use** (still used internally by `./start`)  
  - Replacement: Use `./start` (delegates to this script) or the upcoming `aos`/`aosctl` CLIs for managed workflows.  
  - Notes: Not a parallel boot path; `./start` is the canonical entrypoint and reuses this implementation.  
  - Sources: `[source: aos L1-L220]`

- `launch.sh` (root)  
  - Status: **DEPRECATED**  
  - Replacement: `aos` and `aosctl` CLIs.  
  - Notes: Use `aos` to manage local services and `aosctl` for system operations instead of shell launchers.  
  - Sources: `[source: aos-launch L1-L220]`

## Root Binaries

- `registry_migrate.rs`
  - Status: **DEPRECATED**
  - Replacement: `aosctl registry-migrate`
  - Notes: Registry migration functionality is now integrated into `aosctl` as `registry-migrate` command.
  - Sources: `[source: registry_migrate.rs L1-L348]`

## `scripts/` Directory Deprecations

- `scripts/service-manager.sh`  
  - Status: **DEPRECATED**  
  - Replacement:  
    - `aos` (service lifecycle for backend, UI, and menu bar app on local node)  
    - `aosctl` (cluster-aware operations, DB, and maintenance tasks)  
  - Notes: New behavior should not be added here; instead, extend the Rust CLIs.  
  - Sources: `[source: aos L1-L220]`

- `scripts/run_complete_system.sh`  
  - Status: **DEPRECATED (shim)**  
  - Replacement: `./start`  
  - Notes: Emits a deprecation banner and 15s prompt (default No) before redirecting to `./start`. Only use if explicitly testing the legacy flow.  
  - Sources: `[source: scripts/run_complete_system.sh L1-L17]`

- `scripts/bootstrap_integration_test.sh`  
  - Status: **DEPRECATED**  
  - Replacement: `./start`  
  - Notes: Legacy bootstrap harness; now guarded by a 15s prompt (default No). Prefer `./start` for all boot validation.  
  - Sources: `[source: scripts/bootstrap_integration_test.sh L1-L20]`

- `scripts/bootstrap_with_checkpoints.sh`  
  - Status: **DEPRECATED**  
  - Replacement: `./start`  
  - Notes: Legacy resumable bootstrap; now guarded by a 15s prompt (default No). Prefer `./start`.  
  - Sources: `[source: scripts/bootstrap_with_checkpoints.sh L1-L20]`

- `scripts/migrate.sh`  
  - Status: **DEPRECATED**  
  - Replacement: `aosctl db migrate`  
  - Notes: All database migrations should be driven through `aosctl` once implemented.  
  - Sources: `[source: scripts/migrate.sh L1-L20]`

- `scripts/deploy_adapters.sh`  
  - Status: **DEPRECATED**  
  - Replacement: `aosctl deploy adapters`  
  - Notes: Use `aosctl` for adapter deployment workflows.  
  - Sources: `[source: scripts/deploy_adapters.sh L1-L20]`

- `scripts/verify-determinism-loop.sh`
  - Status: **DEPRECATED**
  - Replacement: `aosctl verify determinism-loop`
  - Notes: Determinism checks are now part of the unified `aosctl verify` subcommand group.
  - Sources: `[source: scripts/verify-determinism-loop.sh L1-L10]`

- `scripts/gc_bundles.sh`  
  - Status: **DEPRECATED**  
  - Replacement: `aosctl maintenance gc-bundles`  
  - Notes: Bundle/artifact GC is a system maintenance concern and belongs in `aosctl`.  
  - Sources: `[source: scripts/gc_bundles.sh L1-L20]`

- `scripts/aos.sh`
  - Status: **DEPRECATED (shim)**
  - Replacement: `aos` Rust binary (installed via Cargo or system package).
  - Notes: Exists only as a compatibility shim and exits with a deprecation message; new tooling must invoke `aos` directly.
  - Sources: `[source: scripts/aos.sh L1-L20]`

### Migration/Refactoring Scripts (One-Time Use)

- `scripts/rename_imports.sh`
  - Status: **DEPRECATED**
  - Replacement: None (one-time migration completed)
  - Notes: Bulk renamed `aos_*` imports to `mplora_*`. Phase 1 of naming unification completed; running again would break the codebase.
  - Sources: `[source: scripts/rename_imports.sh L1-L50]`

- `scripts/rename_packages.sh`
  - Status: **DEPRECATED**
  - Replacement: None (one-time migration completed)
  - Notes: Bulk renamed package names in Cargo.toml from `aos-*` to `mplora-*`. Phase 1 completed.
  - Sources: `[source: scripts/rename_packages.sh L1-L50]`

- `scripts/update_cargo_names.sh`
  - Status: **DEPRECATED**
  - Replacement: None (one-time migration completed)
  - Notes: Bulk renamed Cargo.toml from `mplora-*` to `adapteros-*`. Phase 2 completed; codebase now uses adapteros-* naming.
  - Sources: `[source: scripts/update_cargo_names.sh L1-L50]`

- `scripts/update_rust_imports.sh`
  - Status: **DEPRECATED**
  - Replacement: None (one-time migration completed)
  - Notes: Bulk renamed Rust imports from `mplora_*` to `adapteros_*`. Phase 2 completed.
  - Sources: `[source: scripts/update_rust_imports.sh L1-L50]`

- `scripts/create_compat_shims.sh`
  - Status: **DEPRECATED**
  - Replacement: None (never executed)
  - Notes: Planned but never executed. Would have created compatibility shim crates in `crates/compat/`. No compat/ directory exists.
  - Sources: `[source: scripts/create_compat_shims.sh L1-L50]`

- `scripts/analyze_branch_differences.sh`
  - Status: **DEPRECATED**
  - Replacement: Native Git commands
  - Notes: One-time branch reconciliation script. Use `git log --oneline <branch_a> ^<branch_b>` directly.
  - Sources: `[source: scripts/analyze_branch_differences.sh L1-L100]`

### Build Scripts (Obsolete/Redundant)

- `scripts/build_ui.sh`
  - Status: **DEPRECATED**
  - Replacement: `cd crates/adapteros-ui && trunk build --release`
  - Notes: Obsolete WebAssembly/Trunk approach. UI is now React-based using pnpm. Zero active usage.
  - Sources: `[source: scripts/build_ui.sh L1-L30]`

- `scripts/build_web_ui.sh`
  - Status: **DEPRECATED**
  - Replacement: `cd crates/adapteros-ui && trunk build --release`
  - Notes: Redundant with `pnpm build`. Contains outdated path assumptions.
  - Sources: `[source: scripts/build_web_ui.sh L1-L30]`

### Audit/Validation Scripts (Unused/Redundant)

- `scripts/archive_audit.sh`
  - Status: **DEPRECATED**
  - Replacement: `cargo xtask archive-audit` or direct zstd command
  - Notes: Standalone utility with minimal integration. Referenced only in documentation, not actively used.
  - Sources: `[source: scripts/archive_audit.sh L1-L40]`

- `scripts/audit_api_coverage.sh`
  - Status: **DEPRECATED**
  - Replacement: `scripts/audit_api_endpoints.sh` (if audit functionality needed)
  - Notes: Duplicate/redundant variant of audit_api_endpoints.sh. No active usage.
  - Sources: `[source: scripts/audit_api_coverage.sh L1-L100]`

- `scripts/validate_qa_setup.sh`
  - Status: **DEPRECATED**
  - Replacement: None
  - Notes: References non-existent scripts (check_coverage.py, compare_benchmarks.py). Orphaned QA infrastructure.
  - Sources: `[source: scripts/validate_qa_setup.sh L1-L80]`

### Test Scripts (Unused/Superseded)

- `scripts/run-benchmarks.sh`
  - Status: **DEPRECATED**
  - Replacement: `scripts/run_benchmarks.sh` (underscore version)
  - Notes: Duplicate of run_benchmarks.sh. Use the underscore version as canonical.
  - Sources: `[source: scripts/run-benchmarks.sh L1-L100]`

- `scripts/run-mlx-tests.sh`
  - Status: **DEPRECATED**
  - Replacement: `cargo test --features mlx`
  - Notes: Zero active usage. MLX backend testing is integrated into standard Rust test infrastructure.
  - Sources: `[source: scripts/run-mlx-tests.sh L1-L100]`

- `scripts/smoke_test.sh`
  - Status: **DEPRECATED**
  - Replacement: `installer/smoke_test.sh`
  - Notes: Minimal 38-line stub. The installer version (316 lines) is comprehensive and canonical.
  - Sources: `[source: scripts/smoke_test.sh L1-L40]`

- `scripts/test_middleware.sh`
  - Status: **DEPRECATED**
  - Replacement: `scripts/test_api_endpoints.sh` or integration tests
  - Notes: Completely unused. Contains valuable middleware coverage but zero integration.
  - Sources: `[source: scripts/test_middleware.sh L1-L100]`

### Infrastructure Scripts (Superseded)

- `scripts/init_cp.sh`
  - Status: **DEPRECATED**
  - Replacement: `./start` (unified boot system)
  - Notes: Superseded by modern boot system. Contains outdated paths (/srv/aos instead of var/).
  - Sources: `[source: scripts/init_cp.sh L1-L80]`

- `scripts/start_server.sh`
  - Status: **DEPRECATED**
  - Replacement: `./start` or `scripts/service-manager.sh start-backend`
  - Notes: Legacy startup wrapper. All references are in deprecated documentation.
  - Sources: `[source: scripts/start_server.sh L1-L100]`

- `scripts/apply_shutdown_coordinator.sh`
  - Status: **DEPRECATED**
  - Replacement: `scripts/graceful-shutdown.sh` and `scripts/service-manager.sh stop`
  - Notes: Incomplete patching guide. Shutdown is now handled by graceful-shutdown.sh.
  - Sources: `[source: scripts/apply_shutdown_coordinator.sh L1-L50]`

### Utility Scripts (Redundant/Unused)

- `scripts/download_model.sh` (deprecated - use `aosctl models seed` instead)
  - Status: **DEPRECATED**
  - Replacement: `scripts/download-model.sh` (hyphenated version)
  - Notes: Duplicate of download-model.sh. Use the hyphenated version as canonical (more references).
  - Sources: `[source: scripts/download_model.sh L1-L100]`

- `scripts/unload_model.sh`
  - Status: **DEPRECATED**
  - Replacement: `aosctl models unload <model_id>` or HTTP API
  - Notes: Simple wrapper around HTTP endpoint. No added value over direct API call.
  - Sources: `[source: scripts/unload_model.sh L1-L30]`

- `scripts/unload_model_db.sh`
  - Status: **DEPRECATED**
  - Replacement: `aosctl models unload <model_id>` or HTTP API
  - Notes: Anti-pattern. Direct database manipulation bypasses API safeguards.
  - Sources: `[source: scripts/unload_model_db.sh L1-L30]`

- `scripts/generate-mocks.sh`
  - Status: **DEPRECATED**
  - Replacement: Test factory patterns in Rust tests
  - Notes: Zero usage. Generated mocks/ directory not referenced in any UI components or tests.
  - Sources: `[source: scripts/generate-mocks.sh L1-L100]`

- `scripts/generate_openapi_docs.sh`
  - Status: **DEPRECATED**
  - Replacement: `cargo xtask openapi-docs`
  - Notes: Complex shell wrapper. Functionality exists in xtask with better integration.
  - Sources: `[source: scripts/generate_openapi_docs.sh L1-L50]`

- `scripts/generate_openapi_simple.sh`
  - Status: **DEPRECATED**
  - Replacement: `cargo xtask openapi-docs`
  - Notes: Requires running server. xtask implementation doesn't require running server.
  - Sources: `[source: scripts/generate_openapi_simple.sh L1-L50]`

### Setup Scripts (Obsolete/Redundant)

- `scripts/setup_sqlx_offline.sh`
  - Status: **DEPRECATED**
  - Replacement: `export SQLX_OFFLINE=true`
  - Notes: One-liner script. Use environment variable directly.
  - Sources: `[source: scripts/setup_sqlx_offline.sh L1-L10]`

- `scripts/setup_pre_commit.sh`
  - Status: **DEPRECATED**
  - Replacement: `scripts/install_git_hooks.sh`
  - Notes: Superseded by modern .githooks approach with `git config core.hooksPath .githooks`.
  - Sources: `[source: scripts/setup_pre_commit.sh L1-L20]`

- `scripts/check_sqlx_cache.sh`
  - Status: **DEPRECATED**
  - Replacement: `cargo check --workspace --quiet`
  - Notes: Minimal 13-line script. Inline cargo check if validation is needed.
  - Sources: `[source: scripts/check_sqlx_cache.sh L1-L15]`

### Quality/Analysis Scripts (Superseded)

- `scripts/ai-slop-detector.sh`
  - Status: **DEPRECATED**
  - Replacement: `bash scripts/run_jscpd.sh` and `cargo clippy`
  - Notes: Script itself states "For duplication, prefer scripts/run_jscpd.sh". Superseded by authoritative tools.
  - Sources: `[source: scripts/ai-slop-detector.sh L1-L150]`

### Already in `scripts/deprecated/` Directory

- `scripts/deprecated/start.sh`
  - Status: **DEPRECATED**
  - Replacement: `./start`
  - Notes: Already relocated to deprecated directory. Contains hardcoded deprecation notice.
  - Sources: `[source: scripts/deprecated/start.sh L1-L50]`

- `scripts/deprecated/run_complete_system.sh`
  - Status: **DEPRECATED**
  - Replacement: `./start`
  - Notes: Already relocated to deprecated directory. Full functionality in unified boot system.
  - Sources: `[source: scripts/deprecated/run_complete_system.sh L1-L300]`

## CLI Command Changes (Breaking)

### Verify Subcommand Group

As of 2025-01-16, all verify commands have been consolidated into a unified `aosctl verify` subcommand group:

**Old Commands → New Commands:**
- `aosctl verify <bundle>` → `aosctl verify bundle <bundle>`
- `aosctl verify-adapter --adapters-root <dir> --adapter-id <id>` → `aosctl verify adapter --adapters-root <dir> --adapter-id <id>`
- `aosctl verify-adapters` → `aosctl verify adapters`
- `aosctl verify-determinism-loop` → `aosctl verify determinism-loop`
- `aosctl telemetry-verify --bundle-dir <dir>` → `aosctl verify telemetry --bundle-dir <dir>`
- `aosctl federation-verify --bundle-dir <dir>` → `aosctl verify federation --bundle-dir <dir>`

**Note:** The old standalone commands have been removed. Update scripts and CI workflows to use the new subcommand structure.

MLNavigator Inc 2025-12-06.
