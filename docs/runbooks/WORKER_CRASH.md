# Runbook: Worker Process Crash

**Scenario:** Worker process (aos-worker) terminated unexpectedly

**Severity:** SEV-1 (Immediate response)

**Last Updated:** 2025-12-15

---

## Symptoms

### Alert Indicators
- **Alert:** `WorkerCrashed` (worker health status = crashed)
- **Alert:** `WorkerNotResponding` (health check failures)
- **Alert:** `InferenceServiceDown` (503 errors from API)
- **Prometheus Query:** `worker_health_status{status="crashed"} == 1`

### User Reports
- All inference requests failing with 503 Service Unavailable
- Chat interface shows "Service temporarily unavailable"
- Training jobs stuck in "Starting" state
- UI shows worker status as "Offline" or "Crashed"

### System Indicators
- Worker process not visible in `ps aux | grep aos-worker`
- Socket file missing: `var/run/aos/{tenant}/worker.sock`
- Recent panic or fatal error in `var/aos-worker.log`
- Control plane logs show "worker not responding" errors

---

## Diagnosis Steps

### 1. Verify Worker Status

```bash
# Check if worker process is running
ps aux | grep aos-worker | grep -v grep

# Expected: One or more aos-worker processes
# If empty: Worker is crashed

# Check worker socket exists
ls -la var/run/aos/*/worker.sock

# Expected: Socket file(s) present with recent timestamp
# If missing: Worker not initialized or crashed before socket creation

# Check control plane connectivity
curl -f http://localhost:8080/healthz
curl -s http://localhost:8080/api/v1/workers | jq '.[] | {id, status, health_status}'
```

**If worker not running:** Proceed to Step 2
**If worker running but not responding:** Proceed to Step 5

### 2. Check Worker Exit Reason

```bash
# Check worker logs for panic/fatal errors
tail -200 var/aos-worker.log | grep -i "panic\|fatal\|error"

# Check systemd logs (if using systemd)
journalctl -u aos-worker -n 100 --no-pager

# Check service manager logs
grep "aos-worker" scripts/service-manager.log | tail -50

# Look for crash dumps
ls -lat var/crash-dumps/ | head -10
```

**Common Exit Reasons:**
- **PANIC:** Rust panic (unrecoverable error)
- **OOM:** Out of memory (killed by system)
- **SIGKILL:** Manual termination or timeout
- **SIGABRT:** Assertion failure
- **Exit code 1:** Controlled exit with error

### 3. Check System Resources

```bash
# Check available memory
free -h 2>/dev/null || vm_stat | grep "Pages free"

# Check disk space (worker needs space for model cache)
df -h var/

# Check for OOM kills in system log (macOS)
log show --predicate 'eventMessage contains "killed"' --info --last 1h | grep aos

# Check for OOM kills (Linux)
dmesg | grep -i "killed process" | grep aos-worker

# Check system load
uptime
```

**If OOM kill detected:** Memory pressure issue (see Step 4)
**If disk full:** Disk space issue (see DISK_FULL.md)
**Otherwise:** Application error (see Step 6)

### 4. Diagnose Memory Issues

```bash
# Check memory usage before crash (from metrics)
sqlite3 var/aos-cp.sqlite3 "
SELECT timestamp, memory_usage_bytes, memory_pressure_level
FROM system_metrics
WHERE timestamp > datetime('now', '-1 hour')
ORDER BY timestamp DESC
LIMIT 20;"

# Check loaded adapters at time of crash
sqlite3 var/aos-cp.sqlite3 "
SELECT COUNT(*) as loaded_count,
       SUM(vram_mb) as total_vram_mb
FROM adapters
WHERE status='Loaded';"

# Check base model size
du -sh var/model-cache/*/

# Check total memory requirements
echo "Estimated memory: Base model + Adapters + System overhead"
```

**Memory Calculation:**
```
Required Memory = Base Model (4-8GB) + (K × Adapter Size) + 15% Headroom + 2GB System
Example: Qwen 2.5 7B (5GB) + (3 × 64MB) + 15% + 2GB ≈ 8.5GB minimum
```

