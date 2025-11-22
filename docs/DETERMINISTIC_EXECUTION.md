# Deterministic Execution

**Purpose:** Detailed documentation for deterministic execution patterns in AdapterOS

**Last Updated:** 2025-11-22

---

## Overview

AdapterOS guarantees reproducible execution through HKDF-seeded randomness, global tick ledger synchronization, and multi-agent coordination barriers.

For detailed diagrams and implementation patterns, see:
- [ARCHITECTURE_PATTERNS.md#hkdf-seeding-hierarchy](ARCHITECTURE_PATTERNS.md#hkdf-seeding-hierarchy) - HKDF seed derivation tree
- [ARCHITECTURE_PATTERNS.md#multi-agent-coordination](ARCHITECTURE_PATTERNS.md#multi-agent-coordination) - AgentBarrier synchronization

---

## HKDF Seeding Hierarchy

All randomness in AdapterOS is derived from a global seed using HKDF (HMAC-based Key Derivation Function) with domain separation labels.

### Seed Derivation

**Source:** `crates/adapteros-core/src/hash.rs`

```rust
use adapteros_core::{B3Hash, derive_seed};

// Global seed from manifest hash
let manifest_hash = manifest.compute_hash()?;
let global_seed = derive_seed(&manifest_hash, "executor");

// Domain-separated seeds
let router_seed = derive_seed(&manifest_hash, "router");
let dropout_seed = derive_seed(&manifest_hash, "dropout");
let sampling_seed = derive_seed(&manifest_hash, "sampling");
```

### Domain Labels

| Label | Purpose | Component |
|-------|---------|-----------|
| `router` | K-sparse tie-breaking | `adapteros-lora-router` |
| `dropout` | LoRA dropout masks | `adapteros-lora-worker` |
| `sampling` | Token sampling | `adapteros-lora-mlx-ffi` |
| `lora_trainer` | Weight initialization | `adapteros-lora-worker/training` |
| `gate_noise` | Gate perturbations | `adapteros-lora-router` |
| `executor` | Task scheduling | `adapteros-deterministic-exec` |

---

## Global Tick Ledger

The deterministic executor maintains a global tick counter for serializable execution ordering.

**Source:** `crates/adapteros-deterministic-exec/src/global_ledger.rs`

### Initialization

```rust
use adapteros_deterministic_exec::{init_global_executor, ExecutorConfig};

let config = ExecutorConfig {
    global_seed,
    enable_event_logging: true,
    ..Default::default()
};
init_global_executor(config)?;
```

### Properties

- **Serial FIFO execution:** Tasks execute in deterministic order
- **No concurrent mutation:** Single-threaded task execution
- **Tick-based ordering:** Global tick counter provides total ordering

---

## Multi-Agent Coordination

`AgentBarrier` synchronizes multiple agents at tick boundaries.

**Source:** `crates/adapteros-deterministic-exec/src/multi_agent.rs`

### Usage

```rust
use adapteros_deterministic_exec::AgentBarrier;

let barrier = Arc::new(AgentBarrier::new(vec!["a".into(), "b".into(), "c".into()]));

// All agents wait at barrier
barrier.wait("a", tick).await?;

// Dead agent handling
barrier.mark_agent_dead("c")?;
```

### Failure Handling

- **Timeout:** 30s default, triggers graceful degradation
- **Dead agents:** Explicit removal via `mark_agent_dead()`
- **CAS races:** Handled with Acquire ordering

---

## Policy Enforcement

### Determinism Policy Pack

**Source:** `crates/adapteros-policy/src/packs/determinism.rs`

Validates:
- HKDF-seeded RNG usage (blocks non-deterministic RNG)
- Precompiled metallib embeds (no runtime kernel compilation)
- Global seed format (32-byte hex string)

### Kernel Determinism

Metal kernels are embedded as precompiled `.metallib` files with hash verification:
- Build-time hash in `metallib_manifest.json`
- Runtime verification before kernel execution
- Mismatch triggers `AosError::Kernel`

---

## References

- [ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - Detailed diagrams
- [DETERMINISM_GUARANTEES.md](DETERMINISM_GUARANTEES.md) - Guarantee specifications
- [architecture/DETERMINISTIC_VALIDATION.md](architecture/DETERMINISTIC_VALIDATION.md) - Validation findings
- [CLAUDE.md](../CLAUDE.md) - Developer quick reference
