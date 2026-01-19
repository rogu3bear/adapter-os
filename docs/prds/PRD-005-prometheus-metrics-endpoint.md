# PRD-005: Prometheus Metrics Endpoint

**Status**: Draft
**Priority**: P1 (High)
**Estimated Effort**: 4-6 hours
**Owner**: TBD

---

## 1. Problem Statement

AdapterOS has comprehensive internal telemetry via `tracing` and health check endpoints, but lacks a Prometheus-compatible `/metrics` endpoint for integration with standard observability stacks (Prometheus, Grafana, Datadog, etc.).

Current state:
- Health checks: `/healthz`, `/readyz` (available)
- Tracing: OpenTelemetry export (available)
- **Metrics scraping: Not available**

This blocks:
- Integration with Kubernetes monitoring (kube-prometheus-stack)
- Grafana dashboards for operational visibility
- Alert manager integration for production alerting
- SRE standard tooling adoption

---

## 2. Scope

### In Scope

| Metric Category | Examples |
|-----------------|----------|
| **Inference** | requests_total, latency_seconds, tokens_generated |
| **Routing** | adapter_selections, routing_entropy, k_value_distribution |
| **Resources** | memory_bytes, gpu_utilization, adapter_cache_size |
| **Database** | query_duration, connection_pool_size, transaction_count |
| **HTTP** | request_duration, status_codes, active_connections |
| **Receipts** | receipts_generated, verification_success, signature_failures |

### Out of Scope

- Push-based metrics (Prometheus is pull-based)
- Custom metric backends (only Prometheus format)
- Real-time streaming metrics (use tracing for that)

---

## 3. Technical Design

### 3.1 Endpoint Specification

**Endpoint**: `GET /metrics`

**Content-Type**: `text/plain; version=0.0.4; charset=utf-8`

**Authentication**: Configurable (default: no auth for internal scraping)

**Response Format**: Prometheus text exposition format

```
# HELP aos_inference_requests_total Total inference requests
# TYPE aos_inference_requests_total counter
aos_inference_requests_total{tenant="tenant-1",status="success"} 1234
aos_inference_requests_total{tenant="tenant-1",status="error"} 12

# HELP aos_inference_duration_seconds Inference request duration
# TYPE aos_inference_duration_seconds histogram
aos_inference_duration_seconds_bucket{le="0.1"} 100
aos_inference_duration_seconds_bucket{le="0.5"} 500
aos_inference_duration_seconds_bucket{le="1.0"} 800
aos_inference_duration_seconds_bucket{le="+Inf"} 1000
aos_inference_duration_seconds_sum 456.78
aos_inference_duration_seconds_count 1000
```

### 3.2 Metrics Registry

Use `prometheus` crate for metric collection:

```rust
use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramVec,
    Opts, Registry, TextEncoder, Encoder,
};

pub struct MetricsRegistry {
    pub registry: Registry,

    // Inference metrics
    pub inference_requests: CounterVec,
    pub inference_duration: HistogramVec,
    pub inference_tokens_generated: CounterVec,
    pub inference_tokens_per_second: GaugeVec,

    // Routing metrics
    pub routing_decisions: CounterVec,
    pub routing_entropy: HistogramVec,
    pub routing_k_value: HistogramVec,

    // Resource metrics
    pub memory_bytes: GaugeVec,
    pub gpu_utilization: Gauge,
    pub adapter_cache_entries: Gauge,
    pub adapter_cache_bytes: Gauge,

    // Database metrics
    pub db_query_duration: HistogramVec,
    pub db_pool_connections: GaugeVec,
    pub db_transactions: CounterVec,

    // Receipt metrics
    pub receipts_generated: CounterVec,
    pub receipt_verification: CounterVec,
    pub receipt_signature_duration: Histogram,

    // HTTP metrics
    pub http_requests: CounterVec,
    pub http_duration: HistogramVec,
    pub http_active_connections: Gauge,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        let registry = Registry::new();

        let inference_requests = CounterVec::new(
            Opts::new("aos_inference_requests_total", "Total inference requests"),
            &["tenant", "status", "model"]
        ).unwrap();
        registry.register(Box::new(inference_requests.clone())).unwrap();

        let inference_duration = HistogramVec::new(
            prometheus::HistogramOpts::new(
                "aos_inference_duration_seconds",
                "Inference request duration in seconds"
            ).buckets(vec![0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
            &["tenant", "model"]
        ).unwrap();
        registry.register(Box::new(inference_duration.clone())).unwrap();

        // ... register all other metrics

        Self {
            registry,
            inference_requests,
            inference_duration,
            // ...
        }
    }

    pub fn encode(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}
```

### 3.3 Metric Definitions

#### Inference Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `aos_inference_requests_total` | Counter | tenant, status, model | Total inference requests |
| `aos_inference_duration_seconds` | Histogram | tenant, model | Request duration |
| `aos_inference_tokens_generated_total` | Counter | tenant, model | Total tokens generated |
| `aos_inference_tokens_per_second` | Gauge | tenant, model | Current throughput |
| `aos_inference_queue_depth` | Gauge | tenant | Pending requests |

