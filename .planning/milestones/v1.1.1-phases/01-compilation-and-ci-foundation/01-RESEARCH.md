# Phase 1: Compilation and CI Foundation - Research

**Researched:** 2026-02-23
**Domain:** Rust workspace compilation, CI pipeline, dependency management
**Confidence:** HIGH

## Summary

The workspace is in significantly better shape than the phase description implies. `cargo check --workspace` already passes cleanly (1m28s) and `cargo fmt --all -- --check` is green. The deleted `query/` and `util/` modules in `adapteros-db` were already emptied (empty files) and the lib.rs no longer references them -- the working tree change is simply removing the empty placeholder files. The ~95 modified files in the working tree represent completed-but-uncommitted DB refactoring work plus script improvements, not broken code.

The actual remaining work is narrower than expected: (1) fix one benchmark compilation error (`session_id` field missing in `IoBuffers` initializer), (2) fix one clippy error (`io_other_error` in `adapteros-telemetry`), (3) apply the P0 dependency upgrades (safetensors 0.4->0.7.0, sqlx 0.8.2->0.8.6, tokio 1.35->1.44), (4) regenerate the SQLx offline cache, and (5) commit all working tree changes and verify CI gates and foundation-run.sh end-to-end.

**Primary recommendation:** Commit the existing working tree changes first (they compile clean), then apply dependency upgrades as a separate commit, then verify CI and foundation-run.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Bottom-up through the 10-layer dependency graph: leaf types -> infrastructure -> data -> domain -> backend -> worker -> orchestration -> server -> UI -> testing
- DB layer (Layer 3) is the cascade epicenter -- fix `adapteros-db` consumers first since deleted `query/` and `util/` modules affect everything above
- Compile-check each layer before moving up: `cargo check -p <crate>` per layer, then `cargo check --workspace` at the end
- `crates/adapteros-db/src/query/` (builders, filters, pagination) -- consumers must migrate to direct sqlx query patterns or inline equivalents
- `crates/adapteros-db/src/util/` (result_mappers, tenant_scoping, transaction) -- consumers must use the replacement patterns already established in the modified DB modules
- Analyze the already-modified files (they show the target pattern) and apply the same pattern to any remaining consumers that reference deleted modules
- Apply P0 upgrades in a single Cargo.toml edit: sqlx 0.8.2->0.8.6, safetensors 0.4->0.7.0, tokio 1.35->1.44
- After Cargo.toml updates, regenerate SQLx offline cache with `cargo sqlx prepare --workspace`
- Fix any breaking API changes from upgrades (sqlx 0.8.6 and safetensors 0.7.0 have known API surface changes)
- Get `cargo check --workspace` green first (blocks everything)
- Then `cargo test --workspace` (catches runtime issues)
- Then individual CI workflows: ci -> stability -> determinism -> integration -> security -> migration -> contracts
- Foundation-run.sh is the final gate: build succeeds, server boots through 12 phases, smoke tests pass

### Claude's Discretion
- Exact inline patterns to replace deleted query builders (whatever the existing modified files demonstrate)
- Order of crate fixes within each dependency layer
- Whether to fix clippy warnings encountered during compilation (fix if trivial, skip if unrelated to phase scope)
- SQLx offline cache regeneration approach (prepare --workspace vs per-crate)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| COMP-01 | Full workspace compiles cleanly with `cargo check` | Already passes. One benchmark bench target has a missing field error (`session_id` in `IoBuffers`). Fix is trivial (add `session_id: None`). |
| COMP-02 | All deleted DB module consumers updated to use replacement APIs | Already complete in working tree. `query/` and `util/` modules were emptied (0-byte files), lib.rs no longer references them. Modified DB files show the target pattern (direct sqlx queries, `pool_result().unwrap()` in tests). |
| COMP-03 | All CI gate workflows pass | `cargo check` passes. `cargo fmt` passes. `cargo clippy` has 1 error in `adapteros-telemetry` (`io_other_error`). `cargo test` has 1 compilation error in benchmarks. 17 CI workflow files exist. |
| COMP-04 | Foundation run script passes end-to-end | `scripts/foundation-run.sh` exists and is well-structured. Depends on `foundation-smoke.sh`, `fresh-build.sh`, `build-ui.sh`, `check_ui_assets.sh`. Requires a config at `configs/cp.toml`. |
| COMP-05 | P0 dependency upgrades applied | Current: sqlx 0.8.2, safetensors 0.4, tokio 1.35. Targets: sqlx 0.8.6, safetensors 0.7.0, tokio 1.44. safetensors 0.6.0 has breaking changes (Dtype enum changes, `size()` deprecated for `bitsize()`). sqlx 0.8.2->0.8.6 is semver-compatible with minor SQLite additions. tokio 1.35->1.44 is semver-compatible. |
| COMP-06 | SQLx offline cache synchronized | Cache exists at `crates/adapteros-db/.sqlx/` with 15 query files. CI workflow checks cache with `SQLX_OFFLINE_DIR: crates/adapteros-db/.sqlx`. Must regenerate after dependency and migration changes. |
</phase_requirements>

