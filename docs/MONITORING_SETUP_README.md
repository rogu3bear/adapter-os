# Production Monitoring & Metal ↔ Hot-Swap Integration Setup

This guide consolidates three complementary documentation resources for production-grade monitoring and GPU management in AdapterOS.

---

## Documentation Files Created

### 1. **METAL_HOTSWAP_INTEGRATION.md**

**Location:** `/docs/METAL_HOTSWAP_INTEGRATION.md`

**Purpose:** Deep-dive technical guide for Metal kernel ↔ hot-swap system integration

**Key Sections:**
- **Architecture Diagram**: Visual flow of adapter lifecycle through hot-swap system
- **GPU Memory Management**: Memory pool pooling, pressure handling, eviction flows
- **Adapter ID Mapping**: BLAKE3-based deterministic u16 indexing (no collisions at scale)
- **Cross-Layer Integrity**: Metadata + GPU fingerprint verification for determinism
- **Sequence Diagrams**: Normal swaps, failures, rollbacks, concurrent inference
- **Error Recovery**: Panic handling, buffer corruption detection, device recovery
- **Integration Points**: How lifecycle manager, router, and metal kernels interact

**Use This Document When:**
- Investigating hot-swap latency issues
- Debugging GPU memory pressure
- Understanding determinism violation alerts
- Implementing new metal kernel backends
- Troubleshooting GPU panics or buffer corruption

**Key Diagrams:**
```
┌─────────────────────────────────────────────────────┐
│ Adapter Hot-Swap System                              │
├─────────────────────────────────────────────────────┤
│ Router Selection (K-sparse) → Lifecycle Manager      │
│ → Hot-Swap Coordinator (Preload/Swap/Verify)        │
│ → Metal GPU Layer (FusedMlp, FusedQkv, etc.)        │
│ → GpuMemoryPool (Allocation/Eviction)                │
│ → RecoveryWrapper (Panic Handling)                   │
└─────────────────────────────────────────────────────┘
```

---

### 2. **PRODUCTION_MONITORING.md**

**Location:** `/docs/PRODUCTION_MONITORING.md`

**Purpose:** Operational excellence guide with metrics, alerts, runbooks

**Key Sections:**
- **High-Level Monitoring Architecture**: Prometheus → Grafana → AlertManager flow
- **Core Metrics Definition (8 categories)**:
  - Inference Performance (latency, P99)
  - GPU Memory Management (pressure, fragmentation, reuse ratio)
  - Hot-Swap Operations (latency, memory freed, rollback rate)
  - Determinism & Integrity (violations, hash mismatches, collisions)
  - Metal Backend Health (panics, recovery success rate)
  - Adapter Lifecycle (state transitions, activation %)
  - Policy Compliance (violation counters)
  - System Resources (CPU, memory, GPU util)

- **Alert Rules (8 Prometheus rules)** with thresholds and escalation
- **Grafana Dashboard Configuration**: 8-panel overview dashboard
- **Performance Tuning Guidelines**: Root cause analysis matrix
- **Production Runbooks** (4 detailed):
  - High Inference Latency diagnosis & fixes
  - High Hot-Swap Latency troubleshooting
  - Determinism Violation emergency response
  - GPU Memory Pressure escalation
- **SLO Targets**: Availability, latency, memory, swap, determinism
- **Integration Examples**: Prometheus config, AlertManager routes, external systems

**Use This Document When:**
- Setting up Prometheus + Grafana monitoring
- Configuring alert rules
- Troubleshooting production issues via runbooks
- Tuning performance parameters
- Defining SLOs with stakeholders
- Integrating with PagerDuty/Slack

**Metrics by Category:**

| Category | Key Metrics | Alert Threshold |
|----------|-------------|-----------------|
| Inference | `inference_latency_ms` (p50/p95/p99) | p99 > 300ms |
| GPU Memory | `gpu_memory_pressure` | > 0.85 (critical) |
| Hot-Swap | `hotswap_latency_ms`, `swap_rollback_count` | p95 > 100ms |
| Determinism | `determinism_violations_total` | > 0 (zero-tolerance) |
| Metal | `metal_kernel_panic_count` | > 3 in 5m |
| Adapter Lifecycle | `adapter_state_transitions_total` | Thrashing detection |
| Policy | `policy_violations_total` | High/critical only |
| System | `cpu_usage_percent`, `memory_usage_bytes` | > 80% / leak detection |

---

### 3. **Critical Component Metrics Implementation**

