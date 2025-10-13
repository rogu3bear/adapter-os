# AdapterOS Naming Unification Plan

**Version:** 1.0  
**Date:** 2025-10-13  
**Status:** Approved for implementation

## Executive Summary

This document defines the systematic rename of all crates from `mplora-*` and `aos-*` prefixes to the canonical `adapteros-*` namespace. The goal is to establish **AdapterOS** as the single product brand, with **MPLoRA** (Memory-Parallel Low-Rank Adaptation) as a feature module within the system.

## Naming Philosophy

- **Brand:** AdapterOS (single canonical name)
- **Feature Module:** MPLoRA lives under `adapteros-lora-*` namespace
- **Infrastructure:** Core infrastructure uses `adapteros-*` directly
- **Determinism:** Specialized determinism crates already use `adapteros-*` (keep as-is)

## Rename Mapping

### Category: Core Infrastructure

| Current Name | New Name | Rationale |
|--------------|----------|-----------|
| `mplora-core` | `adapteros-core` | Foundation types and error handling |
| `mplora-crypto` | `adapteros-crypto` | Cryptographic primitives |
| `mplora-manifest` | `adapteros-manifest` | Configuration and manifest parsing |
| `mplora-registry` | `adapteros-registry` | Adapter registry management |
| `mplora-artifacts` | `adapteros-artifacts` | Content-addressed artifact store |
| `mplora-db` | `adapteros-db` | Database layer and migrations |
| `mplora-git` | `adapteros-git` | Git repository integration |
| `mplora-node` | `adapteros-node` | Node management |

### Category: LoRA Feature Module

| Current Name | New Name | Rationale |
|--------------|----------|-----------|
| `mplora-kernel-api` | `adapteros-lora-kernel-api` | Kernel trait definitions |
| `mplora-kernel-mtl` | `adapteros-lora-kernel-mtl` | Metal kernel implementations |
| `mplora-kernel-prof` | `adapteros-lora-kernel-prof` | Kernel profiling |
| `mplora-router` | `adapteros-lora-router` | K-sparse routing |
| `mplora-rag` | `adapteros-lora-rag` | Evidence retrieval |
| `mplora-worker` | `adapteros-lora-worker` | Inference engine |
| `mplora-plan` | `adapteros-lora-plan` | Plan building |
| `mplora-quant` | `adapteros-lora-quant` | Quantization utilities |
| `mplora-lifecycle` | `adapteros-lora-lifecycle` | Adapter lifecycle |
| `mplora-mlx` | `adapteros-lora-mlx` | MLX backend (disabled) |

### Category: Control Plane

| Current Name | New Name | Rationale |
|--------------|----------|-----------|
| `mplora-server` | `adapteros-server` | Control plane API server |
| `mplora-server-api` | `adapteros-server-api` | REST API handlers |
| `mplora-orchestrator` | `adapteros-orchestrator` | Multi-node orchestration |

### Category: Security & Policy

| Current Name | New Name | Rationale |
|--------------|----------|-----------|
| `mplora-policy` | `adapteros-policy` | Policy enforcement (20 packs) |
| `mplora-secd` | `adapteros-secd` | Secure Enclave daemon |
| `mplora-sbom` | `adapteros-sbom` | Software Bill of Materials |

### Category: Observability

| Current Name | New Name | Rationale |
|--------------|----------|-----------|
| `mplora-telemetry` | `adapteros-telemetry` | Event logging and tracing |
| `mplora-system-metrics` | `adapteros-system-metrics` | System metrics collection |
| `mplora-profiler` | `adapteros-profiler` | Adapter profiling |
| `mplora-metrics-exporter` | `adapteros-metrics-exporter` | Metrics export |

### Category: User-Facing Interfaces

| Current Name | New Name | Rationale |
|--------------|----------|-----------|
| `mplora-cli` | `adapteros-cli` | Command-line tool |
| `mplora-api` | `adapteros-api` | Public API types |
| `mplora-client` | `adapteros-client` | Client library |
| `mplora-chat` | `adapteros-chat` | Chat protocol |

### Category: Infrastructure (Special)

| Current Name | New Name | Rationale |
|--------------|----------|-----------|
| `mplora-codegraph` | `adapteros-codegraph` | Code graph analysis |

### Category: No Change Required

| Current Name | New Name | Rationale |
|--------------|----------|-----------|
| `adapteros-deterministic-exec` | `adapteros-deterministic-exec` | Already correct |
| `adapteros-graph` | `adapteros-graph` | Already correct |
| `adapteros-replay` | `adapteros-replay` | Already correct |
| `adapteros-trace` | `adapteros-trace` | Already correct |
| `adapteros-numerics` | `adapteros-numerics` | Already correct |
| `adapteros-memory` | `adapteros-memory` | Already correct |
| `adapteros-compiler-lock` | `adapteros-compiler-lock` | Already correct |
| `adapteros-lint` | `adapteros-lint` | Already correct |
| `adapteros-verify` | `adapteros-verify` | Already correct |
| `adapteros-domain` | `adapteros-domain` | Already correct |
| `xtask` | `xtask` | Standard Rust convention |
| `fuzz` | `fuzz` | Standard fuzzing crate |

