# KV Operations Metrics

**Status:** Implemented
**Module:** `crates/adapteros-db/src/kv_metrics.rs`
**Version:** v0.3-alpha

## Overview

The KV metrics system provides comprehensive tracking of key-value storage backend operations, enabling monitoring, performance analysis, and operational visibility during the SQL-to-KV migration.

## Metrics Collected

### Operation Counts

| Metric | Description |
|--------|-------------|
| `reads_total` | Total number of KV read operations |
| `writes_total` | Total number of KV write operations |
| `deletes_total` | Total number of KV delete operations |
| `scans_total` | Total number of KV scan/prefix operations |
| `index_queries_total` | Total number of secondary index queries |
| `operations_total` | Sum of all operations |

### Latency Metrics

Latency is tracked using histogram buckets for accurate percentile calculation:

| Metric | Description |
|--------|-------------|
| `read_avg_ms` | Average read latency (milliseconds) |
| `write_avg_ms` | Average write latency (milliseconds) |
| `delete_avg_ms` | Average delete latency (milliseconds) |
| `scan_avg_ms` | Average scan latency (milliseconds) |
| `read_p50_ms` | 50th percentile read latency |
| `read_p95_ms` | 95th percentile read latency |
| `read_p99_ms` | 99th percentile read latency |
| `write_p50_ms` | 50th percentile write latency |
| `write_p95_ms` | 95th percentile write latency |
| `write_p99_ms` | 99th percentile write latency |

**Latency Buckets:** <1ms, 1-5ms, 5-10ms, 10-50ms, 50-100ms, 100-500ms, >500ms

### SQL Fallback Tracking

Critical for monitoring migration health:

| Metric | Description |
|--------|-------------|
| `fallback_reads_total` | Number of times KV read failed and fell back to SQL |
| `fallback_writes_total` | Number of times KV write failed and fell back to SQL |
| `fallback_deletes_total` | Number of times KV delete failed and fell back to SQL |
| `fallback_operations_total` | Total fallback operations |

### Error Tracking

Categorized error counts for debugging:

| Metric | Description |
|--------|-------------|
| `errors_not_found` | Key not found errors |
| `errors_serialization` | Serialization/deserialization errors |
| `errors_backend` | Backend storage errors |
| `errors_timeout` | Operation timeout errors |
| `errors_other` | Uncategorized errors |
| `errors_total` | Total error count |

## Usage

### 1. Basic Integration with KV Backend

Add metrics to `KvDb` operations in `kv_backend.rs`:

```rust
use crate::kv_metrics::{global_kv_metrics, KvOperationTimer, KvOperationType, KvErrorType};

impl KvDb {
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let _timer = KvOperationTimer::new(KvOperationType::Read);

        self.backend
            .get(key)
            .await
            .map_err(|e| {
                global_kv_metrics().record_error(KvErrorType::Backend);
                AosError::Database(format!("KV get failed: {}", e))
            })
    }

    pub async fn set(&self, key: &str, value: Vec<u8>) -> Result<()> {
        let _timer = KvOperationTimer::new(KvOperationType::Write);

        self.backend
            .set(key, value)
            .await
            .map_err(|e| {
                global_kv_metrics().record_error(KvErrorType::Backend);
                AosError::Database(format!("KV set failed: {}", e))
            })
    }

    pub async fn delete(&self, key: &str) -> Result<bool> {
        let _timer = KvOperationTimer::new(KvOperationType::Delete);

        self.backend
            .delete(key)
            .await
            .map_err(|e| {
                global_kv_metrics().record_error(KvErrorType::Backend);
                AosError::Database(format!("KV delete failed: {}", e))
            })
    }

    pub async fn scan_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let _timer = KvOperationTimer::new(KvOperationType::Scan);

        self.backend
            .scan_prefix(prefix)
            .await
            .map_err(|e| {
                global_kv_metrics().record_error(KvErrorType::Backend);
                AosError::Database(format!("KV scan_prefix failed: {}", e))
            })
    }
}
```

### 2. Tracking SQL Fallbacks in Dual-Write Mode

In `adapters_kv.rs` or other KV operation modules:

```rust
use crate::kv_metrics::global_kv_metrics;

async fn register_adapter_dual_write(params: AdapterRegistrationParams) -> Result<String> {
    let metrics = global_kv_metrics();

    // Try KV write
    match register_adapter_kv(params.clone()).await {
        Ok(id) => {
            // Also write to SQL for consistency
            register_adapter_sql(params).await?;
            Ok(id)
        }
        Err(e) => {
            warn!("KV write failed, falling back to SQL: {}", e);
            metrics.record_fallback_write();

            // Fallback to SQL-only
            register_adapter_sql(params).await
        }
    }
}
```

### 3. Error Type Categorization

Categorize errors appropriately:

```rust
match kv_operation().await {
    Err(AosError::Database(msg)) if msg.contains("not found") => {
        metrics.record_error(KvErrorType::NotFound);
    }
    Err(AosError::Database(msg)) if msg.contains("serialize") => {
        metrics.record_error(KvErrorType::Serialization);
    }
    Err(AosError::Database(msg)) if msg.contains("timeout") => {
        metrics.record_error(KvErrorType::Timeout);
    }
    Err(_) => {
        metrics.record_error(KvErrorType::Backend);
    }
    Ok(_) => {}
}
```

## REST API Endpoint

Add a metrics endpoint in `crates/adapteros-server-api/src/handlers/metrics.rs`:

```rust
use adapteros_db::{global_kv_metrics, KvMetricsSnapshot};
use axum::{Json, response::IntoResponse};
use serde_json::json;

/// GET /v1/metrics/kv
///
/// Returns current KV backend metrics snapshot
pub async fn get_kv_metrics() -> impl IntoResponse {
    let metrics = global_kv_metrics();
    let snapshot = metrics.snapshot();

    Json(json!({
        "kv_metrics": snapshot,
        "timestamp_ms": chrono::Utc::now().timestamp_millis(),
    }))
}
```

Register the route in `routes.rs`:

```rust
Router::new()
    .route("/v1/metrics/kv", get(handlers::metrics::get_kv_metrics))
```

## CLI Command

Add a metrics command in `aosctl`:

```bash
# View KV metrics
aosctl metrics kv

# Watch metrics in real-time
aosctl metrics kv --watch

# Export metrics to JSON
aosctl metrics kv --json > kv_metrics.json
```

Implementation in `crates/adapteros-cli/src/commands/metrics.rs`:

```rust
use adapteros_db::global_kv_metrics;

pub async fn show_kv_metrics(json: bool, watch: bool) -> Result<()> {
    loop {
        let metrics = global_kv_metrics();
        let snapshot = metrics.snapshot();

        if json {
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        } else {
            println!("\n=== KV Backend Metrics ===\n");
            println!("Operations:");
            println!("  Reads:    {}", snapshot.reads_total);
            println!("  Writes:   {}", snapshot.writes_total);
            println!("  Deletes:  {}", snapshot.deletes_total);
            println!("  Scans:    {}", snapshot.scans_total);
            println!("  Total:    {}", snapshot.operations_total);
            println!("\nLatency (ms):");
            println!("  Read  avg: {:.2}, p95: {:.2}, p99: {:.2}",
                snapshot.read_avg_ms, snapshot.read_p95_ms, snapshot.read_p99_ms);
            println!("  Write avg: {:.2}, p95: {:.2}, p99: {:.2}",
                snapshot.write_avg_ms, snapshot.write_p95_ms, snapshot.write_p99_ms);
            println!("\nSQL Fallbacks:");
            println!("  Reads:    {}", snapshot.fallback_reads_total);
            println!("  Writes:   {}", snapshot.fallback_writes_total);
            println!("  Deletes:  {}", snapshot.fallback_deletes_total);
            println!("  Total:    {}", snapshot.fallback_operations_total);
            println!("\nErrors:");
            println!("  Backend:        {}", snapshot.errors_backend);
            println!("  Serialization:  {}", snapshot.errors_serialization);
            println!("  Not Found:      {}", snapshot.errors_not_found);
            println!("  Timeout:        {}", snapshot.errors_timeout);
            println!("  Other:          {}", snapshot.errors_other);
            println!("  Total:          {}", snapshot.errors_total);
        }

        if !watch {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    Ok(())
}
```

## Telemetry Integration

Export metrics to telemetry system for monitoring:

```rust
use adapteros_telemetry::TelemetryEvent;

async fn export_kv_metrics_to_telemetry() {
    let metrics = global_kv_metrics();
    let snapshot = metrics.snapshot();

    let event = TelemetryEvent::new("kv_metrics")
        .with_field("reads_total", snapshot.reads_total)
        .with_field("writes_total", snapshot.writes_total)
        .with_field("read_p95_ms", snapshot.read_p95_ms)
        .with_field("write_p95_ms", snapshot.write_p95_ms)
        .with_field("fallback_operations", snapshot.fallback_operations_total)
        .with_field("errors_total", snapshot.errors_total);

    telemetry::emit(event).await;
}
```