### 5. Diagnose Hung Worker (Running but Not Responding)

```bash
# Check if worker is stuck
# Get worker PID
WORKER_PID=$(pgrep -f aos-worker)

# Check thread state (macOS)
sample $WORKER_PID 3 -file worker-sample.txt

# Check thread state (Linux)
cat /proc/$WORKER_PID/status

# Check if worker can handle signals
kill -USR1 $WORKER_PID  # Should trigger debug dump
sleep 2
tail -20 var/aos-worker.log

# Check socket connectivity
echo "test" | nc -U var/run/aos/default/worker.sock

# Check for deadlock
# If worker compiled with debug symbols:
lldb -p $WORKER_PID -batch -o "thread backtrace all" > worker-threads.txt
```

**Common Hung States:**
- Deadlock on mutex/rwlock
- Blocked on I/O (disk read, network)
- Infinite loop in backend code
- GPU command buffer stalled

### 6. Analyze Application Error

```bash
# Extract panic backtrace from logs
grep -A 50 "thread.*panicked" var/aos-worker.log | tail -100

# Common panic patterns:
grep -E "unwrap|expect|index out of bounds|assertion failed" var/aos-worker.log | tail -20

# Check for specific error types
grep -E "Backend.*error|Model.*error|Adapter.*error" var/aos-worker.log | tail -20

# Check for corruption
grep -i "corrupt\|invalid\|mismatch" var/aos-worker.log | tail -20
```

---

## Resolution

### Quick Fix: Restart Worker

**Automatic Restart (Service Manager):**
```bash
# Service manager should auto-restart within 30 seconds
# Check if it restarted
sleep 30
ps aux | grep aos-worker | grep -v grep

# Check restart count
sqlite3 var/aos-cp.sqlite3 "
SELECT worker_id, restart_count, last_restart_at
FROM workers
ORDER BY last_restart_at DESC
LIMIT 5;"
```

**Manual Restart:**
```bash
# If auto-restart failed or disabled:

# 1. Ensure no zombie process
pkill -9 -f aos-worker

# 2. Clean up socket files
rm -f var/run/aos/*/worker.sock

# 3. Start worker manually (for testing)
cargo run --bin aos_worker --release -- \
  --socket var/run/aos/default/worker.sock \
  --tenant-id default

# 4. Or restart via service manager
./scripts/service-manager.sh restart worker

# 5. Verify worker started
sleep 5
ps aux | grep aos-worker
ls -la var/run/aos/*/worker.sock
```

**Verify Recovery:**
```bash
# Check worker health
curl -s http://localhost:8080/api/v1/workers | jq '.[] | {id, status, health_status}'

# Test inference
curl -X POST http://localhost:8080/api/v1/infer \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "test connectivity",
    "adapter_id": "test-adapter",
    "max_tokens": 10
  }'

# Check logs for errors
tail -20 var/aos-worker.log
```

### Root Cause Fix: Memory Issues

**If OOM Kills Detected:**
```bash
# 1. Reduce memory footprint
# Edit configs/cp.toml:
# [memory]
# min_headroom_pct = 20  # Increase from 15
# max_adapters_per_tenant = 3  # Reduce from 10

# 2. Evict cold adapters proactively
aosctl lifecycle evict --strategy=lowest_activation_pct --count=5

# 3. Switch to smaller quantized model
# Replace 7B model with 4-bit quantized version
# Edit configs/cp.toml:
# [model]
# quantization = "int4"  # Instead of fp16

# 4. Restart worker
pkill -f aos-worker
sleep 5
ps aux | grep aos-worker  # Should auto-restart

# 5. Monitor memory
watch -n 10 'aosctl metrics show | grep memory'
```

**If System Has Insufficient RAM:**
```bash
# Temporary: Reduce concurrent requests
# Edit configs/cp.toml:
# [server]
# max_concurrent_requests = 5

# Long-term: Hardware upgrade or architecture change
# - Add more RAM
# - Deploy multiple smaller workers
# - Use CPU-only mode (slower but less memory)
```

### Root Cause Fix: Application Errors

