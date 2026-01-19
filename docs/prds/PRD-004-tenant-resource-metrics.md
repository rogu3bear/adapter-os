# PRD-004: Tenant Resource Metrics Implementation

**Status**: Draft
**Priority**: P1 (High)
**Estimated Effort**: 6-8 hours
**Owner**: TBD

---

## 1. Problem Statement

The multi-tenant resource metrics have placeholder implementations returning hardcoded zeros:

```rust
// crates/adapteros-db/src/tenants.rs:829-833
storage_used_gb: 0.0,      // TODO: calculate from artifacts
cpu_usage_pct: 0.0,        // TODO: from metrics
gpu_usage_pct: 0.0,        // TODO: from metrics
memory_used_gb: 0.0,       // TODO: from metrics
memory_total_gb: 0.0,      // TODO: from metrics
```

This blocks:
- **Resource quotas**: Cannot enforce storage/compute limits per tenant
- **Billing**: Cannot bill tenants based on actual usage
- **Capacity planning**: Cannot identify resource-heavy tenants
- **Fair scheduling**: Cannot prioritize or throttle based on usage

---

## 2. Scope

### In Scope

| Metric | Source | Granularity |
|--------|--------|-------------|
| `storage_used_gb` | Database + artifact store | Per-tenant |
| `cpu_usage_pct` | Per-request tracking | Per-tenant, rolling window |
| `gpu_usage_pct` | Metal/MLX metrics | Per-tenant, rolling window |
| `memory_used_gb` | Process memory attribution | Per-tenant, snapshot |
| `memory_total_gb` | Sysinfo total | Global |

### Out of Scope

- Per-request billing (separate PRD)
- Historical usage analytics (separate PRD)
- Cost allocation models (separate PRD)

---

## 3. Technical Analysis

### 3.1 Storage Metrics

**Data Sources**:

1. **Database storage**: Sum of blob sizes in tenant-scoped tables
2. **Artifact storage**: Sum of files in `var/artifacts/{tenant_id}/`
3. **Model cache**: Shared models not attributed to tenants

**Query for Database Storage**:

```sql
SELECT
    tenant_id,
    SUM(
        COALESCE(LENGTH(adapter_weights), 0) +
        COALESCE(LENGTH(training_data), 0) +
        COALESCE(LENGTH(checkpoint_data), 0)
    ) / (1024.0 * 1024.0 * 1024.0) as storage_gb
FROM adapters
GROUP BY tenant_id;
```

**Artifact Storage Calculation**:

```rust
pub async fn calculate_artifact_storage(tenant_id: &str) -> Result<f64, IoError> {
    let artifact_path = PathBuf::from(format!("var/artifacts/{}", tenant_id));

    if !artifact_path.exists() {
        return Ok(0.0);
    }

    let mut total_bytes: u64 = 0;
    for entry in WalkDir::new(&artifact_path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            total_bytes += entry.metadata()?.len();
        }
    }

    Ok(total_bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}
```

### 3.2 CPU Metrics

**Approach**: Track CPU time per request, aggregate by tenant over rolling window.

**Data Structure**:

```rust
pub struct TenantCpuTracker {
    /// CPU time in microseconds per tenant, keyed by tenant_id
    usage: DashMap<String, AtomicU64>,
    /// Window start time
    window_start: AtomicU64,
    /// Window duration in seconds
    window_duration: u64,
}

impl TenantCpuTracker {
    pub fn record_cpu_time(&self, tenant_id: &str, cpu_micros: u64) {
        self.usage
            .entry(tenant_id.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(cpu_micros, Ordering::Relaxed);
    }

    pub fn get_cpu_percent(&self, tenant_id: &str) -> f64 {
        let tenant_micros = self.usage
            .get(tenant_id)
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0);

        let window_micros = self.window_duration * 1_000_000;
        let num_cpus = num_cpus::get() as u64;

        // CPU percent = (tenant_cpu_time / (window_duration * num_cpus)) * 100
        (tenant_micros as f64 / (window_micros * num_cpus) as f64) * 100.0
    }
}
```

**Integration Point**: Wrap inference handlers with CPU time measurement:

