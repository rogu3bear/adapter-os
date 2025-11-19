# Health Check Failures

Component degraded/unhealthy status and health endpoint troubleshooting.

## Health Check Overview

AdapterOS provides component-level health checks via HTTP endpoints:

```
GET /healthz/all            - All components aggregate
GET /healthz/router         - Router component
GET /healthz/loader         - Adapter loader component
GET /healthz/kernel         - Inference kernel component
GET /healthz/db             - Database component
GET /healthz/telemetry      - Telemetry component
GET /healthz/system-metrics - System metrics component
```

**Status Levels:**
- `healthy`: Component operating normally
- `degraded`: Component operational but with issues
- `unhealthy`: Component not functioning (503 status)

## Symptoms

- Health endpoint returns `degraded` or `unhealthy` status
- `aosctl doctor` reports failures
- Component-specific warning messages
- API returning 503 Service Unavailable

## Component-Specific Failures

### 1. Router Component Degraded/Unhealthy

**Symptoms:**
```json
{
  "component": "router",
  "status": "degraded",
  "message": "High queue depth: 150"
}
```

**Root Causes:**
- High request queue depth (> 100)
- No routing decisions made yet
- Router overhead too high
- Adapter selection failures

**Diagnostic Commands:**
```bash
# Check router health
curl http://localhost:8080/healthz/router | jq

# Check routing telemetry
aosctl telemetry-list --event-type router.decision --limit 20

# Check queue depth
curl http://localhost:8080/healthz/router | jq '.details.queue_depth'

# Check metrics
aosctl status metrics
```

**Fix Procedure:**

**If "Router has not processed any requests yet":**
```bash
# Normal on first startup
# Send test inference to initialize
aosctl infer --prompt "test" --max-tokens 16

# Verify router now healthy
curl http://localhost:8080/healthz/router
```

**If "High queue depth":**
```bash
# Step 1: Check queue depth
DEPTH=$(curl -s http://localhost:8080/healthz/router | jq '.details.queue_depth')
echo "Current queue depth: $DEPTH"

# Step 2: Reduce incoming load
# Stop sending new requests temporarily

# Step 3: Monitor queue draining
watch -n 2 'curl -s http://localhost:8080/healthz/router | jq ".details.queue_depth"'

# Step 4: If queue not draining, check for stuck adapters
aosctl adapter list --state loading

# Step 5: Unload stuck adapters
aosctl adapter unload <adapter-id>
```

**Prevention:**
- Implement request rate limiting
- Monitor queue depth trends
- Set queue depth alerts (threshold: 50)
- Scale router if sustained high load

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:80-125` - Router health check
- `/Users/star/Dev/aos/crates/adapteros-lora-router/src/lib.rs` - Router implementation

### 2. Loader Component Degraded/Unhealthy

**Symptoms:**
```json
{
  "component": "loader",
  "status": "degraded",
  "message": "3 adapter(s) stuck in loading state"
}
```

**Root Causes:**
- Adapters stuck in `loading` state
- Adapter loading failures
- Missing adapter files
- Lifecycle manager not responding

**Diagnostic Commands:**
```bash
# Check loader health
curl http://localhost:8080/healthz/loader | jq

# List adapters by state
aosctl adapter list --state loading
aosctl adapter list --state failed

# Check lifecycle manager
aosctl status lifecycle
```

**Fix Procedure:**

**Step 1: Identify Stuck Adapters**
```bash
# Find adapters stuck in loading
aosctl adapter list --json | jq '.[] | select(.current_state == "loading")'

# Check how long they've been loading
sqlite3 var/aos-cp.sqlite3 <<EOF
SELECT adapter_id, name, current_state,
       datetime(updated_at, 'unixepoch') as last_update
FROM adapters
WHERE current_state = 'loading';
EOF
```

**Step 2: Attempt to Transition**
```bash
# Try to load stuck adapters
aosctl adapter load <adapter-id>

# If that fails, transition to failed state
aosctl adapter transition <adapter-id> --to-state failed

# Then retry load
aosctl adapter load <adapter-id>
```

**Step 3: Force Recovery if Needed**
```bash
# Update database directly (last resort)
sqlite3 var/aos-cp.sqlite3 <<EOF
UPDATE adapters
SET current_state = 'cold', updated_at = strftime('%s', 'now')
WHERE current_state = 'loading'
  AND updated_at < strftime('%s', 'now') - 300;
