# Log Analysis

What to look for in logs and log-based troubleshooting.

## Log Locations

```
var/aos-cp.log              - Main control plane log
var/aos-cp-debug.log        - Debug-level logs (if enabled)
var/telemetry/              - Telemetry event logs (NDJSON)
/var/log/aos/               - System logs (production)
```

## Log Levels

```
ERROR - Critical failures requiring immediate attention
WARN  - Warnings, degraded operation, non-critical issues
INFO  - Normal operational messages
DEBUG - Detailed diagnostic information
TRACE - Very verbose tracing (development only)
```

**Default level:** `INFO`

**Change level:**
```bash
# Via environment variable
RUST_LOG=debug cargo run --bin aos-cp

# Or in config
export RUST_LOG="info,aos_cp=debug,adapteros_db=debug"
```

## Key Log Patterns

### 1. Startup Sequence (Normal)

```log
[INFO] Loading configuration from configs/cp.toml
[INFO] Connecting to database: var/aos-cp.sqlite3
[INFO] Initializing deterministic executor
[INFO] Loaded and validated manifest for executor seeding
[INFO] Running database migrations...
[INFO] ✓ All 20 migration signatures verified
[INFO] ✓ No environment drift detected
[INFO] PID lock acquired: var/aos-cp.pid
[INFO] Starting alert watcher
[INFO] Policy hash watcher started (60s interval)
[INFO] UDS metrics exporter started on var/run/metrics.sock
[INFO] Git subsystem disabled in configuration
[INFO] Status writer started (5s interval)
[INFO] TTL cleanup task started (5 minute interval)
[INFO] Heartbeat recovery task started (5 minute interval, 300s timeout)
[INFO] Starting control plane on 127.0.0.1:8080
[INFO] UI available at http://127.0.0.1:8080/
[INFO] API available at http://127.0.0.1:8080/api/
```

**What to check:**
- All components start successfully
- No errors during initialization
- Port binding succeeds
- Background tasks start

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:220-964` - Startup sequence

### 2. Database Operations

**Normal patterns:**
```log
[INFO] Running database migrations...
[INFO] ✓ All 20 migration signatures verified
[INFO] Database at migration version 20 (add_audit_log) - expected version 20
[INFO] ✓ Schema version verified: 20
```

**Problem patterns:**
```log
[ERROR] Migration signature verification failed
[ERROR] ❌ SCHEMA VERSION MISMATCH: Database at version 18, expected 20
[WARN] Failed to query migration version: database is locked
[ERROR] Database connection failed: timeout
```

**Actions:**
- Schema mismatch → See [Database Failures](./database-failures.md#2-schema-version-mismatch)
- Signature failed → See [Database Failures](./database-failures.md#1-migration-signature-verification-failed)
- Database locked → See [Database Failures](./database-failures.md#3-database-locked-errors)

### 3. Memory Pressure

**Warning patterns:**
```log
[WARN] Memory pressure elevated: 18% headroom
[WARN] Starting adapter eviction due to memory pressure
[INFO] Evicting adapter ephemeral_adapter_001 (LRU)
```

**Critical patterns:**
```log
[ERROR] Critical memory pressure: 12% headroom
[ERROR] System under pressure (level: Critical)
[ERROR] Inference request blocked: insufficient memory
```

**Actions:**
- Pressure warning → Monitor, consider cleanup
- Critical pressure → See [Memory Pressure](./memory-pressure.md#1-critical-memory-pressure-during-inference)

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/lib.rs` - Pressure monitoring

### 4. Adapter Lifecycle

**Normal patterns:**
```log
[INFO] Adapter transition: adapter_123 cold -> loading
[INFO] Loading adapter adapter_123 (rank: 16)
[INFO] Adapter transition: adapter_123 loading -> warm
[INFO] Adapter transition: adapter_123 warm -> hot
```

**Problem patterns:**
```log
[WARN] Adapter adapter_123 stuck in loading state (5 minutes)
[ERROR] Failed to load adapter: file not found
[ERROR] Adapter validation failed: hash mismatch
[INFO] Adapter transition: adapter_123 loading -> failed
```