```rust
pub async fn inference_handler(req: InferRequest, state: AppState) -> Response {
    let start = std::time::Instant::now();
    let cpu_start = get_thread_cpu_time();

    let response = do_inference(req, &state).await;

    let cpu_end = get_thread_cpu_time();
    let cpu_micros = (cpu_end - cpu_start).as_micros() as u64;

    state.cpu_tracker.record_cpu_time(&req.tenant_id, cpu_micros);

    response
}
```

### 3.3 GPU Metrics

**Approach**: Use Metal performance counters or MLX memory stats.

**Metal GPU Utilization** (macOS):

```rust
#[cfg(target_os = "macos")]
pub fn get_gpu_utilization() -> Result<f64, GpuError> {
    use metal::Device;

    let device = Device::system_default().ok_or(GpuError::NoDevice)?;

    // Metal tracks memory pressure as proxy for utilization
    let current_memory = device.current_allocated_size();
    let max_memory = device.recommended_max_working_set_size();

    Ok((current_memory as f64 / max_memory as f64) * 100.0)
}
```

**Per-Tenant GPU Attribution**:

Since GPU work is serialized, attribute GPU time to the tenant whose request is executing:

```rust
pub struct TenantGpuTracker {
    /// Current tenant using GPU (if any)
    current_tenant: RwLock<Option<String>>,
    /// GPU time in microseconds per tenant
    usage: DashMap<String, AtomicU64>,
}

impl TenantGpuTracker {
    pub fn begin_gpu_work(&self, tenant_id: &str) {
        *self.current_tenant.write() = Some(tenant_id.to_string());
    }

    pub fn end_gpu_work(&self, tenant_id: &str, gpu_micros: u64) {
        self.usage
            .entry(tenant_id.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(gpu_micros, Ordering::Relaxed);
        *self.current_tenant.write() = None;
    }
}
```

### 3.4 Memory Metrics

**Total Memory**:

```rust
pub fn get_total_memory_gb() -> f64 {
    use sysinfo::{System, SystemExt};
    let sys = System::new_all();
    sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0)
}
```

**Per-Tenant Memory** (approximation via adapter/cache size):

```rust
pub fn get_tenant_memory_gb(tenant_id: &str, state: &AppState) -> f64 {
    let mut total_bytes: u64 = 0;

    // Loaded adapters for this tenant
    for adapter in state.adapter_cache.get_loaded_for_tenant(tenant_id) {
        total_bytes += adapter.memory_size_bytes();
    }

    // KV cache allocations for active sessions
    for session in state.session_manager.get_sessions_for_tenant(tenant_id) {
        total_bytes += session.kv_cache_size_bytes();
    }

    total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
}
```

---

## 4. Implementation Plan

### Phase 1: Storage Metrics (2 hours)

1. Add storage calculation queries to `adapteros-db`
2. Implement `calculate_artifact_storage()` filesystem walk
3. Update `TenantMetrics::storage_used_gb` to call real calculation
4. Cache results with 5-minute TTL (storage changes slowly)

### Phase 2: Metrics Collection (2 hours)

1. Add `sysinfo` dependency for memory metrics
2. Implement `TenantCpuTracker` with rolling window
3. Implement `TenantGpuTracker` for GPU attribution
4. Add metrics collection middleware

### Phase 3: Integration (2 hours)

1. Wire trackers into `AppState`
2. Update `get_tenant_metrics()` to pull from trackers
3. Add `/api/tenants/{id}/metrics` endpoint
4. Add Prometheus export for tenant metrics

### Phase 4: Testing & Validation (2 hours)

1. Unit tests for each metric calculation
2. Integration test with synthetic load
3. Validate metrics accuracy
4. Load test to ensure metrics don't add significant overhead

---

## 5. API Changes

### New Endpoint: GET /api/tenants/{tenant_id}/metrics

**Response**:

```json
{
  "tenant_id": "tenant-abc",
  "metrics": {
    "storage_used_gb": 12.5,
    "cpu_usage_pct": 15.3,
    "gpu_usage_pct": 45.0,
    "memory_used_gb": 8.2,
    "memory_total_gb": 64.0
  },
  "window": {
    "start": "2025-01-18T10:00:00Z",
    "end": "2025-01-18T10:05:00Z",
    "duration_seconds": 300
  },
  "collected_at": "2025-01-18T10:05:00Z"
}
```

---

## 6. Acceptance Criteria

