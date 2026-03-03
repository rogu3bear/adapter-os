# Phase 1: Compilation and CI Foundation - Context

**Gathered:** 2026-02-23
**Status:** Ready for planning

<domain>
## Phase Boundary

Restore clean workspace compilation across all 85 crates, get all CI gate workflows passing, and verify the foundation-run script works end-to-end. This phase fixes the ~70 modified files from the incomplete DB refactor, applies P0 dependency upgrades, and synchronizes the SQLx offline cache. No new features — pure stabilization.

</domain>

<decisions>
## Implementation Decisions

### Fix ordering strategy
- Bottom-up through the 10-layer dependency graph: leaf types → infrastructure → data → domain → backend → worker → orchestration → server → UI → testing
- DB layer (Layer 3) is the cascade epicenter — fix `adapteros-db` consumers first since deleted `query/` and `util/` modules affect everything above
- Compile-check each layer before moving up: `cargo check -p <crate>` per layer, then `cargo check --workspace` at the end

### Deleted module migration
- `crates/adapteros-db/src/query/` (builders, filters, pagination) — consumers must migrate to direct sqlx query patterns or inline equivalents
- `crates/adapteros-db/src/util/` (result_mappers, tenant_scoping, transaction) — consumers must use the replacement patterns already established in the modified DB modules
- Analyze the already-modified files (they show the target pattern) and apply the same pattern to any remaining consumers that reference deleted modules

### Dependency upgrade approach
- Apply P0 upgrades in a single Cargo.toml edit: sqlx 0.8.2→0.8.6, safetensors 0.4→0.7.0, tokio 1.35→1.44
- After Cargo.toml updates, regenerate SQLx offline cache with `cargo sqlx prepare --workspace`
- Fix any breaking API changes from upgrades (sqlx 0.8.6 and safetensors 0.7.0 have known API surface changes)

### CI gate priority
- Get `cargo check --workspace` green first (blocks everything)
- Then `cargo test --workspace` (catches runtime issues)
- Then individual CI workflows: ci → stability → determinism → integration → security → migration → contracts
- Foundation-run.sh is the final gate: build succeeds, server boots through 12 phases, smoke tests pass

### Claude's Discretion
- Exact inline patterns to replace deleted query builders (whatever the existing modified files demonstrate)
- Order of crate fixes within each dependency layer
- Whether to fix clippy warnings encountered during compilation (fix if trivial, skip if unrelated to phase scope)
- SQLx offline cache regeneration approach (prepare --workspace vs per-crate)

</decisions>

<specifics>
## Specific Ideas

- The already-modified files in the working tree show the target patterns — use those as the template for remaining migrations
- `scripts/foundation-run.sh` is the single-command validation: build + boot (12-phase startup) + smoke tests
- Deleted `crates/adapteros-ui/crates/adapteros-server/src/main.rs` needs its references cleaned up
- `scripts/smoke-reference.sh` and `scripts/smoke_reference.sh` were deleted — verify no scripts reference them

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 01-compilation-and-ci-foundation*
*Context gathered: 2026-02-23*