## Compatibility Layer

To avoid breaking existing code immediately, we will create **compatibility shim crates** in `crates/compat/` that re-export the new names with deprecation warnings.

### Shim Structure

```
crates/compat/
├── mplora-core/
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs  (pub use adapteros_core::*; with #[deprecated])
├── mplora-worker/
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs  (pub use adapteros_lora_worker::*; with #[deprecated])
└── ... (one shim per renamed crate)
```

### Example Shim: `crates/compat/mplora-core/src/lib.rs`

```rust
#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-core instead. This compatibility crate will be removed in 0.3.0."
)]

pub use adapteros_core::*;
```

### Example Shim: `crates/compat/mplora-worker/src/lib.rs`

```rust
#![deprecated(
    since = "0.2.0",
    note = "Use adapteros-lora-worker instead. This compatibility crate will be removed in 0.3.0."
)]

pub use adapteros_lora_worker::*;
```

## Binary Renames

### CLI Tool

- **Current:** `aosctl` (and references to `mplora`)
- **New:** `adapteros`
- **Transition:** Provide `aosctl` as a symlink or wrapper script with deprecation message

## Documentation Updates

All documentation must be updated to reflect the new naming:

### Files to Update

- `README.md` - Update all references to mplora/aos → adapteros
- `CLAUDE.md` - Update architecture documentation
- `docs/*.md` - All documentation files
- `configs/cp.toml` - Comments and configuration keys
- `examples/*.rs` - Example code and comments
- Test files in `tests/` - Update imports and references

### Search & Replace Patterns

1. `mplora` → `adapteros` or `adapteros-lora` (context-dependent)
2. `MPLoRA` → `AdapterOS` (product references)
3. `aosctl` → `adapteros` (CLI tool)
4. `aos-*` → `adapteros-*` (prefix references)

### Keep MPLoRA References

- Technical documentation referring to the LoRA routing algorithm
- Academic citations and references
- Internal comments explaining the "Memory-Parallel Low-Rank Adaptation" feature

## Implementation Steps

### Phase 1: Directory Renames (Day 1)

1. Rename all `crates/mplora-*` directories to `crates/adapteros-*` or `crates/adapteros-lora-*`
2. Update `Cargo.toml` `[package]` name fields
3. Update workspace `members` list in root `Cargo.toml`

### Phase 2: Compatibility Shims (Day 1)

1. Create `crates/compat/` directory
2. Generate shim crates for all renamed packages
3. Add shims to workspace members
4. Verify shims compile with deprecation warnings

### Phase 3: Internal References (Day 2-3)

1. Update all `Cargo.toml` dependencies to use new names
2. Update all Rust imports (`use mplora_*` → `use adapteros_*`)
3. Update all binary entry points (`main.rs` files)
4. Update test files

### Phase 4: Documentation (Day 3)

1. Update all markdown documentation
2. Update code comments
3. Update CLI help text
4. Update error messages

### Phase 5: Verification (Day 4)

1. Run `cargo check --workspace` - must pass
2. Run `cargo test --workspace` - must pass
3. Run `cargo clippy --workspace` - must pass
4. Verify deprecation warnings appear for old imports
5. Build release binaries twice, verify identical checksums

## Rollback Plan

If critical issues are discovered:

1. **Immediate:** Git revert to pre-rename commit
2. **Shims:** Shim crates continue to work during rollback
3. **Timeline:** One release cycle with shims allows safe rollback

## Migration Guide for Users

To be published with v0.2.0:

```markdown
# Migration Guide: v0.1 → v0.2

## Crate Renames

All `mplora-*` crates have been renamed to `adapteros-*`:

- Core crates: `mplora-core` → `adapteros-core`
- LoRA crates: `mplora-worker` → `adapteros-lora-worker`

## Automatic Migration

Compatibility shims are provided in v0.2.0:

```toml
# Old (works with deprecation warning)
mplora-worker = "0.2"

# New (recommended)
adapteros-lora-worker = "0.2"
```

## CLI Tool Rename

The `aosctl` command is now `adapteros`:

```bash
# Old
aosctl policy list

# New
adapteros policy list
```

## Import Updates

```rust
// Old
use mplora_worker::Worker;

// New
use adapteros_lora_worker::Worker;
```

## Timeline

- **v0.2.0:** Shims available, deprecation warnings
- **v0.3.0:** Shims removed, old names no longer work
```

## Success Criteria

- [ ] All crates renamed according to mapping
- [ ] All compatibility shims created and compiling
- [ ] Workspace builds successfully with `cargo build --workspace`
- [ ] All tests pass with `cargo test --workspace`
- [ ] Documentation updated consistently
- [ ] CLI tool renamed and functional
- [ ] Deprecation warnings present and informative
- [ ] Binary checksums identical on repeated builds
- [ ] Migration guide published

## References

- Inventory: `tools/inventory/crates.json`
- Policy Canon: `tools/inventory/policies.json`
- Issue Tracker: GitHub milestones for v0.2.0
- RFC: [Internal RFC-001: Naming Unification]

---

**Approved by:** System Architect  
**Implementation Start:** 2025-10-13  
**Target Completion:** 2025-10-17 (4 days)

