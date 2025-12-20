# Runbook: Memory Pressure

**Scenario:** High memory usage threatening system stability

**Severity:** SEV-2 (15-minute response time, escalates to SEV-1 if critical)

**Last Updated:** 2025-12-15

---

## Symptoms

### Alert Indicators
- **Alert:** `MemoryPressureHigh` (pressure level > 0.75 for 5+ minutes)
- **Alert:** `MemoryPressureCritical` (pressure level > 0.85 for 2+ minutes)
- **Alert:** `MemoryHeadroomLow` (free memory < 15% for 5+ minutes)
- **Prometheus Query:** `memory_pressure_level > 0.85`

### User Reports
- Adapter loading failures ("Out of memory" errors)
- Slow inference responses
- "Memory limit exceeded" errors in UI
- Training jobs failing to start

### System Indicators
- Memory usage > 85% (target: < 80%)
- Headroom < 15% (target: > 15%)
- Frequent adapter evictions
- Swap usage increasing (if swap enabled)
- OOM warnings in system logs

---

## Diagnosis Steps

### 1. Verify Memory Pressure Level

```bash
# Check current memory metrics
aosctl metrics show --json | jq '{
  memory_used_pct: .memory.used_percent,
  pressure_level: .memory.pressure_level,
  headroom_pct: .memory.headroom_pct
}'

# Check system memory (macOS)
vm_stat | perl -ne '/page size of (\d+)/ and $size=$1; /Pages\s+([^:]+)[^\d]+(\d+)/ and printf("%-16s % 16.2f Mi\n", "$1:", $2 * $size / 1048576);'

# Check system memory (Linux)
free -h

# Get memory trend (last 2 hours)
aosctl metrics history --hours 2 | grep memory
```

**Thresholds:**
- **Low:** < 0.60 (60% usage) - Normal
- **Medium:** 0.60-0.75 - Monitor closely
- **High:** 0.75-0.85 - Action needed
- **Critical:** > 0.85 - Immediate action

**If Critical (> 0.85):** Escalate to SEV-1, proceed immediately to Quick Fix
**If High (0.75-0.85):** Continue diagnosis

### 2. Identify Memory Consumers

```bash
# Check adapter memory usage
sqlite3 var/aos-cp.sqlite3 "
SELECT adapter_id, name, status, vram_mb,
       CASE WHEN status='Loaded' THEN vram_mb ELSE 0 END as active_mem_mb
FROM adapters
ORDER BY active_mem_mb DESC
LIMIT 20;"

# Sum total loaded adapter memory
sqlite3 var/aos-cp.sqlite3 "
SELECT
  COUNT(*) as loaded_count,
  SUM(vram_mb) as total_adapter_mb,
  AVG(vram_mb) as avg_adapter_mb
FROM adapters
WHERE status='Loaded';"

# Check base model memory
du -sh var/model-cache/*/
ls -lh var/model-cache/*/model.safetensors

# Check process memory (macOS)
ps aux | grep -E 'aos-worker|adapteros-server' | awk '{print $3, $4, $11}'

# Check process memory (Linux)
ps aux | grep -E 'aos-worker|adapteros-server' | awk '{print $3, $4, $11}'
```

**Memory Breakdown:**
```
Total = Base Model + Loaded Adapters + Prefix Cache + System Overhead
Example:
  - Base Model (Qwen 2.5 7B int4): 5 GB
  - Adapters (10 × 64MB):          640 MB
  - Prefix KV Cache:                512 MB
  - System Overhead:                2 GB
  - Total:                          ≈8.2 GB
```

### 3. Check Eviction Status

```bash
# Check recent evictions
aosctl metrics show --json | jq '.adapters.eviction_count'

# Get eviction rate (last 5 minutes)
sqlite3 var/aos-cp.sqlite3 "
SELECT COUNT(*) as recent_evictions
FROM telemetry_events
WHERE event_type='adapter.evicted'
  AND created_at > datetime('now', '-5 minutes');"

# Check failed evictions
grep -i "eviction.*fail\|eviction.*error" var/aos-cp.log | tail -20

# Check pinned adapters (cannot be evicted)
sqlite3 var/aos-cp.sqlite3 "
SELECT adapter_id, pinned_reason, pinned_until
FROM pinned_adapters
WHERE pinned_until > datetime('now')
OR pinned_until IS NULL;"
```

**If eviction rate high:** Auto-eviction working (proceed to Step 4)
**If evictions failing:** Pinned adapters or eviction policy issue (proceed to Step 5)

### 4. Check for Memory Leaks