**Adapter Corruption:**
```bash
# If panic mentions specific adapter:
# 1. Identify corrupted adapter from logs
CORRUPT_ADAPTER=$(grep "adapter.*corrupt\|adapter.*invalid" var/aos-worker.log | tail -1 | grep -oE 'adapter_[a-zA-Z0-9_-]+')

# 2. Quarantine adapter
sqlite3 var/aos-cp.sqlite3 "
UPDATE adapters
SET status='Quarantined', quarantine_reason='Crash on load'
WHERE adapter_id='$CORRUPT_ADAPTER';"

# 3. Restart worker (will skip quarantined adapter)
pkill -f aos-worker

# 4. Investigate adapter integrity
./aosctl adapter inspect var/adapters/$CORRUPT_ADAPTER.aos
```

**Backend Initialization Failure:**
```bash
# If backend (MLX/Metal/CoreML) fails to initialize:

# 1. Check if stub backend fallback is working
grep -i "stub.*backend.*active" var/aos-worker.log

# 2. Rebuild with correct backend
cargo build --release --features mlx-backend  # macOS only

# 3. Verify Metal shaders (if using Metal)
cd metal && bash build.sh
ls -la crates/adapteros-lora-kernel-mtl/metal/kernels.metallib

# 4. Check GPU availability
system_profiler SPDisplaysDataType | grep Metal  # macOS

# 5. Restart worker with working backend
pkill -f aos-worker
```

**Model File Corruption:**
```bash
# If base model corrupted:

# 1. Verify model files
ls -la var/model-cache/qwen2.5-7b-mlx/

# 2. Check model integrity (if checksums available)
sha256sum var/model-cache/qwen2.5-7b-mlx/*.safetensors

# 3. Re-download model if corrupted
rm -rf var/model-cache/qwen2.5-7b-mlx/
./scripts/download-model.sh qwen2.5-7b-instruct

# 4. Restart worker
pkill -f aos-worker
```

### Root Cause Fix: Deadlock/Hang

**If Worker Hung (Not Crashed):**
```bash
# 1. Collect diagnostics before stopping
WORKER_PID=$(pgrep -f aos-worker)
kill -QUIT $WORKER_PID  # Trigger core dump (if enabled)

# Or get thread backtrace
lldb -p $WORKER_PID -batch -o "thread backtrace all" > worker-deadlock.txt

# 2. Force stop and restart
kill -9 $WORKER_PID
sleep 5

# 3. Analyze deadlock pattern
# Review worker-deadlock.txt for mutex contention
# Common patterns:
# - AdapterTable::swap() concurrent access
# - Backend lock during eviction
# - Database connection pool exhaustion

# 4. If reproducible, enable debug logging
export RUST_LOG=debug
export AOS_WORKER_DEBUG=1
pkill -f aos-worker
```

---

## Validation

After worker restart, verify stability:

```bash
# 1. Worker process running
ps aux | grep aos-worker | grep -v grep

# 2. Health check passing
curl -f http://localhost:8080/healthz

# 3. Worker registered
curl -s http://localhost:8080/api/v1/workers | jq '.[] | {id, status, health_status}'

# 4. Inference working
time curl -X POST http://localhost:8080/api/v1/infer \
  -H "Content-Type: application/json" \
  -d '{"prompt": "Hello", "max_tokens": 5}'

# 5. No errors in logs
tail -50 var/aos-worker.log | grep ERROR

# 6. Memory stable (monitor for 15 minutes)
watch -n 60 'aosctl metrics show | grep -E "memory|worker"'

# 7. No crash loops
sqlite3 var/aos-cp.sqlite3 "
SELECT worker_id, restart_count, last_restart_at
FROM workers
WHERE last_restart_at > datetime('now', '-1 hour');"
```

**Success Criteria:**
- Worker process running for 15+ minutes without restart
- Health status = "Healthy"
- Inference requests succeeding (< 5% error rate)
- Memory usage stable (not climbing)
- No panics in logs

---

## Root Cause Prevention

### Post-Incident Actions

