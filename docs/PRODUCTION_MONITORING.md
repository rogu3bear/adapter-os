# Production Monitoring & Observability Guide

**Purpose:** Define key metrics, alerting thresholds, and operational runbooks for AdapterOS production deployment

**Last Updated:** 2025-11-21

**Scope:** AdapterOS cluster monitoring, dashboards, alert escalation

---

## High-Level Monitoring Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        Telemetry Sources                          │
├──────────────────────────────────────────────────────────────────┤
│
│ ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ │ adapteros-   │  │ adapteros-   │  │ adapteros-   │
│ │ lora-worker  │  │ lora-kernel- │  │ lora-         │
│ │              │  │ mtl          │  │ lifecycle    │
│ └──────┬───────┘  └──────┬───────┘  └──────┬───────┘
│        │                 │                 │
│        ├─► Kernel        ├─► GPU Memory   ├─► Adapter
│        │   Latency       │   Pressure     │   State
│        │                 │                │   Transitions
│        │                 ├─► Metal        │
│        │                 │   Panics       │
│        │                 │                │
│        └─────────┬───────┴────────┬───────┘
│                  │                │
│                  ▼                ▼
│        ┌────────────────────────────────────┐
│        │ adapteros-telemetry               │
│        │ MetricsCollector + Prometheus     │
│        │ - Histogram buckets               │
│        │ - Counter increments              │
│        │ - Gauge snapshots                 │
│        └────────────┬─────────────┬────────┘
│                     │             │
│        ┌────────────▼──┐   ┌──────▼─────────┐
│        │ Prometheus    │   │ UDS Exporter   │
│        │ :9090         │   │ (macOS only)   │
│        └────────────┬──┘   └────────────────┘
│                     │
│                     ▼
│        ┌──────────────────────────┐
│        │ Grafana Dashboard        │
│        │ - Real-time metrics      │
│        │ - Historical trends      │
│        │ - Alert status           │
│        └──────────────────────────┘
│
│        ┌──────────────────────────┐
│        │ Alert Manager            │
│        │ - Rule evaluation        │
│        │ - Escalation logic       │
│        │ - Pagerduty/Slack        │
│        └──────────────────────────┘
│
└──────────────────────────────────────────────────────────────────┘
```

---

## Core Metrics Definition

### 1. Inference Performance

#### Metric: `inference_latency_ms`

**Type:** Histogram with buckets: [1, 5, 10, 25, 50, 100, 250, 500, 1000, 5000]

**Description:** End-to-end inference request latency (request received → response sent)

**Labels:**
- `adapter_id`: Selected adapter or stack ID
- `k`: K-sparse selection size (2, 4, 8, 16)
- `batch_size`: Request batch size
- `backend`: CoreML | MLX | Metal

**Prometheus Query:**
```promql
# P50 latency per adapter
histogram_quantile(0.50, rate(inference_latency_ms_bucket[5m]))

# P99 latency trend
histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m]))

# Alert: P99 latency > 500ms
histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m])) > 500
```

**Alert Threshold:**
- **WARNING:** p99 > 300ms for 5 minutes
- **CRITICAL:** p99 > 1000ms for 2 minutes
- **Action:** Page on-call SRE

**Runbook Link:** [inference-latency-high](#runbook-high-inference-latency)

---

#### Metric: `router_latency_ms`

**Type:** Histogram

**Description:** K-sparse adapter selection latency

**Labels:**
- `k`: K-sparse size
- `num_adapters`: Total adapters in registry

**Prometheus Query:**
```promql
# Router should be sub-millisecond
histogram_quantile(0.99, router_latency_ms_bucket) < 10
```

**Alert Threshold:**
- **WARNING:** p99 > 50ms
- **CRITICAL:** p99 > 200ms
- **Root Cause:** Router gate computation expensive, embedding computation slow

---

#### Metric: `kernel_latency_ms`

**Type:** Histogram

**Description:** Metal kernel execution time (dispatch → completion)

**Labels:**
- `kernel_type`: FusedMlp | FusedQkv | RmsNorm | etc.
- `adapter_id`: Which adapter's kernel
- `input_shape`: Input tensor dimensions

**Prometheus Query:**
```promql
# Per-kernel performance
histogram_quantile(0.95, kernel_latency_ms_bucket{kernel_type="FusedMlp"})