- [ ] `storage_used_gb` reflects actual database + artifact storage
- [ ] `cpu_usage_pct` updates based on inference workload
- [ ] `gpu_usage_pct` reflects Metal/MLX utilization during inference
- [ ] `memory_used_gb` reflects loaded adapters and KV cache
- [ ] `memory_total_gb` returns actual memory
- [ ] Metrics endpoint returns data within 100ms
- [ ] Metrics overhead < 1% of request latency

---

## 7. Testing Strategy

### Unit Tests

```rust
#[test]
fn test_storage_calculation() {
    let db = TestDb::with_tenant("tenant-1");
    db.insert_adapter_with_size("adapter-1", 1024 * 1024 * 100); // 100MB

    let metrics = get_tenant_metrics(&db, "tenant-1").await;
    assert!((metrics.storage_used_gb - 0.1).abs() < 0.01);
}

#[test]
fn test_cpu_tracking() {
    let tracker = TenantCpuTracker::new(Duration::from_secs(60));

    tracker.record_cpu_time("tenant-1", 1_000_000); // 1 second
    tracker.record_cpu_time("tenant-2", 2_000_000); // 2 seconds

    // On 8-core machine, 1 second in 60-second window = 0.2% per core
    let pct = tracker.get_cpu_percent("tenant-1");
    assert!(pct > 0.0 && pct < 5.0);
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_metrics_endpoint() {
    let app = TestApp::new();
    app.create_tenant("tenant-1").await;

    // Generate some load
    for _ in 0..10 {
        app.inference("tenant-1", "Hello").await;
    }

    let metrics = app.get_tenant_metrics("tenant-1").await;
    assert!(metrics.cpu_usage_pct > 0.0);
    assert!(metrics.memory_used_gb > 0.0);
}
```

---

## 8. Performance Considerations

| Operation | Target Latency | Approach |
|-----------|---------------|----------|
| Storage calculation | < 500ms | Cache with 5-min TTL |
| CPU/GPU metrics | < 1ms | In-memory atomic counters |
| Memory metrics | < 10ms | Direct sysinfo query |
| Full metrics response | < 100ms | Parallel collection |

**Caching Strategy**:

```rust
pub struct MetricsCache {
    storage: Cache<String, f64>,  // tenant_id -> storage_gb
    ttl: Duration,
}

impl MetricsCache {
    pub async fn get_storage(&self, tenant_id: &str, db: &Db) -> f64 {
        self.storage
            .try_get_with(tenant_id.to_string(), async {
                calculate_storage(tenant_id, db).await
            })
            .await
            .unwrap_or(0.0)
    }
}
```

---

## 9. Prometheus Metrics Export

```rust
// Gauge metrics per tenant
gauge!("aos.tenant.storage_gb", "tenant_id" => tenant_id).set(storage);
gauge!("aos.tenant.cpu_pct", "tenant_id" => tenant_id).set(cpu_pct);
gauge!("aos.tenant.gpu_pct", "tenant_id" => tenant_id).set(gpu_pct);
gauge!("aos.tenant.memory_gb", "tenant_id" => tenant_id).set(memory_gb);

// Global metrics
gauge!("aos.memory_total_gb").set(total_memory);
gauge!("aos.gpu_available").set(gpu_available as i64);
```

---

## 10. Success Metrics

| Metric | Before | After | Target |
|--------|--------|-------|--------|
| Storage accuracy | 0% (hardcoded) | ±5% of actual | ±5% |
| CPU tracking | 0% (hardcoded) | ±10% of actual | ±10% |
| GPU tracking | 0% (hardcoded) | ±15% of actual | ±15% |
| Memory tracking | 0% (hardcoded) | ±10% of actual | ±10% |
| Metrics latency | N/A | < 100ms | < 100ms |

---

## 11. Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Storage calculation slow on large artifact directories | Medium | Medium | Cache results with 5-min TTL; use async calculation |
| CPU tracking overhead affects inference latency | Low | High | Use atomic counters; measure overhead < 1% |
| GPU attribution inaccurate under high concurrency | Medium | Medium | Serialize GPU work tracking; document limitations |
| Memory attribution incomplete (shared model weights) | High | Low | Document that shared resources aren't attributed; use approximation |
| Metrics cardinality explosion with many tenants | Medium | Medium | Implement tenant label limits; aggregate small tenants |
| Rolling window data loss on server restart | Medium | Low | Accept loss for CPU/GPU; storage is persistent |

