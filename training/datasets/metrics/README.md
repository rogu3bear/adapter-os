# Metrics & Telemetry Training Data

**Purpose:** Train adapters to emit structured telemetry events and recognize anomalous patterns

## Overview

Telemetry events provide observability into router decisions, barrier coordination, lifecycle transitions, and policy enforcement. Events must follow canonical JSON schemas.

## Key Concepts

- **Event Types:** Barrier, Router, Lifecycle, Policy
- **Metadata Standards:** Component, timestamp, correlation IDs
- **Log Levels:** trace, debug, info, warn, error
- **Quality Thresholds:** Relevance, confidence, source validation
- **Telemetry Bundles:** Batched events with signatures

## Training Example Schema

```jsonl
{
  "input": {
    "event_type": "barrier.generation_advanced",
    "agent_id": "agent-A",
    "tick": 100,
    "generation": 5
  },
  "target": {
    "component": "adapteros-deterministic-exec",
    "log_level": "info",
    "metadata": {
      "agent_id": "agent-A",
      "tick": 100,
      "generation": 5,
      "wait_duration_ms": 250,
      "living_agents": 3,
      "dead_agents": 0
    },
    "timestamp_us": 1700000000000
  },
  "metadata": {
    "quality": 0.85,
    "label": "positive"
  }
}
```

## Event Catalog

### Barrier Coordination
- `barrier.wait_start` - Agent enters barrier
- `barrier.generation_advanced` - CAS winner advances generation
- `barrier.cas_loser_proceed` - CAS loser detects change
- `barrier.agent.removed` - Dead agent excluded
- `barrier.timeout` - 30s timeout indicates failure

### Lifecycle
- `adapter_promoted` - Tier promotion
- `adapter_demoted` - Tier demotion
- `adapter_evicted` - Memory pressure eviction
- `adapter_crash_detected` - Heartbeat recovery

### Router
- `router.decision` - K-sparse selection
- `router.entropy_floor_violated` - Entropy < ε

### Tick Ledger
- `tick_ledger.consistent` - Cross-host verification passed
- `tick_ledger.inconsistent` - Divergence detected

## Quality Criteria

- **Min Examples:** 300
- **Min Relevance:** 0.80
- **Min Confidence:** 0.85
- **Schema Compliance:** 100%

## Data Sources

1. **Telemetry Events:** `adapteros_telemetry::events`
2. **Database:** `telemetry_events` table
3. **Routing Decisions:** `routing_decisions` table
4. **Consistency Reports:** `tick_ledger_consistency_reports`

## Example Datasets

- `barrier_events/` - Multi-agent coordination
- `router_decisions/` - K-sparse selections
- `lifecycle_events/` - State transitions
- `consistency_reports/` - Cross-host verification
- `anomaly_patterns/` - Outlier detection

## References

- `crates/adapteros-telemetry/src/events.rs` - Event definitions
- `crates/adapteros-deterministic-exec/src/multi_agent.rs` - Barrier telemetry
- `AGENTS.md` - Telemetry event catalog
