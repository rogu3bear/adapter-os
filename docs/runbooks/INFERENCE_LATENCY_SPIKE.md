# Runbook: Inference Latency Spike

**Scenario:** High inference latency affecting user experience

**Severity:** SEV-2 (15-minute response time)

**Last Updated:** 2025-12-15

---

## Symptoms

### Alert Indicators
- **Alert:** `InferenceLatencyHigh` (P99 > 300ms for 5+ minutes)
- **Alert:** `InferenceLatencyCritical` (P99 > 1000ms for 2+ minutes)
- **Prometheus Query:** `histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m])) > 500`

### User Reports
- Slow chat responses in UI
- Timeout errors in API calls
- "Still processing..." messages lasting > 10 seconds

### Metrics Dashboard
- P99 latency > 500ms (target: < 300ms)
- P95 latency > 300ms (target: < 150ms)
- Queue depth increasing
- Request success rate declining

---

## Diagnosis Steps

### 1. Verify Latency Spike

```bash
# Check current latency metrics
aosctl metrics show --json | jq '.inference.p99_latency_ms, .inference.p95_latency_ms'

# Get historical trend (last 2 hours)
aosctl metrics history --hours 2 | grep latency

# Alternative: Query Prometheus
curl -s 'http://localhost:9090/api/v1/query?query=histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m]))' | jq .
```

**Expected:** P99 < 300ms, P95 < 150ms
**If higher:** Proceed to next step

### 2. Identify Bottleneck Component

```bash
# Check router latency
curl -s 'http://localhost:9090/api/v1/query?query=histogram_quantile(0.99, router_latency_ms_bucket[5m])' | jq .

# Check kernel execution time
aosctl metrics show --json | jq '.kernel.avg_latency_ms'

# Check worker queue depth
aosctl metrics show --json | jq '.inference.queue_depth'
```

**Decision Tree:**
- **Router latency > 50ms** → Router bottleneck (go to Step 3)
- **Kernel latency > 200ms** → Inference backend issue (go to Step 4)
- **Queue depth > 50** → Capacity issue (go to Step 5)
- **All metrics normal** → Network or database issue (go to Step 6)

### 3. Diagnose Router Bottleneck

```bash
# Check adapter count and K-sparse setting
sqlite3 var/aos-cp.sqlite3 "
SELECT COUNT(*) as total_adapters,
       SUM(CASE WHEN status='Loaded' THEN 1 ELSE 0 END) as loaded_adapters
FROM adapters;"

# Check router configuration
grep -E "k_sparse|max_adapters" configs/cp.toml

# Look for router errors
grep -i "router.*error\|gate.*error" var/logs/backend.log | tail -20
```

**Common Causes:**
- Too many loaded adapters increasing gate computation
- K-sparse value too high (K=8 vs K=3)
- Q15 gate cache misses

### 4. Diagnose Inference Backend Issue

```bash
# Check if stub backend is active (CRITICAL)
grep -i "stub\|fallback" var/logs/backend.log | grep -i "mlx\|metal\|coreml" | tail -10

# Check GPU/ANE utilization (macOS)
sudo powermetrics --samplers gpu_power -n 3 | grep "GPU Active"

# Check memory pressure
aosctl metrics show --json | jq '.memory.pressure_level'

# Look for backend errors
grep -i "backend.*error\|kernel.*panic\|metal.*error" var/logs/backend.log | tail -20
```

**Common Causes:**
- Stub backend active (10-100x slower than real backend)
- High memory pressure causing thrashing
- GPU thermal throttling
- Backend initialization failures

### 5. Diagnose Capacity Issue

```bash
# Check worker process status
ps aux | grep aos-worker

# Check system load
uptime

# Check CPU usage
top -l 1 | head -20  # macOS
top -bn1 | head -20  # Linux

# Check request rate
aosctl metrics show --json | jq '.inference.requests_per_second'
```

**Common Causes:**
- Single worker can't handle request volume
- CPU saturation
- Insufficient system resources

### 6. Diagnose Database/Network Issue

```bash
# Check database response time
time sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*) FROM adapters;"

# Check for database locks
lsof var/aos-cp.sqlite3

# Check network latency to control plane
curl -w "@curl-format.txt" -o /dev/null -s http://localhost:8080/healthz

# Look for database errors
grep -i "database.*error\|sqlite.*lock" var/logs/backend.log | tail -20
```

---

## Resolution

### Quick Fix: Router Bottleneck

**Immediate Action:**
```bash
# Reduce K-sparse to minimal (K=1 for single adapter selection)
# Edit configs/cp.toml
# [router]
# k_sparse = 1  # Temporarily reduce from 3

# Restart control plane
pkill -f adapteros-server
./start up
```

**Root Cause Fix:**
```bash
# Evict cold adapters to reduce gate computation
aosctl lifecycle evict --strategy=lowest_activation_pct --count=5

# Verify improvement
aosctl metrics show --json | jq '.inference.p99_latency_ms'

# Monitor for 15 minutes
watch -n 30 'aosctl metrics show | grep latency'

# If stable, gradually increase K-sparse back to optimal (K=3)
```

### Quick Fix: Inference Backend Issue

**Immediate Action:**
```bash
# If stub backend detected (CRITICAL):
# 1. Check feature flags
cargo tree -p adapteros-lora-worker -f "{p} {f}" | grep -E "mlx|metal|coreml"

# 2. If missing, rebuild with real backend
cargo build --release --features mlx-backend  # macOS only

# 3. Restart worker
pkill -f aos-worker
# Worker will auto-restart via service manager
```