EOF

# Verify recovery
aosctl adapter list --state loading
# Should return empty
```

**Prevention:**
- Set adapter load timeout (5 minutes default)
- Monitor adapter state transitions
- Implement automatic recovery for stuck loads
- Validate adapter files before loading

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:127-190` - Loader health check
- `/Users/star/Dev/aos/crates/adapteros-core/src/lifecycle.rs` - Lifecycle state machine

### 3. Kernel Component Degraded/Unhealthy

**Symptoms:**
```json
{
  "component": "kernel",
  "status": "degraded",
  "message": "Worker not initialized"
}
```

**Root Causes:**
- Worker process not started
- Metal/GPU initialization failed
- Inference pipeline not ready
- Model not loaded

**Diagnostic Commands:**
```bash
# Check kernel health
curl http://localhost:8080/healthz/kernel | jq

# Check worker status
aosctl status workers

# Check Metal/GPU
system_profiler SPDisplaysDataType | grep -i "metal"
```

**Fix Procedure:**

**Step 1: Verify Worker Configuration**
```bash
# Check if worker is configured
cat configs/cp.toml | grep -A 5 "\[worker\]"

# Check if model manifest exists
ls -la models/qwen2.5-7b-mlx/manifest.json
```

**Step 2: Initialize Worker**
```bash
# Start worker via serve command
aosctl serve --tenant default --plan <plan-id> \
  --socket var/run/aos.sock

# Verify worker started
aosctl status workers
```

**Step 3: Check Metal/GPU Availability**
```bash
# Verify Metal is available
./metal/test_metal.sh || echo "Metal not available"

# Check GPU memory
system_profiler SPDisplaysDataType | grep -i "vram"
```

**Prevention:**
- Verify GPU/Metal support during installation
- Include worker initialization in startup
- Monitor worker health
- Set worker restart policy

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:192-220` - Kernel health check
- `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/lib.rs` - Worker implementation

### 4. Database Component Degraded/Unhealthy

**Symptoms:**
```json
{
  "component": "db",
  "status": "degraded",
  "message": "High query latency: 150ms"
}
```

**Root Causes:**
- Query latency > 100ms
- Connection pool saturation > 80%
- Database locked
- Large WAL file

**Diagnostic Commands:**
```bash
# Check DB health
curl http://localhost:8080/healthz/db | jq

# Check query latency
curl http://localhost:8080/healthz/db | jq '.details.query_latency_ms'

# Check pool saturation
curl http://localhost:8080/healthz/db | jq '.details.pool_saturation_pct'

# Check WAL size
ls -lh var/aos-cp.sqlite3-wal
```

**Fix Procedure:**

**If "High query latency":**
```bash
# Step 1: Run optimization
sqlite3 var/aos-cp.sqlite3 "PRAGMA optimize;"

# Step 2: Analyze slow queries
sqlite3 var/aos-cp.sqlite3 ".trace stdout"

# Step 3: Checkpoint WAL
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"

# Step 4: Vacuum if needed
sqlite3 var/aos-cp.sqlite3 "VACUUM;"
```

**If "High pool saturation":**
```bash
# Step 1: Check active connections
sqlite3 var/aos-cp.sqlite3 <<EOF
.mode column
.headers on
SELECT * FROM pragma_database_list;
EOF

# Step 2: Kill long-running queries (requires custom script)
# Restart server if pool exhausted
pkill -SIGTERM aos-cp
sleep 5
./scripts/start_server.sh
```

**Prevention:**
- Monitor query latency trends
- Run regular PRAGMA optimize
- Checkpoint WAL periodically
- Increase pool size if needed (max: 20)

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:222-308` - DB health check
- `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs:33-56` - Connection pool config

### 5. Telemetry Component Degraded/Unhealthy

**Symptoms:**
```json
{
  "component": "telemetry",
  "status": "degraded",
  "message": "No telemetry activity recorded yet"
}
```

**Root Causes:**
- Telemetry writer not initialized
- No events recorded yet (normal on startup)
- High latency (> 1s)
- Telemetry buffer full

**Diagnostic Commands:**
```bash
# Check telemetry health
curl http://localhost:8080/healthz/telemetry | jq

# List recent telemetry
aosctl telemetry-list --limit 10

# Check telemetry bundle size
du -sh var/telemetry/
```

