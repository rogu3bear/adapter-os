# Critical Components Runbook

Production operational procedures for AdapterOS critical subsystems.

**Last Updated:** 2025-11-21
**Scope:** Metal Backend, Hot-Swap, Deterministic Execution, Content Addressing
**Related:** [ESCALATION.md](./ESCALATION.md) | [MEMORY-PRESSURE.md](./MEMORY-PRESSURE.md) | [PRODUCTION_MONITORING.md](../PRODUCTION_MONITORING.md)

---

## Table of Contents

1. [Metal Backend Issues](#1-metal-backend-issues)
2. [Hot-Swap Failures](#2-hot-swap-failures)
3. [Deterministic Execution](#3-deterministic-execution)
4. [Content Addressing](#4-content-addressing)

---

## 1. Metal Backend Issues

### 1.1 GPU Memory Exhaustion

**Severity:** Critical (SEV1 if inference blocked, SEV2 otherwise)

**Symptoms:**
```
[ERROR] Metal allocation failed: out of memory
[ERROR] Failed to allocate buffer of size 2147483648
[ERROR] Kernel execution failed: insufficient resources
[WARN] gpu_memory_pressure gauge > 0.85
```

**Prometheus Indicators:**
- `gpu_memory_pressure > 0.85` for > 2 minutes
- `gpu_memory_pool_reuse_ratio < 0.5` (fragmentation)
- `adapter_evictions_total` increasing rapidly

**Diagnostic Commands:**
```bash
# Step 1: Check GPU memory pressure
curl -s http://localhost:8080/healthz/system-metrics | jq '.details | {memory_pressure: .gpu_memory_pressure, available_mb: .memory_available_mb}'

# Step 2: Check loaded adapters consuming VRAM
aosctl adapter list --state hot --json | jq '.[] | {adapter_id, vram_mb: .vram_bytes / 1048576, activation_pct}'

# Step 3: Check Metal device status (macOS)
system_profiler SPDisplaysDataType | grep -A 20 "Metal Family"

# Step 4: Review kernel allocation patterns
grep "Metal allocation" var/aos-cp.log | tail -20
```

**Resolution Steps:**

**Step 1: Reduce Immediate Load**
```bash
# List adapters by activation percentage (lowest first)
aosctl adapter list --json | jq 'sort_by(.activation_percentage) | .[:5] | .[].adapter_id'

# Unload lowest-activation adapters
aosctl adapter unload <adapter-id-1>
aosctl adapter unload <adapter-id-2>

# Force garbage collection
aosctl maintenance gc --force

# Verify pressure reduced
curl -s http://localhost:8080/healthz/system-metrics | jq '.details.gpu_memory_pressure'
```

**Step 2: Check for Memory Pool Fragmentation**
```bash
# If reuse ratio < 0.5, memory pool is fragmented
curl -s http://localhost:9090/metrics | grep gpu_memory_pool_reuse_ratio

# Reset memory pool (requires service restart)
pkill -SIGTERM aos-cp
sleep 10
./scripts/start_server.sh
```

**Step 3: Review Buffer Allocation Patterns**
```bash
# Check for large buffer allocations
grep "allocate buffer" var/aos-cp.log | grep -E "[0-9]{10}" | tail -10

# If buffers > 2GB, may need to reduce batch size or model dimensions
```

**Prevention:**
- Set `gpu_memory_pressure` alert at 0.75 for early warning
- Configure lifecycle manager with aggressive eviction threshold (0.80)
- Use rank-8 adapters unless rank-16+ specifically required
- Implement adapter tiering (ephemeral vs persistent)

**Related Files:**
- `crates/adapteros-lora-kernel-mtl/src/gpu_memory_pool.rs` - Memory pool implementation
- `crates/adapteros-lora-kernel-mtl/src/vram.rs` - VRAM tracking
- `crates/adapteros-telemetry/src/metrics/critical_components.rs` - Metrics

---

### 1.2 Kernel Compilation Failures

**Severity:** SEV2 (service degraded) to SEV1 (if all kernels fail)

**Symptoms:**
```
[ERROR] Metal compilation failed: missing function 'fused_mlp_forward'
[ERROR] metallib verification failed: hash mismatch
[ERROR] Failed to create pipeline state for kernel
[WARN] metal_kernel_panic_count_total increased
```

**Prometheus Indicators:**
- `metal_kernel_panic_count_total` > 0
- `gpu_device_recovery_count_total{status="failed"}` > 0

**Diagnostic Commands:**
```bash
# Step 1: Verify embedded metallib integrity
aosctl verify-metallib --check-hash

# Step 2: Check kernel manifest
aosctl kernel-manifest --show

# Step 3: Review compilation errors
grep -E "Metal compilation|metallib|pipeline state" var/aos-cp.log | tail -30

# Step 4: Check Metal toolchain version
xcrun metal --version
xcrun metallib --version
```

**Resolution Steps:**

**Step 1: Verify Metal Toolchain**
```bash
# Check Xcode Command Line Tools
xcode-select -p

# Ensure correct version (15.0+)
xcrun metal --version

# If wrong version, install correct tools
sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
```

**Step 2: Rebuild Metallib**
```bash
# Navigate to metal shaders directory
cd metal/

# Clean and rebuild
make clean
make all

# Verify hash
cat shaders/kernel_hash.txt

# Restart service
pkill -SIGTERM aos-cp
./scripts/start_server.sh
```

**Step 3: Check Device Family Compatibility**
```bash
# Get Metal device family
system_profiler SPDisplaysDataType | grep "Metal Family"

# If using older device family, may need fallback kernels
# Check supported features in kernel manifest
aosctl kernel-manifest --show | grep "device_family"
```

**Prevention:**
- Lock toolchain versions in `metal/toolchain.toml`
- Run kernel verification in CI before deployment
- Maintain separate metallibs for different device families
- Store kernel hashes in deployment artifacts

**Related Files:**
- `metal/aos_kernels.metal` - Source shaders
- `metal/ci_build.sh` - Build script
- `crates/adapteros-lora-kernel-mtl/src/manifest.rs` - Manifest verification

---

### 1.3 Performance Degradation

**Severity:** SEV3 (degraded) to SEV2 (if > 50% throughput reduction)

**Symptoms:**
```
[WARN] Kernel latency p95 increased 50% over baseline
[WARN] Inference throughput below threshold
[INFO] metal_kernel_execution_us histogram showing increased latency
```

**Prometheus Indicators:**
- `histogram_quantile(0.95, metal_kernel_execution_us_bucket) > 2x baseline`
- `inference_latency_ms` p99 > 500ms
- `kernel_latency_ms{kernel_type="FusedMlp"}` p95 increased > 50%

**Diagnostic Commands:**
```bash
# Step 1: Get kernel execution times
curl -s http://localhost:9090/metrics | grep metal_kernel_execution_us

# Step 2: Compare against baseline
# (Baseline should be stored in deployment artifacts)
diff expected_metrics.txt <(curl -s http://localhost:9090/metrics | grep metal_kernel)

# Step 3: Check for thermal throttling (macOS)
sudo powermetrics --samplers smc -n 1 | grep -E "CPU|GPU"

# Step 4: Check concurrent operations
aosctl status workers | grep active
```

**Resolution Steps:**

**Step 1: Identify Slow Kernels**
```bash
# Query Prometheus for kernel breakdown
curl -s 'http://localhost:9090/api/v1/query?query=histogram_quantile(0.95,metal_kernel_execution_us_bucket)' | jq

# Check for specific kernel regression
grep "kernel execution" var/aos-cp.log | grep -E "[0-9]{4,}us" | tail -20
```

**Step 2: Check for Contention**
```bash
# Check adapter swap frequency (swaps cause kernel pipeline stalls)
curl -s http://localhost:9090/metrics | grep swap_count_total

# If high swap rate, pin frequently-used adapters
aosctl pin-adapter --tenant default --adapter <hot-adapter> --reason "Performance optimization"
```

**Step 3: Verify GPU State**
```bash
# Check GPU utilization
sudo powermetrics --samplers gpu_power -n 5 --interval 1000

# If GPU underutilized, check for pipeline bubbles
grep "pipeline" var/aos-cp.log | tail -20
```

**Step 4: Reset GPU State (Last Resort)**
```bash
# Graceful restart resets GPU command queues
pkill -SIGTERM aos-cp
sleep 10
./scripts/start_server.sh

# Verify performance restored
aosctl benchmark --quick
```

**Prevention:**
- Establish baseline metrics during deployment
- Set alerts for 25% deviation from baseline
- Monitor thermal state on shared GPU systems
- Use dedicated GPU for production workloads

**Related Files:**
- `crates/adapteros-lora-kernel-mtl/src/optimization.rs` - Kernel optimizer
- `crates/adapteros-telemetry/src/metrics/critical_components.rs` - Kernel timers

---

### 1.4 Determinism Violations (Metal)

**Severity:** SEV2 (compliance) to SEV1 (if affecting production inference)

**Symptoms:**
```
[ERROR] Determinism violation detected: output hash mismatch
[ERROR] GPU buffer fingerprint mismatch for adapter_id
[WARN] determinism_violations_total counter increased
[ERROR] Cross-layer hash mismatch between adapters
```

**Prometheus Indicators:**
- `determinism_violations_total > 0`
- `gpu_buffer_integrity_violations_total > 0`
- `cross_layer_hash_mismatches_total > 0`

**Diagnostic Commands:**
```bash
# Step 1: Check violation counter
curl -s http://localhost:9090/metrics | grep determinism_violations

# Step 2: Get violation details from telemetry
sqlite3 var/aos-cp.sqlite3 "SELECT * FROM telemetry_events WHERE event_type = 'determinism.violation' ORDER BY created_at DESC LIMIT 10;"

# Step 3: Check GPU buffer fingerprints
aosctl adapter verify-gpu --adapter-id <adapter>

# Step 4: Compare against golden run
aosctl golden compare --run-id <golden-run-id> --current
```

**Resolution Steps:**

**Step 1: Identify Violation Type**
```bash
# Get violation breakdown
curl -s http://localhost:9090/metrics | grep -E "determinism_violations_total{violation_type"

# Categories:
# - "hash_mismatch": Output differs from expected
# - "seed_collision": HKDF seed space exhausted
# - "floating_point": FP rounding difference
# - "ordering": Non-deterministic operation order
```

**Step 2: Isolate Affected Adapter**
```bash
# List adapters with recent violations
sqlite3 var/aos-cp.sqlite3 "
  SELECT DISTINCT json_extract(event_data, '$.adapter_id') as adapter_id,
         COUNT(*) as violations
  FROM telemetry_events
  WHERE event_type = 'determinism.violation'
    AND created_at > datetime('now', '-1 hour')
  GROUP BY adapter_id
  ORDER BY violations DESC;
"

# Quarantine affected adapter
aosctl adapter quarantine --adapter-id <adapter>
```

**Step 3: Verify Metal Kernel Compliance**
```bash
# Check fast-math is disabled
grep "pragma clang fp" metal/aos_kernels.metal

# Expected: #pragma clang fp contract(off)

# Verify IEEE 754 compliance
aosctl verify-metallib --ieee754-check
```

**Step 4: Reset Adapter State**
```bash
# Unload and reload affected adapter
aosctl adapter unload <adapter-id>
sleep 5
aosctl adapter load <adapter-id>

# Verify with test inference
aosctl infer --adapter-id <adapter-id> --prompt "test" --verify-determinism
```

**Prevention:**
- Always use precompiled metallib (AOT, not JIT)
- Ensure `#pragma clang fp contract(off)` in all shaders
- Run determinism tests before deployment
- Store golden run outputs for comparison

**Related Files:**
- `metal/aos_kernels.metal:19` - FP pragma
- `crates/adapteros-lora-kernel-mtl/src/lib.rs:252-265` - Hash verification
- `crates/adapteros-telemetry/src/metrics/critical_components.rs` - Violation tracking

---

## 2. Hot-Swap Failures

### 2.1 Swap Timeout Handling

**Severity:** SEV2 (degraded)

**Symptoms:**
```
[ERROR] Hot-swap operation timed out after 30s
[WARN] Adapter swap incomplete: staged adapters not activated
[ERROR] Swap rollback triggered: timeout exceeded
[WARN] swap_rollback_count_total{reason="timeout"} increased
```

**Prometheus Indicators:**
- `hotswap_latency_ms > 30000` (p99)
- `swap_rollback_count_total{reason="timeout"}` increasing
- `adapter_swap_count{status="failed"}` increasing

**Diagnostic Commands:**
```bash
# Step 1: Check swap operation status
curl -s http://localhost:8080/healthz/hotswap | jq

# Step 2: List stuck adapters
aosctl adapter list --state staged --json | jq '.[] | {adapter_id, staged_at, vram_mb}'

# Step 3: Check swap latency histogram
curl -s http://localhost:9090/metrics | grep hotswap_latency_ms

# Step 4: Review swap logs
grep -E "swap|preload|activate" var/aos-cp.log | tail -50
```

**Resolution Steps:**

**Step 1: Clear Staged Adapters**
```bash
# List staged adapters blocking the swap
aosctl adapter list --state staged

# Clear staging area
aosctl adapter clear-staged

# Verify cleared
aosctl adapter list --state staged
```

**Step 2: Check for Resource Contention**
```bash
# Check concurrent swap operations
grep "swap operation" var/aos-cp.log | grep -c "in progress"

# If multiple concurrent swaps, wait for completion
sleep 60

# Check memory pressure during swap
curl -s http://localhost:8080/healthz/system-metrics | jq '.details.gpu_memory_pressure'
```

**Step 3: Force Rollback**
```bash
# Trigger manual rollback to last known-good state
aosctl adapter rollback --force

# Verify rollback successful
aosctl adapter list --state active
```

**Step 4: Retry Swap with Reduced Batch**
```bash
# Instead of swapping multiple adapters at once, swap one at a time
aosctl adapter swap --add <adapter-1> --timeout 60s
aosctl adapter swap --add <adapter-2> --timeout 60s
```

**Prevention:**
- Preload adapters before swap window
- Use smaller swap batches (1-2 adapters at a time)
- Set adequate timeout (60s for large adapters)
- Monitor swap latency and alert at p95 > 10s

**Related Files:**
- `crates/adapteros-lora-worker/src/adapter_hotswap.rs` - Swap implementation
- `docs/HOT_SWAP.md` - Protocol specification

---

### 2.2 Quarantined Adapter Recovery

**Severity:** SEV3 (degraded)

**Symptoms:**
```
[WARN] Adapter <id> quarantined: policy violation
[WARN] Adapter <id> quarantined: hash mismatch
[ERROR] Cannot load quarantined adapter
[INFO] adapter_state_transitions{to_state="quarantined"} increased
```

**Prometheus Indicators:**
- `adapter_state_transitions{to_state="quarantined"}` > 0
- `adapter_activation_percentage{adapter_id="..."}` = 0 (stuck)

**Diagnostic Commands:**
```bash
# Step 1: List quarantined adapters
aosctl adapter list --state quarantined --json | jq

# Step 2: Get quarantine reason
sqlite3 var/aos-cp.sqlite3 "
  SELECT adapter_id, quarantine_reason, quarantined_at
  FROM adapters
  WHERE current_state = 'quarantined'
  ORDER BY quarantined_at DESC;
"

# Step 3: Check related policy violations
sqlite3 var/aos-cp.sqlite3 "
  SELECT * FROM telemetry_events
  WHERE event_type LIKE 'policy.%'
    AND json_extract(event_data, '$.adapter_id') = '<adapter-id>'
  ORDER BY created_at DESC LIMIT 5;
"
```

**Resolution Steps:**

**Step 1: Diagnose Quarantine Reason**
```bash
# Common reasons:
# - hash_mismatch: Adapter contents changed since registration
# - policy_violation: Adapter violates runtime policy
# - corruption: GPU buffer integrity check failed

# Check specific reason
aosctl adapter info <adapter-id> | grep quarantine_reason
```

**Step 2: For Hash Mismatch - Re-register Adapter**
```bash
# Verify current adapter file hash
aosctl adapter verify-hash --adapter-id <adapter-id>

# If file changed, re-register
aosctl adapter unregister <adapter-id>
aosctl adapter register --path /path/to/adapter.aos
```

**Step 3: For Policy Violation - Review and Fix**
```bash
# Check which policy was violated
aosctl policy validate --adapter-id <adapter-id>

# Common fixes:
# - Update adapter ACL
# - Change adapter tier
# - Fix naming convention

# After fixing, release from quarantine
aosctl adapter release-quarantine <adapter-id>
```

**Step 4: For Corruption - Reload from Source**
```bash
# Unregister corrupted adapter
aosctl adapter unregister <adapter-id>

# Verify source file integrity
sha256sum /path/to/adapter.aos
aosctl verify-aos --path /path/to/adapter.aos

# Re-register
aosctl adapter register --path /path/to/adapter.aos
```

**Prevention:**
- Implement pre-flight validation before registration
- Store adapter source files with checksums
- Run periodic integrity checks
- Alert on quarantine events

**Related Files:**
- `crates/adapteros-db/src/repositories.rs` - Quarantine state management
- `crates/adapteros-policy/src/policy_packs.rs` - Policy enforcement

---

### 2.3 Checkpoint Corruption Recovery

**Severity:** SEV2 (data at risk)

**Symptoms:**
```
[ERROR] Checkpoint file corrupted: invalid header
[ERROR] Failed to restore from checkpoint: hash mismatch
[WARN] Rollback state unavailable: checkpoint corrupted
[ERROR] Recovery failed: cannot read checkpoint data
```

**Diagnostic Commands:**
```bash
# Step 1: List available checkpoints
ls -la var/checkpoints/

# Step 2: Verify checkpoint integrity
for f in var/checkpoints/*.ckpt; do
  echo "Checking $f..."
  aosctl checkpoint verify --path "$f" || echo "CORRUPTED: $f"
done

# Step 3: Check checkpoint metadata in database
sqlite3 var/aos-cp.sqlite3 "
  SELECT checkpoint_id, created_at, hash_b3, status
  FROM checkpoints
  ORDER BY created_at DESC LIMIT 10;
"
```

**Resolution Steps:**

**Step 1: Identify Last Valid Checkpoint**
```bash
# List checkpoints with verification status
aosctl checkpoint list --verify

# Find most recent valid checkpoint
aosctl checkpoint list --verify | grep "valid" | head -1
```

**Step 2: Restore from Valid Checkpoint**
```bash
# Stop service
pkill -SIGTERM aos-cp

# Restore from checkpoint
aosctl checkpoint restore --checkpoint-id <valid-checkpoint-id>

# Restart service
./scripts/start_server.sh
```

**Step 3: If No Valid Checkpoint - Cold Start**
```bash
# Stop service
pkill -SIGTERM aos-cp

# Remove corrupted checkpoints
rm var/checkpoints/*.corrupted

# Reset adapter states to unloaded
sqlite3 var/aos-cp.sqlite3 "UPDATE adapters SET current_state = 'unloaded';"

# Restart service (will reload adapters from source)
./scripts/start_server.sh

# Manually load required adapters
aosctl adapter load <adapter-id-1>
aosctl adapter load <adapter-id-2>
```

**Step 4: Create New Checkpoint**
```bash
# After system is stable, create new checkpoint
aosctl checkpoint create --name "recovery-$(date +%Y%m%d-%H%M%S)"

# Verify new checkpoint
aosctl checkpoint verify --latest
```

**Prevention:**
- Create checkpoints after each successful swap
- Verify checkpoints immediately after creation
- Maintain multiple checkpoint generations
- Store checkpoints on reliable storage

---

### 2.4 Memory Leak Detection (Hot-Swap Related)

**Severity:** SEV2 (service degradation over time)

**Symptoms:**
```
[WARN] Retired adapters not being cleaned up
[WARN] Refcount stuck at > 0 for retired adapter
[INFO] retired_stacks queue size: 50 (growing)
[WARN] GPU memory not released after unload
```

**Prometheus Indicators:**
- `gpu_memory_pressure` gradually increasing over time
- `hotswap_memory_freed_mb_total` not increasing after unloads
- `adapter_evictions_total` increasing but pressure not decreasing

**Diagnostic Commands:**
```bash
# Step 1: Check refcount status
aosctl adapter list --json | jq '.[] | select(.refcount > 0) | {adapter_id, refcount, state}'

# Step 2: Check retirement queue
aosctl status hotswap | grep retired_stacks

# Step 3: Track memory over time
watch -n 60 'curl -s http://localhost:8080/healthz/system-metrics | jq ".details.memory_used_mb"'

# Step 4: Check for stuck inference requests
aosctl status workers --show-requests
```

**Resolution Steps:**

**Step 1: Identify Stuck Refcounts**
```bash
# Find adapters with non-zero refcount in retired state
sqlite3 var/aos-cp.sqlite3 "
  SELECT adapter_id, current_state, refcount
  FROM adapters
  WHERE current_state IN ('retired', 'unloading')
    AND refcount > 0;
"
```

**Step 2: Check for Orphaned Inference Requests**
```bash
# List active inference requests
aosctl status workers --json | jq '.workers[].active_requests'

# If requests are stuck, cancel them
aosctl worker cancel-request --request-id <request-id>
```

**Step 3: Force Refcount Reset**
```bash
# WARNING: Only use if no active inference
aosctl adapter force-cleanup --adapter-id <stuck-adapter>

# This will:
# - Reset refcount to 0
# - Trigger retirement task wake-up
# - Free GPU memory
```

**Step 4: Restart Retirement Task**
```bash
# If retirement task is stuck
aosctl hotswap restart-retirement-task

# Verify cleanup
sleep 30
curl -s http://localhost:8080/healthz/system-metrics | jq '.details.gpu_memory_pressure'
```

**Prevention:**
- Use `InferenceGuard` pattern for automatic dec_ref on drop
- Set timeout on inference requests
- Monitor refcount trends
- Alert if retired queue size > 10

**Related Files:**
- `crates/adapteros-lora-worker/src/adapter_hotswap.rs` - Retirement task
- `docs/HOT_SWAP.md` - RCU retirement protocol

---

## 3. Deterministic Execution

### 3.1 Non-Reproducible Results Debugging

**Severity:** SEV2 (compliance/audit concern)

**Symptoms:**
```
[ERROR] Replay verification failed: output hash mismatch
[WARN] Inference output differs from golden run
[ERROR] Determinism check failed: expected <hash1>, got <hash2>
[INFO] Non-deterministic operation detected in task execution
```

**Prometheus Indicators:**
- `determinism_violations_total{violation_type="hash_mismatch"}` > 0

**Diagnostic Commands:**
```bash
# Step 1: Compare current output with golden run
aosctl golden compare --run-id <golden-run-id> --verbose

# Step 2: Get execution event log
aosctl replay export-events --session-id <session-id> --format json > events.json

# Step 3: Check task execution order
jq '[.events[] | select(.event_type == "TaskSpawned" or .event_type == "TaskCompleted")] | sort_by(.tick)' events.json

# Step 4: Verify seed chain
aosctl verify-seed-chain --session-id <session-id>
```

**Resolution Steps:**

**Step 1: Identify Divergence Point**
```bash
# Compare event logs between golden and current run
aosctl replay diff --golden <golden-events.json> --current events.json

# Look for first divergence
aosctl replay diff --golden <golden-events.json> --current events.json | head -20
```

**Step 2: Check for Non-Deterministic Operations**
```bash
# Search for wall-clock usage
grep -r "SystemTime::now\|Instant::now" crates/ --include="*.rs" | grep -v "test"

# Search for random without seed
grep -r "thread_rng\|OsRng" crates/ --include="*.rs" | grep -v "test"

# Search for HashMap iteration (non-deterministic order)
grep -r "for.*in.*HashMap" crates/ --include="*.rs"
```

**Step 3: Verify HKDF Seed Derivation**
```bash
# Check seed used for session
sqlite3 var/aos-cp.sqlite3 "
  SELECT session_id, global_seed, manifest_hash
  FROM inference_sessions
  WHERE session_id = '<session-id>';
"

# Verify seed derivation is deterministic
aosctl verify-hkdf --seed <global-seed> --domain "executor"
```

**Step 4: Re-run with Full Event Logging**
```bash
# Enable verbose event logging
export AOS_DETERMINISTIC_TRACE=1

# Run inference
aosctl infer --prompt "test" --adapter-id <adapter> --trace-events

# Compare trace output
diff expected_trace.json actual_trace.json
```

**Prevention:**
- Use `DeterministicExecutor` for all task execution
- Derive all randomness from HKDF (never use `thread_rng`)
- Use `BTreeMap` instead of `HashMap` where iteration order matters
- Run determinism tests in CI

**Related Files:**
- `crates/adapteros-deterministic-exec/src/lib.rs` - Executor
- `crates/adapteros-core/src/hash.rs` - HKDF seed derivation
- `docs/DETERMINISM-AUDIT.md` - Full audit findings

---

### 3.2 Tick Ledger Synchronization Issues

**Severity:** SEV3 (degraded) to SEV2 (if affecting multi-agent coordination)

**Symptoms:**
```
[ERROR] Tick ledger out of sync: local=1000, remote=1050
[WARN] Task execution skipped: tick mismatch
[ERROR] Federation consensus failed: tick divergence
[WARN] Replay failed: tick sequence gap detected
```

**Diagnostic Commands:**
```bash
# Step 1: Check local tick counter
aosctl status executor | grep tick

# Step 2: Check tick ledger entries
sqlite3 var/aos-cp.sqlite3 "
  SELECT tick_id, tick_value, event_hash, created_at
  FROM tick_ledger
  ORDER BY tick_value DESC LIMIT 20;
"

# Step 3: Check for gaps in tick sequence
sqlite3 var/aos-cp.sqlite3 "
  SELECT t1.tick_value as gap_start, t2.tick_value as gap_end
  FROM tick_ledger t1
  LEFT JOIN tick_ledger t2 ON t1.tick_value + 1 = t2.tick_value
  WHERE t2.tick_value IS NULL
  ORDER BY t1.tick_value DESC LIMIT 10;
"

# Step 4: Check federation sync status (if multi-node)
aosctl federation status --show-ticks
```

**Resolution Steps:**

**Step 1: Reset Local Tick Counter**
```bash
# Stop accepting new tasks
aosctl executor pause

# Get highest tick from database
sqlite3 var/aos-cp.sqlite3 "SELECT MAX(tick_value) FROM tick_ledger;"

# Reset executor to latest tick
aosctl executor reset-tick --value <max-tick>

# Resume execution
aosctl executor resume
```

**Step 2: Rebuild Tick Sequence**
```bash
# Export tick ledger
sqlite3 var/aos-cp.sqlite3 ".mode csv" ".headers on" "SELECT * FROM tick_ledger ORDER BY tick_value;" > tick_backup.csv

# Clear and rebuild
aosctl tick-ledger rebuild --from-events

# Verify sequence integrity
aosctl tick-ledger verify
```

**Step 3: Sync with Federation (Multi-Node)**
```bash
# Check federation consensus
aosctl federation tick-sync --check

# If out of sync, request consensus update
aosctl federation tick-sync --request-update

# Wait for convergence
sleep 30
aosctl federation status --show-ticks
```

**Prevention:**
- Log all tick advances with event hashes
- Implement tick checkpoint every N ticks
- Monitor tick advance rate for anomalies
- Alert on tick gaps > 10

**Related Files:**
- `crates/adapteros-deterministic-exec/src/lib.rs` - Tick counter
- `migrations/0035_tick_ledger_federation.sql` - Ledger schema

---

### 3.3 Task Queue Overflow

**Severity:** SEV2 (service degraded)

**Symptoms:**
```
[ERROR] Task queue full: cannot spawn new task
[WARN] Task queue depth exceeds threshold: 1000
[ERROR] Executor backpressure: new tasks rejected
[WARN] Memory pressure from pending tasks
```

**Prometheus Indicators:**
- Task queue depth consistently > 500
- Task completion rate < spawn rate
- Memory usage increasing with queue size

**Diagnostic Commands:**
```bash
# Step 1: Check queue depth
aosctl status executor | grep queue_depth

# Step 2: Check task completion rate
curl -s http://localhost:9090/metrics | grep task_completed_total

# Step 3: Check for slow tasks
sqlite3 var/aos-cp.sqlite3 "
  SELECT task_description, AVG(duration_ms) as avg_duration
  FROM executor_events
  WHERE event_type = 'TaskCompleted'
    AND created_at > datetime('now', '-1 hour')
  GROUP BY task_description
  ORDER BY avg_duration DESC LIMIT 10;
"

# Step 4: Check for stuck tasks
aosctl status executor --show-pending | grep "pending_seconds > 60"
```

**Resolution Steps:**

**Step 1: Drain Pending Tasks**
```bash
# Pause new task acceptance
aosctl executor pause

# Wait for queue to drain
while [ $(aosctl status executor | grep queue_depth | awk '{print $2}') -gt 10 ]; do
  echo "Queue depth: $(aosctl status executor | grep queue_depth)"
  sleep 5
done

# Resume execution
aosctl executor resume
```

**Step 2: Increase Processing Capacity**
```bash
# If using deterministic executor, increase tick rate
aosctl executor config --tick-rate-hz 100  # Default is 60

# Note: Cannot add parallelism (breaks determinism)
```

**Step 3: Implement Request Throttling**
```bash
# Reduce incoming request rate
aosctl rate-limit set --max-rps 50

# Or reject low-priority requests
aosctl rate-limit set --reject-tier ephemeral
```

**Step 4: Cancel Timeout Tasks**
```bash
# Cancel tasks exceeding timeout
aosctl executor cancel-timeout --older-than 300s

# Force cancel stuck tasks (last resort)
aosctl executor force-cancel --task-id <stuck-task>
```

**Prevention:**
- Set queue depth limit (500-1000)
- Implement request throttling at API layer
- Monitor queue depth trends
- Alert at queue depth > 200

---

### 3.4 Seed Collision Investigation

**Severity:** SEV2 (determinism compromised)

**Symptoms:**
```
[ERROR] HKDF seed collision detected
[WARN] adapter_id_collisions_total counter increased
[ERROR] Two adapters mapping to same u16 index
[WARN] Seed exhaustion risk: high collision rate
```

**Prometheus Indicators:**
- `adapter_id_collisions_total > 0`
- `adapter_id_mapping_errors_total > 0`

**Diagnostic Commands:**
```bash
# Step 1: Check collision counter
curl -s http://localhost:9090/metrics | grep adapter_id_collisions

# Step 2: Get collision details
sqlite3 var/aos-cp.sqlite3 "
  SELECT * FROM telemetry_events
  WHERE event_type = 'adapter.id_collision'
  ORDER BY created_at DESC LIMIT 10;
"

# Step 3: Analyze adapter ID distribution
sqlite3 var/aos-cp.sqlite3 "
  SELECT adapter_id, hash_b3
  FROM adapters
  WHERE active = 1;
" | aosctl analyze-hash-distribution

# Step 4: Check u16 mapping density
aosctl router analyze-mapping --show-density
```

**Resolution Steps:**

**Step 1: Identify Colliding Adapters**
```bash
# Get the two adapters with same u16 mapping
sqlite3 var/aos-cp.sqlite3 "
  SELECT a1.adapter_id, a2.adapter_id,
         substr(a1.hash_b3, 1, 4) as shared_prefix
  FROM adapters a1, adapters a2
  WHERE a1.adapter_id < a2.adapter_id
    AND substr(a1.hash_b3, 1, 4) = substr(a2.hash_b3, 1, 4);
"
```

**Step 2: Re-hash Colliding Adapter**
```bash
# Collision is based on first 2 bytes of BLAKE3 hash
# Need to change adapter ID to get different hash prefix

# Rename adapter (changes hash)
aosctl adapter rename <adapter-id> <new-adapter-id>

# Or re-register with salt
aosctl adapter unregister <adapter-id>
aosctl adapter register --path <adapter.aos> --id-salt "v2"
```

**Step 3: Expand Mapping Space**
```bash
# If approaching u16 limit (65536 adapters), need architecture change
# Check current adapter count
sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*) FROM adapters;"

# If > 50000, consider:
# - Archiving unused adapters
# - Using hierarchical mapping
# - Upgrading to u32 mapping (requires code change)
```

**Prevention:**
- Monitor collision rate
- Alert when adapter count > 50000
- Use semantic naming to distribute hash entropy
- Consider birthday paradox: collisions likely at ~256 adapters for 16-bit space

**Related Files:**
- `crates/adapteros-lora-router/src/lib.rs` - u16 mapping
- `crates/adapteros-telemetry/src/metrics/critical_components.rs` - Collision tracking

---

## 4. Content Addressing

### 4.1 Hash Mismatch Investigation

**Severity:** SEV2 (data integrity concern)

**Symptoms:**
```
[ERROR] Content hash mismatch: expected <hash1>, computed <hash2>
[ERROR] Adapter integrity check failed
[WARN] File modified since registration
[ERROR] .aos archive hash verification failed
```

**Diagnostic Commands:**
```bash
# Step 1: Get expected vs actual hash
aosctl adapter verify-hash --adapter-id <adapter-id>

# Step 2: Re-compute hash from file
aosctl hash-file --path /path/to/adapter.aos

# Step 3: Check file modification time
stat /path/to/adapter.aos

# Step 4: Compare against database record
sqlite3 var/aos-cp.sqlite3 "
  SELECT adapter_id, hash_b3, created_at, file_path
  FROM adapters
  WHERE adapter_id = '<adapter-id>';
"
```

**Resolution Steps:**

**Step 1: Determine Source of Mismatch**
```bash
# Check if file was modified
ls -la /path/to/adapter.aos
# Compare mtime with registration time

# Check for symlink issues
file /path/to/adapter.aos
readlink -f /path/to/adapter.aos

# Check for encoding issues (line endings, BOM)
file /path/to/adapter.aos
hexdump -C /path/to/adapter.aos | head -5
```

**Step 2: If File Was Legitimately Updated**
```bash
# Unregister old version
aosctl adapter unregister <adapter-id>

# Register new version
aosctl adapter register --path /path/to/adapter.aos

# Update any references
aosctl adapter-stack update --stack-id <stack> --replace <old-id> <new-id>
```

**Step 3: If File Was Corrupted**
```bash
# Check for backup
ls -la /path/to/adapter.aos.bak

# Restore from backup if available
cp /path/to/adapter.aos.bak /path/to/adapter.aos

# Verify restored file
aosctl verify-aos --path /path/to/adapter.aos

# Re-register
aosctl adapter unregister <adapter-id>
aosctl adapter register --path /path/to/adapter.aos
```

**Step 4: If No Backup Available**
```bash
# Check if we have cached weights in GPU
aosctl adapter dump-weights --adapter-id <adapter-id> --output /tmp/recovered.weights

# Reconstruct .aos from weights (if manifest available)
aosctl aos-pack --weights /tmp/recovered.weights --manifest <manifest.json> --output /path/to/adapter.aos
```

**Prevention:**
- Use read-only file permissions after registration
- Store adapters on checksummed filesystem (ZFS, Btrfs)
- Implement periodic integrity verification
- Maintain backup copies of all adapters

**Related Files:**
- `crates/adapteros-core/src/hash.rs` - B3Hash implementation
- `crates/adapteros-artifacts/src/lib.rs` - CAS implementation

---

### 4.2 Integrity Verification Failures

**Severity:** SEV2 (cannot trust adapter contents)

**Symptoms:**
```
[ERROR] GPU buffer integrity verification failed
[ERROR] Fingerprint mismatch after load
[WARN] gpu_buffer_integrity_violations_total increased
[ERROR] Weight corruption detected during inference
```

**Prometheus Indicators:**
- `gpu_buffer_integrity_violations_total > 0`
- `gpu_buffer_corruption_detections_total > 0`

**Diagnostic Commands:**
```bash
# Step 1: Check integrity violation counter
curl -s http://localhost:9090/metrics | grep gpu_buffer_integrity

# Step 2: Get GPU buffer fingerprint
aosctl adapter gpu-fingerprint --adapter-id <adapter-id>

# Step 3: Compare against expected fingerprint
sqlite3 var/aos-cp.sqlite3 "
  SELECT adapter_id, gpu_fingerprint, fingerprint_computed_at
  FROM adapters
  WHERE adapter_id = '<adapter-id>';
"

# Step 4: Run full GPU memory scan
aosctl gpu verify-all-buffers
```

**Resolution Steps:**

**Step 1: Unload and Reload Affected Adapter**
```bash
# Unload from GPU
aosctl adapter unload <adapter-id>

# Wait for GPU memory to clear
sleep 10

# Reload
aosctl adapter load <adapter-id>

# Verify fingerprint
aosctl adapter gpu-fingerprint --adapter-id <adapter-id>
```

**Step 2: Check for GPU Memory Corruption**
```bash
# Run GPU memory test (requires service pause)
aosctl gpu memory-test --full

# Check for hardware errors
system_profiler SPDisplaysDataType | grep -i error
dmesg | grep -i gpu
```

**Step 3: Check for Thermal Issues**
```bash
# GPU throttling can cause bit errors
sudo powermetrics --samplers smc -n 1 | grep -i temp

# If temperature > 95C, reduce load
aosctl adapter unload-all
sleep 60  # Allow cooling
```

**Step 4: Re-register Adapter**
```bash
# If corruption persists, re-register from source
aosctl adapter unregister <adapter-id>
aosctl adapter register --path /path/to/adapter.aos --verify-gpu
```

**Prevention:**
- Run GPU fingerprint verification after every load
- Monitor GPU temperature
- Use ECC memory if available
- Alert on any integrity violation

---

### 4.3 HKDF Seed Exhaustion

**Severity:** SEV3 (potential future issue)

**Symptoms:**
```
[WARN] HKDF context counter approaching limit
[WARN] High seed derivation rate detected
[INFO] Seed context exhaustion risk: counter at 2^30
```

**Diagnostic Commands:**
```bash
# Step 1: Check seed derivation counter
aosctl status hkdf | grep counter

# Step 2: Check derivation rate
sqlite3 var/aos-cp.sqlite3 "
  SELECT COUNT(*) / 3600.0 as derivations_per_hour
  FROM hkdf_derivations
  WHERE created_at > datetime('now', '-1 hour');
"

# Step 3: Review derivation domains
sqlite3 var/aos-cp.sqlite3 "
  SELECT domain, COUNT(*) as count
  FROM hkdf_derivations
  GROUP BY domain
  ORDER BY count DESC;
"
```

**Resolution Steps:**

**Step 1: Review Derivation Patterns**
```bash
# Check which domains are deriving most seeds
aosctl hkdf analyze-domains

# Common domains:
# - "executor": Task scheduling
# - "router": Adapter selection
# - "dropout": Training dropout masks
# - "sampling": Token sampling
```

**Step 2: Optimize High-Volume Derivations**
```bash
# If "sampling" domain is high volume, consider caching
# derived seeds per session

# For "dropout", use fixed mask per training batch
# instead of per-token derivation
```

**Step 3: Rotate Master Seed**
```bash
# If counter > 2^31, rotate master seed
aosctl hkdf rotate-master --new-seed-source entropy

# This will:
# - Generate new master seed
# - Reset all counters
# - Invalidate cached derived seeds
```

**Prevention:**
- Cache derived seeds where possible
- Use hierarchical derivation (session seed → operation seeds)
- Monitor derivation counter
- Alert at counter > 2^28

---

### 4.4 Performance Degradation (Content Addressing)

**Severity:** SEV3 (degraded)

**Symptoms:**
```
[WARN] Hash computation latency increased
[INFO] B3Hash::hash_file taking > 100ms for small files
[WARN] Content addressing bottleneck detected
```

**Diagnostic Commands:**
```bash
# Step 1: Benchmark hash performance
aosctl benchmark-hash --file /path/to/adapter.aos

# Step 2: Check I/O performance
time dd if=/path/to/adapter.aos of=/dev/null bs=1M

# Step 3: Check for memory-mapped file issues
lsof | grep adapter.aos

# Step 4: Check disk I/O stats
iostat -d 1 5
```

**Resolution Steps:**

**Step 1: Identify Bottleneck**
```bash
# Hash computation vs I/O
aosctl benchmark-hash --file /path/to/adapter.aos --breakdown

# If I/O bound:
# - Check disk health
# - Move to faster storage

# If CPU bound:
# - Check for other CPU-intensive processes
# - Consider hardware acceleration
```

**Step 2: Use Memory-Mapped Hashing**
```bash
# For large files, memory-mapped I/O is faster
# This should be automatic, but verify:
aosctl config get --key hash.use_mmap
# Should be: true

# If not set:
aosctl config set --key hash.use_mmap --value true
```

**Step 3: Enable Hash Caching**
```bash
# Cache computed hashes for unchanged files
aosctl config set --key hash.cache_enabled --value true
aosctl config set --key hash.cache_ttl_secs --value 3600

# Clear cache if needed
aosctl cache clear --type hash
```

**Step 4: Parallelize Multi-File Operations**
```bash
# When verifying multiple adapters, use parallel verification
aosctl adapter verify-all --parallel 8
```

**Prevention:**
- Use SSD storage for adapter files
- Enable hash caching
- Precompute hashes during quiet periods
- Store hash in database to avoid recomputation

---

## Quick Reference

### Emergency Commands

```bash
# Stop all operations immediately
pkill -SIGTERM aos-cp

# Force stop (last resort)
pkill -SIGKILL aos-cp

# Clear all adapter state and restart
aosctl adapter unload-all
./scripts/start_server.sh

# Force garbage collection
aosctl maintenance gc --force --aggressive

# Create diagnostic bundle
./scripts/collect-diagnostic-bundle.sh
```

### Key Metrics to Monitor

| Metric | Warning | Critical | Runbook Section |
|--------|---------|----------|-----------------|
| `gpu_memory_pressure` | > 0.75 | > 0.85 | 1.1 |
| `metal_kernel_panic_count_total` | > 0 | > 5 | 1.2 |
| `determinism_violations_total` | > 0 | > 10 | 1.4, 3.1 |
| `hotswap_latency_ms` p99 | > 10000 | > 30000 | 2.1 |
| `swap_rollback_count_total` | > 0 | > 5 | 2.1 |
| `adapter_id_collisions_total` | > 0 | > 10 | 3.4 |
| `gpu_buffer_integrity_violations_total` | > 0 | > 0 | 4.2 |

### Related Documentation

- [ESCALATION.md](./ESCALATION.md) - When to escalate
- [MEMORY-PRESSURE.md](./MEMORY-PRESSURE.md) - Memory management
- [../PRODUCTION_MONITORING.md](../PRODUCTION_MONITORING.md) - Full metrics reference
- [../HOT_SWAP.md](../HOT_SWAP.md) - Hot-swap protocol
- [../DETERMINISM-AUDIT.md](../DETERMINISM-AUDIT.md) - Determinism analysis

---

**Maintained by:** Operations Team
**Copyright:** 2025 JKCA / James KC Auchterlonie. All rights reserved.