---

## 12. Monitoring & Alerting

### Metrics Collection Health

```rust
// Self-monitoring metrics
counter!("aos.metrics.storage_calculation.success").increment(1);
counter!("aos.metrics.storage_calculation.error").increment(1);
histogram!("aos.metrics.storage_calculation.duration_seconds").record(duration);

gauge!("aos.metrics.cpu_tracker.tenants_tracked").set(tenant_count);
gauge!("aos.metrics.gpu_tracker.tenants_tracked").set(tenant_count);
```

### Alerts

| Alert | Condition | Severity |
|-------|-----------|----------|
| `StorageCalculationFailing` | Storage calculation errors > 5 in 5 min | P2 |
| `MetricsEndpointSlow` | `/api/tenants/{id}/metrics` p99 > 500ms | P3 |
| `TenantStorageExceeded` | `storage_used_gb` > quota (when quotas enabled) | P2 |
| `HighTenantCpuUsage` | `cpu_usage_pct` > 80% sustained 10 min | P3 |
| `MetricsDataStale` | No metric updates in 10 min | P2 |

---

## 13. Security Considerations

| Concern | Mitigation |
|---------|------------|
| Tenant data isolation | Metrics queries scoped by tenant_id; no cross-tenant access |
| Resource exhaustion via metrics | Rate limit `/api/tenants/{id}/metrics` to 10 req/min/tenant |
| Information disclosure | Metrics endpoint requires tenant authentication |
| Path traversal in artifact storage | Validate tenant_id format; use canonical paths |
| Timing attacks | Constant-time comparison not needed (non-security-sensitive) |

**Tenant ID Validation**:

```rust
pub fn validate_tenant_id(tenant_id: &str) -> Result<(), ValidationError> {
    // Only allow alphanumeric, hyphens, underscores
    let valid = tenant_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_');
    if !valid || tenant_id.is_empty() || tenant_id.len() > 64 {
        return Err(ValidationError::InvalidTenantId);
    }
    Ok(())
}
```

---

## 14. Dependencies

### Crate Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `sysinfo` | 0.30+ | System memory metrics |
| `walkdir` | 2.4+ | Artifact directory traversal |
| `dashmap` | 5.5+ | Concurrent tenant tracking maps |
| `num_cpus` | 1.16+ | CPU count for percentage calculation |

### Internal Dependencies

- `adapteros-db`: Storage queries for tenant artifacts
- `adapteros-telemetry`: Prometheus gauge/counter macros
- `adapteros-core`: Tenant ID validation utilities

### PRD Dependencies

- **PRD-005** (Prometheus Metrics Endpoint): Provides the metrics export infrastructure this PRD builds upon

---

## 15. Rollout Plan

### Week 1: Storage Metrics (Low Risk)

1. Deploy storage calculation with caching
2. Populate `storage_used_gb` in tenant metrics response
3. Monitor calculation latency and cache hit rate
4. **Rollback**: Revert to hardcoded 0.0 if calculation causes issues

### Week 2: Memory Metrics (Low Risk)

1. Deploy `sysinfo`-based memory metrics
2. Populate `memory_total_gb` (global) and `memory_used_gb` (per-tenant approximation)
3. Validate accuracy against system monitors
4. **Rollback**: Disable memory collection flag

### Week 3: CPU/GPU Tracking (Medium Risk)

1. Deploy `TenantCpuTracker` and `TenantGpuTracker`
2. Instrument inference handlers
3. Monitor overhead impact on inference latency
4. **Rollback**: Disable tracking; return 0.0 for CPU/GPU metrics

### Week 4: Full Integration

1. Enable all metrics in production
2. Configure Prometheus scraping for tenant metrics
3. Import Grafana dashboards for tenant resource visibility
4. Set up alerting for quota violations (if quotas enabled)

### Feature Flags

```toml
# configs/cp.toml
[metrics.tenant]
storage_enabled = true      # Phase 1
memory_enabled = true       # Phase 2
cpu_tracking_enabled = true # Phase 3
gpu_tracking_enabled = true # Phase 3
```

---

## 16. Future Work

- **Historical analytics**: Store metrics over time for trend analysis
- **Quota enforcement**: Block requests when tenant exceeds limits
- **Billing integration**: Export metrics to billing engine
- **Predictive scaling**: Use metrics to predict capacity needs
