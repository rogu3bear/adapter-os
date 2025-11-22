<!-- 92756382-2cc0-4ef6-9dd1-0cdd12512b56 f5f10e1c-1c34-478a-8d0e-a315d10d8286 -->
# AdapterOS Codebase Unification (Phases 0-5)

## Phase 0: Inventory & Planning

Generate machine-readable inventories of current state:

**Deliverables:**

- `tools/inventory/crates.json` - all crates with paths, deps, current naming
- `tools/inventory/policies.json` - existing Policy impls with file locations
- `tools/inventory/configs.json` - all *.toml files with precedence hints
- `tools/inventory/migrations.json` - ordered migrations with checksums
- `tools/inventory/metal.json` - kernel names, parameters, call sites
- `docs/RENAMING_PLAN.md` - mapping {mplora-*, aos-*} → adapteros-*

**Critical files to inventory:**

- Workspace members from `Cargo.toml` (56 total)
- Policy implementations in `crates/mplora-policy/src/`
- Metal shaders: `aos_kernels.metal`, `fused_attention.metal`, `common.metal`
- Migrations in `migrations/*.sql` (13 files + 3 disabled)
- Config files: `configs/cp.toml`, manifest examples

## Phase 1: Naming Unification

Apply systematic rename to `adapteros-*` namespace with backward compatibility.

**Rename strategy:**

- `mplora-*` → `adapteros-lora-*` (keep MPLoRA as feature module)
- Core infrastructure → `adapteros-*` (e.g., `mplora-core` → `adapteros-core`)
- Keep existing `adapteros-*` crates as-is

**Compatibility layer:**

- Create shim crates in `crates/compat/mplora-*/` with:
  - `pub use adapteros_lora_*::*;`
  - `#[deprecated(since = "0.2.0", note = "Use adapteros-lora-* instead")]`
- Add `compat/` to workspace members
- Update root `Cargo.toml` workspace dependencies

**Files to update:**

- Root `Cargo.toml` workspace members (lines 11-57)
- All `Cargo.toml` files with inter-crate dependencies
- Binary entry points: `crates/mplora-cli/src/main.rs`, `crates/mplora-server/src/main.rs`
- Test files in `tests/` directory
- Documentation references in `docs/`, `CLAUDE.md`, `README.md`

## Phase 2: Policy Registry (20 Canonical Packs)

Implement enforceable registry with exactly 20 policy packs.

**Canonical 20 packs:**

1. Egress, 2. Determinism, 3. Router, 4. Evidence, 5. Refusal
2. Numeric, 7. RAG, 8. Isolation, 9. Telemetry, 10. Retention
3. Performance, 12. Memory, 13. Artifacts, 14. Secrets, 15. Build/Release
4. Compliance, 17. Incident, 18. Output, 19. Adapters, 20. Deterministic I/O

**Implementation:**

- New crate: `crates/adapteros-policy/`
- Core registry: `src/registry.rs` with `pub static POLICY_INDEX: [PolicySpec; 20]`
- Trait: `pub trait Policy { fn enforce(&self, ctx: &Context) -> Result<Audit, Violation>; }`
- Each pack in separate module: `src/packs/{egress,determinism,router,...}.rs`

**CLI integration:**

- Extend `crates/adapteros-cli/src/commands/policy.rs`:
  - `adapteros policy list` - show all 20 with status
  - `adapteros policy explain <pack>` - detailed documentation
  - `adapteros policy enforce [--pack <id>|--all] [--dry-run]` - run checks

**Documentation:**

- Auto-generate `docs/POLICIES.md` from registry doc comments
- CI check: `scripts/verify_policy_count.sh` fails if count ≠ 20

**Integration points:**

- `crates/adapteros-lora-worker/src/lib.rs` - call policy checks
- `crates/adapteros-lora-server/src/main.rs` - load policy config

## Phase 3: Metal Kernel Refactor

Modularize monolithic kernels with parameter structs and versioned hashing.

**Split strategy:**

- `metal/aos_kernels.metal` (366 lines) → split by function:
  - `metal/src/kernels/attention.metal` - attention ops from fused_attention.metal
  - `metal/src/kernels/mlp.metal` - MLP ops (currently empty fused_mlp.metal)
  - `metal/src/kernels/lora.metal` - LoRA mixing and routing
  - `metal/src/kernels/utils.metal` - common utilities (absorb common.metal)
- Keep shared types in `metal/src/types.metal`

**Parameter structs:**

Replace 50+ parameter kernel signatures with:

```metal
struct LoraConfig {
    uint adapter_count;
    uint k_sparse;
    uint rank;
    float entropy_floor;
};

struct RingBuffer {
    device float* data;
    uint capacity;
    uint head;
    uint tail;
};
```

**Kernel registry:**

- `metal/kernels.json`:
```json
{
  "kernels": [
    {"name": "fused_attention_lora", "version": "1.0", "hash": "b3:..."},
    {"name": "sparse_mlp", "version": "1.0", "hash": "b3:..."}
  ],
  "compiler": {"version": "...", "sdk": "...", "flags": [...]}
}
```


**Build system:**

- Update `metal/build.sh` to:
  - Compile modular sources
  - Compute BLAKE3 hashes of `.metallib` outputs
  - Write to `metal/build_metadata.json`
  - Fail if hashes drift unless `--update-kernels` flag set
- Update `metal/ci_build.sh` for deterministic CI builds

**Testing:**

- Add `tests/metal_determinism.rs`:
  - Load kernels, verify hashes match registry
  - Compile twice, assert byte-identical outputs

**Documentation:**

