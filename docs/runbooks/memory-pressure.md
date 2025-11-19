# Memory Pressure

High memory usage, adapter eviction, and GPU memory management.

## Symptoms

- "Critical memory pressure" warnings
- Adapters being evicted unexpectedly
- Inference requests failing with OOM
- System becoming unresponsive
- Health check showing degraded system-metrics
- UMA (Unified Memory Architecture) headroom below 15%

## Understanding Memory Levels

AdapterOS uses tiered memory pressure levels:

```
Headroom > 30%:  Normal    - All operations proceed normally
Headroom > 20%:  Medium    - Start selective eviction
Headroom > 15%:  High      - Aggressive eviction, warnings logged
Headroom ≤ 15%:  Critical  - Block new operations, emergency eviction
```

**Headroom** = Available memory as percentage of total memory

## Common Failure Modes

### 1. Critical Memory Pressure During Inference

**Symptoms:**
```
[ERROR] System under pressure (level: Critical), retry in 30s or reduce max_tokens
[WARN] Critical memory pressure (14% headroom, 2048 MB available)
[ERROR] Inference request failed: insufficient memory
```

**Root Cause:**
- Too many adapters loaded simultaneously
- Large batch inference requests
- Memory leak in adapter code
- Base model too large for available RAM
- Insufficient headroom buffer

**Diagnostic Commands:**
```bash
# Check current memory pressure
aosctl status memory

# Check UMA monitor stats
curl http://localhost:8080/healthz/system-metrics | jq '.details'

# List loaded adapters
aosctl adapter list --state loaded

# Check adapter memory usage
ps aux | grep aos | awk '{print $2, $6, $11}'
```

**Expected Output:**
```json
{
  "uma_monitor_active": true,
  "memory_used_mb": 28672,
  "memory_total_mb": 32768,
  "memory_available_mb": 6144,
  "headroom_pct": 18.75,
  "pressure_level": "high"
}
```

**Fix Procedure:**

**Step 1: Immediate Relief**
```bash
# Evict ephemeral adapters
aosctl maintenance gc --force

# Reduce loaded adapter count
aosctl adapter unload <adapter-id>

# Wait for memory to stabilize
sleep 30

# Check pressure level
aosctl status memory
```

**Step 2: Reduce Active Load**
```bash
# List adapters by state
aosctl adapter list --json | jq '.[] | select(.current_state == "hot" or .current_state == "warm")'

# Unload non-critical adapters
aosctl adapter unload <adapter-id-1>
aosctl adapter unload <adapter-id-2>

# Verify memory improved
curl http://localhost:8080/healthz/system-metrics | jq '.details.headroom_pct'
```

**Step 3: Reduce Inference Load**
```bash
# Lower max_tokens for pending requests
aosctl infer --prompt "test" --max-tokens 64  # instead of 256

# Batch smaller requests
# Split large prompts into smaller chunks
```

**Step 4: Emergency Restart (Last Resort)**
```bash
# Graceful shutdown
pkill -SIGTERM aos-cp

# Wait for cleanup
sleep 10

# Restart with clean slate
./scripts/start_server.sh

# Verify healthy
aosctl doctor
```

**Prevention:**
- Monitor memory headroom continuously
- Set conservative memory limits
- Pin critical adapters, evict ephemeral ones
- Use adapter TTLs appropriately
- Implement request throttling

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/lib.rs` - UMA pressure monitor
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:371-456` - System metrics health check
- `/Users/star/Dev/aos/crates/adapteros-cli/src/main.rs:1605-1617` - Inference pressure check

### 2. Adapter Eviction Thrashing

**Symptoms:**
```
[INFO] Evicting adapter adapter_123 due to memory pressure
[INFO] Loading adapter adapter_123
[INFO] Evicting adapter adapter_123 due to memory pressure
[WARN] Adapter eviction thrashing detected
```

**Root Cause:**
- Memory headroom too tight for working set
- Routing decisions loading/unloading same adapters
- Insufficient pinning of frequently-used adapters
- LRU cache too aggressive

**Diagnostic Commands:**
```bash
# Check eviction frequency
grep "Evicting adapter" var/aos-cp.log | tail -20

# Check adapter load/unload cycles
grep "adapter_" var/aos-cp.log | grep -E "Loading|Evicting" | tail -30

# Check pinned adapters
aosctl adapter list --pinned

# Check routing decisions
aosctl telemetry-list --event-type router.decision --limit 50
```

**Fix Procedure:**

**Step 1: Pin Frequently-Used Adapters**
```bash
# Identify hot adapters from telemetry
aosctl telemetry-list --event-type router.decision --limit 100 | \
  jq -r '.events[].adapter_id' | sort | uniq -c | sort -rn | head -5

# Pin top adapters
aosctl pin-adapter --tenant default --adapter <hot-adapter-1> \
  --reason "High routing frequency"

aosctl pin-adapter --tenant default --adapter <hot-adapter-2> \
  --reason "High routing frequency"
```

**Step 2: Adjust Tier Strategy**
```bash
# Promote ephemeral to persistent if frequently used
aosctl adapter update <adapter-id> --tier persistent

# List by tier
aosctl adapter list --tier ephemeral
aosctl adapter list --tier persistent
```

**Step 3: Increase Memory Headroom Target**
```bash
# This requires code change - target > 20% instead of > 15%
# Or reduce total adapter count to give more headroom
aosctl maintenance gc --aggressive
```