# Anomaly detection: kernel time increased 2x
kernel_latency_ms_bucket{quantile="p95"} >
  2 * avg_over_time(kernel_latency_ms_bucket[1h])
```

**Alert Threshold:**
- **WARNING:** p95 latency increased > 50% in 5min
- **CRITICAL:** p95 latency increased > 100% in 5min
- **Action:** Investigate GPU memory fragmentation, adapter swap overhead

---

### 2. GPU Memory Management

#### Metric: `gpu_memory_pressure`

**Type:** Gauge (0.0 - 1.0)

**Description:** GPU memory utilization ratio (current / total_available)

**Prometheus Query:**
```promql
# Current pressure
gpu_memory_pressure

# Alert when approaching threshold
gpu_memory_pressure > 0.85

# Trend analysis (pressure increasing?)
(gpu_memory_pressure - gpu_memory_pressure offset 5m) / gpu_memory_pressure offset 5m > 0.1
```

**Alert Threshold:**
- **WARNING:** > 0.75 for 5 minutes
- **CRITICAL:** > 0.85 for 2 minutes
- **Action:** Trigger adapter eviction, page on-call

**Recovery Actions:**
1. Lifecycle manager initiates eviction of lowest-activation adapters
2. GpuMemoryPool deallocates idle buffers
3. Monitor pressure every 10 seconds until < 0.70

---

#### Metric: `gpu_memory_pool_reuse_ratio`

**Type:** Gauge (0.0 - 1.0)

**Description:** Ratio of reused buffers to total allocations

**Prometheus Query:**
```promql
# High reuse ratio is good (less allocation overhead)
gpu_memory_pool_reuse_ratio > 0.90

# Low reuse indicates fragmentation
gpu_memory_pool_reuse_ratio < 0.50
```

**Alert Threshold:**
- **WARNING:** < 0.50 for 10 minutes (pool fragmentation)
- **Action:** Increase idle_timeout_secs in config, review buffer allocation pattern

---

#### Metric: `hotswap_memory_freed_mb`

**Type:** Counter

**Description:** Total GPU memory freed by hot-swap operations

**Prometheus Query:**
```promql
# Freed memory rate (MB/min)
rate(hotswap_memory_freed_mb_total[1m])

# Cumulative freed
increase(hotswap_memory_freed_mb_total[1h])
```

**Use Case:** Track memory reclamation efficiency after evictions

---

### 3. Hot-Swap Operations

#### Metric: `hotswap_latency_ms`

**Type:** Histogram

**Description:** Total hot-swap operation duration (preload + swap + verify)

**Labels:**
- `operation`: preload | swap | verify
- `adapter_count`: Number of adapters swapped
- `status`: success | rollback | failure

**Prometheus Query:**
```promql
# P95 latency per operation type
histogram_quantile(0.95, hotswap_latency_ms_bucket{operation="swap"})

# Failure rate
rate(hotswap_latency_ms_bucket{status="failure"}[5m]) /
rate(hotswap_latency_ms_bucket[5m]) > 0.01
```

**Alert Threshold:**
- **WARNING:** p95 > 100ms for 5 minutes
- **CRITICAL:** p95 > 250ms for 2 minutes
- **Failure Rate:** > 1% failures

**Runbook Link:** [hotswap-latency-high](#runbook-high-hotswap-latency)

---

#### Metric: `swap_rollback_count_total`

**Type:** Counter

**Description:** Total number of hot-swap rollbacks (failed swaps)

**Labels:**
- `reason`: gpu_fingerprint_mismatch | memory_unavailable | kernel_panic | timeout

**Prometheus Query:**
```promql
# Rollback rate
rate(swap_rollback_count_total[5m])

# By reason
rate(swap_rollback_count_total{reason="gpu_fingerprint_mismatch"}[5m])

# Alert: > 5% of swaps fail
rate(swap_rollback_count_total[5m]) / rate(swap_count_total[5m]) > 0.05
```

**Alert Threshold:**
- **WARNING:** > 1 rollback per minute
- **CRITICAL:** > 5 per minute
- **Action:** Page SRE, review adapter health

---

### 4. Determinism & Integrity

#### Metric: `determinism_violations_total`

**Type:** Counter

**Description:** Total determinism policy violations detected

**Labels:**
- `violation_type`: hash_mismatch | random_access | float_non_determinism | gpu_buffer_corruption
- `adapter_id`

**Prometheus Query:**
```promql
# Any violation is critical
increase(determinism_violations_total[5m]) > 0