**Fix Procedure:**

**If "No telemetry activity":**
```bash
# Normal on first startup
# Generate telemetry by using system
aosctl infer --prompt "test" --max-tokens 16

# Verify telemetry now recording
curl http://localhost:8080/healthz/telemetry | jq '.details.total_requests'
```

**If "High latency":**
```bash
# Step 1: Check bundle size
ls -lh var/telemetry/bundle_*.ndjson

# Step 2: Rotate bundles
aosctl maintenance rotate-telemetry

# Step 3: Clean old bundles
aosctl maintenance cleanup-telemetry --older-than 7d
```

**Prevention:**
- Monitor telemetry latency
- Rotate bundles regularly
- Set bundle size limits
- Archive old telemetry

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:310-363` - Telemetry health check
- `/Users/star/Dev/aos/crates/adapteros-telemetry/src/lib.rs` - Telemetry writer

### 6. System-Metrics Component Degraded/Unhealthy

**Symptoms:**
```json
{
  "component": "system-metrics",
  "status": "degraded",
  "message": "Critical memory pressure (14% headroom)"
}
```

**Root Causes:**
- Memory pressure critical (≤ 15% headroom)
- Memory pressure elevated (< 20% headroom)
- UMA monitor not initialized
- Worker not available

**Diagnostic Commands:**
```bash
# Check system-metrics health
curl http://localhost:8080/healthz/system-metrics | jq

# Check memory pressure level
curl http://localhost:8080/healthz/system-metrics | jq '.details.pressure_level'

# Check headroom
curl http://localhost:8080/healthz/system-metrics | jq '.details.headroom_pct'
```

**Fix Procedure:**

**See [Memory Pressure Runbook](./memory-pressure.md) for detailed procedures**

**Quick fix:**
```bash
# Run garbage collection
aosctl maintenance gc --force

# Check improved headroom
curl http://localhost:8080/healthz/system-metrics | jq '.details.headroom_pct'
```

**Prevention:**
- Monitor memory headroom continuously
- Set alerts at 20% threshold
- Implement automatic GC at 25% threshold
- Pin critical adapters only

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:365-456` - System-metrics health check
- `/Users/star/Dev/aos/crates/adapteros-lora-worker/src/lib.rs` - UMA pressure monitor

## Aggregate Health Check

**Endpoint:** `GET /healthz/all`

Returns overall system status (worst component wins):
- If any component `unhealthy` → overall `unhealthy`
- Else if any component `degraded` → overall `degraded`
- Else → overall `healthy`

**Usage:**
```bash
# Check all components
curl http://localhost:8080/healthz/all | jq

# Check overall status
curl http://localhost:8080/healthz/all | jq '.overall_status'

# List degraded components
curl http://localhost:8080/healthz/all | jq '.components[] | select(.status == "degraded")'

# Count unhealthy components
curl http://localhost:8080/healthz/all | jq '[.components[] | select(.status == "unhealthy")] | length'
```

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:459-556` - Aggregate health check

## Health Check Integration

### Kubernetes/Docker
```yaml
livenessProbe:
  httpGet:
    path: /healthz/all
    port: 8080
  initialDelaySeconds: 30
  periodSeconds: 10
  failureThreshold: 3

readinessProbe:
  httpGet:
    path: /healthz/all
    port: 8080
  initialDelaySeconds: 10
  periodSeconds: 5
```

### Monitoring/Alerting
```bash
# Prometheus metrics export (via UDS)
socat - UNIX-CONNECT:var/run/metrics.sock

# Alert on degraded status
curl -s http://localhost:8080/healthz/all | \
  jq -e '.overall_status == "healthy"' || \
  echo "ALERT: System degraded"
```

### CLI Integration
```bash
# Use aosctl doctor for comprehensive check
aosctl doctor

# Check specific component
aosctl doctor --component db
```

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-cli/src/commands/doctor.rs` - Doctor command

## Related Runbooks

- [Memory Pressure](./memory-pressure.md)
- [Database Failures](./database-failures.md)
- [Log Analysis](./log-analysis.md)
- [Metrics Review](./metrics-review.md)

## Escalation Criteria

Escalate if:
- Multiple components unhealthy simultaneously
- Component stuck in unhealthy state > 10 minutes
- Health checks timing out
- Repeated degraded→healthy→degraded cycling
- See [Escalation Guide](./escalation.md)