**Location:** `crates/adapteros-telemetry/src/metrics/critical_components.rs`

**Purpose:** Production-ready Prometheus metric collectors for real-time observability

**Components:**

1. **CriticalComponentMetrics** struct
   - 20+ registered Prometheus metrics
   - Histogram collectors (execution time, latency)
   - Counter collectors (violations, panics, evictions)
   - Gauge collectors (memory pressure, activation %)

2. **Metric Types:**
   - **Histograms** (with buckets):
     - `metal_kernel_execution_us`: [10us, 25us, 50us, 100us, ..., 10ms]
     - `hotswap_latency_ms`: [1ms, 5ms, 10ms, ..., 1000ms]
     - `gpu_fingerprint_sample_time_us`: Sampling overhead tracking

   - **Counters** (increment-only):
     - `metal_kernel_panic_count_total`
     - `swap_rollback_count_total`
     - `determinism_violations_total`
     - `adapter_id_collisions_total`

   - **Gauges** (point-in-time):
     - `gpu_memory_pressure` (0.0-1.0)
     - `gpu_memory_pool_reuse_ratio`
     - `adapter_activation_percentage`

3. **Helper Classes:**
   - `KernelExecutionTimer`: Automatic duration recording on drop
   - `HotSwapTimer`: Records operation latency with status

4. **Integration Methods:**
   - `export()`: Prometheus text format export
   - `record_*()` methods: Direct metric recording
   - Thread-safe (Arc-wrapped Registry)

**Example Usage:**

```rust
// Initialize metrics
let metrics = Arc::new(CriticalComponentMetrics::new()?);

// Record kernel execution
let _timer = KernelExecutionTimer::new(
    "FusedMlp",
    "adapter-a",
    "4096",
    metrics.clone(),
);
// ... kernel execution ...
// Timer records duration automatically on drop

// Record hot-swap operation
let swap_timer = HotSwapTimer::new("swap", 2, metrics.clone());
// ... perform swap ...
swap_timer.record("success");

// Record determinism violation (critical alert)
metrics.record_determinism_violation(
    "hash_mismatch",
    "adapter-x",
    "critical",
);

// Set GPU memory pressure gauge
metrics.set_gpu_memory_pressure("gpu-0", 0.85);

// Export to Prometheus format
let prometheus_text = metrics.export()?;
println!("{}", prometheus_text);
```

**Integration Points:**
- Metal kernel dispatch wrapper: Record execution time + panics
- Hot-swap coordinator: Record swap operations + memory freed
- Lifecycle manager: Record state transitions + evictions
- GPU memory pool: Record pressure + fragmentation
- Determinism verifier: Record violations + hash mismatches
- HTTP /metrics endpoint: Export all metrics to Prometheus

---

## Quick Start

### 1. Deploy Prometheus + Grafana

```bash
# Install Prometheus
brew install prometheus grafana

# Update prometheus.yml
cat > /usr/local/etc/prometheus.yml << 'EOF'
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: adapteros
    static_configs:
      - targets: ['localhost:9090']
EOF

# Start Prometheus
prometheus --config.file=/usr/local/etc/prometheus.yml

# Start Grafana
brew services start grafana
# Navigate to http://localhost:3000 (admin/admin)
```

### 2. Integrate CriticalComponentMetrics

```rust
// In your worker initialization
use adapteros_telemetry::CriticalComponentMetrics;

let metrics = Arc::new(CriticalComponentMetrics::new()?);

// Expose metrics endpoint (Axum example)
app.route("/metrics", get({
    let metrics = metrics.clone();
    || async move {
        metrics.export().unwrap_or_default()
    }
}));

// Record metrics during operations
metrics.record_metal_kernel_execution(
    "FusedMlp",
    adapter_id,
    "4096",
    duration_us,
);
```

### 3. Import Documentation

All three documents are checked into `/docs/`:
- Use METAL_HOTSWAP_INTEGRATION.md for technical deep dives
- Use PRODUCTION_MONITORING.md for operational runbooks
- Use critical_components.rs for implementation details

---

## Alert Response Flowchart

