# AdapterOS CLI & Script Deprecations

This document tracks deprecated CLIs and scripts, along with their supported replacements. Adding a new script under `scripts/` requires either:

- An entry here (with a clear deprecation plan), or  
- An entry in `docs/internal/cli-inventory.md` (for active, non-deprecated scripts).

The CI guardrail fails the build if any `scripts/*.sh` file is not referenced by at least one of these documents.

## Shell Scripts (Root)

- `service-manager.sh`  
  - Status: **DEPRECATED**  
  - Replacement: `aos` (Rust) for local service control and `aosctl` (Rust) for system/cluster operations once available.  
  - Notes: Existing behavior is presently wired via `aos-launch` and root `./aos`.  
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
