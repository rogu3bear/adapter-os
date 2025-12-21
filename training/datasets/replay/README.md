# Replay & Verification Training Data

**Purpose:** Train adapters to enable deterministic replay and cross-host verification

## Overview

Replay capability is essential for audit trails, debugging, and cross-host consistency verification. All execution must be reproducible from tick ledger entries.

## Key Concepts

- **Global Tick Ledger:** Merkle chain of execution events
- **RNG Snapshots:** Full ChaCha20Rng state capture
- **HKDF Seeding:** Domain-separated seed derivation
- **Bundle Signatures:** Ed25519-signed telemetry bundles
- **Cross-Host Consistency:** Hash-based verification

## Training Example Schema

```jsonl
{
  "input": {
    "tick": 42,
    "task_id": "task-123",
    "manifest_hash": "blake3-hash",
    "seed_label": "router",
    "rng_snapshot": {
      "global_nonce": 100,
      "step_count": 5
    }
  },
  "target": {
    "entry_hash": "blake3-ledger-hash",
    "prev_hash": "blake3-prev-hash",
    "reproduced_output": "deterministic-result",
    "consistent": true
  },
  "metadata": {
    "quality": 0.95,
    "label": "positive",
    "verified": true
  }
}
```

## Replay Types

1. **Local Replay:** Same host, same manifest
2. **Cross-Host Replay:** Different host, same manifest
3. **Audit Replay:** Historical execution verification
4. **Debugging Replay:** Step-through with RNG state

## Quality Criteria

- **Min Examples:** 200
- **Min Relevance:** 0.95
- **Min Confidence:** 0.95
- **Reproduction Success Rate:** 100%

## Data Sources

1. **Tick Ledger:** `tick_ledger` table
2. **Consistency Reports:** `tick_ledger_consistency_reports` table
3. **Telemetry Bundles:** `telemetry_bundles` with signatures
4. **RNG Traces:** Executor event logs

## Example Datasets

- `tick_ledger_chains/` - Merkle chain examples
- `rng_snapshots/` - ChaCha20Rng state captures
- `cross_host_verification/` - Consistency proofs
- `bundle_signatures/` - Ed25519 signing examples
- `replay_sessions/` - Full session reproduction

## References

- `crates/adapteros-deterministic-exec/src/global_ledger.rs` - Tick ledger
- `crates/adapteros-telemetry/src/bundles.rs` - Bundle signing
- `migrations/0032_tick_ledger.sql` - Ledger schema
- `migrations/0035_tick_ledger_federation.sql` - Federation columns
- `AGENTS.md` - Deterministic executor seeding
