# System-Metrics Simulation Dataset

**Type:** Type 6 - System-Metrics Simulation Dataset (For Metrics CI)
**Purpose:** Validates metrics ingestion + DB schema, tests `/v1/metrics/summary` endpoint
**Entry Count:** 320 entries
**Time Span:** 24 hours
**Format:** JSONL (JSON Lines)

---

## Overview

This dataset contains synthetic system metrics simulating 24 hours of adapterOS runtime telemetry. It covers all core components (router, lifecycle, memory, deterministic_exec, training, policy, telemetry) with realistic value distributions and temporal patterns.

## Dataset Schema

Each entry follows this structure:

```json
{
  "timestamp": 1763355502,
  "component": "router",
  "metric": "decision_rate",
  "value": 124.3,
  "unit": "decisions/sec"
}
```

### Fields

- **timestamp** (integer): Unix timestamp (seconds since epoch)
- **component** (string): System component name
- **metric** (string): Metric identifier
- **value** (number): Metric value (integer or float depending on metric type)
- **unit** (string): Measurement unit for context

## Component Coverage

| Component | Entries | Metrics | Description |
|-----------|---------|---------|-------------|
| `router` | 58 | 4 | K-sparse routing decisions, gate computation, tie-breaks |
| `lifecycle` | 57 | 4 | Adapter promotions/demotions/evictions, activation rates |
| `memory` | 43 | 3 | VRAM usage, headroom, adapter counts |
| `deterministic_exec` | 43 | 3 | Tick rates, task queue depth, barrier waits |
| `training` | 44 | 3 | Job counts, loss metrics, token throughput |
| `policy` | 43 | 3 | Egress blocks, determinism checks, violations |
| `telemetry` | 32 | 2 | Event emission rates, bundle sizes |

**Total:** 320 entries across 7 components and 22 unique metrics

## Metric Patterns

The dataset uses realistic distribution patterns:

- **Normal Distribution** - Most metrics (e.g., `tick_rate`, `gate_computation_ms`)
- **Spiky** - Occasional bursts (e.g., `decision_rate`)
- **Trending** - Gradual increase/decrease (e.g., `vram_usage_mb`, `avg_loss`)
- **Rare Events** - Mostly zero with occasional spikes (e.g., `evictions`, `policy_violations`)
- **Discrete** - Integer step values (e.g., `k_sparse_selections`, `job_count`)

## Use Cases

### 1. Metrics Ingestion Testing

```bash
# Ingest dataset into database
cat metrics_dataset.jsonl | while read line; do
  curl -X POST http://localhost:8080/v1/metrics/ingest \
    -H "Content-Type: application/json" \
    -d "$line"
done
```

### 2. `/v1/metrics/summary` Endpoint Validation

```bash
# Test summary endpoint
curl http://localhost:8080/v1/metrics/summary?component=router&hours=24
```

### 3. DB Schema Validation

Ensures the metrics table schema supports:
- Timestamp indexing for time-range queries
- Component/metric filtering
- Numeric value storage (INTEGER/REAL types)
- Unit metadata

### 4. CI/CD Integration

```bash
# Automated test in CI pipeline
python3 generate_metrics.py
./validate_metrics_schema.py metrics_dataset.jsonl
cargo test -p adapteros-system-metrics
```

## Files

- **metrics_dataset.jsonl** - Primary dataset (320 entries, JSONL format)
- **metrics_stats.json** - Summary statistics (min/max/avg per metric)
- **generate_metrics.py** - Generator script (reproducible, configurable)
- **validate_metrics_schema.py** - Schema validation script
- **README.md** - This file

## Generation

To regenerate the dataset with custom parameters:

```bash
python3 generate_metrics.py

# Output:
# ✓ Generated 320 metric entries -> metrics_dataset.jsonl
# ✓ Summary statistics -> metrics_stats.json
```

### Customization

Edit `generate_metrics.py` to adjust:

```python
# Number of entries
entries = generate_dataset(num_entries=500, time_span_hours=48)

# Add new metrics
METRIC_CONFIGS["my_component"] = [
    {"name": "my_metric", "min": 0.0, "max": 100.0, "unit": "percent", "pattern": "normal"}
]
```

## Validation

Run the validation script to ensure schema compliance:

```bash
python3 validate_metrics_schema.py metrics_dataset.jsonl
# ✓ Schema validation passed: 320 entries
# ✓ All timestamps are valid Unix timestamps
# ✓ All components are recognized
# ✓ All values match expected types
```

## Integration with adapterOS

### Database Schema Expectations

```sql
CREATE TABLE system_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    component TEXT NOT NULL,
    metric TEXT NOT NULL,
    value REAL NOT NULL,
    unit TEXT,
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX idx_metrics_component ON system_metrics(component);
CREATE INDEX idx_metrics_timestamp ON system_metrics(timestamp);
CREATE INDEX idx_metrics_component_metric ON system_metrics(component, metric);
```

### Expected Queries

```rust
// Time-range query
let metrics = sqlx::query_as::<_, MetricEntry>(
    "SELECT * FROM system_metrics
     WHERE component = ? AND timestamp >= ? AND timestamp <= ?
     ORDER BY timestamp ASC"
).fetch_all(&pool).await?;

// Aggregation
let summary = sqlx::query_as::<_, MetricSummary>(
    "SELECT component, metric,
            COUNT(*) as count,
            AVG(value) as avg_value,
            MIN(value) as min_value,
            MAX(value) as max_value
     FROM system_metrics
     WHERE timestamp >= ?
     GROUP BY component, metric"
).fetch_all(&pool).await?;
```

## Known Patterns & Edge Cases

### Edge Cases Covered

1. **Zero Values** - Policy violations, evictions (rare events)
2. **High Variance** - Decision rate spikes, barrier wait times
3. **Monotonic Trends** - Decreasing training loss, increasing VRAM usage
4. **Discrete Steps** - Adapter count changes in steps of 3-5
5. **Boundary Values** - Memory headroom approaching 5% (critical threshold)

### Realistic Scenarios

- **Memory Pressure Event** - VRAM usage climbs from 2GB → 8GB over 12 hours
- **Training Job Lifecycle** - Job count 0→3→5→2→0 with corresponding loss decrease
- **Router Spike** - Decision rate bursts to 200/sec during high load
- **Policy Enforcement** - 2-3 violation events across 24 hours

## References

- [AGENTS.md § Telemetry](../../AGENTS.md#telemetry-event-catalog) - Telemetry event standards
- [crates/adapteros-system-metrics/](../../../crates/adapteros-system-metrics/) - Metrics implementation
- [crates/adapteros-telemetry/](../../../crates/adapteros-telemetry/) - Telemetry infrastructure
- [migrations/](../../../migrations/) - Database schema

---

**Generated:** 2025-11-18
**Version:** 1.0
**License:** © 2025 JKCA. All rights reserved.
