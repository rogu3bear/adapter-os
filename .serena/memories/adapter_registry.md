# Adapter Registry and Management System

## Overview

The adapter registry is the central system for managing LoRA adapters in AdapterOS. It provides storage, lifecycle management, versioning, and loading capabilities for adapters used in inference.

## Core Storage Layer (`crates/adapteros-db/`)

### Primary Storage Files

- **`adapters/mod.rs`** - Main adapter storage with dual-write pattern (SQL + KV)
- **`adapter_record.rs`** - Structured adapter record with 36+ fields organized into sub-structures
- **`registry/mod.rs`** - Registry compatibility layer consolidating functionality

### Dual-Write Storage Pattern

The system implements a **SQL + KV dual-write strategy**:

```
SQL (SQLite)          KV Store
     |                   |
     +----> Dual Write <-+
     |                   |
  Authoritative      Validated
```

**Modes:**
- **Strict Atomic** (default): KV failures propagate errors and trigger SQL rollback
- **Best-Effort**: KV writes are logged-on-failure but don't block operations

Configuration via `AOS_ATOMIC_DUAL_WRITE_STRICT` environment variable.

### Adapter Record Structure (`AdapterRecordV1`)

Composed of specialized sub-structures:

| Sub-Structure | Purpose |
|---------------|---------|
| `AdapterIdentity` | Core ID, adapter_id, name, BLAKE3 hash (immutable) |
| `AccessControl` | Tenant ID, ACL JSON |
| `LoRAConfig` | Rank, alpha, target modules JSON |
| `TierConfig` | Tier (persistent/warm/ephemeral), category, scope |
| `LifecycleState` | Current state, load state, lifecycle state, memory, activations |
| `CodeIntelligence` | Framework, repo_id, commit_sha, languages |
| `SemanticNaming` | Taxonomy: {tenant}/{domain}/{purpose}/{revision} |
| `ForkMetadata` | Parent ID, fork type (parameter/data/architecture) |
| `ArtifactInfo` | .aos file path/hash, determinism metadata |
| `SchemaMetadata` | Version, timestamps |

## Lifecycle Management

### Lifecycle States

```
Draft -> Training -> Ready -> Active -> Deprecated -> Retired
           \          \        \ rollback /
            \          \---------> Failed
             \------------------>
```

**State Definitions:**
- `draft` - Initial registration, not yet trained
- `training` - Currently being trained
- `ready` - Training complete, awaiting activation
- `active` - In production use
- `deprecated` - Marked for phase-out
- `retired` - End of life, no longer usable
- `failed` - Training or validation failed

### Tier-Specific Rules

| Tier | Behavior |
|------|----------|
| `persistent` | Full lifecycle, never auto-evicted |
| `warm` | Standard lifecycle with eviction under pressure |
| `ephemeral` | Active -> Retired directly (skips Deprecated) |

### Lifecycle Transitions (`lifecycle.rs`)

Key operations:
- `transition_adapter_lifecycle()` - Atomically transitions state with history recording
- `transition_stack_lifecycle()` - Transitions stack lifecycle with composition snapshot
- `get_adapter_lifecycle_history()` - Returns all transitions ordered by timestamp
- `check_active_stack_references()` - Validates adapter isn't in active stacks before deprecation

Uses IMMEDIATE transactions to prevent race conditions.

### Lifecycle Rules (`lifecycle_rules.rs`)

Configurable rules with:
- **Scopes**: System, Tenant, Category, Adapter
- **Types**: TTL, Retention, Demotion, Promotion, Archival, Cleanup, StateTransition
- **Actions**: Evict, Delete, TransitionState, Archive, Notify

## Adapter Stacks (`stacks_kv.rs`)

Stacks are ordered collections of adapters for inference.

### Stack KV Keys
```
tenant/{tenant_id}/stack/{stack_id}        -> AdapterStackKv (primary)
tenant/{tenant_id}/stacks                   -> Vec<stack_id> (listing)
tenant/{tenant_id}/stack-by-name/{name}     -> stack_id (name lookup)
tenant/{tenant_id}/stacks-by-state/{state}  -> Vec<stack_id> (state filter)
stack-lookup/{stack_id}                     -> tenant_id (reverse lookup)
```