- `metal/README.md` - kernel module structure, parameter struct fields
- Document strict math flags and compiler fingerprinting

## Phase 4: Deterministic Config System

Freeze configuration at startup with precedence enforcement.

**New crate: `crates/adapteros-config/`**

**Core implementation:**

- `src/precedence.rs`:
  - Load order: CLI args > ENV vars > manifest file
  - Struct: `DeterministicConfig` with hash method
  - Freeze at startup, return immutable ref
- `src/loader.rs`:
  - Parse manifest (TOML)
  - Merge with `std::env::vars()` filtered by prefix
  - Override with CLI flags
- `src/guards.rs`:
  - Runtime guard: panic if `std::env::var()` called post-freeze
  - Compile-time lint via `adapteros-lint`

**Integration:**

- Replace env reads in:
  - `crates/adapteros-lora-server/src/main.rs`
  - `crates/adapteros-lora-worker/src/lib.rs`
  - `crates/adapteros-lora-cli/src/main.rs`
- Inject `&DeterministicConfig` instead of ad-hoc reads

**Trace integration:**

- Hash config at freeze time
- Include hash in root trace node (`crates/adapteros-trace/src/writer.rs`)

**Testing:**

- `tests/config_precedence.rs`:
  - Test CLI overrides ENV overrides manifest
  - Test freeze enforcement (mutate env → panic in strict mode)
  - Test hash stability (same inputs → same hash)

**Documentation:**

- `docs/CONFIG_PRECEDENCE.md`:
  - Precedence rules with examples
  - Freeze mechanism explanation
  - Failure modes and debugging

## Phase 5: Database Schema Lifecycle

Formalize migration tracking with version gates and rollback playbooks.

**Schema version table:**

- Add `migrations/0017_schema_version.sql`:
```sql
CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL,
    checksum TEXT NOT NULL
);
```


**Startup gate:**

- Update `crates/adapteros-lora-db/src/lib.rs`:
  - Check `schema_version` against embedded migration list
  - Refuse to start if mismatch
  - Log required vs applied versions

**CLI commands:**

- `adapteros db verify`:
  - Compare applied migrations vs repo
  - Print diff with checksums
  - Exit code 0 if match, 1 if drift
- `adapteros db migrate`:
  - Apply pending migrations
  - Update schema_version table

**Documentation generation:**

- Script: `scripts/generate_migration_docs.sh`
- Parse `migrations/*.sql` comments for:
  - Purpose (from `-- Purpose:` comment)
  - Tables impacted (from `CREATE/ALTER` statements)
  - Forward/backward compatibility notes
- Output to `docs/MIGRATIONS.md`

**Rollback playbooks:**

- Create `docs/rollback/` directory
- Document last 3 migrations with inverse operations:
  - `docs/rollback/0016_git_sessions.md`
  - `docs/rollback/0015_previous.md`
  - `docs/rollback/0014_previous.md`

**Files to modify:**

- `crates/adapteros-lora-db/src/lib.rs` - add version checks
- `crates/adapteros-cli/src/commands/db.rs` - add verify/migrate subcommands
- `migrations/` - add schema_version table

## Acceptance Gates

**Phase 0-2 Gates (Prompt A):**

- [ ] `adapteros policy list` prints exactly 20 canonical packs
- [ ] `adapteros policy explain determinism` shows detailed docs
- [ ] All rename shims compile with deprecation warnings
- [ ] Inventories committed: `tools/inventory/*.json`
- [ ] `cargo build --release` twice → identical binary checksums

**Phase 3-5 Gates (Prompt B):**

- [ ] Metal kernel hash snapshots pass in CI
- [ ] `adapteros about --json` shows toolchain + kernel hashes
- [ ] Config freeze enforced: `tests/config_precedence.rs` passes
- [ ] App refuses to start on stale schema
- [ ] `adapteros db verify` exits 0 on clean repo

**Determinism verification:**

- [ ] `adapteros verify --trace t1 --trace t2` reports identical hashes
- [ ] Epsilon bounds maintained within configured thresholds
- [ ] No nondeterministic I/O, timestamps, or RNG outside HKDF seeding

### To-dos

- [ ] Generate machine-readable inventories (crates, policies, configs, migrations, metal) and RENAMING_PLAN.md
- [ ] Apply systematic rename to adapteros-* with compatibility shims and workspace updates
- [ ] Implement 20-pack policy registry with enforcement trait, CLI commands, and auto-generated docs
- [ ] Add CI check that fails if policy count ≠ 20 or names deviate
- [ ] Split monolithic Metal kernels into modular sources with parameter structs
- [ ] Create kernel registry JSON with versioned BLAKE3 hashes and build metadata
- [ ] Add Metal determinism tests with hash snapshots and compiler fingerprinting
- [ ] Create adapteros-config crate with precedence rules and freeze mechanism
- [ ] Replace ad-hoc env reads with DeterministicConfig injection across worker/server/cli
- [ ] Add runtime guards and tests for config freeze enforcement
- [ ] Add schema_version table and startup gate refusing stale migrations
- [ ] Implement adapteros db verify/migrate CLI commands
- [ ] Auto-generate MIGRATIONS.md and create rollback playbooks for last 3 migrations
- [ ] Run all acceptance gates: policy list, Metal hashes, config freeze, DB verify, determinism checkslay out a plan to patch in full with citations per best practice and codebase standardsl
- [ ] conduct a full hallucination audit with citations
- [ ] lay out a plan to patch in full with citations per best practice and codebase standards
- [ ] execute the plan in full per best practices and codebase standards 