#### Routing Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `aos_routing_decisions_total` | Counter | tenant, adapter_count | Routing decisions made |
| `aos_routing_entropy` | Histogram | tenant | Routing decision entropy |
| `aos_routing_k_value` | Histogram | tenant | K-sparse value used |
| `aos_routing_gate_max` | Histogram | tenant | Maximum gate value |

#### Resource Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `aos_memory_bytes` | Gauge | type (heap, gpu, cache) | Memory usage |
| `aos_gpu_utilization_ratio` | Gauge | device | GPU utilization 0-1 |
| `aos_adapter_cache_entries` | Gauge | - | Cached adapter count |
| `aos_adapter_cache_bytes` | Gauge | - | Cache memory usage |
| `aos_kv_cache_entries` | Gauge | tenant | KV cache entries |

#### Database Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `aos_db_query_duration_seconds` | Histogram | query_type | Query latency |
| `aos_db_pool_connections` | Gauge | state (active, idle) | Connection pool |
| `aos_db_transactions_total` | Counter | status | Transaction count |

#### Receipt Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `aos_receipts_generated_total` | Counter | tenant, type | Receipts created |
| `aos_receipt_verification_total` | Counter | result | Verification outcomes |
| `aos_receipt_signature_seconds` | Histogram | - | Signing duration |

#### HTTP Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `aos_http_requests_total` | Counter | method, path, status | HTTP requests |
| `aos_http_duration_seconds` | Histogram | method, path | Request duration |
| `aos_http_active_connections` | Gauge | - | Open connections |

### 3.4 Handler Implementation

```rust
use axum::{routing::get, Router, response::IntoResponse};
use std::sync::Arc;

pub fn metrics_router(metrics: Arc<MetricsRegistry>) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(metrics)
}

async fn metrics_handler(
    State(metrics): State<Arc<MetricsRegistry>>,
) -> impl IntoResponse {
    let body = metrics.encode();
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
}
```

### 3.5 Instrumentation Integration

Add metric recording to existing handlers:

```rust
// In inference handler
pub async fn inference_handler(
    State(state): State<AppState>,
    Json(req): Json<InferRequest>,
) -> Result<Json<InferResponse>, ApiError> {
    let start = Instant::now();
    let tenant = req.tenant_id.clone();
    let model = req.model_id.clone();

    let result = do_inference(&state, req).await;

    let duration = start.elapsed().as_secs_f64();
    let status = if result.is_ok() { "success" } else { "error" };

    // Record metrics
    state.metrics.inference_requests
        .with_label_values(&[&tenant, status, &model])
        .inc();

    state.metrics.inference_duration
        .with_label_values(&[&tenant, &model])
        .observe(duration);

    if let Ok(ref response) = result {
        state.metrics.inference_tokens_generated
            .with_label_values(&[&tenant, &model])
            .inc_by(response.tokens_generated as f64);
    }

    result.map(Json)
}
```

---

## 4. Implementation Plan

### Phase 1: Core Infrastructure (2 hours)

1. Add `prometheus` crate dependency
2. Create `MetricsRegistry` struct with all metric definitions
3. Implement `/metrics` endpoint handler
4. Register metrics router in server setup

### Phase 2: Inference Instrumentation (1.5 hours)

1. Add inference metrics recording to handlers
2. Add routing metrics recording to router
3. Add token counting to streaming handlers

### Phase 3: Resource Instrumentation (1.5 hours)

1. Add memory metrics collection (periodic task)
2. Add GPU metrics collection (if available)
3. Add adapter cache metrics
4. Add database pool metrics

### Phase 4: Testing & Documentation (1 hour)

1. Add unit tests for metric encoding
2. Add integration test for `/metrics` endpoint
3. Create Grafana dashboard JSON
4. Document metrics in API reference

---

## 5. Configuration

```toml
# configs/cp.toml

[metrics]
enabled = true
endpoint = "/metrics"
# Optional: require auth for metrics endpoint
auth_required = false
# Optional: filter metrics by prefix
include_prefixes = ["aos_"]
# Optional: exclude high-cardinality metrics
exclude_metrics = []
```

---

## 6. Acceptance Criteria

- [ ] `GET /metrics` returns valid Prometheus format
- [ ] All inference requests increment counters
- [ ] Duration histograms have reasonable bucket distribution
- [ ] Resource gauges update every 15 seconds
- [ ] Endpoint responds in < 50ms under load
- [ ] Prometheus can scrape endpoint successfully
- [ ] Grafana can visualize metrics

---

## 7. Testing Strategy

### Unit Tests

