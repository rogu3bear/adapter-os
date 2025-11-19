# Telemetry Event Catalog

**Purpose:** Complete reference for structured telemetry events in AdapterOS

**Last Updated:** 2025-01-19

---

## Event Catalog

### Barrier Coordination Events

**Source:** `adapteros-deterministic-exec/src/multi_agent.rs`

| Event Type | Level | When Emitted | Metadata |
|------------|-------|--------------|----------|
| `barrier.wait_start` | Debug | Agent enters barrier (lines 212-228) | agent_id, tick, generation, total_agents |
| `barrier.generation_advanced` | Info | CAS winner advances generation (lines 328-353) | agent_id, tick, generation, wait_duration_ms, living_agents, dead_agents |
| `barrier.cas_loser_proceed` | Debug | CAS loser detects generation change (lines 370-390) | agent_id, expected_gen, actual_gen |
| `barrier.agent.removed` | Warn | Agent marked as dead (lines 153-174) | agent_id, dead_count, remaining_agents, generation |
| `barrier.timeout` | Error | Barrier timeout (30s default) (lines 260-282) | agent_id, tick, timeout_seconds, wait_duration_ms |

**Correlation:** All events include `generation` field for correlating barrier operations with tick ledger entries.

### Lifecycle Events

**Source:** `adapteros-lora-lifecycle/src/lib.rs`

| Event Type | Level | Description | Metadata |
|------------|-------|-------------|----------|
| `adapter_crash_detected` | Info | Stale adapter recovered during crash recovery sweep (lines 213-346) | adapter_id, last_seen, recovery_timestamp |
| `adapter_evicted` | Info | Adapter evicted due to memory pressure | adapter_id, tier, memory_freed_mb, reason |
| `adapter_promoted` | Info | Adapter tier promoted (activation % threshold crossed) | adapter_id, old_tier, new_tier, activation_pct |
| `adapter_demoted` | Info | Adapter tier demoted (inactivity timeout) | adapter_id, old_tier, new_tier, inactivity_duration_s |

### Divergence Detection Events

**Source:** `adapteros-deterministic-exec/src/global_ledger.rs`

| Event Type | Level | When Emitted | Metadata |
|------------|-------|--------------|----------|
| `tick_ledger.consistent` | Info | Cross-host consistency verified (lines 368-399) | tick, entry_hash, host_count |
| `tick_ledger.inconsistent` | Warn | Divergence detected between hosts (lines 368-399) | tick, divergence_count, expected_hash, actual_hash |

---

## Event Metadata Best Practices

### Example: Barrier Generation Advanced

```rust
use adapteros_telemetry::{TelemetryEventBuilder, EventType, LogLevel};
use serde_json::json;

let event = TelemetryEventBuilder::new(
    EventType::Custom("barrier.generation_advanced".to_string()),
    LogLevel::Info,
    format!("Barrier generation advanced at tick {}", tick),
)
.component("adapteros-deterministic-exec".to_string())
.metadata(json!({
    "agent_id": agent_id,
    "tick": tick,
    "generation": new_gen,
    "wait_duration_ms": elapsed.as_millis(),
    "living_agents": living_count,
    "dead_agents": dead_count,
}))
.build();

event.emit().await?;
```

### Required Fields

All telemetry events must include:
- `component` - Source crate/module
- `timestamp` - ISO8601 UTC timestamp (auto-added)
- `event_type` - Canonical event type string
- `log_level` - Debug | Info | Warn | Error
- `message` - Human-readable description

### Optional Metadata

Context-specific fields:
- `agent_id`, `tenant_id`, `adapter_id` - Entity identifiers
- `tick`, `generation` - Deterministic execution state
- `duration_ms`, `wait_duration_ms` - Performance metrics
- `error_kind`, `error_message` - Failure details

---

## Querying Telemetry

### Find All Barrier Timeouts (Last Hour)

```sql
SELECT * FROM telemetry_events
WHERE event_type = 'barrier.timeout'
  AND timestamp >= datetime('now', '-1 hour')
ORDER BY timestamp DESC;
```

### Find Divergence Incidents (Last 24 Hours)

```sql
SELECT * FROM tick_ledger_consistency_reports
WHERE consistent = 0
  AND created_at >= datetime('now', '-24 hours');
```

### Adapter Lifecycle Transitions (Last 7 Days)

```sql
SELECT * FROM telemetry_events
WHERE event_type IN ('adapter_promoted', 'adapter_demoted', 'adapter_evicted')
  AND timestamp >= datetime('now', '-7 days')
ORDER BY timestamp DESC;
```

---

## Integration Points

### Server Integration

Telemetry events are automatically emitted by:
- `LifecycleManager` (adapter state transitions)
- `AgentBarrier` (multi-agent coordination)
- `GlobalTickLedger` (divergence detection)
- `UmaPressureMonitor` (memory pressure events)

### Database Schema

Events stored in `telemetry_events` table:
```sql
CREATE TABLE telemetry_events (
    id INTEGER PRIMARY KEY,
    event_type TEXT NOT NULL,
    log_level TEXT NOT NULL,
    message TEXT NOT NULL,
    component TEXT,
    metadata TEXT, -- JSON
    timestamp TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_telemetry_events_type ON telemetry_events(event_type);
CREATE INDEX idx_telemetry_events_timestamp ON telemetry_events(timestamp);
```

### API Endpoint

Query events via REST API:
```
GET /v1/telemetry/events?event_type=barrier.timeout&since=2025-01-01T00:00:00Z
```

---

## See Also

- [TELEMETRY_ARCHITECTURE.md](TELEMETRY_ARCHITECTURE.md) - Overall telemetry design
- [TELEMETRY_QUICK_REFERENCE.md](TELEMETRY_QUICK_REFERENCE.md) - Quick start guide
- [CLAUDE.md](../CLAUDE.md) - Developer guide
