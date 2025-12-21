# Health Check & Metrics Validation Dataset

**Category:** System Health & Observability
**Format:** JSON health responses
**Count:** 4 files
**Endpoints:** Health check endpoints

## Files

### healthz_basic.json
**Endpoint:** `GET /healthz`
**Purpose:** Basic health status check

**Schema:**
```json
{
  "status": "healthy",
  "timestamp": 1737201600
}
```

**Values:**
- `status`: "healthy" | "degraded" | "unhealthy"
- `timestamp`: Unix timestamp (seconds)

---

### healthz_all.json
**Endpoint:** `GET /healthz/all`
**Purpose:** System-wide health check (all components healthy)

**Schema:**
```json
{
  "overall_status": "healthy",
  "components": [
    {
      "component": "router",
      "status": "healthy",
      "message": "Router operating normally",
      "details": {
        "avg_decision_rate": 125.5,
        "avg_overhead_pct": 2.45,
        "anomaly_rate": 0.003
      },
      "timestamp": 1737201600
    }
  ],
  "timestamp": 1737201600
}
```

**Components (6 total):**
1. **router** - Decision rate, overhead metrics
2. **loader** - Stuck adapters, loaded/total count
3. **kernel** - Worker availability, GPU memory headroom
4. **db** - Connection pool, migrations applied
5. **telemetry** - Event ingestion rate, latency
6. **system-metrics** - UMA memory pressure

---

### healthz_degraded.json
**Endpoint:** `GET /healthz/all`
**Purpose:** System with degraded components (mixed health states)

**Degraded Components:**
- **loader** - 2 stuck adapters
- **telemetry** - Buffer at 87.3% utilization
- **system-metrics** - Memory pressure elevated (17.2% headroom)

**overall_status:** "degraded" (worst component status)

---

### healthz_router.json
**Endpoint:** `GET /healthz/router`
**Purpose:** Component-specific health details

**Schema:**
```json
{
  "component": "router",
  "status": "healthy",
  "message": "Router operating normally",
  "details": {
    "avg_decision_rate": 125.5,
    "avg_overhead_pct": 2.45,
    "anomaly_rate": 0.003,
    "active_stacks": 12,
    "total_decisions_24h": 10872450
  },
  "timestamp": 1737201600
}
```

## Test Coverage

### Status Validation
- Valid status values: "healthy", "degraded", "unhealthy"
- Overall status = worst component status
- All components have required fields

### Component Presence
- All 6 components present in `/healthz/all`
- Each component has: component, status, message, timestamp
- Optional details field with component-specific metrics

### Degradation Detection
- At least one degraded component when overall_status = "degraded"
- Degraded reasons included in message field

## Validation Rules

| Field | Rule |
|-------|------|
| **status** | "healthy" \| "degraded" \| "unhealthy" |
| **overall_status** | Max severity of all components |
| **component** | One of 6 canonical names |
| **timestamp** | Non-zero Unix timestamp |
| **details** | Optional JSON object (component-specific) |

## Component Health Indicators

### Router
- Decision rate (decisions/sec)
- Average overhead percentage
- Anomaly rate (high overhead decisions)

### Loader
- Loaded adapters count
- Total adapters count
- Stuck adapters (loading > 5min)

### Kernel
- Available workers
- Total workers
- GPU memory headroom %

### DB
- Pool size
- Active connections
- Migrations applied

### Telemetry
- Events per second
- Average latency (ms)
- Buffer usage %

### System-Metrics
- Memory pressure level (normal, medium, high, critical)
- Headroom percentage
- CPU usage %

## Example Test

```rust
#[test]
fn test_healthz_all_components() {
    let json = include_str!("../../../tests/training/datasets/metrics/healthz_all.json");
    let response: SystemHealthResponse = serde_json::from_str(json).unwrap();

    assert_eq!(response.components.len(), 6);

    let component_names: Vec<String> = response.components
        .iter()
        .map(|c| c.component.clone())
        .collect();

    let expected = ["router", "loader", "kernel", "db", "telemetry", "system-metrics"];
    for name in expected {
        assert!(component_names.contains(&name.to_string()),
                "Missing component: {}", name);
    }
}

#[test]
fn test_healthz_degraded_detection() {
    let json = include_str!("../../../tests/training/datasets/metrics/healthz_degraded.json");
    let response: SystemHealthResponse = serde_json::from_str(json).unwrap();

    assert_eq!(response.overall_status, ComponentStatus::Degraded);

    let degraded_count = response.components
        .iter()
        .filter(|c| c.status == ComponentStatus::Degraded)
        .count();

    assert!(degraded_count > 0, "Should have at least one degraded component");
}
```

## Memory Pressure Levels

**UMA Memory Pressure:**
- **Normal:** > 30% headroom
- **Medium:** 20-30% headroom
- **High:** 15-20% headroom (triggers eviction)
- **Critical:** < 15% headroom (aggressive eviction)

## References

- [API Contract Tests](../../../crates/adapteros-server-api/tests/api_contracts.rs)
- [Health Check Implementation](../../../crates/adapteros-server-api/src/health.rs)
- [UMA Pressure Monitoring](../../../AGENTS.md#uma-backpressure--eviction)
- [Component Health Checks](../../../crates/adapteros-server-api/src/health.rs)