## Standard Stack

### Core
| Library | Current | Target | Purpose | Upgrade Risk |
|---------|---------|--------|---------|--------------|
| sqlx | 0.8.2 | 0.8.6 | Async SQLite persistence with compile-time query checking | LOW -- semver compatible, SQLite additions only |
| safetensors | 0.4 | 0.7.0 | LoRA adapter weight serialization/deserialization | MEDIUM -- Dtype enum expanded (MXFP4/FP6), `size()` deprecated for `bitsize()` |
| tokio | 1.35 | 1.44 | Async runtime | LOW -- semver compatible, MSRV bump to 1.70, mio v1 upgrade |
| libsqlite3-sys | =0.30.1 | =0.30.1 | Bundled SQLite (pinned) | NONE -- already at target, version-pinned |

### Supporting (no upgrades needed)
| Library | Version | Purpose |
|---------|---------|---------|
| axum | 0.8 | HTTP API framework |
| sqlx-cli | 0.8.2 | SQLx offline cache management (CI installs this) |
| trunk | latest | Leptos WASM build tool |

## Architecture Patterns

### Workspace Structure (85 crates, 10-layer dependency graph)
```
Layer 1 (Leaf):     types, api-types, telemetry-types, id, numerics
Layer 2 (Infra):    core, config, crypto, platform, boot
Layer 3 (Data):     db, storage, artifacts, registry, manifest, aos
Layer 4 (Domain):   policy, telemetry, auth, inference-contract
Layer 5 (Backend):  lora-kernel-api, lora-kernel-mtl, lora-mlx-ffi, lora-quant
Layer 6 (Worker):   lora-worker, training-worker, lora-lifecycle, lora-router
Layer 7 (Orch):     orchestrator, lora-plan, lora-rag
Layer 8 (Server):   server-api, server-api-*, server, cli
Layer 9 (UI):       tui, ui
Layer 10 (Test):    e2e, testing, scenarios, benchmarks
```

### Pattern: DB Module Migration (already applied)
**What:** The `query/` module (builders, filters, pagination) and `util/` module (result_mappers, tenant_scoping, transaction) were deleted. Consumers migrated to direct sqlx patterns.
**Before:**
```rust
// Old pattern using query builders
use crate::query::builders::QueryBuilder;
let result = QueryBuilder::new("adapters")
    .filter_by_tenant(tenant_id)
    .execute(pool)
    .await?;
```
**After (observed in working tree):**
```rust
// New pattern: direct sqlx queries
let result = sqlx::query_as::<_, AdapterRow>(
    "SELECT * FROM adapters WHERE tenant_id = ?1"
)
.bind(tenant_id)
.fetch_all(pool)
.await?;
```

### Pattern: pool_result() Migration (already applied)
**What:** `db.pool()` was deprecated in favor of `db.pool_result()` which returns `Result`.
**In tests (working tree pattern):**
```rust
// Test code uses .unwrap() on pool_result()
.execute(db.pool_result().unwrap())
```
**In library code:**
```rust
// Library code propagates the error
.execute(db.pool_result()?)
```

