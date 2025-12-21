# Determinism Guardrail Training Data

**Purpose:** Train adapters to enforce reproducible execution and detect non-deterministic behavior

## Overview

Determinism is a core policy in AdapterOS. All randomness must be seeded via HKDF, and execution must be reproducible given identical inputs and seeds.

## Key Concepts

- **HKDF Seeding:** Domain-separated seed derivation from manifest hash
- **Global Tick Ledger:** Merkle chain of execution events
- **RNG Snapshots:** ChaCha20Rng state for replay
- **Barrier Coordination:** Multi-agent synchronization
- **Serial FIFO Execution:** No concurrent task execution

## Training Example Schema

```jsonl
{
  "input": {
    "manifest_hash": "blake3-hash",
    "seed_label": "router",
    "operation": "sample_token"
  },
  "target": {
    "derived_seed": "hkdf-output",
    "rng_state": {
      "global_nonce": 42,
      "step_count": 5
    },
    "output": "deterministic-result"
  },
  "metadata": {
    "quality": 0.95,
    "label": "positive",
    "violation": null
  }
}
```

## Violation Categories

1. **Unseeded Randomness:** Use of `rand::thread_rng()` (BLOCKED)
2. **Concurrent Execution:** Multiple tasks executing in parallel
3. **Non-deterministic Syscalls:** Unbounded waits, async without seed
4. **Tick Mismatch:** Ledger entries out of order
5. **Barrier Timeout:** Agents fail to synchronize

## Quality Criteria

- **Min Examples:** 500
- **Min Relevance:** 0.95
- **Min Confidence:** 0.95
- **Violation Detection Rate:** >99%

## Data Sources

1. **Tick Ledger:** `tick_ledger` table entries
2. **Barrier Events:** `barrier.*` telemetry
3. **RNG Traces:** Deterministic executor logs
4. **Violation Reports:** Policy engine alerts

## Example Datasets

- `hkdf_patterns/` - Seed derivation examples
- `rng_snapshots/` - ChaCha20Rng state transitions
- `barrier_coordination/` - Multi-agent synchronization
- `tick_ledger_consistency/` - Cross-host verification
- `violations/` - Negative examples (policy breaches)

## References

- `crates/adapteros-deterministic-exec/` - Executor implementation
- `crates/adapteros-core/src/hash.rs` - HKDF functions
- `crates/adapteros-deterministic-exec/src/multi_agent.rs` - AgentBarrier
- `AGENTS.md` - Determinism policy (#2)