```bash
# Monitor memory growth over time
# Run this in a loop and observe trend
while true; do
  echo "$(date): $(ps aux | grep aos-worker | awk '{print $4}')"
  sleep 60
done

# Check for leaked adapters in memory but not in DB
# Compare process memory to expected usage
EXPECTED_MB=$(sqlite3 var/aos-cp.sqlite3 "SELECT SUM(vram_mb) FROM adapters WHERE status='Loaded';")
ACTUAL_MB=$(ps aux | grep aos-worker | awk '{print int($6/1024)}')
echo "Expected: ${EXPECTED_MB}MB, Actual: ${ACTUAL_MB}MB"

# Check for prefix cache growth
grep -i "prefix.*cache" var/aos-worker.log | tail -20

# Look for memory leak warnings
grep -i "leak\|reference.*count\|refcount" var/aos-worker.log | tail -20
```

**Signs of Memory Leak:**
- Memory grows continuously over time (> 1% per hour)
- Actual memory >> expected memory (> 20% difference)
- Worker restart clears memory but it grows back

### 5. Check System-Wide Memory Usage

```bash
# Check for other processes consuming memory
top -o MEM -l 1 | head -20  # macOS
top -o %MEM -bn1 | head -20  # Linux

# Check for swap activity (bad sign)
vm_stat | grep -E "Pageouts|Swapouts"  # macOS
vmstat 1 5  # Linux

# Check for memory-intensive user processes
ps aux | sort -k4 -r | head -20

# Check disk cache usage (can be reclaimed)
vm_stat | grep "File-backed pages"  # macOS
free -h  # Linux shows cache/buffers
```

---

## Resolution

### Quick Fix: Force Adapter Eviction (Critical Pressure > 0.85)

**Immediate Action:**
```bash
# 1. Trigger manual eviction (evict 5 cold adapters)
curl -X POST http://localhost:8080/api/v1/lifecycle/evict \
  -H "Content-Type: application/json" \
  -d '{
    "strategy": "lowest_activation_pct",
    "count": 5
  }'

# Or via CLI (if available)
aosctl lifecycle evict --strategy=lowest_activation_pct --count=5

# 2. Verify eviction succeeded
aosctl metrics show --json | jq '.memory.pressure_level, .adapters.loaded_count'

# 3. Check logs for eviction confirmation
grep "adapter.*evicted" var/aos-cp.log | tail -10

# 4. If still critical, evict more aggressively
curl -X POST http://localhost:8080/api/v1/lifecycle/evict \
  -H "Content-Type: application/json" \
  -d '{
    "strategy": "lowest_activation_pct",
    "count": 10
  }'
```

**Verify Pressure Reduced:**
```bash
# Should show pressure < 0.75 within 30 seconds
watch -n 5 'aosctl metrics show | grep -E "pressure|headroom"'
```

### Root Cause Fix: Adjust Memory Limits

**Reduce Concurrent Adapters:**
```bash
# Edit configs/cp.toml
# [memory]
# max_adapters_per_tenant = 5  # Reduce from 10
# min_headroom_pct = 20         # Increase from 15

# Restart control plane to apply
pkill -f adapteros-server
make dev

# Verify new limits
grep -A5 "\[memory\]" configs/cp.toml
```

**Enable Aggressive Auto-Eviction:**
```bash
# Edit configs/cp.toml
# [memory]
# enable_auto_eviction = true
# eviction_threshold_pct = 75    # Lower from 85 (evict sooner)
# eviction_batch_size = 3         # Evict more per cycle

# Restart to apply
pkill -f adapteros-server
make dev
```

### Root Cause Fix: Handle Pinned Adapters

**If Eviction Blocked by Pins:**
```bash
# 1. List pinned adapters
sqlite3 var/aos-cp.sqlite3 "
SELECT adapter_id, pinned_reason, pinned_at, pinned_until
FROM pinned_adapters
WHERE pinned_until > datetime('now')
   OR pinned_until IS NULL
ORDER BY pinned_at;"

# 2. Review pin reasons
# Valid reasons:
# - "active_inference" (temporary, auto-expires)
# - "training_in_progress" (temporary, auto-expires)
# - "critical_service" (permanent, requires review)

# 3. Force unpin if necessary (CAUTION)
# Only for non-critical pins during emergency
sqlite3 var/aos-cp.sqlite3 "
DELETE FROM pinned_adapters
WHERE pinned_reason NOT IN ('active_inference', 'training_in_progress')
  AND pinned_until IS NULL;"

# 4. Re-attempt eviction
aosctl lifecycle evict --count=5
```

### Root Cause Fix: Restart Worker (Memory Leak Suspected)