```
┌─────────────────────────────────┐
│ Alert Fired (Prometheus Rule)   │
├─────────────────────────────────┤
│
├─► High Inference Latency?
│   └─► See: PRODUCTION_MONITORING.md → Runbook: High Inference Latency
│   └─► Check: router_latency_ms vs kernel_latency_ms
│   └─► Action: Profile bottleneck, tune parameters
│
├─► GPU Memory Pressure Critical?
│   └─► See: METAL_HOTSWAP_INTEGRATION.md → Memory Pressure Flow
│   └─► Check: gpu_memory_pressure gauge, adapter sizes
│   └─► Action: Trigger eviction, reduce K-sparse size
│
├─► High Hot-Swap Latency?
│   └─► See: PRODUCTION_MONITORING.md → Runbook: High Hot-Swap Latency
│   └─► Check: operation="preload" vs "swap" vs "verify"
│   └─► Action: Profile disk I/O or GPU fingerprinting
│
├─► Determinism Violation?
│   └─► IMMEDIATE CRITICAL PAGE
│   └─► See: METAL_HOTSWAP_INTEGRATION.md → Error Recovery Flows
│   └─► Action: Quarantine adapter, preserve GPU state, investigate
│
└─► GPU Panic Detected?
    └─► See: METAL_HOTSWAP_INTEGRATION.md → Panic Recovery
    └─► Action: Trigger GPU recovery, check for repeat panics
```

---

## File Location Reference

| Document | Path | Primary Use |
|----------|------|-------------|
| Metal ↔ Hot-Swap Integration | `docs/METAL_HOTSWAP_INTEGRATION.md` | Technical deep dives, debugging GPU issues |
| Production Monitoring | `docs/PRODUCTION_MONITORING.md` | Prometheus setup, runbooks, tuning |
| Critical Components Code | `crates/adapteros-telemetry/src/metrics/critical_components.rs` | Implementation, code examples |
| This README | `docs/MONITORING_SETUP_README.md` | Overview, quick start, integration summary |

---

## Metrics Roadmap

### Phase 1: Core Metrics (Current)
- Inference latency (p50/p95/p99)
- GPU memory pressure
- Hot-swap latency
- Metal kernel panics
- Determinism violations

### Phase 2: Extended Metrics (Planned)
- Per-adapter profiling (latency breakdown)
- Router gate computation cost
- Memory fragmentation analysis
- Pinned adapter tracking
- RCU retirement backlog

### Phase 3: Advanced Features (Future)
- Anomaly detection (statistical)
- Predictive alerts (trending)
- Cost optimization recommendations
- Multi-tenant resource tracking
- Cross-cluster federation metrics

---

## Testing Metrics Locally

```bash
# Build telemetry crate
cargo build -p adapteros-telemetry

# Run metric tests
cargo test -p adapteros-telemetry metrics::critical_components

# Generate sample metrics
cargo run --example critical_metrics_demo

# Export to Prometheus format
curl http://localhost:9090/metrics > metrics.txt
```

---

## Common Issues & Solutions

| Issue | Symptom | Solution |
|-------|---------|----------|
| Metrics not exported | `/metrics` endpoint empty | Verify CriticalComponentMetrics::export() is wired to HTTP handler |
| Prometheus not scraping | Targets show "Down" | Check `scrape_interval` in prometheus.yml, verify network connectivity |
| High cardinality metrics | Memory explosion in Prometheus | Limit label values (adapter_id), use aggregation |
| Missing histograms | Buckets not appearing | Ensure HistogramVec is registered with Registry |
| Gauge not updating | Stale values in Grafana | Verify set_gpu_memory_pressure() is called periodically |

---

## Production Deployment Checklist

- [ ] Prometheus configured with 15s scrape interval
- [ ] Grafana dashboard imported and validated
- [ ] AlertManager configured with escalation routes
- [ ] PagerDuty/Slack integration tested
- [ ] CriticalComponentMetrics integrated into worker code
- [ ] Runbooks printed/posted in NOC
- [ ] On-call engineer trained on alert response
- [ ] SLO targets documented and tracked
- [ ] Backup/retention policy for metrics (~15 days)
- [ ] Load testing to validate thresholds

---

## References

- [METAL_HOTSWAP_INTEGRATION.md](METAL_HOTSWAP_INTEGRATION.md) - Full technical documentation
- [PRODUCTION_MONITORING.md](PRODUCTION_MONITORING.md) - Operational guide
- [ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - System architecture
- [CLAUDE.md](../CLAUDE.md) - Project standards
- Prometheus Docs: https://prometheus.io/docs/
- Grafana Docs: https://grafana.com/docs/grafana/latest/

---

**Status:** Production-Ready (Alpha v0.01-1)
**Last Updated:** 2025-11-21
**Maintained by:** James KC Auchterlonie