**Memory Pressure Fix:**
```bash
# Force adapter eviction to free memory
aosctl lifecycle evict --count=3

# Check pressure level
aosctl metrics show --json | jq '.memory.pressure_level'

# If still high, restart worker (clears memory leaks)
pkill -f aos-worker
sleep 5
ps aux | grep aos-worker  # Should auto-restart
```

**GPU Throttling (macOS):**
```bash
# Check GPU temperature (if available)
sudo powermetrics --samplers gpu_power -n 1 | grep -i temp

# If throttling: reduce load temporarily
# Unload non-critical adapters
curl -X POST http://localhost:8080/v1/adapters/{adapter_id}/unload
```

### Quick Fix: Capacity Issue

**Immediate Action:**
```bash
# If single-worker deployment and under load:
# Option 1: Rate limit requests (temporary)
# Add to configs/cp.toml:
# [server]
# max_concurrent_requests = 10  # Default is unlimited

# Option 2: Reject non-critical requests
# Enable graceful degradation in application code
```

**Long-term Fix:**
```bash
# Scale horizontally (requires architecture changes)
# 1. Deploy additional worker processes
# 2. Implement load balancing
# 3. Partition tenants across workers

# For immediate relief: Optimize current worker
# - Pre-warm frequently used adapters
# - Enable prefix KV cache
# - Reduce unnecessary logging
```

### Quick Fix: Database Issue

**Immediate Action:**
```bash
# If database locked:
# Check for duplicate server processes
ps aux | grep adapteros-server | grep -v grep

# Kill duplicates
pkill -f adapteros-server
sleep 2
./start up

# If WAL file too large:
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"

# Verify database health
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"
```

---

## Validation

After applying fixes, verify latency has returned to normal:

```bash
# 1. Check immediate metrics (should show improvement)
aosctl metrics show --json | jq '.inference.p99_latency_ms, .inference.p95_latency_ms'

# 2. Monitor for 15 minutes
watch -n 60 'aosctl metrics show | grep -E "latency|queue"'

# 3. Check Prometheus trend
curl -s 'http://localhost:9090/api/v1/query?query=histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m]))' | jq .

# 4. Verify no error spikes
grep ERROR var/logs/backend.log | tail -20

# 5. Test end-to-end latency
time curl -X POST http://localhost:8080/v1/infer \
  -H "Content-Type: application/json" \
  -d '{"prompt": "test", "adapter_id": "test-adapter"}'
```

**Success Criteria:**
- P99 latency < 300ms
- P95 latency < 150ms
- Queue depth < 10
- No errors in logs
- Stable for 15+ minutes

---

## Root Cause Prevention

### Post-Incident Actions

1. **If Router Bottleneck:**
   - Review adapter loading strategy
   - Implement adapter LRU eviction
   - Consider caching gate computations
   - Set max_loaded_adapters per tenant

2. **If Backend Issue:**
   - Add alert for stub backend detection
   - Implement pre-deployment backend verification
   - Monitor GPU utilization continuously
   - Set up memory pressure auto-eviction

3. **If Capacity Issue:**
   - Review capacity planning model
   - Set up horizontal scaling runbook
   - Implement request rate limiting
   - Add queue depth alerting

4. **If Database Issue:**
   - Enable database connection pooling
   - Schedule WAL checkpoint jobs
   - Monitor database query performance
   - Consider read replicas for metrics queries

### Monitoring Improvements

```yaml
# Add to Prometheus alert rules
groups:
  - name: adapteros.latency
    rules:
      # Early warning (before user impact)
      - alert: InferenceLatencyElevated
        expr: histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m])) > 200
        for: 3m
        labels:
          severity: warning
        annotations:
          summary: "P99 latency elevated ({{ $value }}ms) - investigate"
          runbook: "docs/runbooks/INFERENCE_LATENCY_SPIKE.md"

      # User-facing impact
      - alert: InferenceLatencyHigh
        expr: histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m])) > 300
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "P99 latency high ({{ $value }}ms) - user impact"
          action: "Page on-call engineer"

      # Stub backend detection (should never happen in production)
      - alert: StubBackendDetected
        expr: stub_backend_active == 1
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "Stub backend active - 10-100x performance degradation"
          action: "Immediate rebuild with real backend"
```

---

## Escalation

### Escalate to Senior Engineer If:
- Latency remains > 500ms after 30 minutes of troubleshooting
- Multiple components showing issues (suggests systemic problem)
- Quick fixes unsuccessful
- Root cause unclear after diagnosis

### Escalate to Engineering Manager If:
- SEV-1 upgrade (all inference failing, P99 > 2000ms)
- Customer complaints escalating
- Estimated resolution time > 2 hours
- Requires emergency deployment or rollback

### Notify Security Team If:
- Latency spike correlates with unusual traffic patterns
- Potential DoS attack
- Policy enforcement overhead causing slowdown

---

## Notes

**Common Pitfalls:**
- Don't immediately restart everything - diagnose first
- Check for stub backend before deep investigation
- Memory pressure can cause cascading latency issues
- Router latency scales with adapter count (non-linear)

**Known Issues:**
- Q15 gate cache not yet implemented (see backlog)
- Prefix KV cache can cause memory pressure under load
- Metal backend has higher variance than MLX on M-series

**Performance Targets:**
- P50: < 50ms
- P95: < 150ms
- P99: < 300ms
- P99.9: < 1000ms

---

**Owner:** SRE Team
**Last Incident:** [Link to most recent postmortem]
**Related Runbooks:** [MEMORY_PRESSURE.md](MEMORY_PRESSURE.md), [WORKER_CRASH.md](WORKER_CRASH.md)