**If Memory Leak Detected:**
```bash
# 1. Capture diagnostics before restart
ps aux | grep aos-worker > pre-restart-memory.txt
aosctl metrics show > pre-restart-metrics.txt

# 2. Gracefully stop worker (allows cleanup)
pkill -TERM -f aos-worker

# 3. Wait for graceful shutdown (max 30 seconds)
sleep 30

# 4. Force kill if still running
pkill -9 -f aos-worker

# 5. Clear any stuck state
rm -f var/run/aos/*/worker.sock

# 6. Restart worker (auto-restarts via service manager)
sleep 5
ps aux | grep aos-worker

# 7. Verify memory reset
ps aux | grep aos-worker
aosctl metrics show | grep memory

# 8. Monitor for leak recurrence
watch -n 60 'ps aux | grep aos-worker | awk "{print \$4}"'
```

### Root Cause Fix: Reduce K-Sparse (Temporary Relief)

**Lower Adapter Mixing:**
```bash
# Reduces concurrent adapter loading

# Edit configs/cp.toml
# [router]
# k_sparse = 1  # Reduce from 3 (single adapter selection)

# Restart control plane
pkill -f adapteros-server
make dev

# Verify change
grep "k_sparse" configs/cp.toml

# Test inference (should use less memory)
curl -X POST http://localhost:8080/api/v1/infer \
  -H "Content-Type: application/json" \
  -d '{"prompt": "test", "adapter_id": "test-adapter"}'
```

**Note:** K=1 disables multi-adapter mixing. Use only as temporary measure.

### Root Cause Fix: Optimize Prefix Cache

**If Prefix Cache Growing:**
```bash
# Check current cache size
grep "prefix.*cache.*size" var/aos-worker.log | tail -10

# Reduce cache size in configs/worker.toml
# [inference]
# prefix_cache_max_entries = 100  # Reduce from 1000
# prefix_cache_ttl_secs = 300     # 5 minutes instead of 1 hour

# Restart worker
pkill -f aos-worker

# Monitor cache size
grep -i "prefix.*cache" var/aos-worker.log | tail -f
```

---

## Validation

After applying fixes, verify memory pressure resolved:

```bash
# 1. Check pressure level (should be < 0.75)
aosctl metrics show --json | jq '.memory.pressure_level'

# 2. Verify headroom restored (should be > 15%)
aosctl metrics show --json | jq '.memory.headroom_pct'

# 3. Check loaded adapter count reduced
aosctl metrics show --json | jq '.adapters.loaded_count'

# 4. Monitor stability (15 minutes)
watch -n 60 'aosctl metrics show | grep -E "memory|pressure"'

# 5. Verify auto-eviction working
sqlite3 var/aos-cp.sqlite3 "
SELECT event_type, COUNT(*) as count
FROM telemetry_events
WHERE event_type IN ('adapter.evicted', 'adapter.loaded')
  AND created_at > datetime('now', '-15 minutes')
GROUP BY event_type;"

# 6. Check for OOM warnings (should be none)
grep -i "out of memory\|oom" var/aos-cp.log | tail -10
grep -i "out of memory\|oom" var/aos-worker.log | tail -10
```

**Success Criteria:**
- Memory pressure < 0.75 (< 75% usage)
- Headroom > 15%
- No eviction failures
- Pressure stable for 15+ minutes
- No OOM warnings

---

## Root Cause Prevention

### Post-Incident Actions

1. **Capacity Planning:**
   ```bash
   # Calculate safe limits based on system RAM
   SYSTEM_RAM_GB=$(free -g | awk '/^Mem:/{print $2}')  # Linux
   # SYSTEM_RAM_GB=$(sysctl hw.memsize | awk '{print int($2/1073741824)}')  # macOS

   # Recommended allocations:
   # - Base Model: 40% of RAM
   # - Adapters: 30% of RAM
   # - System: 15% of RAM
   # - Headroom: 15% of RAM

   echo "For ${SYSTEM_RAM_GB}GB system:"
   echo "  Max adapters: $(( SYSTEM_RAM_GB * 1024 * 30 / 100 / 64 ))"
   echo "  (assuming 64MB per adapter)"
   ```

2. **Implement Memory Monitoring Dashboard:**
   ```yaml
   # Grafana dashboard panels:
   - Memory Pressure Gauge (0-1 scale)
   - Headroom Trend (line chart)
   - Loaded Adapter Count (line chart)
   - Eviction Rate (bar chart)
   - Memory Breakdown (stacked area: model + adapters + cache + system)
   ```

3. **Tune Auto-Eviction:**
   ```toml
   # configs/cp.toml - Proactive eviction
   [memory]
   enable_auto_eviction = true
   eviction_threshold_pct = 70      # Lower threshold
   eviction_batch_size = 5          # More aggressive
   eviction_check_interval_secs = 30  # More frequent

   [lifecycle]
   adapter_ttl_secs = 3600          # 1 hour idle time
   enable_lru_eviction = true
   ```