## Alerting Rules

Suggested alerts based on metrics:

### High Fallback Rate
```yaml
alert: HighKvFallbackRate
expr: |
  (kv_metrics.fallback_operations_total / kv_metrics.operations_total) > 0.1
for: 5m
severity: warning
description: "KV backend fallback rate above 10%"
```

### High Error Rate
```yaml
alert: HighKvErrorRate
expr: |
  (kv_metrics.errors_total / kv_metrics.operations_total) > 0.05
for: 5m
severity: critical
description: "KV backend error rate above 5%"
```

### High Latency
```yaml
alert: HighKvLatency
expr: kv_metrics.write_p95_ms > 100
for: 5m
severity: warning
description: "KV write p95 latency above 100ms"
```

## Performance Considerations

### Atomic Operations
All metrics use `AtomicU64` with `Ordering::Relaxed` for minimal performance overhead:
- No memory barriers on metric updates
- Suitable for high-throughput scenarios
- Thread-safe without locks

### Memory Footprint
- Global singleton: Single instance shared across threads
- Total size: ~1KB (7 buckets × 4 operations + counters)
- Negligible impact on memory

### Overhead Benchmarks
Expected overhead per operation:
- Timer creation/drop: ~20ns
- Atomic increment: ~5ns
- Total per operation: <30ns

## Migration Workflow Integration

### Phase 1: SQL Only (Baseline)
- No KV metrics (operations_total = 0)
- Establish SQL performance baseline

### Phase 2: Dual Write (Validation)
- Monitor `fallback_writes_total` (should be 0)
- Compare KV vs SQL write latency
- Track `errors_total` for KV stability

### Phase 3: KV Primary (Cutover)
- Monitor `fallback_reads_total` (should be 0)
- Validate read latency improvement
- Track error rates during cutover

### Phase 4: KV Only (Complete)
- Fallback metrics should be 0
- Error rate should be <1%
- Latency should be optimal

## Example: Migration Health Dashboard

```rust
pub struct MigrationHealth {
    pub kv_operations_total: u64,
    pub sql_operations_total: u64,
    pub fallback_rate: f64,
    pub error_rate: f64,
    pub latency_improvement: f64,
    pub status: MigrationStatus,
}

pub fn calculate_migration_health() -> MigrationHealth {
    let kv = global_kv_metrics().snapshot();
    let sql = global_sql_metrics().snapshot(); // Hypothetical

    let total_ops = kv.operations_total + sql.operations_total;
    let fallback_rate = if total_ops > 0 {
        kv.fallback_operations_total as f64 / total_ops as f64
    } else {
        0.0
    };

    let error_rate = if kv.operations_total > 0 {
        kv.errors_total as f64 / kv.operations_total as f64
    } else {
        0.0
    };

    let status = if fallback_rate > 0.1 || error_rate > 0.05 {
        MigrationStatus::Degraded
    } else if kv.operations_total > sql.operations_total {
        MigrationStatus::Healthy
    } else {
        MigrationStatus::Validating
    };

    MigrationHealth {
        kv_operations_total: kv.operations_total,
        sql_operations_total: sql.operations_total,
        fallback_rate,
        error_rate,
        latency_improvement: (sql.read_avg_ms - kv.read_avg_ms) / sql.read_avg_ms,
        status,
    }
}
```

## Testing

Run the built-in tests:

```bash
cargo test -p adapteros-db --lib kv_metrics
```

All 7 tests should pass:
- `test_metrics_basic_operations`
- `test_metrics_latency_tracking`
- `test_metrics_fallback_tracking`
- `test_metrics_error_tracking`
- `test_metrics_reset`
- `test_operation_timer`
- `test_concurrent_metrics`

## References

- Source: `crates/adapteros-db/src/kv_metrics.rs`
- KV Backend: `crates/adapteros-db/src/kv_backend.rs`
- Storage Modes: `crates/adapteros-db/src/lib.rs` (StorageMode enum)
- Telemetry: `crates/adapteros-telemetry/`

---

**Copyright:** 2025 JKCA / James KC Auchterlonie
