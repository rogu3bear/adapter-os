# Alert Playbooks

Step-by-step response procedures for each Prometheus alert in AdapterOS.

**Last Updated:** 2025-11-21
**Scope:** All production alerts
**Related:** [ESCALATION.md](./ESCALATION.md) | [CRITICAL_COMPONENTS_RUNBOOK.md](./CRITICAL_COMPONENTS_RUNBOOK.md)

---

## Table of Contents

1. [Alert Overview](#alert-overview)
2. [Inference Performance Alerts](#inference-performance-alerts)
3. [GPU Memory Alerts](#gpu-memory-alerts)
4. [Metal Kernel Alerts](#metal-kernel-alerts)
5. [Hot-Swap Alerts](#hot-swap-alerts)
6. [Determinism Alerts](#determinism-alerts)
7. [Adapter Lifecycle Alerts](#adapter-lifecycle-alerts)
8. [Database Alerts](#database-alerts)
9. [System Health Alerts](#system-health-alerts)
10. [Security Alerts](#security-alerts)

---

## Alert Overview

### Severity Levels

| Severity | Response Time | Notification | Auto-Remediation |
|----------|---------------|--------------|------------------|
| **Critical** | Immediate (< 5 min) | Page on-call | Attempt automatic recovery |
| **Warning** | 15 minutes | Slack/Email | Log and monitor |
| **Info** | Best effort | Dashboard only | None |

### Alert State Transitions

```
INACTIVE → PENDING (condition true) → FIRING (for > threshold)
    ↑                                       │
    └───────── RESOLVED (condition false) ──┘
```

---

## Inference Performance Alerts

### ALERT: InferenceLatencyHigh

**Alert Definition:**
```yaml
alert: InferenceLatencyHigh
expr: histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m])) > 500
for: 5m
severity: warning
```

**Description:** P99 inference latency exceeds 500ms for 5 minutes.

**Severity:** Warning (escalate to Critical if > 1000ms for > 2 min)

**Impact:** User-facing latency degradation, potential SLA breach.

**Initial Triage Steps:**

1. **Check current latency:**
   ```bash
   curl -s 'http://localhost:9090/api/v1/query?query=histogram_quantile(0.99,rate(inference_latency_ms_bucket[5m]))' | jq '.data.result[0].value[1]'
   ```

2. **Identify slow component:**
   ```bash
   # Check router latency
   curl -s http://localhost:9090/metrics | grep router_latency_ms

   # Check kernel latency
   curl -s http://localhost:9090/metrics | grep kernel_latency_ms

   # Check queue depth
   aosctl status executor | grep queue_depth
   ```

3. **Check for resource pressure:**
   ```bash
   curl -s http://localhost:8080/healthz/system-metrics | jq '.details | {memory_pressure: .gpu_memory_pressure, headroom_pct: .headroom_pct}'
   ```

**Resolution Checklist:**

- [ ] Verify baseline latency from golden metrics
- [ ] Check GPU memory pressure (Section: GPU Memory Alerts)
- [ ] Check adapter swap frequency
- [ ] Review recent deployments/changes
- [ ] If kernel latency high, see [CRITICAL_COMPONENTS_RUNBOOK.md#13-performance-degradation](./CRITICAL_COMPONENTS_RUNBOOK.md#13-performance-degradation)
- [ ] If router latency high, check adapter count and consider pruning

**Escalation Path:**
1. On-call engineer (if > 5 min)
2. Platform team (if > 15 min without resolution)
3. Engineering manager (if > 30 min SEV2)

**Auto-Remediation:** None (requires investigation)

---

### ALERT: InferenceLatencyCritical

**Alert Definition:**
```yaml
alert: InferenceLatencyCritical
expr: histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m])) > 1000
for: 2m
severity: critical
```

**Description:** P99 inference latency exceeds 1 second.

**Severity:** Critical (SEV1)

**Impact:** Service effectively unavailable for production use.

**Initial Triage Steps:**

1. **Page on-call immediately**

2. **Check for systemic issues:**
   ```bash
   # All health checks
   curl -s http://localhost:8080/healthz/all | jq

   # GPU status
   aosctl status gpu

   # Active workers
   aosctl status workers
   ```

3. **Check for queue backup:**
   ```bash
   aosctl status executor --show-pending
   ```

**Resolution Checklist:**

- [ ] Page on-call engineer
- [ ] Check for service-wide outage
- [ ] Implement emergency load shedding if needed:
  ```bash
  aosctl rate-limit set --max-rps 10
  ```
- [ ] Check for stuck inference requests and cancel
- [ ] Consider graceful restart if all else fails
- [ ] Document timeline for incident report

**Escalation Path:** Immediate page to on-call, notify stakeholders.

---

### ALERT: RouterLatencyHigh

**Alert Definition:**
```yaml
alert: RouterLatencyHigh
expr: histogram_quantile(0.99, rate(router_latency_ms_bucket[5m])) > 50
for: 5m
severity: warning
```

**Description:** K-sparse router selection taking > 50ms (should be < 10ms).

**Severity:** Warning

**Impact:** Router becoming a bottleneck in inference pipeline.

**Initial Triage Steps:**

1. **Check adapter count:**
   ```bash
   sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*) FROM adapters WHERE active = 1;"
   ```

2. **Check router gate computation:**
   ```bash
   grep "router.*gate" var/aos-cp.log | tail -20
   ```

3. **Profile router operation:**
   ```bash
   aosctl router profile --duration 10s
   ```

**Resolution Checklist:**

- [ ] If adapter count > 1000, consider archiving unused adapters
- [ ] Check for embedding computation bottleneck
- [ ] Verify Q15 quantization is enabled
- [ ] Check for non-deterministic sorting (should be stable)

**Escalation Path:** Engineering team if no obvious cause.

---

## GPU Memory Alerts

### ALERT: GPUMemoryPressureWarning

**Alert Definition:**
```yaml
alert: GPUMemoryPressureWarning
expr: gpu_memory_pressure > 0.75
for: 5m
severity: warning
```

**Description:** GPU memory utilization above 75%.

**Severity:** Warning

**Impact:** Approaching eviction threshold, potential performance degradation.

**Initial Triage Steps:**

1. **Check current pressure:**
   ```bash
   curl -s http://localhost:8080/healthz/system-metrics | jq '.details | {pressure: .gpu_memory_pressure, available_mb: .memory_available_mb}'
   ```

2. **List hot adapters:**
   ```bash
   aosctl adapter list --state hot --json | jq '.[] | {adapter_id, vram_mb: (.vram_bytes / 1048576)}'
   ```

3. **Check eviction queue:**
   ```bash
   aosctl lifecycle eviction-candidates | head -10
   ```

**Resolution Checklist:**

- [ ] Verify eviction is working (adapters being demoted)
- [ ] Check for memory leak indicators
- [ ] Review adapter loading patterns
- [ ] Consider preemptive eviction of low-activation adapters

**Escalation Path:** Monitor closely, escalate if pressure increases.

---

### ALERT: GPUMemoryPressureCritical

**Alert Definition:**
```yaml
alert: GPUMemoryPressureCritical
expr: gpu_memory_pressure > 0.85
for: 2m
severity: critical
```

**Description:** GPU memory utilization above 85%, system at risk.

**Severity:** Critical (SEV2)

**Impact:** New adapter loads will fail, inference may fail with OOM.

**Initial Triage Steps:**

1. **Trigger immediate eviction:**
   ```bash
   aosctl maintenance gc --force --aggressive
   ```

2. **Block new adapter loads:**
   ```bash
   aosctl lifecycle pause-loading
   ```

3. **Unload non-essential adapters:**
   ```bash
   # Get non-pinned, low-activation adapters
   aosctl adapter list --json | jq '[.[] | select(.pinned == false and .activation_percentage < 5)] | sort_by(.activation_percentage) | .[:5]'

   # Unload them
   aosctl adapter unload <adapter-id>
   ```

**Resolution Checklist:**

- [ ] Immediate eviction of low-priority adapters
- [ ] Check for memory leak (see [CRITICAL_COMPONENTS_RUNBOOK.md#24](./CRITICAL_COMPONENTS_RUNBOOK.md#24-memory-leak-detection-hot-swap-related))
- [ ] Review adapter pinning strategy
- [ ] Consider emergency restart if pressure persists
- [ ] Document cause for post-mortem

**Escalation Path:** Immediate notification, page if > 5 min.

**Auto-Remediation:**
```bash
# Automated response (if enabled)
if [ $(curl -s http://localhost:9090/metrics | grep gpu_memory_pressure | awk '{print $2}') > 0.85 ]; then
  aosctl maintenance gc --force
  aosctl adapter unload $(aosctl adapter list --json | jq -r '[.[] | select(.pinned == false)] | sort_by(.activation_percentage) | .[0].adapter_id')
fi
```

---

### ALERT: GPUMemoryPoolFragmentation

**Alert Definition:**
```yaml
alert: GPUMemoryPoolFragmentation
expr: gpu_memory_pool_reuse_ratio < 0.5
for: 10m
severity: warning
```

**Description:** Memory pool reuse ratio below 50%, indicating fragmentation.

**Severity:** Warning

**Impact:** Increased allocation overhead, eventual memory exhaustion.

**Initial Triage Steps:**

1. **Check pool stats:**
   ```bash
   curl -s http://localhost:9090/metrics | grep gpu_memory_pool
   ```

2. **Check allocation patterns:**
   ```bash
   grep "allocate buffer" var/aos-cp.log | tail -50 | awk '{print $NF}' | sort | uniq -c
   ```

**Resolution Checklist:**

- [ ] Schedule maintenance window for pool reset (requires restart)
- [ ] Review buffer size distribution
- [ ] Consider enabling compaction if available

**Escalation Path:** Schedule maintenance if persistent.

---

## Metal Kernel Alerts

### ALERT: MetalKernelPanic

**Alert Definition:**
```yaml
alert: MetalKernelPanic
expr: increase(metal_kernel_panic_count_total[5m]) > 0
severity: critical
```

**Description:** Metal kernel panic detected.

**Severity:** Critical (SEV2)

**Impact:** GPU operations failing, potential service instability.

**Initial Triage Steps:**

1. **Get panic details:**
   ```bash
   grep "kernel panic\|Metal panic" var/aos-cp.log | tail -10
   ```

2. **Check GPU device status:**
   ```bash
   system_profiler SPDisplaysDataType | grep -A 10 "Chipset"
   ```

3. **Check which kernel panicked:**
   ```bash
   curl -s http://localhost:9090/metrics | grep "metal_kernel_panic_count_total{kernel_type"
   ```

**Resolution Checklist:**

- [ ] Identify affected kernel and adapter
- [ ] Check for GPU hardware issues (thermal, driver)
- [ ] Attempt GPU device recovery
- [ ] Restart service if recovery fails
- [ ] See [CRITICAL_COMPONENTS_RUNBOOK.md#12](./CRITICAL_COMPONENTS_RUNBOOK.md#12-kernel-compilation-failures)

**Escalation Path:** Immediate notification, page if recurring.

---

### ALERT: GPUDeviceRecoveryFailed

**Alert Definition:**
```yaml
alert: GPUDeviceRecoveryFailed
expr: increase(gpu_device_recovery_count_total{status="failed"}[5m]) > 0
severity: critical
```

**Description:** GPU device recovery attempt failed.

**Severity:** Critical (SEV1)

**Impact:** GPU potentially in bad state, service at risk.

**Initial Triage Steps:**

1. **Check device status:**
   ```bash
   system_profiler SPDisplaysDataType
   dmesg | grep -i gpu | tail -20
   ```

2. **Check for thermal issues:**
   ```bash
   sudo powermetrics --samplers smc -n 1 | grep -E "temp|GPU"
   ```

**Resolution Checklist:**

- [ ] Attempt service restart
- [ ] If restart fails, check for hardware issues
- [ ] May require system restart in severe cases
- [ ] Document for hardware review

**Escalation Path:** Immediate page, potential hardware incident.

---

### ALERT: KernelLatencyRegression

**Alert Definition:**
```yaml
alert: KernelLatencyRegression
expr: histogram_quantile(0.95, rate(metal_kernel_execution_us_bucket[5m])) > 2 * histogram_quantile(0.95, rate(metal_kernel_execution_us_bucket[1h] offset 1h))
for: 5m
severity: warning
```

**Description:** Kernel latency increased > 2x compared to baseline.

**Severity:** Warning

**Impact:** Performance degradation for affected kernel operations.

**Initial Triage Steps:**

1. **Identify affected kernel:**
   ```bash
   curl -s 'http://localhost:9090/api/v1/query?query=histogram_quantile(0.95,rate(metal_kernel_execution_us_bucket[5m]))' | jq '.data.result[] | {kernel: .metric.kernel_type, latency: .value[1]}'
   ```

2. **Check for thermal throttling:**
   ```bash
   sudo powermetrics --samplers smc -n 3 --interval 1000 | grep -E "temp|throttle"
   ```

**Resolution Checklist:**

- [ ] Check for increased input sizes
- [ ] Check for concurrent GPU operations
- [ ] Review recent adapter changes
- [ ] See [CRITICAL_COMPONENTS_RUNBOOK.md#13](./CRITICAL_COMPONENTS_RUNBOOK.md#13-performance-degradation)

**Escalation Path:** Engineering if no obvious cause.

---

## Hot-Swap Alerts

### ALERT: HotSwapLatencyHigh

**Alert Definition:**
```yaml
alert: HotSwapLatencyHigh
expr: histogram_quantile(0.99, rate(hotswap_latency_ms_bucket[5m])) > 10000
for: 5m
severity: warning
```

**Description:** Hot-swap operations taking > 10 seconds.

**Severity:** Warning

**Impact:** Slow adapter transitions, potential request latency spikes.

**Initial Triage Steps:**

1. **Check swap operation details:**
   ```bash
   grep "swap\|preload" var/aos-cp.log | tail -30
   ```

2. **Check for concurrent swaps:**
   ```bash
   aosctl status hotswap
   ```

**Resolution Checklist:**

- [ ] Check for large adapter files (I/O bound)
- [ ] Check GPU memory availability
- [ ] Consider reducing swap batch size
- [ ] See [CRITICAL_COMPONENTS_RUNBOOK.md#21](./CRITICAL_COMPONENTS_RUNBOOK.md#21-swap-timeout-handling)

**Escalation Path:** Engineering if persistent.

---

### ALERT: SwapRollbacksIncreasing

**Alert Definition:**
```yaml
alert: SwapRollbacksIncreasing
expr: increase(swap_rollback_count_total[15m]) > 3
severity: warning
```

**Description:** Multiple swap rollbacks in 15 minutes.

**Severity:** Warning (escalate to Critical if > 10)

**Impact:** Adapters failing to load, potential service degradation.

**Initial Triage Steps:**

1. **Check rollback reasons:**
   ```bash
   curl -s http://localhost:9090/metrics | grep "swap_rollback_count_total{reason"
   ```

2. **Get recent rollback events:**
   ```bash
   grep "rollback" var/aos-cp.log | tail -20
   ```

**Resolution Checklist:**

- [ ] Identify failing adapter(s)
- [ ] Check adapter integrity (hash verification)
- [ ] Check for memory pressure during swap
- [ ] Consider quarantining problematic adapter

**Escalation Path:** Engineering if specific adapter keeps failing.

---

### ALERT: AdapterSwapFailures

**Alert Definition:**
```yaml
alert: AdapterSwapFailures
expr: rate(adapter_swap_count_total{status="failed"}[5m]) > 0.1
severity: warning
```

**Description:** Adapter swap failure rate above threshold.

**Severity:** Warning

**Impact:** Adapters not loading as expected.

**Initial Triage Steps:**

1. **Check failure count:**
   ```bash
   curl -s http://localhost:9090/metrics | grep 'adapter_swap_count_total{status="failed"}'
   ```

2. **Get failure details:**
   ```bash
   grep "swap.*failed\|swap.*error" var/aos-cp.log | tail -20
   ```

**Resolution Checklist:**

- [ ] Identify which adapters are failing
- [ ] Check adapter file integrity
- [ ] Check GPU memory availability
- [ ] Review error messages for specific cause

**Escalation Path:** Engineering team.

---

## Determinism Alerts

### ALERT: DeterminismViolation

**Alert Definition:**
```yaml
alert: DeterminismViolation
expr: increase(determinism_violations_total[5m]) > 0
severity: critical
```

**Description:** Determinism violation detected - outputs not reproducible.

**Severity:** Critical (SEV2 - compliance issue)

**Impact:** Cannot guarantee reproducible results, audit trail compromised.

**Initial Triage Steps:**

1. **Get violation details:**
   ```bash
   curl -s http://localhost:9090/metrics | grep "determinism_violations_total{" | head -10
   ```

2. **Check telemetry for details:**
   ```bash
   sqlite3 var/aos-cp.sqlite3 "
     SELECT * FROM telemetry_events
     WHERE event_type = 'determinism.violation'
     ORDER BY created_at DESC LIMIT 5;
   "
   ```

3. **Identify affected adapter:**
   ```bash
   curl -s http://localhost:9090/metrics | grep 'determinism_violations_total{.*adapter_id' | head -5
   ```

**Resolution Checklist:**

- [ ] Document violation type and affected adapter
- [ ] Quarantine affected adapter if critical
- [ ] Check for non-deterministic code paths
- [ ] See [CRITICAL_COMPONENTS_RUNBOOK.md#31](./CRITICAL_COMPONENTS_RUNBOOK.md#31-non-reproducible-results-debugging)
- [ ] Run replay verification
- [ ] Schedule engineering review

**Escalation Path:** Immediate notification to compliance team.

---

### ALERT: GPUBufferIntegrityViolation

**Alert Definition:**
```yaml
alert: GPUBufferIntegrityViolation
expr: increase(gpu_buffer_integrity_violations_total[5m]) > 0
severity: critical
```

**Description:** GPU buffer fingerprint mismatch - potential corruption.

**Severity:** Critical (SEV2)

**Impact:** Adapter data integrity compromised.

**Initial Triage Steps:**

1. **Identify affected adapter:**
   ```bash
   curl -s http://localhost:9090/metrics | grep "gpu_buffer_integrity_violations_total{adapter_id"
   ```

2. **Check for hardware issues:**
   ```bash
   dmesg | grep -i "gpu\|metal\|error" | tail -20
   ```

**Resolution Checklist:**

- [ ] Unload affected adapter immediately
- [ ] Check for thermal issues
- [ ] Reload from source
- [ ] Run GPU memory diagnostic
- [ ] See [CRITICAL_COMPONENTS_RUNBOOK.md#42](./CRITICAL_COMPONENTS_RUNBOOK.md#42-integrity-verification-failures)

**Escalation Path:** Immediate notification, potential hardware review.

---

### ALERT: AdapterIDCollision

**Alert Definition:**
```yaml
alert: AdapterIDCollision
expr: increase(adapter_id_collisions_total[1h]) > 0
severity: warning
```

**Description:** BLAKE3 hash collision in adapter ID mapping.

**Severity:** Warning

**Impact:** Potential routing errors between colliding adapters.

**Initial Triage Steps:**

1. **Get collision details:**
   ```bash
   curl -s http://localhost:9090/metrics | grep adapter_id_collisions_total
   ```

2. **Check adapter count:**
   ```bash
   sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*) FROM adapters;"
   ```

**Resolution Checklist:**

- [ ] Identify colliding adapter pair
- [ ] Rename one adapter to change hash
- [ ] See [CRITICAL_COMPONENTS_RUNBOOK.md#34](./CRITICAL_COMPONENTS_RUNBOOK.md#34-seed-collision-investigation)

**Escalation Path:** Engineering if adapter count approaching limit.

---

## Adapter Lifecycle Alerts

### ALERT: AdapterEvictionThrashing

**Alert Definition:**
```yaml
alert: AdapterEvictionThrashing
expr: increase(adapter_evictions_total[5m]) > 10
severity: warning
```

**Description:** Frequent adapter evictions indicating memory pressure or poor tiering.

**Severity:** Warning

**Impact:** Increased latency from frequent adapter loading.

**Initial Triage Steps:**

1. **Check eviction pattern:**
   ```bash
   grep "Evicting adapter" var/aos-cp.log | tail -30
   ```

2. **Check memory pressure:**
   ```bash
   curl -s http://localhost:8080/healthz/system-metrics | jq '.details.gpu_memory_pressure'
   ```

**Resolution Checklist:**

- [ ] Review adapter tiering strategy
- [ ] Pin frequently-used adapters
- [ ] Check for routing patterns causing thrashing
- [ ] See [MEMORY-PRESSURE.md](./MEMORY-PRESSURE.md)

**Escalation Path:** Engineering for capacity review.

---

### ALERT: AdaptersStuckLoading

**Alert Definition:**
```yaml
alert: AdaptersStuckLoading
expr: count(adapter_state_transitions{to_state="loading"}) - count(adapter_state_transitions{from_state="loading"}) > 5
for: 5m
severity: warning
```

**Description:** Multiple adapters stuck in loading state.

**Severity:** Warning (escalate to Critical if > 10)

**Impact:** Adapters unavailable for inference.

**Initial Triage Steps:**

1. **List stuck adapters:**
   ```bash
   aosctl adapter list --state loading --json | jq '.[] | {adapter_id, loading_since}'
   ```

2. **Check for blocking operations:**
   ```bash
   aosctl status workers --show-blocked
   ```

**Resolution Checklist:**

- [ ] Check GPU memory availability
- [ ] Check for deadlocks
- [ ] Force-cancel stuck loads if safe
- [ ] See [CRITICAL_COMPONENTS_RUNBOOK.md#22](./CRITICAL_COMPONENTS_RUNBOOK.md#22-quarantined-adapter-recovery)

**Escalation Path:** Engineering if persists.

---

### ALERT: AdapterQuarantined

**Alert Definition:**
```yaml
alert: AdapterQuarantined
expr: increase(adapter_state_transitions_total{to_state="quarantined"}[15m]) > 0
severity: warning
```

**Description:** Adapter has been quarantined due to violation.

**Severity:** Warning

**Impact:** Adapter unavailable until manually released.

**Initial Triage Steps:**

1. **List quarantined adapters:**
   ```bash
   aosctl adapter list --state quarantined
   ```

2. **Get quarantine reason:**
   ```bash
   sqlite3 var/aos-cp.sqlite3 "
     SELECT adapter_id, quarantine_reason, quarantined_at
     FROM adapters WHERE current_state = 'quarantined';
   "
   ```

**Resolution Checklist:**

- [ ] Review quarantine reason
- [ ] Fix underlying issue
- [ ] Release from quarantine or re-register
- [ ] See [CRITICAL_COMPONENTS_RUNBOOK.md#22](./CRITICAL_COMPONENTS_RUNBOOK.md#22-quarantined-adapter-recovery)

**Escalation Path:** Owner of affected adapter.

---

## Database Alerts

### ALERT: DatabaseConnectionPoolExhausted

**Alert Definition:**
```yaml
alert: DatabaseConnectionPoolExhausted
expr: database_pool_available_connections < 2
for: 2m
severity: critical
```

**Description:** Database connection pool nearly exhausted.

**Severity:** Critical (SEV2)

**Impact:** New operations will fail to get database connections.

**Initial Triage Steps:**

1. **Check pool status:**
   ```bash
   curl -s http://localhost:8080/healthz/db | jq
   ```

2. **Check for long-running queries:**
   ```bash
   sqlite3 var/aos-cp.sqlite3 ".stats on" "SELECT 1;"
   ```

**Resolution Checklist:**

- [ ] Identify long-running operations
- [ ] Check for query bottlenecks
- [ ] Consider increasing pool size
- [ ] Check for connection leaks
- [ ] See [DATABASE-FAILURES.md](./DATABASE-FAILURES.md)

**Escalation Path:** Immediate notification.

---

### ALERT: DatabaseQueryLatencyHigh

**Alert Definition:**
```yaml
alert: DatabaseQueryLatencyHigh
expr: histogram_quantile(0.99, rate(database_query_latency_ms_bucket[5m])) > 100
for: 5m
severity: warning
```

**Description:** Database query latency exceeding 100ms.

**Severity:** Warning

**Impact:** All operations depending on database slowed.

**Initial Triage Steps:**

1. **Check specific query types:**
   ```bash
   grep "query took" var/aos-cp.log | grep -E "[0-9]{3}ms" | tail -20
   ```

2. **Check database health:**
   ```bash
   sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"
   ```

**Resolution Checklist:**

- [ ] Run WAL checkpoint
  ```bash
  sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"
  ```
- [ ] Check for missing indexes
- [ ] Review query patterns
- [ ] See [DATABASE-OPTIMIZATION.md](./DATABASE-OPTIMIZATION.md)

**Escalation Path:** Engineering for optimization.

---

## System Health Alerts

### ALERT: HealthCheckFailing

**Alert Definition:**
```yaml
alert: HealthCheckFailing
expr: up{job="adapteros"} == 0
for: 1m
severity: critical
```

**Description:** AdapterOS health check endpoint not responding.

**Severity:** Critical (SEV1)

**Impact:** Service potentially down.

**Initial Triage Steps:**

1. **Check process status:**
   ```bash
   ps aux | grep aos-cp
   pgrep -f aos-cp
   ```

2. **Check for crash logs:**
   ```bash
   tail -100 var/aos-cp.log | grep -E "panic|error|crash"
   ```

3. **Check port binding:**
   ```bash
   lsof -i :8080
   ```

**Resolution Checklist:**

- [ ] Attempt service restart
- [ ] Check for resource exhaustion
- [ ] Check logs for crash cause
- [ ] See [STARTUP-FAILURES.md](./STARTUP-FAILURES.md)

**Escalation Path:** Immediate page.

---

### ALERT: SystemMemoryPressure

**Alert Definition:**
```yaml
alert: SystemMemoryPressure
expr: (1 - (node_memory_MemAvailable_bytes / node_memory_MemTotal_bytes)) > 0.90
for: 5m
severity: warning
```

**Description:** System memory usage above 90%.

**Severity:** Warning

**Impact:** Risk of OOM killer, service instability.

**Initial Triage Steps:**

1. **Check memory status:**
   ```bash
   vm_stat | head -15
   top -l 1 -n 10 -o mem
   ```

2. **Check aos-cp memory:**
   ```bash
   ps aux | grep aos-cp | awk '{print $4, $6}'
   ```

**Resolution Checklist:**

- [ ] Run garbage collection
- [ ] Unload unused adapters
- [ ] Check for memory leaks
- [ ] Consider service restart
- [ ] See [MEMORY-PRESSURE.md](./MEMORY-PRESSURE.md)

**Escalation Path:** Operations team.

---

### ALERT: DiskSpaceLow

**Alert Definition:**
```yaml
alert: DiskSpaceLow
expr: (node_filesystem_avail_bytes{mountpoint="/"} / node_filesystem_size_bytes{mountpoint="/"}) < 0.10
for: 10m
severity: warning
```

**Description:** Disk space below 10%.

**Severity:** Warning (escalate to Critical at 5%)

**Impact:** Service may fail to write logs, checkpoints, telemetry.

**Initial Triage Steps:**

1. **Check disk usage:**
   ```bash
   df -h
   du -sh var/*
   ```

2. **Identify large files:**
   ```bash
   find var/ -type f -size +100M
   ```

**Resolution Checklist:**

- [ ] Clean old telemetry bundles
- [ ] Rotate logs
- [ ] Remove old checkpoints
- [ ] See [CLEANUP-PROCEDURES.md](./CLEANUP-PROCEDURES.md)

**Escalation Path:** Operations team.

---

## Security Alerts

### ALERT: UnauthorizedAccessAttempt

**Alert Definition:**
```yaml
alert: UnauthorizedAccessAttempt
expr: increase(auth_failures_total[5m]) > 10
severity: warning
```

**Description:** Multiple authentication failures.

**Severity:** Warning (escalate to Critical if > 50)

**Impact:** Potential security threat.

**Initial Triage Steps:**

1. **Check auth failure logs:**
   ```bash
   grep "auth.*failed\|unauthorized" var/aos-cp.log | tail -30
   ```

2. **Identify source:**
   ```bash
   grep "auth.*failed" var/aos-cp.log | awk '{print $NF}' | sort | uniq -c | sort -rn | head -10
   ```

**Resolution Checklist:**

- [ ] Identify source IP/user
- [ ] Check for credential stuffing
- [ ] Consider temporary IP block
- [ ] Review access logs for pattern
- [ ] Notify security team

**Escalation Path:** Security team if > 50 attempts.

---

### ALERT: PolicyViolationDetected

**Alert Definition:**
```yaml
alert: PolicyViolationDetected
expr: increase(policy_violations_total{severity="blocker"}[5m]) > 0
severity: critical
```

**Description:** Blocker-level policy violation detected.

**Severity:** Critical (SEV2)

**Impact:** Operation blocked due to policy enforcement.

**Initial Triage Steps:**

1. **Get violation details:**
   ```bash
   sqlite3 var/aos-cp.sqlite3 "
     SELECT * FROM telemetry_events
     WHERE event_type LIKE 'policy.violation%'
     ORDER BY created_at DESC LIMIT 5;
   "
   ```

2. **Identify affected operation:**
   ```bash
   grep "policy.*violation\|PolicyViolation" var/aos-cp.log | tail -10
   ```

**Resolution Checklist:**

- [ ] Identify violated policy
- [ ] Review operation that triggered violation
- [ ] Fix configuration or code
- [ ] Document for compliance

**Escalation Path:** Compliance team, engineering owner.

---

### ALERT: EgressViolation

**Alert Definition:**
```yaml
alert: EgressViolation
expr: increase(egress_violations_total[5m]) > 0
severity: critical
```

**Description:** Network egress attempted in production mode.

**Severity:** Critical (SEV1)

**Impact:** Security boundary potentially breached.

**Initial Triage Steps:**

1. **Get violation details:**
   ```bash
   grep "egress\|network" var/aos-cp.log | tail -20
   ```

2. **Check configuration:**
   ```bash
   aosctl config get --key server.production_mode
   aosctl config get --key server.uds_socket
   ```

**Resolution Checklist:**

- [ ] Immediately quarantine affected component
- [ ] Identify source of egress attempt
- [ ] Review network configuration
- [ ] Notify security team
- [ ] Document for incident report

**Escalation Path:** Immediate security team notification.

---

## Quick Reference

### Alert Response Priority

| Priority | Response | Examples |
|----------|----------|----------|
| **P1** | Page immediately | HealthCheckFailing, EgressViolation |
| **P2** | Respond within 15 min | GPUMemoryPressureCritical, DeterminismViolation |
| **P3** | Respond within 1 hour | InferenceLatencyHigh, SwapRollbacksIncreasing |
| **P4** | Next business day | GPUMemoryPoolFragmentation |

### Common Resolution Paths

| Symptom | First Check | Quick Fix | Runbook |
|---------|-------------|-----------|---------|
| High latency | GPU memory | `aosctl maintenance gc` | [InferenceLatencyHigh](#alert-inferencelatencyhigh) |
| Swap failures | Adapter integrity | `aosctl adapter verify-hash` | [SwapRollbacksIncreasing](#alert-swaprollbacksincreasing) |
| Memory pressure | Loaded adapters | `aosctl adapter unload` | [GPUMemoryPressureCritical](#alert-gpumemorypressurecritical) |
| Determinism | Violation type | Quarantine adapter | [DeterminismViolation](#alert-determinismviolation) |

### Silencing Alerts

```bash
# Silence specific alert during maintenance
amtool silence add alertname=GPUMemoryPressureWarning --duration=2h --comment="Scheduled maintenance"

# List active silences
amtool silence list

# Expire silence early
amtool silence expire <silence-id>
```

---

**Maintained by:** Operations Team
**Copyright:** 2025 JKCA / James KC Auchterlonie. All rights reserved.
