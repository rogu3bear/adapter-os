# Upload Metrics - Quick Start Guide

**Quick reference for integrating upload metrics into your code.**

---

## 1. Add to State (state.rs)

```rust
use crate::upload_metrics::UploadMetricsCollector;
use std::sync::Arc;

pub struct AppState {
    // ... existing fields ...
    pub upload_metrics: Arc<UploadMetricsCollector>,
}

// In initialization:
let upload_metrics = Arc::new(UploadMetricsCollector::new()?);
```

---

## 2. Add Routes (routes.rs)

```rust
use crate::handlers::upload_metrics_handler;

// In router setup:
.route("/v1/metrics/uploads",
    get(upload_metrics_handler::get_upload_metrics))
.route("/v1/metrics/uploads/health",
    get(upload_metrics_handler::get_upload_health))
.route("/v1/metrics/uploads/prometheus",
    get(upload_metrics_handler::get_prometheus_metrics))
```

---

## 3. Integrate into Upload Handler (aos_upload.rs)

```rust
use crate::upload_metrics::UploadTimer;

pub async fn upload_aos_adapter(
    State(state): State<AppState>,
    // ... other params ...
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Create timer at start
    let mut timer = UploadTimer::new();

    // ... multipart processing ...

    // After streaming phase completes
    timer.mark_streaming_complete();

    // ... database registration ...

    // On success, record metrics
    state.upload_metrics.record_upload_success(
        &tenant_id,
        &tier,
        file_size,
        timer.streaming_duration().unwrap(),
        timer.database_duration().unwrap(),
    );

    // On failure
    state.upload_metrics.record_upload_failure(
        &tenant_id,
        "reason: ...",
        "ErrorType"
    );

    // On rate limit
    state.upload_metrics.record_rate_limited(&tenant_id);

    // ... rest of handler ...
}
```

---

## 4. Monitor Metrics

### JSON API
```bash
curl http://localhost:8000/v1/metrics/uploads
curl http://localhost:8000/v1/metrics/uploads/health
```

### Prometheus Scrape
```yaml
scrape_configs:
  - job_name: 'adapteros_uploads'
    static_configs:
      - targets: ['localhost:8000']
    metrics_path: '/v1/metrics/uploads/prometheus'
    scrape_interval: 30s
```

### Grafana Queries
```promql
# Success rate
rate(adapteros_uploads_successful_total[5m]) / (rate(adapteros_uploads_successful_total[5m]) + rate(adapteros_uploads_failed_total[5m]))

# P95 duration
histogram_quantile(0.95, rate(adapteros_upload_duration_total_seconds[5m]))

# Top tenants
topk(10, adapteros_uploads_per_tenant_total)

# Queue depth
adapteros_upload_queue_depth
```

---

## 5. Key Metrics

**Duration (seconds):**
- `adapteros_upload_duration_streaming_seconds` - File transfer phase
- `adapteros_upload_duration_database_seconds` - DB registration phase
- `adapteros_upload_duration_total_seconds` - Complete upload

**File Size (bytes):**
- `adapteros_upload_file_size_bytes` - With size categories

**Success/Failure:**
- `adapteros_uploads_successful_total` - By tenant, tier
- `adapteros_uploads_failed_total` - By tenant, reason
- `adapteros_uploads_rate_limited_total` - By tenant
- `adapteros_uploads_aborted_total` - By tenant

**Per-Tenant:**
- `adapteros_uploads_per_tenant_total` - Count by tenant
- `adapteros_bytes_uploaded_per_tenant_total` - Volume by tenant

**Queue & Cleanup:**
- `adapteros_upload_queue_depth` - Current depth
- `adapteros_cleanup_operations_total` - Cleanup ops
- `adapteros_cleanup_errors_total` - Cleanup errors
- `adapteros_temp_files_deleted_total` - Deleted files

**Rate Limiter:**
- `adapteros_rate_limit_tokens_available` - Available tokens
- `adapteros_rate_limit_refills_total` - Token refills

---

## 6. Health Check

```bash
curl http://localhost:8000/v1/metrics/uploads/health
```

**Response:**
```json
{
  "status": "healthy|degraded|unhealthy",
  "metrics": {
    "success_rate_percent": 98.94,
    "p95_upload_duration_ms": 431.49,
    "queue_depth": 3,
    "cleanup_errors_in_last_hour": 2,
    "rate_limited_attempts_in_last_hour": 12
  }
}
```

**Thresholds:**
- Healthy: >95% success, p95 < 5s, queue < 100, < 10 errors
- Degraded: >90% success or elevated metrics
- Unhealthy: Low success or high errors

---

## 7. Telemetry Events (Automatic)

Events logged automatically:
- `upload.success` - File uploaded successfully
- `upload.failure` - Upload failed
- `upload.rate_limited` - Rate limit exceeded
- `cleanup.completed` - Cleanup operation finished

No action needed - integrated into metrics recording.

---

## 8. Testing

```rust
#[tokio::test]
async fn test_upload_metrics() {
    let collector = UploadMetricsCollector::new().unwrap();

    // Record success
    collector.record_upload_success(
        "tenant-a",
        "warm",
        1_000_000,
        Duration::from_millis(100),
        Duration::from_millis(50),
    );

    // Record failure
    collector.record_upload_failure(
        "tenant-a",
        "validation error",
        "ValidationError",
    );

    // Get snapshot
    let snapshot = collector.get_metrics_snapshot().await;
    assert_eq!(snapshot.success_rates.successful_uploads_total, 1);
}
```

---

## 9. Alert Rules (Prometheus)

```yaml
groups:
  - name: adapteros_uploads
    interval: 30s
    rules:
      - alert: UploadSuccessRateLow
        expr: |
          (rate(adapteros_uploads_successful_total[5m]) /
           (rate(adapteros_uploads_successful_total[5m]) +
            rate(adapteros_uploads_failed_total[5m]))) < 0.95
        for: 5m
        annotations:
          summary: "Upload success rate below 95%"

      - alert: UploadDurationHigh
        expr: histogram_quantile(0.95, adapteros_upload_duration_total_seconds) > 5
        for: 10m
        annotations:
          summary: "P95 upload duration exceeds 5 seconds"
```

---

## 10. Troubleshooting

**No metrics appearing?**
- Ensure UploadMetricsCollector initialized in AppState
- Verify routes registered
- Check metrics recording calls in upload handler

**Prometheus scrape failing?**
- Verify endpoint accessible: `curl /v1/metrics/uploads/prometheus`
- Check Prometheus config pointing to correct path
- Verify OpenMetrics format is valid

**Health endpoint always unhealthy?**
- Check success rate threshold (>95%)
- Check duration threshold (p95 < 5s)
- Check error rates

---

**Full documentation:** `/Users/star/Dev/aos/docs/UPLOAD_METRICS_INTEGRATION.md`
**Module reference:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/upload_metrics.rs`
**Handler reference:** `/Users/star/Dev/aos/crates/adapteros-server-api/src/handlers/upload_metrics_handler.rs`