### Stack Operations
- Create, update, delete stacks
- Add/remove adapters from stack
- Reorder adapters within stack
- Activate/deactivate stack
- Workflow types: Sequential, Parallel, etc.

## Collections (`collections_kv.rs`)

Document collections for training data organization.

### Collection KV Keys
```
tenant/{tenant_id}/collection/{id}                  -> DocumentCollectionKv
tenant/{tenant_id}/collections                       -> Vec<collection_id>
tenant/{tenant_id}/collection-by-name/{name}         -> collection_id
tenant/{tenant_id}/collection/{id}/doc/{doc_id}      -> CollectionDocumentLink
tenant/{tenant_id}/document/{doc_id}/collections     -> Vec<collection_id>
```

## Lineage and Versioning (`registry/lineage.rs`)

### Lineage Validation
- **Revision Monotonicity**: New revisions must be greater than existing
- **Gap Constraints**: Cannot skip more than 5 revisions
- **Circular Dependency Detection**: Uses SQLite recursive CTEs

### Semantic Naming Convention
```
{tenant_namespace}/{domain}/{purpose}/{revision}
Example: acme-corp/engineering/code-review/r042
```

Revision format: `rNNN` (e.g., r001, r042)

## Promotion System (`promotions.rs`)

Golden run promotion workflow:
1. Create promotion request
2. Record gate results (validation checks)
3. Record approvals (Ed25519 signed)
4. Update golden run stage
5. Record promotion history

Supports rollback to previous golden run.

## Eviction System (`registry/eviction.rs`)

### Zeroization Policy
- Zeroize system memory (default: true)
- Zeroize VRAM (default: true)
- Zeroize disk cache (default: false)
- Configurable zeroization passes

### Eviction Strategies
- LRU (Least Recently Used)
- Priority-based (lowest first)
- Size-based (largest first)
- Custom order

### Memory Pressure Selector
Triggers eviction when memory pressure exceeds threshold.

## Adapter Loading for Inference (`adapteros-lora-lifecycle/src/loader.rs`)

### Supported Formats (Priority Order)
1. `.sealed` - Cryptographically sealed containers (Ed25519 signed)
2. `.aos` - Standard adapter archive format
3. `.safetensors` - Raw weights

### Loading Process
1. Resolve adapter path (flat or directory structure)
2. Verify signatures (required in production)
3. Validate kernel version compatibility
4. Verify per-layer hashes for integrity
5. Extract SafeTensors weights
6. Validate CoreML placement spec (if present)
7. Compare against expected hash
8. Return `AdapterHandle` with metadata

### Security Features
- Ed25519 signature verification
- BLAKE3 hash verification (whole-adapter and per-layer)
- Base model identity validation
- Trusted signer key management
- Zeroize-on-drop for weights

## Registration (`adapters/mod.rs`)

### AdapterRegistrationBuilder

Fluent builder pattern with validation:
```rust
AdapterRegistrationBuilder::new()
    .adapter_id("my-adapter")
    .tenant_id("tenant-1")
    .name("My Adapter")
    .hash_b3("b3:...")
    .rank(8)
    .tier("warm")
    .with_aos_metadata(&metadata)
    .build()?
```

### Required Fields
- `adapter_id`, `name`, `hash_b3`, `rank`

### Auto-Derived Fields
- `alpha` defaults to `rank * 2.0`
- `tier` defaults to "warm"
- `content_hash_b3` defaults to `hash_b3`
- `category` defaults to "code"
- `scope` defaults to "global"

## Key Database Tables

- `adapters` - Main adapter records (60+ columns)
- `adapter_stacks` - Stack definitions
- `adapter_lifecycle_history` - Transition audit trail
- `stack_version_history` - Stack version snapshots
- `lifecycle_rules` - Configurable lifecycle rules
- `golden_run_promotion_requests` - Promotion workflow
- `aos_adapter_metadata` - Extended .aos file metadata

## Integration Points

### Server API
- REST endpoints for CRUD operations
- Lifecycle transition handlers
- Stack management endpoints

### Inference Engine
- `AdapterLoader` for hot-swap loading
- Hash verification pipeline
- Memory management with zeroization

### Training Pipeline
- Dataset linkage via `training_dataset_hash_b3`
- Checkpoint management
- Provenance tracking via `provenance_json`