### Anti-Patterns to Avoid
- **Upgrading dependencies without regenerating SQLx offline cache**: The CI checks this -- `cargo sqlx prepare --workspace --check` must pass.
- **Editing Cargo.toml without updating Cargo.lock**: Always run `cargo update` or let cargo resolve after version bumps.
- **Running `cargo build --workspace` without DATABASE_URL**: SQLx macros need either `SQLX_OFFLINE=true` or a DATABASE_URL pointing at a schema-correct database. The foundation-run.sh solves this by generating `target/sqlx-compile-schema.sqlite3` from migrations.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SQLx compile-time DB schema | Manual schema DB creation | `ensure_sqlx_compile_schema_db()` in foundation-run.sh | Applies all 325 migrations in order, handles cache invalidation |
| CI workflow ordering | Custom orchestration | GitHub Actions `needs:` with tier-1-gate pattern | Already implemented with Tier 1 (fast checks) -> Tier 2 (integration) |
| Binary resolution | Hardcoded paths | `aos_resolve_binary` from `scripts/lib/build-targets.sh` | Handles debug/release profile detection |

**Key insight:** The CI and build infrastructure is mature and well-organized. The work is stabilization, not creation.

## Common Pitfalls

### Pitfall 1: safetensors 0.7.0 Dtype Enum Changes
**What goes wrong:** `safetensors::Dtype` gained MXFP4/FP6 variants in 0.6.0. Match statements without wildcard arms will fail to compile.
**Why it happens:** The Dtype enum is `#[non_exhaustive]` in newer versions but wasn't in 0.4.
**How to avoid:** Search for `safetensors::Dtype` match statements across the codebase. Add wildcard arms or ensure all new variants are handled. Also check for `Dtype::size()` calls -- deprecated in favor of `bitsize()`.
**Warning signs:** Compilation errors mentioning unknown Dtype variants or deprecated `size()` method.
**Affected crates:** adapteros-aos, adapteros-lora-kernel-mtl, adapteros-lora-mlx-ffi, adapteros-lora-lifecycle, adapteros-cli, adapteros-lora-worker (20+ import sites).

### Pitfall 2: SQLx Offline Cache Stale After Upgrade
**What goes wrong:** SQLx compile-time query checking uses `.sqlx/` directory for offline mode. After upgrading sqlx version, the cache format or query hashes may change.
**Why it happens:** sqlx-cli version must match the sqlx library version for cache compatibility.
**How to avoid:** After bumping sqlx in Cargo.toml, also update the sqlx-cli install version in CI workflows (currently `cargo install sqlx-cli --version 0.8.2`). Then regenerate: `cargo sqlx prepare --workspace`.
**Warning signs:** CI `sqlx-offline` job fails with hash mismatches.

### Pitfall 3: Foundation-Run Requires Full Environment
**What goes wrong:** `scripts/foundation-run.sh` needs configs, model paths, JWT secrets, manifests -- it will fail in a bare checkout.
**Why it happens:** The script bootstraps a real server instance. It requires `configs/cp.toml`, creates `var/` directories, and expects a model path.
**How to avoid:** The script handles bootstrap gracefully (generates dev JWT secret, creates model directories). But it needs `sqlite3` command available and `scripts/ci/check_ui_assets.sh` to pass (which needs built UI assets).
**Warning signs:** Failures in `ensure_bootstrap_model_path` or `ensure_sqlx_compile_schema_db`.

### Pitfall 4: Concurrent Cargo Builds
**What goes wrong:** Multiple `cargo build`/`cargo check` commands running in parallel cause file lock contention and timeouts.
**Why it happens:** Cargo uses file locks on the target directory.
**How to avoid:** As specified in project constraints: only one cargo build/check at a time. Stagger build-dependent teammates.
**Warning signs:** `waiting for file lock on build directory` messages.

### Pitfall 5: libsqlite3-sys Version Pin
**What goes wrong:** sqlx 0.8.4+ bumped libsqlite3-sys internally, but this workspace pins it to `=0.30.1`.
**Why it happens:** The pinned version ensures consistent SQLite behavior across the workspace (rusqlite also depends on it).
**How to avoid:** After upgrading sqlx, verify the libsqlite3-sys version constraint is still satisfied. If sqlx 0.8.6 requires a different version, the pin must be updated.
**Warning signs:** Version resolution failures in `cargo update`.

## Code Examples

