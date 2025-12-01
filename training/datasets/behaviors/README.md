# Adapter Behavior Training Data

**Purpose:** Train adapters to learn adapter lifecycle behavior patterns and runtime characteristics

## Overview

Adapter behavior encompasses the full lifecycle state machine, memory management, tier transitions, and operational patterns.

## Key Concepts

- **Lifecycle States:** Unloaded → Cold → Warm → Hot → Resident
- **Tier Transitions:** Promotion (activation% ↑) / Demotion (activation% ↓)
- **Memory Pressure:** Auto-eviction at 85% usage
- **Heartbeat Recovery:** 5-minute timeout detection
- **Pinning:** Protection from eviction

## Training Example Schema

```jsonl
{
  "input": {
    "adapter_id": "adapter-001",
    "load_state": "warm",
    "activation_pct": 0.65,
    "memory_mb": 150,
    "last_used": "2025-11-18T05:00:00Z"
  },
  "target": {
    "next_state": "hot",
    "action": "promote",
    "reason": "activation_threshold_crossed",
    "memory_delta": 50
  },
  "metadata": {
    "quality": 0.90,
    "label": "positive",
    "policy_compliant": true
  }
}
```

## Behavior Categories

1. **Promotion:** Cold→Warm, Warm→Hot, Hot→Resident
2. **Demotion:** Hot→Warm, Warm→Cold (inactivity timeout)
3. **Eviction:** Cold/Warm→Unloaded (memory pressure)
4. **Pinning:** Resident protection, TTL enforcement
5. **Recovery:** Stale adapter detection, crash recovery

## Quality Criteria

- **Min Examples:** 500
- **Min Relevance:** 0.85
- **Min Confidence:** 0.90
- **State Transition Validity:** 100%

## Data Sources

1. **Lifecycle Manager:** `crates/adapteros-lora-lifecycle/src/lib.rs`
2. **Adapter Table:** `adapters` with `load_state`, `activation_pct`
3. **Telemetry:** `adapter_promoted`, `adapter_evicted` events
4. **Heartbeat:** `last_heartbeat`, `stale_adapters` view

## Example Datasets

- `state_transitions/` - Valid lifecycle transitions
- `tier_promotion/` - Activation threshold examples
- `memory_eviction/` - Pressure-based eviction
- `heartbeat_recovery/` - Stale adapter detection
- `pinning_patterns/` - Pin/unpin examples
- `ttl_enforcement/` - Expiration patterns

## Detailed Documentation

For full implementation details, schema validation, generation strategies, and CLI usage, see [docs/BEHAVIOR_TRAINING.md](docs/BEHAVIOR_TRAINING.md).

## References

- `crates/adapteros-lora-lifecycle/src/lib.rs` - Lifecycle manager
- `crates/adapteros-db/src/adapters.rs` - Adapter CRUD
- `crates/adapteros-db/src/pinned_adapters.rs` - Pinning API
- `migrations/0065_heartbeat_mechanism.sql` - Heartbeat schema
- `CLAUDE.md` - Lifecycle state machine diagram