# Per-adapter violations
increase(determinism_violations_total[5m]) by (adapter_id)
```

**Alert Threshold:**
- **CRITICAL:** Any violation (zero-tolerance)
- **Action:** Immediate page, quarantine adapter, manual investigation

**Runbook Link:** [determinism-violation](#runbook-determinism-violation)

---

#### Metric: `gpu_buffer_integrity_violations_total`

**Type:** Counter

**Description:** GPU buffer fingerprint mismatches detected

**Labels:**
- `adapter_id`
- `check_type`: pre_swap | post_inference | scheduled_verify

**Prometheus Query:**
```promql
# Critical: buffer corruption
increase(gpu_buffer_integrity_violations_total[5m]) > 0

# Frequency trend
rate(gpu_buffer_integrity_violations_total[5m])
```

**Alert Threshold:**
- **CRITICAL:** Any violation detected
- **Action:** Mark adapter as corrupted, trigger manual recovery

---

#### Metric: `adapter_id_collisions_total`

**Type:** Counter

**Description:** BLAKE3 hash collisions in adapter ID → u16 mapping

**Labels:**
- `adapter_id_1`
- `adapter_id_2`

**Prometheus Query:**
```promql
# Collision detection
increase(adapter_id_collisions_total[1h]) > 0
```

**Alert Threshold:**
- **CRITICAL:** Any collision (should be 0 at scale)
- **Action:** Manual investigation, document collision, consider u32 mapping

---

### 5. Metal Backend Health

#### Metric: `metal_kernel_panic_count_total`

**Type:** Counter

**Description:** GPU kernel panics caught by RecoveryWrapper

**Labels:**
- `kernel_type`: FusedMlp | FusedQkv | RmsNorm | etc.
- `adapter_id`

**Prometheus Query:**
```promql
# Panic rate per kernel type
rate(metal_kernel_panic_count_total[5m]) by (kernel_type)

# Alert: > 3 panics in 5 minutes
increase(metal_kernel_panic_count_total[5m]) > 3
```

**Alert Threshold:**
- **WARNING:** > 1 panic in 5 minutes
- **CRITICAL:** > 3 panics in 5 minutes
- **Action:** Page on-call, trigger GPU recovery

---

#### Metric: `gpu_device_recovery_count_total`

**Type:** Counter

**Description:** Successful GPU device recovery operations

**Labels:**
- `recovery_type`: command_queue | device_reset | kernel_reload
- `status`: success | failure

**Prometheus Query:**
```promql
# Recovery success rate
rate(gpu_device_recovery_count_total{status="success"}[5m]) /
rate(gpu_device_recovery_count_total[5m])

# Alert: recovery failures
increase(gpu_device_recovery_count_total{status="failure"}[5m]) > 0
```

**Alert Threshold:**
- **WARNING:** Recovery success rate < 95%
- **CRITICAL:** Recovery failure detected
- **Action:** Manual GPU restart may be needed

---

### 6. Adapter Lifecycle

#### Metric: `adapter_state_transitions_total`

**Type:** Counter

**Description:** Adapter state changes (Unloaded → Cold → Warm → Hot → Resident)

**Labels:**
- `adapter_id`
- `from_state`: Unloaded | Cold | Warm | Hot | Resident
- `to_state`: Same
- `reason`: router_selection | eviction | timeout | manual_load

**Prometheus Query:**
```promql
# Promotion rate
rate(adapter_state_transitions_total{to_state="Hot"}[5m])

# Eviction rate
rate(adapter_state_transitions_total{from_state!="Unloaded", to_state="Unloaded"}[5m])

# Thrashing detection: many transitions for single adapter
rate(adapter_state_transitions_total{adapter_id="adapter-x"}[5m]) > 1
```

**Alert Threshold:**
- **WARNING:** Adapter thrashing (> 1 transition/min for same adapter)
- **Action:** Review router selection logic, check memory pressure

---

#### Metric: `adapter_activation_percentage`

**Type:** Gauge

**Description:** Percentage of time adapter was actively used (for lifecycle scoring)

**Labels:**
- `adapter_id`
- `time_window`: 5m | 1h | 24h

**Prometheus Query:**
```promql
# Lowest activation adapters (eviction candidates)
sort(adapter_activation_percentage)[0:5]