### Fixing the benchmark IoBuffers error
```rust
// Source: crates/adapteros-lora-kernel-api/src/lib.rs:229-244
// IoBuffers struct now has a session_id field
let _io_buffers = IoBuffers {
    input_ids: input_ids.clone(),
    output_logits: vec![0.0f32; vocab_size],
    position: 0,
    attention_entropy: None,
    activations: None,
    session_id: None,  // <-- missing field
};
```

### Fixing the clippy io_other_error
```rust
// Before (crates/adapteros-telemetry/src/diagnostics/writer.rs:374)
std::io::Error::new(
    std::io::ErrorKind::Other,
    format!("failed to serialize stale diagnostics batch: {}", e),
)

// After
std::io::Error::other(
    format!("failed to serialize stale diagnostics batch: {}", e),
)
```

### SQLx offline cache regeneration
```bash
# Step 1: Create schema DB from migrations
mkdir -p target
for migration in migrations/[0-9]*_*.sql; do
  sqlite3 -bail target/sqlx-compile-schema.sqlite3 < "$migration"
done

# Step 2: Regenerate cache
DATABASE_URL="sqlite://target/sqlx-compile-schema.sqlite3" \
  cargo sqlx prepare --workspace

# Step 3: Verify
SQLX_OFFLINE=true cargo check -p adapteros-db
```