**Actions:**
- Stuck in loading → See [Health Check Failures](./health-check-failures.md#2-loader-component-degradedunhealthy)
- Hash mismatch → Verify adapter integrity
- Load failed → Check adapter files exist

### 5. Router Decisions

**Normal patterns:**
```log
[INFO] Router decision: adapter_prod_001 (confidence: 0.87)
[DEBUG] Router evaluated 5 candidates in 12ms
[INFO] Routing telemetry: stack=stack-001, adapter=adapter_prod_001
```

**Problem patterns:**
```log
[WARN] No suitable adapter found for request
[WARN] All adapters below confidence threshold (max: 0.45)
[ERROR] Router decision timeout after 5000ms
[WARN] High queue depth: 150 requests
```

**Actions:**
- No adapter → Check adapter registration
- Low confidence → Review router weights
- High queue → Reduce load or scale

### 6. Cleanup Operations

**Normal patterns:**
```log
[INFO] TTL cleanup: found 3 expired adapters
[INFO] Deleting expired adapter: temp_adapter_123 (expired 2 hours ago)
[INFO] Cleaned up 3 expired pins
[INFO] GC collected 5 orphaned adapters
```

**Problem patterns:**
```log
[WARN] Failed to delete expired adapter: constraint violation
[ERROR] GC failed: database locked
[WARN] Orphaned adapter cannot be cleaned: file in use
```

**Actions:**
- Constraint violation → Check foreign keys
- Database locked → Retry or restart
- File in use → Identify holding process

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:843-895` - TTL cleanup task

### 7. Health Check Degradation

**Warning patterns:**
```log
[WARN] Component degraded: router (high queue depth: 120)
[WARN] Component degraded: db (query latency: 150ms)
[WARN] Component degraded: system-metrics (memory pressure: high)
```

**Critical patterns:**
```log
[ERROR] Component unhealthy: db (connection failed)
[ERROR] Component unhealthy: loader (3 adapters stuck)
[ERROR] Overall system status: unhealthy
```

**Actions:**
- See [Health Check Failures](./health-check-failures.md) for component-specific procedures

### 8. Security Events

**Normal patterns:**
```log
[INFO] PF security check passed
[INFO] Environment fingerprint verified
[INFO] Executor bootstrap event logged to audit trail
[INFO] Audit event: user_login (user: admin, status: success)
```

**Problem patterns:**
```log
[ERROR] PF security check failed: egress not blocked
[WARN] Environment drift detected: cpu_model changed
[ERROR] Audit event: unauthorized_access (user: unknown, ip: 10.0.0.5)
[ERROR] Policy violation detected: quarantine activated
```

**Actions:**
- PF check failed → Fix firewall rules
- Drift detected → Review changes
- Unauthorized access → Security investigation
- Quarantine → Review policy violation

## Log Analysis Commands

### Search for Errors
```bash
# Recent errors
grep ERROR var/aos-cp.log | tail -20

# Error frequency
grep ERROR var/aos-cp.log | wc -l

# Errors by type
grep ERROR var/aos-cp.log | cut -d']' -f2 | sort | uniq -c | sort -rn
```

### Memory Tracking
```bash
# Memory pressure events
grep -E "memory|pressure" var/aos-cp.log | tail -20

# Adapter evictions
grep "Evicting adapter" var/aos-cp.log | tail -10

# Memory trends
grep "memory_used_mb" var/aos-cp.log | awk '{print $NF}' | tail -20
```

### Adapter Lifecycle
```bash
# Adapter transitions
grep "Adapter transition" var/aos-cp.log | tail -20

# Failed loads
grep "Failed to load adapter" var/aos-cp.log

# Stuck adapters
grep "stuck in loading" var/aos-cp.log
```

### Router Activity
```bash
# Router decisions
grep "Router decision" var/aos-cp.log | tail -20

# Queue depth
grep "queue depth" var/aos-cp.log | tail -10

# No adapter found
grep "No suitable adapter" var/aos-cp.log
```

### Database Operations
```bash
# Migrations
grep -i migration var/aos-cp.log | tail -10

# Schema version
grep "Schema version" var/aos-cp.log

# Database locks
grep "database is locked" var/aos-cp.log

# Query latency
grep "query_latency_ms" var/aos-cp.log | tail -10
```

### Cleanup Operations
```bash
# TTL cleanup
grep "TTL cleanup" var/aos-cp.log | tail -10

# GC operations
grep -E "GC|garbage collection" var/aos-cp.log | tail -10

# Expired adapters
grep "expired adapter" var/aos-cp.log | tail -10
```

## Advanced Log Analysis

### Log Correlation
```bash
# Extract timestamp and correlate events
grep "adapter_123" var/aos-cp.log | \
  awk '{print $1, $2, $0}' | \
  sort

# Find events within time window
# (Assuming ISO 8601 timestamps)
grep "2025-11-19T10:30" var/aos-cp.log
```

### Performance Analysis
```bash
# Extract latency metrics
grep "_ms" var/aos-cp.log | \
  grep -oE '[0-9]+ms' | \
  sed 's/ms//' | \
  awk '{sum+=$1; n++} END {print "Avg:", sum/n, "ms"}'

# Find slow operations
grep -E "[0-9]{3,}ms" var/aos-cp.log | tail -20
```

### Error Patterns
```bash
# Group errors by hour
grep ERROR var/aos-cp.log | \
  cut -d'T' -f2 | \
  cut -d':' -f1 | \
  sort | uniq -c

# Find recurring errors
grep ERROR var/aos-cp.log | \
  cut -d']' -f2- | \
  sort | uniq -c | \
  sort -rn | head -10
```

## Log Monitoring Setup

### Tail with Filter
```bash
# Follow log with grep
tail -f var/aos-cp.log | grep ERROR

# Follow with multiple patterns
tail -f var/aos-cp.log | grep -E "ERROR|WARN|adapter_123"

# Follow with color
tail -f var/aos-cp.log | grep --color=always -E "ERROR|WARN|$"
```

### Watch for Patterns
```bash
# Watch for errors every 5 seconds
watch -n 5 'tail -20 var/aos-cp.log | grep ERROR'

# Monitor memory pressure
watch -n 10 'grep memory_used_mb var/aos-cp.log | tail -5'
```

### Log Rotation
```bash
# Manual rotation
mv var/aos-cp.log var/aos-cp.log.$(date +%Y%m%d)
touch var/aos-cp.log
pkill -SIGHUP aos-cp  # Reopen log file

# Or use logrotate
# /etc/logrotate.d/aos
/path/to/aos/var/aos-cp.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    postrotate
        pkill -SIGHUP aos-cp
    endscript
}
```

## Telemetry Event Logs

Telemetry stored in NDJSON format:
```bash
# View recent telemetry
tail var/telemetry/bundle_latest.ndjson | jq

# Filter by event type
jq 'select(.event_type == "router.decision")' var/telemetry/bundle_latest.ndjson

# Count events by type
jq -r '.event_type' var/telemetry/bundle_latest.ndjson | sort | uniq -c
```

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-telemetry/src/lib.rs` - Telemetry writer

## Log Verbosity Levels

### Production (INFO)
```bash
RUST_LOG=info cargo run --bin aos-cp
```
- Startup/shutdown events
- Component status changes
- Errors and warnings
- Key operational metrics

### Development (DEBUG)
```bash
RUST_LOG=debug cargo run --bin aos-cp
```
- All INFO events
- Detailed state transitions
- Query execution details
- Performance metrics

### Troubleshooting (TRACE)
```bash
RUST_LOG=trace cargo run --bin aos-cp
```
- All DEBUG events
- Function entry/exit
- Variable values
- Low-level operations

## Related Runbooks

- [Health Check Failures](./health-check-failures.md)
- [Database Failures](./database-failures.md)
- [Memory Pressure](./memory-pressure.md)
- [Metrics Review](./metrics-review.md)

## Escalation Criteria

Escalate if:
- Unknown error patterns
- Repeated failures without resolution
- Security-related errors
- See [Escalation Guide](./escalation.md)