# High-value adapters
sort_desc(adapter_activation_percentage)[0:5]
```

**Use Case:** Feed into eviction policy, detect unused adapters

---

### 7. Policy Compliance

#### Metric: `policy_violations_total`

**Type:** Counter

**Description:** Policy enforcement violations (23 canonical policies)

**Labels:**
- `policy`: egress | determinism | router | evidence | telemetry | naming | isolation | ...
- `severity`: low | medium | high | critical
- `action`: blocked | logged | escalated

**Prometheus Query:**
```promql
# By policy
increase(policy_violations_total[5m]) by (policy)

# Critical violations
increase(policy_violations_total{severity="critical"}[5m]) > 0
```

**Alert Threshold:**
- **CRITICAL:** Any high/critical violation
- **Action:** Immediate escalation

---

### 8. System Resource Utilization

#### Metric: `cpu_usage_percent`

**Type:** Gauge

**Description:** System CPU usage across all AOS processes

**Prometheus Query:**
```promql
# Alert: CPU > 80% for 10 min
cpu_usage_percent > 80
```

**Alert Threshold:**
- **WARNING:** > 70% for 5 minutes
- **CRITICAL:** > 90% for 2 minutes

---

#### Metric: `memory_usage_bytes`

**Type:** Gauge

**Labels:**
- `component`: system | adapter_table | gpu_pool | kernel_buffers

**Prometheus Query:**
```promql
# Total system memory
sum(memory_usage_bytes)

# Per-component breakdown
memory_usage_bytes by (component)