1. **Add Crash Dump Collection:**
   ```bash
   # Enable core dumps
   ulimit -c unlimited
   echo "var/crash-dumps/core.%e.%p" | sudo tee /proc/sys/kernel/core_pattern
   ```

2. **Improve Health Monitoring:**
   ```yaml
   # Add to Prometheus alerts
   - alert: WorkerRestartLoop
     expr: rate(worker_restarts_total[5m]) > 2
     for: 0m
     labels:
       severity: critical
     annotations:
       summary: "Worker restarting repeatedly ({{ $value }} restarts/5min)"
       action: "Investigate crash cause immediately"
   ```

3. **Implement Graceful Degradation:**
   ```toml
   # Add to configs/cp.toml
   [worker]
   restart_policy = "exponential_backoff"
   max_restart_attempts = 5
   restart_timeout_secs = 300
   fallback_to_stub = false  # NEVER use stub in production
   ```

4. **Memory Safeguards:**
   ```toml
   # Add to configs/cp.toml
   [memory]
   enable_auto_eviction = true
   eviction_threshold_pct = 80
   min_headroom_pct = 20
   oom_score_adj = -100  # Protect worker from OOM killer
   ```

### Monitoring Improvements

**Add Worker-Specific Alerts:**
```yaml
groups:
  - name: adapteros.worker
    rules:
      - alert: WorkerCrashed
        expr: worker_health_status{status="crashed"} == 1
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "Worker {{ $labels.worker_id }} crashed"
          runbook: "docs/runbooks/WORKER_CRASH.md"

      - alert: WorkerMemoryHigh
        expr: worker_memory_usage_bytes / worker_memory_limit_bytes > 0.85
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Worker memory usage > 85%"

      - alert: WorkerOOMRisk
        expr: worker_memory_usage_bytes / worker_memory_limit_bytes > 0.95
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Worker at risk of OOM kill"
          action: "Force eviction immediately"
```

**Add Structured Logging:**
```rust
// In worker code, add context to panics
panic_handler::register(|info| {
    error!(
        panic_info = ?info,
        adapter_count = loaded_adapters.len(),
        memory_mb = current_memory_mb,
        "Worker panic occurred"
    );
});
```

---

## Escalation

### Escalate to Senior Engineer If:
- Worker crashes more than 3 times in 1 hour
- Root cause unclear after 30 minutes
- Crash dump analysis required
- Suspected backend or kernel bug

### Escalate to Engineering Manager If:
- Worker crash causing extended outage (> 1 hour)
- Data corruption suspected
- Customer data at risk
- Requires emergency hotfix deployment

### Notify Platform Team If:
- OOM kills due to system misconfiguration
- Kernel or OS-level issues
- Hardware failure suspected (GPU, memory)

### Notify Security Team If:
- Crash caused by malformed input (potential exploit)
- Memory corruption from external source
- Unauthorized process termination

---

## Notes

**Common Crash Patterns:**
1. **OOM Kill:** Most common in production (70% of crashes)
2. **Panic on Adapter Load:** Usually corrupted adapter file
3. **Backend Initialization:** Missing MLX/Metal libraries
4. **Deadlock:** Rare but requires code fix
5. **GPU Error:** Metal command buffer failures on M-series

**Known Issues:**
- Worker can OOM if too many adapters preloaded
- Metal backend occasionally panics on M1 (fixed in M2+)
- Prefix KV cache can leak memory under high load (monitor)

**Auto-Recovery Behavior:**
- Service manager restarts worker within 30 seconds
- Exponential backoff after repeated crashes (5, 10, 20, 40 seconds)
- After 5 failures, worker stays down (requires manual intervention)

**Performance Impact:**
- Worker restart causes 30-60 second inference outage
- Loaded adapters must be re-initialized (2-5 seconds each)
- Prefix KV cache cleared (first requests slower)

---

**Owner:** SRE Team
**Last Incident:** [Link to most recent postmortem]
**Related Runbooks:** [MEMORY_PRESSURE.md](MEMORY_PRESSURE.md), [DETERMINISM_VIOLATION.md](DETERMINISM_VIOLATION.md)