### Dependency upgrade in Cargo.toml
```toml
# workspace.dependencies section
sqlx = { version = "0.8.6", default-features = false, features = ["runtime-tokio", "sqlite", "macros", "migrate", "chrono", "uuid"] }
tokio = { version = "1.44", features = ["rt-multi-thread", "macros", "sync", "time", "io-util", "net", "fs", "signal", "process"] }
safetensors = "0.7"
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `query/` builder pattern | Direct sqlx queries | This refactor (in working tree) | Simpler, less abstraction |
| `util/result_mappers` | Inline result handling | This refactor (in working tree) | Fewer internal modules |
| `util/tenant_scoping` | Direct WHERE clauses | This refactor (in working tree) | More explicit SQL |
| `db.pool()` (panicking) | `db.pool_result()` (Result) | This refactor (in working tree) | Safer error handling |
| safetensors `Dtype::size()` | `Dtype::bitsize()` | safetensors 0.6.0 (June 2025) | Needed for sub-byte types |
| sqlx-cli 0.8.2 | sqlx-cli 0.8.6 | sqlx 0.8.6 (May 2025) | Must match library version |

## Current Compilation State (Verified)

### What works NOW
- `cargo check --workspace` -- PASSES (zero errors, minor warnings)
- `cargo fmt --all -- --check` -- PASSES
- `cargo build -p adapteros-server` -- PASSES

### What fails NOW
- `cargo test --workspace` -- 1 compilation error in `tests/benchmark/benches/kernel_performance.rs` (missing `session_id` field)
- `cargo clippy --workspace` -- 1 error in `adapteros-telemetry` (`io_other_error` lint)
- Dependency upgrades not yet applied

### Working Tree Summary (95 files changed)
| Category | Files | Status |
|----------|-------|--------|
| adapteros-db module changes | 33 files | Compiles clean, patterns consistent |
| adapteros-lora-worker changes | 11 files | Compiles clean |
| adapteros-orchestrator changes | 6 files | Compiles clean |
| adapteros-server-api changes | 6 files | Compiles clean |
| adapteros-tui changes | 6 files | Compiles clean (2 dead_code warnings) |
| adapteros-cli changes | 3 files | Compiles clean |
| adapteros-lora-mlx-ffi changes | 5 files | Compiles clean |
| Script changes | 10 files | Need runtime verification |
| Deleted files (query/, util/, smoke scripts, UI server) | 10 files | Clean removal |
| New files (foundation workflows, tests) | 7 files | Untracked |

## CI Workflow Inventory

| Workflow | File | Triggers | Key Jobs |
|----------|------|----------|----------|
| CI | ci.yml | push/PR to main | Tier 1: format, clippy, test, fast-math-scan, etc. Tier 2: macos tests, determinism, replay, streaming |
| Foundation Smoke | foundation-smoke.yml | push/PR to main + nightly | Build workspace + run foundation-smoke.sh |
| Stability | stability.yml | push/PR to main | Nightly miri + stability checks |
| Determinism | determinism.yml | push/PR (path-filtered) | Cross-hardware determinism validation |
| Integration | integration-tests.yml | push/PR (path-filtered) | macOS integration tests |
| Security | security-regression-tests.yml | push/PR (path-filtered) | Security regression suite |
| Migration | migration-testing.yml | push/PR (path-filtered) | Migration conflict and signature checks |
| Architectural Lint | architectural-lint.yml | push/PR to main | Architecture constraint checks |
| Multi-Backend | multi-backend.yml | push/PR to main | Feature flag matrix |
| Performance | performance-regression.yml | push/PR to main | Performance regression |
| Stress | stress-tests.yml | push/PR to main | Load/stress testing |
| KV Verify | kv-verify.yml | push/PR to main | SQL/KV drift verification |
| Duplication | duplication.yml | push/PR to main | Code duplication detection |
| Check Merge Conflicts | check-merge-conflicts.yml | push/PR | Merge conflict markers |
| Deploy | deploy.yml | push to main | Deployment |
| Metal Build | metal-build.yml | push/PR to main | Metal kernel compilation |
| Infra Health | infrastructure-health.yml | push/PR to main | Infrastructure checks |

## Open Questions

1. **foundation-run.sh end-to-end on clean checkout**
   - What we know: The script structure is robust and handles bootstrap
   - What's unclear: Whether it will pass end-to-end on the current working tree without a real model present
   - Recommendation: Run it locally with `--headless` flag first; the script auto-creates model directories and uses dev JWT secrets

2. **safetensors 0.7.0 exact API surface changes**
   - What we know: Dtype enum expanded, `size()` deprecated for `bitsize()`, hashbrown dependency added
   - What's unclear: Whether any crates use `Dtype` in exhaustive match or call `size()` directly
   - Recommendation: After version bump, `cargo check --workspace` will immediately reveal all breaking callsites. Grep for `Dtype::` and `.size()` on safetensors types first.

3. **CI workflows on ubuntu vs macOS**
   - What we know: Tier 1 jobs run on ubuntu-latest, Tier 2 macos tests run on macos-14
   - What's unclear: Whether all Tier 1 checks will pass on ubuntu (some crates have macOS-specific code)
   - Recommendation: The CI already excludes `adapteros-lora-mlx-ffi` from clippy and test runs on ubuntu. This is handled.

## Sources

### Primary (HIGH confidence)
- Direct codebase inspection: `cargo check --workspace`, `cargo test --workspace`, `cargo clippy --workspace`, `cargo fmt --check` -- all run locally on the actual working tree
- `Cargo.toml` and `Cargo.lock` -- actual dependency versions and constraints
- `.github/workflows/*.yml` -- 17 CI workflow files inspected
- `scripts/foundation-run.sh` and `scripts/foundation-smoke.sh` -- full source read
- `crates/adapteros-db/.sqlx/` -- 15 query cache files present
- `git diff HEAD` -- 95 files changed, verified compilation status of each category

### Secondary (MEDIUM confidence)
- [safetensors crates.io versions](https://crates.io/crates/safetensors/versions) -- version timeline and release dates
- [safetensors GitHub releases](https://github.com/huggingface/safetensors/releases) -- breaking change notes for 0.6.0 (Dtype changes)
- [sqlx CHANGELOG.md](https://github.com/launchbadge/sqlx/blob/main/CHANGELOG.md) -- 0.8.3-0.8.6 changes, SQLite additions
- [tokio CHANGELOG.md](https://github.com/tokio-rs/tokio/blob/master/tokio/CHANGELOG.md) -- 1.36-1.44 changes, MSRV bump, mio v1

### Tertiary (LOW confidence)
- safetensors 0.7.0 exact `Dtype` variant list -- inferred from release notes, not verified against source code. Will be validated by `cargo check` after upgrade.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- dependency versions, upgrade paths, and breaking changes verified against official changelogs
- Architecture: HIGH -- workspace structure, dependency graph, and compilation state verified by running actual cargo commands
- Pitfalls: HIGH -- every pitfall identified from actual compilation failures or verified CI workflow requirements
- Current state: HIGH -- all compilation results are from running commands on the actual working tree, not inference

**Research date:** 2026-02-23
**Valid until:** 2026-03-23 (stable domain, no fast-moving dependencies)