# Memory leak detection (monotonic increase)
(memory_usage_bytes - memory_usage_bytes offset 1h) / memory_usage_bytes offset 1h > 0.2
```

**Alert Threshold:**
- **WARNING:** Increase > 20% in 1 hour
- **CRITICAL:** Increase > 50% in 1 hour
- **Action:** Profile memory usage, check for leaks

---

## Alert Rules

### Rule: HighInferenceLaten

**Expression:**
```promql
histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m])) > 300
```

**For:** 5 minutes

**Labels:**
- `severity`: warning

**Annotations:**
- `summary`: "High inference latency (p99 = {{ $value }}ms)"
- `runbook`: "#runbook-high-inference-latency"

---

### Rule: GPUMemoryPressure

**Expression:**
```promql
gpu_memory_pressure > 0.85
```

**For:** 2 minutes

**Labels:**
- `severity`: critical

**Annotations:**
- `summary`: "GPU memory pressure critical ({{ $value | humanizePercentage }})"
- `action`: "Trigger manual eviction or page SRE"

---

### Rule: HotSwapRollbacks

**Expression:**
```promql
increase(swap_rollback_count_total[5m]) > 5
```

**For:** 2 minutes

**Labels:**
- `severity`: critical

**Annotations:**
- `summary`: "{{ $value }} hot-swap rollbacks in 5m"
- `runbook`: "#runbook-hotswap-failures"

---

### Rule: DeterminismViolation

**Expression:**
```promql
increase(determinism_violations_total[5m]) > 0
```

**For:** 0 minutes (immediate)

**Labels:**
- `severity`: critical

**Annotations:**
- `summary`: "Determinism violation detected: {{ $labels.violation_type }}"
- `action`: "IMMEDIATE PAGE - Quarantine adapter {{ $labels.adapter_id }}"

---

### Rule: GPUPanic

**Expression:**
```promql
increase(metal_kernel_panic_count_total[5m]) > 3
```

**For:** 2 minutes

**Labels:**
- `severity`: critical

**Annotations:**
- `summary`: "{{ $value }} GPU panics in 5m"
- `action`: "Trigger GPU recovery or restart worker"

---

## Grafana Dashboard Configuration

### Dashboard: AdapterOS Overview

**Panels:**

1. **Inference Latency (Real-Time)**
   - P50, P95, P99 latency trends (5m window)
   - Query: `histogram_quantile([0.50, 0.95, 0.99], rate(inference_latency_ms_bucket[5m]))`

2. **GPU Memory Pressure Gauge**
   - Current pressure value (0-1 scale)
   - Query: `gpu_memory_pressure`
   - Color coding: green < 0.7, yellow 0.7-0.85, red > 0.85

3. **Hot-Swap Success Rate**
   - Percentage of successful swaps
   - Query: `rate(swap_count_total{status="success"}[5m]) / rate(swap_count_total[5m])`

4. **Adapter Activation Heatmap**
   - X-axis: time, Y-axis: adapter_id, color: activation_%
   - Shows which adapters are hot vs cold

5. **Metal Kernel Execution Times**
   - Per-kernel type (FusedMlp, FusedQkv, etc.)
   - Query: `histogram_quantile([0.50, 0.95], kernel_latency_ms_bucket) by (kernel_type)`

6. **Determinism Violations Timeline**
   - Counter showing any violations (red bar = critical)
   - Query: `increase(determinism_violations_total[5m])`

7. **Router Selection K-Distribution**
   - K=2 vs K=4 vs K=8 request distribution
   - Stacked bar chart

8. **System Resource Usage**
   - CPU%, Memory%, GPU Memory (MB)
   - Combined gauge

---

## Performance Tuning Guidelines

### Inference Latency Optimization

| Symptom | Root Cause | Solution |
|---------|-----------|----------|
| p99 latency > 300ms | Router gate computation expensive | Profile `router_latency_ms`, consider Q15 quantization |
| p99 latency > 500ms | Kernel execution slow | Profile `kernel_latency_ms`, check GPU memory pressure |
| p99 latency increases over time | Memory fragmentation | Monitor `gpu_memory_pool_fragmentation_ratio`, tune idle_timeout |
| p99 latency spikes every N minutes | GC/eviction events | Reduce hot-swap frequency, increase headroom target |

### Memory Pressure Optimization

| Symptom | Root Cause | Solution |
|---------|-----------|----------|
| Pressure > 85% constantly | Too many adapters loaded | Reduce K-sparse size or max_active_adapters |
| Pressure spikes then drops | Poor eviction policy | Review eviction_candidate selection algorithm |
| Pressure never decreases | Pinned adapters blocking eviction | Audit pinned_adapters table, reduce TTL |
| Memory leak (monotonic increase) | Buffer not deallocated | Review GpuMemoryPool cleanup logic |

### Hot-Swap Latency Optimization

| Symptom | Root Cause | Solution |
|---------|-----------|----------|
| Preload > 100ms | Disk I/O slow | Check SSD throughput, enable async preload queue |
| Swap > 50ms | Checkpoint verification expensive | Disable cross-layer hash verification in non-critical regions |
| Rollback > 5% of swaps | GPU fingerprint mismatch | Increase fingerprint sample count, reduce swap concurrency |
| RCU retirement backlog | Retired stacks not cleaned | Lower rcu_check_interval_ms or increase retirement rate |

---

## Runbooks

### Runbook: High Inference Latency

**Alert:** `InferenceLatencyHigh` (p99 > 300ms)

**Diagnosis:**
1. Check router latency
   ```bash
   curl http://localhost:9090/api/v1/query?query=histogram_quantile(0.99, router_latency_ms_bucket)
   ```
   - If > 50ms: Router bottleneck
   - If < 10ms: Kernel bottleneck

2. Check GPU memory pressure
   ```bash
   curl http://localhost:9090/api/v1/query?query=gpu_memory_pressure
   ```
   - If > 0.85: Memory pressure causing slowdown

3. Profile kernel execution
   ```bash
   aosctl metrics get kernel_latency_ms --group-by kernel_type
   ```

**Resolution:**

If **Router Bottleneck:**
- Increase Q15 gate cache TTL
- Profile gate computation (compile with `--profile=release`)
- Consider using approximate nearest neighbor search

If **Kernel Bottleneck:**
- Profile Metal kernel with Instruments.app
  ```bash
  open /Applications/Instruments.app
  # Attach to AOS process, filter by Metal calls
  ```
- Check Metal command queue depth
- Verify no buffer memory issues (run `verify-gpu` endpoint)

If **Memory Pressure:**
- Trigger manual eviction
  ```bash
  aosctl lifecycle evict --count=3
  ```
- Review router K selection (K=4 vs K=8)
- Check for pinned adapters blocking eviction
  ```bash
  aosctl db query "SELECT * FROM pinned_adapters"
  ```

---

### Runbook: High Hot-Swap Latency

**Alert:** `HotSwapLatencyHigh` (p95 > 100ms)

**Diagnosis:**
1. Identify operation bottleneck
   ```bash
   curl "http://localhost:9090/api/v1/query?query=\
     histogram_quantile(0.95, hotswap_latency_ms_bucket) by (operation)"
   ```

2. If `preload > 100ms`: Disk I/O
   ```bash
   iostat -d 1 5  # Check read throughput (MB/s)
   # Expected: > 200 MB/s for NVMe
   ```

3. If `swap > 50ms`: Checkpoint verification
   ```bash
   aosctl metrics get hotswap_latency_ms --filter operation=swap
   ```

4. If `verify > 30ms`: GPU fingerprinting
   ```bash
   aosctl metrics get gpu_fingerprint_sample_time_us
   ```

**Resolution:**

If **Disk I/O Slow:**
- Check disk space (must be < 80% utilized)
  ```bash
  df -h /var/lib/aos/adapters
  ```
- Check for other processes reading disk
  ```bash
  lsof +D /var/lib/aos/adapters
  ```
- Enable read-ahead cache
  ```toml
  [preload]
  read_ahead_size_mb = 64
  ```

If **Swap Latency High:**
- Profile pointer flip and refcount updates
- Check for lock contention in `AdapterTable::swap()`
- Monitor `swap_concurrent_attempts_total`

If **Verify Latency High:**
- Consider sampling fewer buffer locations (2 instead of 3)
- Cache GPU fingerprints instead of recomputing
- Run verification asynchronously post-swap

---

### Runbook: Determinism Violation

**Alert:** `DeterminismViolation` (immediate page)

**Immediate Actions:**
1. Stop all inference (circuit breaker activated)
2. Quarantine affected adapter
   ```bash
   aosctl adapter quarantine --id <adapter_id>
   ```
3. Preserve GPU state for forensics
   ```bash
   aosctl worker debug dump --file /tmp/gpu_state.bin
   ```

**Investigation:**
1. Identify violation type
   ```bash
   curl "http://localhost:9090/api/v1/query?query=\
     increase(determinism_violations_total[5m]) by (violation_type)"
   ```

2. If `hash_mismatch`:
   ```bash
   aosctl db query \
     "SELECT * FROM determinism_log WHERE adapter_id = ? ORDER BY timestamp DESC LIMIT 10"
   ```
   - Compare expected_hash vs actual_hash in event logs

3. If `gpu_buffer_corruption`:
   - Check GPU memory for bit flips
   - Review Metal command queue for errors
   - Check for concurrent buffer access

**Recovery:**
1. Reload adapter from backup
   ```bash
   aosctl adapter reload --id <adapter_id> --from-registry
   ```

2. Run determinism test
   ```bash
   cargo test determinism_tests -- --nocapture
   ```

3. If still failing: escalate to architecture review

**Prevention:**
- Enable buffer fingerprinting on all swaps
- Increase checkpoint history limit (debug mode)
- Monitor GPU thermal throttling
- Schedule GPU memory test during low-traffic windows

---

### Runbook: GPU Memory Pressure Critical

**Alert:** `GPUMemoryPressureCritical` (pressure > 0.85)

**Immediate Actions:**
1. Trigger auto-eviction (should happen automatically)
   ```bash
   aosctl lifecycle check-pressure
   ```

2. Check eviction status
   ```bash
   aosctl metrics get adapter_evictions_total --since=5m
   ```

3. Manual eviction if auto-eviction fails
   ```bash
   aosctl lifecycle evict --count=2 --strategy=lowest_activation_pct
   ```

**Diagnosis:**
1. Identify memory hog adapters
   ```bash
   aosctl db query \
     "SELECT adapter_id, vram_mb FROM adapters WHERE tenant_id = ? \
      ORDER BY vram_mb DESC LIMIT 10"
   ```

2. Check for memory leak
   ```bash
   # Memory increasing monotonically?
   curl "http://localhost:9090/api/v1/query?query=\
     (memory_usage_bytes{component='gpu_pool'} - \
      memory_usage_bytes offset 1h) / \
      memory_usage_bytes offset 1h"
   ```

3. Review buffer pool fragmentation
   ```bash
   aosctl metrics get gpu_memory_pool_fragmentation_ratio
   ```

**Resolution:**

If **Eviction Successful:**
- Monitor pressure for next 10 minutes
- If pressure stays low: resume normal operation
- If pressure increases again: investigate root cause

If **Eviction Fails:**
- Check pinned_adapters table
  ```bash
  aosctl db query "SELECT * FROM pinned_adapters WHERE expires_at IS NULL"
  ```
- Review pinning reasons (should be temporary, not permanent)
- Check for leaked reference counts
  ```bash
  aosctl worker debug refcounts
  ```

If **Chronic High Pressure:**
- Reduce max_active_adapters in config
- Lower K-sparse size (K=4 instead of K=8)
- Increase total GPU memory (hardware upgrade)
- Profile memory usage per adapter type

---

### Runbook: Adapter ID Collision

**Alert:** `AdapterIDCollision` (immediate page)

**Immediate Actions:**
1. Identify colliding adapters
   ```bash
   curl "http://localhost:9090/api/v1/query?query=\
     increase(adapter_id_collisions_total[1h]) > 0"
   ```

2. Rename colliding adapter (u16 collision rate is ultra-low at production scale)
   ```bash
   aosctl adapter rename \
     --old-id <adapter_id_1> \
     --new-id <adapter_id_1>-v2
   ```

3. Verify no further collisions
   ```bash
   aosctl adapter validate-id --id <new_adapter_id>
   ```

**Prevention:**
- This alert should rarely fire (birthday paradox requires ~8000+ adapters before 1% collision rate)
- If firing frequently: migrate to u32 mapping
  ```rust
  // src/adapter_hotswap.rs
  pub fn adapter_id_to_u16(adapter_id: &str) -> u32 {  // Change to u32
      let hash = B3Hash::hash(adapter_id.as_bytes());
      let bytes = hash.to_bytes();
      u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
  }
  ```

---

## Monitoring Dashboard Template

See [monitoring/grafana/dashboards/adapteros-overview.json](monitoring/grafana/dashboards/adapteros-overview.json) for complete dashboard configuration (provisioned with Grafana).

---

## SLO Targets

| Objective | Target | Measurement |
|-----------|--------|-------------|
| **Availability** | 99.9% (43.2 min downtime/month) | Inference request success rate |
| **Inference Latency** | p99 < 300ms | Histogram from metrics collector |
| **GPU Memory Pressure** | < 80% (5-min average) | `gpu_memory_pressure` gauge |
| **Hot-Swap Success Rate** | > 99.5% | swap_count_total{status=success} / total |
| **Determinism Violations** | < 1 per month (critical) | Strict zero-tolerance SLO |
| **Router Latency** | p99 < 50ms | router_latency_ms_bucket |

---

## Common Metrics Queries

### Request Rate (per minute)
```promql
rate(inference_latency_ms_count[1m])
```

### Error Rate by Type
```promql
rate(api_errors_total[5m]) by (error_type)
```

### Memory Leak Detection
```promql
(memory_usage_bytes - memory_usage_bytes offset 1h) / memory_usage_bytes offset 1h > 0.2
```

### Adapter Utilization Ranking
```promql
topk(10, adapter_activation_percentage{time_window="1h"})
```

### GPU Thermal Throttling Frequency
```promql
increase(gpu_thermal_throttle_events_total[1h])
```

---

## Integration with External Systems

### Prometheus Scrape Config (prometheus.yml)
```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: adapteros
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'
```

### AlertManager Routes (alertmanager.yml)
```yaml
global:
  slack_api_url: 'https://hooks.slack.com/...'

route:
  receiver: 'default'
  group_by: ['alertname', 'cluster', 'service']
  routes:
    - match:
        severity: critical
      receiver: 'pagerduty'
      repeat_interval: 5m
    - match:
        severity: warning
      receiver: 'slack'

receivers:
  - name: 'default'
    slack_configs:
      - channel: '#aos-alerts'
  - name: 'pagerduty'
    pagerduty_configs:
      - service_key: 'YOUR_SERVICE_KEY'
```

---

## References

- [docs/ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md)
- [docs/METAL_HOTSWAP_INTEGRATION.md](METAL_HOTSWAP_INTEGRATION.md)
- [crates/adapteros-telemetry/src/metrics.rs](../crates/adapteros-telemetry/src/metrics.rs)
- Prometheus Docs: https://prometheus.io/docs/
- Grafana Docs: https://grafana.com/docs/grafana/latest/

---

**Last Reviewed:** 2025-11-21
**Maintained by:** James KC Auchterlonie