```rust
#[test]
fn test_metrics_encoding() {
    let metrics = MetricsRegistry::new();
    metrics.inference_requests
        .with_label_values(&["tenant-1", "success", "model-1"])
        .inc();

    let output = metrics.encode();
    assert!(output.contains("aos_inference_requests_total"));
    assert!(output.contains("tenant=\"tenant-1\""));
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_metrics_endpoint() {
    let app = TestApp::new();

    // Generate some traffic
    app.inference("tenant-1", "Hello").await;

    // Scrape metrics
    let response = app.get("/metrics").await;
    assert_eq!(response.status(), 200);

    let body = response.text().await;
    assert!(body.contains("aos_inference_requests_total"));
    assert!(body.contains("tenant=\"tenant-1\""));
}
```

### Prometheus Scrape Test

```yaml
# prometheus.yml (test config)
scrape_configs:
  - job_name: 'adapteros'
    static_configs:
      - targets: ['localhost:8080']
    scrape_interval: 15s
```

---

## 8. Grafana Dashboard

Provide pre-built dashboard JSON:

```json
{
  "title": "AdapterOS Overview",
  "panels": [
    {
      "title": "Inference Rate",
      "type": "graph",
      "targets": [{
        "expr": "rate(aos_inference_requests_total[5m])",
        "legendFormat": "{{tenant}} - {{status}}"
      }]
    },
    {
      "title": "Inference Latency (p99)",
      "type": "graph",
      "targets": [{
        "expr": "histogram_quantile(0.99, rate(aos_inference_duration_seconds_bucket[5m]))",
        "legendFormat": "{{tenant}}"
      }]
    },
    {
      "title": "GPU Utilization",
      "type": "gauge",
      "targets": [{
        "expr": "aos_gpu_utilization_ratio * 100"
      }]
    },
    {
      "title": "Memory Usage",
      "type": "graph",
      "targets": [{
        "expr": "aos_memory_bytes",
        "legendFormat": "{{type}}"
      }]
    }
  ]
}
```

---

## 9. Security Considerations

| Concern | Mitigation |
|---------|------------|
| Metric cardinality explosion | Limit label values, avoid user-supplied labels |
| Sensitive data in labels | Never include PII in metric labels |
| DoS via metrics endpoint | Rate limit `/metrics` endpoint |
| Information disclosure | Optional auth requirement for metrics |

**Label Safety Rules**:

```rust
// NEVER do this - user-supplied data as label
metrics.with_label_values(&[&user_input]).inc(); // BAD

// DO this - controlled vocabulary
let status = if success { "success" } else { "error" };
metrics.with_label_values(&[status]).inc(); // GOOD
```

---

## 10. Performance Impact

| Metric Type | Overhead | Notes |
|-------------|----------|-------|
| Counter increment | ~10ns | Atomic operation |
| Histogram observe | ~50ns | Bucket search + atomic |
| Gauge set | ~10ns | Atomic operation |
| Full scrape | < 50ms | Text encoding |

**Optimization**: Use `prometheus::core::AtomicF64` for hot-path metrics.

---

## 11. Success Metrics

| Metric | Target |
|--------|--------|
| `/metrics` latency p99 | < 50ms |
| Scrape success rate | > 99.9% |
| Memory overhead | < 10MB |
| Cardinality | < 10,000 series |

---

## 12. Dependencies

- `prometheus = "0.13"` - Metrics collection
- `lazy_static` or `once_cell` - Global registry (if needed)

---

## 13. Rollout Plan

1. **Week 1**: Deploy with metrics disabled by default
2. **Week 2**: Enable in staging, validate with Prometheus
3. **Week 3**: Enable in production, import Grafana dashboards
4. **Week 4**: Configure alerting rules based on metrics

---

## 14. Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Cardinality explosion | Medium | High | Limit label values to controlled vocabularies; avoid user-supplied strings |
| Memory growth from metrics registry | Low | Medium | Use bounded histograms; periodic metric cleanup |
| Scrape timeout under load | Low | Medium | Pre-compute metric snapshots; cache encoded output |
| Breaking changes in Prometheus format | Low | Low | Pin `prometheus` crate version; test against Prometheus 2.x |
| Sensitive data leakage via labels | Medium | High | Code review for label values; never use PII |

---

## 15. Future Work

- **Push gateway support**: For short-lived jobs and batch training
- **OpenMetrics format**: Native support for the OpenMetrics 1.0 standard
- **Custom metric registration**: Allow adapters to register their own metrics
- **Metric aggregation**: Pre-aggregated metrics for high-cardinality scenarios
- **Exemplars**: Link traces to metrics for debugging
- **Remote write**: Direct integration with Prometheus remote write API

---

## 16. References

- [Prometheus Exposition Formats](https://prometheus.io/docs/instrumenting/exposition_formats/)
- [Prometheus Rust Client](https://docs.rs/prometheus/latest/prometheus/)
- [OpenMetrics Specification](https://openmetrics.io/)
- [Grafana Dashboard Best Practices](https://grafana.com/docs/grafana/latest/dashboards/build-dashboards/best-practices/)