4. **Add Memory Limits to Worker:**
   ```toml
   # configs/worker.toml
   [resource_limits]
   max_memory_mb = 8192  # Hard limit (8GB)
   warn_memory_mb = 7168  # Warning at 7GB
   ```

### Monitoring Improvements

**Enhanced Alerts:**
```yaml
groups:
  - name: adapteros.memory
    rules:
      # Early warning (before critical)
      - alert: MemoryPressureElevated
        expr: memory_pressure_level > 0.65
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Memory pressure elevated ({{ $value | humanizePercentage }})"
          runbook: "docs/runbooks/MEMORY_PRESSURE.md"

      # Approaching critical
      - alert: MemoryPressureHigh
        expr: memory_pressure_level > 0.75
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Memory pressure high - eviction recommended"

      # Critical - immediate action
      - alert: MemoryPressureCritical
        expr: memory_pressure_level > 0.85
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Memory pressure critical - auto-eviction triggered"
          action: "Page on-call if eviction fails"

      # Eviction failures
      - alert: MemoryEvictionFailing
        expr: rate(adapter_eviction_failures_total[5m]) > 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Adapter eviction failing - manual intervention needed"
          action: "Check pinned adapters, force eviction if necessary"

      # Memory leak detection
      - alert: MemoryLeakSuspected
        expr: rate(memory_usage_bytes[1h]) > 10485760  # 10MB/hour growth
        for: 3h
        labels:
          severity: warning
        annotations:
          summary: "Memory usage growing {{ $value | humanize }}/hour - possible leak"
          action: "Monitor and schedule worker restart during low traffic"
```

**Memory Trend Reporting:**
```bash
# Daily memory report (cron job)
#!/bin/bash
# /opt/adapteros/scripts/daily-memory-report.sh

REPORT_DATE=$(date +%Y-%m-%d)

mkdir -p var/reports
sqlite3 var/aos-cp.sqlite3 <<EOF > var/reports/memory-report-${REPORT_DATE}.txt
SELECT
  date(timestamp) as date,
  MAX(memory_usage_bytes) / 1073741824.0 as peak_gb,
  AVG(memory_usage_bytes) / 1073741824.0 as avg_gb,
  MAX(memory_pressure_level) as max_pressure,
  COUNT(CASE WHEN memory_pressure_level > 0.75 THEN 1 END) as high_pressure_count
FROM system_metrics
WHERE timestamp > datetime('now', '-24 hours')
GROUP BY date(timestamp);
EOF

# Email report to team
mail -s "AdapterOS Memory Report ${REPORT_DATE}" sre-team@company.com < var/reports/memory-report-${REPORT_DATE}.txt
```

---

## Escalation

### Escalate to Senior Engineer If:
- Memory pressure > 0.85 for > 15 minutes despite evictions
- Auto-eviction repeatedly failing
- Memory leak confirmed (> 10% growth per hour)
- Requires configuration changes affecting all tenants

### Escalate to Engineering Manager If:
- SEV-1 upgrade (worker OOM killed)
- Memory issue causing service outage
- Requires emergency capacity expansion (hardware)
- Customer SLA at risk

### Notify Platform Team If:
- System-wide memory issue (not AdapterOS-specific)
- Need OS-level memory tuning
- Swap configuration required
- NUMA/memory architecture questions

---

## Notes

**Memory Pressure Levels:**
- **0.0-0.60 (Low):** Healthy, normal operation
- **0.60-0.75 (Medium):** Monitor, consider eviction
- **0.75-0.85 (High):** Active eviction, warn users
- **0.85-1.0 (Critical):** Aggressive eviction, block new loads

**UMA (Unified Memory Architecture):**
- On Apple Silicon, GPU and CPU share memory
- Memory pressure affects both inference and system
- Unlike discrete GPUs, no separate VRAM pool

**Eviction Strategies:**
- **LRU (Least Recently Used):** Evict oldest by last_used_at
- **Lowest Activation %:** Evict adapters with lowest usage rate
- **Largest First:** Evict large adapters first (frees most memory)
- **Cold First:** Evict unloaded adapters before loaded ones

**Common Misconceptions:**
- Eviction is NOT deletion (adapter files preserved)
- Eviction is reversible (can reload adapter)
- Pinned adapters CANNOT be evicted (by design)
- Auto-eviction uses LRU unless configured otherwise

---

**Owner:** SRE Team
**Last Incident:** [Link to most recent postmortem]
**Related Runbooks:** [WORKER_CRASH.md](./WORKER_CRASH.md), [INFERENCE_LATENCY_SPIKE.md](./INFERENCE_LATENCY_SPIKE.md)