**Prevention:**
- Pin adapters with > 10% routing share
- Use persistent tier for production adapters
- Monitor eviction rates
- Set realistic memory capacity limits

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-cli/src/commands/pin.rs` - Pinning commands
- `/Users/star/Dev/aos/crates/adapteros-db/src/plugin_enables.rs` - Pinning logic

### 3. Memory Leak Detection

**Symptoms:**
```
[WARN] Memory usage increasing over time
[WARN] Available memory decreasing without corresponding adapter loads
[INFO] memory_used_mb: 28672 (was 24576 1 hour ago)
```

**Root Cause:**
- Adapter not releasing memory on unload
- Telemetry event accumulation
- KV cache not being cleared
- Resource leak in Metal/GPU layer

**Diagnostic Commands:**
```bash
# Track memory over time
watch -n 60 'curl -s http://localhost:8080/healthz/system-metrics | jq ".details.memory_used_mb"'

# Check process memory
ps aux | grep aos-cp | awk '{print $2, $4, $6}'

# Check for orphaned adapters
aosctl adapter list --state orphaned

# Check telemetry size
du -sh var/telemetry/
```

**Fix Procedure:**

**Step 1: Capture Baseline**
```bash
# Record current state
curl http://localhost:8080/healthz/system-metrics > memory-baseline.json
aosctl adapter list --json > adapters-baseline.json
ps aux | grep aos > process-baseline.txt
```

**Step 2: Force Cleanup**
```bash
# Run garbage collection
aosctl maintenance gc --force

# Clear orphaned adapters
aosctl maintenance sweep-orphaned

# Clear old telemetry
aosctl maintenance cleanup-telemetry --older-than 7d

# Checkpoint database WAL
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"
```

**Step 3: Monitor for Continued Leak**
```bash
# Record after cleanup
curl http://localhost:8080/healthz/system-metrics > memory-after-gc.json

# Compare
diff memory-baseline.json memory-after-gc.json

# Continue monitoring
watch -n 300 'date; curl -s http://localhost:8080/healthz/system-metrics | jq ".details.memory_used_mb"'
```

**Step 4: Restart if Leak Persists**
```bash
# Graceful restart
pkill -SIGTERM aos-cp
sleep 10
./scripts/start_server.sh

# Monitor for leak recurrence
```

**Prevention:**
- Implement periodic restarts (daily maintenance window)
- Monitor memory trends
- Set up alerts for memory growth
- Regular GC and cleanup

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-cli/src/commands/maintenance.rs` - Maintenance commands

### 4. GPU Memory Exhaustion (Metal)

**Symptoms:**
```
[ERROR] Metal allocation failed: out of memory
[ERROR] Failed to allocate buffer of size 2147483648
[ERROR] Kernel execution failed: insufficient resources
```

**Root Cause:**
- Model + adapters exceed GPU memory
- Fragmented GPU memory
- Multiple inference sessions
- Memory not released after inference

**Diagnostic Commands:**
```bash
# Check GPU memory (macOS)
system_profiler SPDisplaysDataType | grep -i "vram\|memory"

# Check active inference sessions
aosctl status workers

# Check loaded adapters
aosctl adapter list --state hot
```

**Fix Procedure:**

**Step 1: Reduce GPU Load**
```bash
# Unload hot adapters
aosctl adapter unload <adapter-id>

# Wait for GPU memory release
sleep 10

# Try inference again
aosctl infer --prompt "test" --max-tokens 64
```

**Step 2: Use Smaller Model**
```bash
# Switch to smaller base model variant
# E.g., qwen2.5-7b instead of qwen2.5-72b
```

**Step 3: Reduce Adapter Rank**
```bash
# Re-train adapters with lower rank
aosctl train --data training.json --output adapter/ --rank 8
# Instead of --rank 16 or --rank 32
```

**Prevention:**
- Choose model size appropriate for GPU memory
- Use rank-8 adapters unless rank-16+ required
- Monitor GPU memory utilization
- Implement adapter rotation strategy

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-lora-kernel-mtl/src/lib.rs` - Metal kernel implementation
- `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/memory.rs` - Memory management

## Memory Optimization Strategies

### Adapter Pinning Strategy
```bash
# Pin production-critical adapters
aosctl pin-adapter --tenant prod --adapter critical-adapter-1 \
  --reason "Production critical"

# Pin with TTL for testing
aosctl pin-adapter --tenant dev --adapter test-adapter \
  --ttl-hours 24 --reason "Testing in progress"

# List pinned
aosctl adapter list --pinned
```

### Tiering Strategy
```
Persistent Tier:
  - Production adapters
  - High-frequency use
  - > 10% routing share
  - Pinned by default

Ephemeral Tier:
  - Development/testing
  - Low-frequency use
  - < 5% routing share
  - Subject to eviction
```

### Memory Headroom Targets
```
32GB RAM:  Maintain > 6GB free  (18.75% headroom)
64GB RAM:  Maintain > 12GB free (18.75% headroom)
128GB RAM: Maintain > 24GB free (18.75% headroom)
```

## Monitoring Commands

```bash
# Real-time memory monitoring
watch -n 5 'curl -s http://localhost:8080/healthz/system-metrics | jq ".details | {used_mb, total_mb, available_mb, headroom_pct, pressure_level}"'

# Memory history
grep "memory_used_mb" var/aos-cp.log | tail -20

# Adapter memory footprint
aosctl adapter list --json | jq '.[] | {adapter_id, rank, size_mb}'

# System memory
vm_stat | head -15
```

## Related Runbooks

- [Health Check Failures](./health-check-failures.md)
- [Cleanup Procedures](./cleanup-procedures.md)
- [Metrics Review](./metrics-review.md)

## Escalation Criteria

Escalate if:
- Memory leak cannot be identified
- Critical pressure persists after cleanup
- GPU memory exhaustion on appropriate hardware
- Adapter eviction causing service degradation
- See [Escalation Guide](./escalation.md)
