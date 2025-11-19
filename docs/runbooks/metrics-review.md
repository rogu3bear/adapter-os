# Metrics Review

Key operational metrics and monitoring best practices.

## Metrics Sources

AdapterOS exposes metrics through multiple channels:

1. **HTTP Health Endpoints** - Component-level health status
2. **UDS Socket** - Prometheus-format metrics (zero-network)
3. **Telemetry Events** - NDJSON event stream
4. **CLI Commands** - Status and diagnostic commands
5. **Database Queries** - Historical metrics

## Key Metrics by Category

### 1. System Resource Metrics

**Memory Metrics:**
```bash
# Via health endpoint
curl http://localhost:8080/healthz/system-metrics | jq '.details'

# Expected fields:
# - memory_used_mb: Current memory usage
# - memory_total_mb: Total system memory
# - memory_available_mb: Available memory
# - headroom_pct: Percentage of free memory
# - pressure_level: normal/medium/high/critical
```

**Thresholds:**
```
headroom_pct > 30%:  Normal (green)
headroom_pct > 20%:  Medium (yellow)
headroom_pct > 15%:  High (orange)
headroom_pct ≤ 15%:  Critical (red)
```

**Actions:**
- < 20%: Plan cleanup
- < 15%: Execute GC immediately
- < 10%: Emergency eviction

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:371-456` - System metrics health

### 2. Database Metrics

**Connection Pool:**
```bash
# Via health endpoint
curl http://localhost:8080/healthz/db | jq '.details'

# Expected fields:
# - pool_size: Total connections
# - pool_active: Active connections
# - pool_idle: Idle connections
# - pool_saturation_pct: Percentage of pool in use
# - max_connections: Pool limit (20)
```

**Thresholds:**
```
pool_saturation_pct < 60%:  Normal
pool_saturation_pct < 80%:  Warning
pool_saturation_pct ≥ 80%:  Critical
```

**Query Performance:**
```bash
# Query latency
curl http://localhost:8080/healthz/db | jq '.details.query_latency_ms'

# Thresholds:
# < 50ms:  Excellent
# < 100ms: Good
# < 200ms: Degraded
# ≥ 200ms: Critical
```

**Actions:**
- Latency > 100ms: Run PRAGMA optimize
- Saturation > 80%: Check for slow queries
- Saturation > 90%: Consider restart

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:222-308` - DB health check
- `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs:33-56` - Pool configuration

### 3. Router Metrics

**Queue Metrics:**
```bash
# Via health endpoint
curl http://localhost:8080/healthz/router | jq '.details'

# Expected fields:
# - queue_depth: Pending requests
# - total_requests: Lifetime request count
```

**Thresholds:**
```
queue_depth < 50:   Normal
queue_depth < 100:  Warning
queue_depth ≥ 100:  Critical
```

**Decision Metrics:**
```bash
# Via telemetry
aosctl telemetry-list --event-type router.decision --limit 100

# Key metrics:
# - adapter_id: Selected adapter
# - confidence: Decision confidence (0.0-1.0)
# - latency_ms: Decision latency
# - alternatives: Alternative adapters considered
```

**Thresholds:**
```
confidence > 0.7:  Strong match
confidence > 0.5:  Acceptable match
confidence > 0.3:  Weak match
confidence ≤ 0.3:  No match
```

**Actions:**
- Queue depth > 50: Monitor load
- Queue depth > 100: Reduce load or scale
- Low confidence: Review router weights

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:80-125` - Router health check
- `/Users/star/Dev/aos/crates/adapteros-lora-router/src/lib.rs` - Router implementation

### 4. Adapter Metrics

**State Distribution:**
```bash
# Via CLI
aosctl adapter list --json | jq 'group_by(.current_state) | map({state: .[0].current_state, count: length})'

# Via database
sqlite3 var/aos-cp.sqlite3 <<EOF
SELECT current_state, COUNT(*) as count
FROM adapters
GROUP BY current_state;
EOF
```

**Expected distribution:**
```
cold:   Most adapters (not loaded)
warm:   Preloaded but not active
hot:    Currently in use
loading: Transitioning (should be < 5)
failed:  Should be 0
```

**Load Time Metrics:**
```bash
# Via telemetry
aosctl telemetry-list --event-type adapter.loaded --limit 20 | \
  jq '.events[].duration_ms' | \
  awk '{sum+=$1; n++} END {print "Avg load time:", sum/n, "ms"}'
```

**Thresholds:**
```
load_time < 1000ms:  Fast
load_time < 3000ms:  Normal
load_time < 5000ms:  Slow
load_time ≥ 5000ms:  Critical
```

**Actions:**
- > 3 stuck in loading: See [Health Check Failures](./health-check-failures.md#2-loader-component-degradedunhealthy)
- Load time > 5s: Check adapter size, disk I/O
- Many failed: Check adapter files

### 5. Telemetry Metrics

**Event Metrics:**
```bash
# Via health endpoint
curl http://localhost:8080/healthz/telemetry | jq '.details'

# Expected fields:
# - total_requests: Lifetime event count
# - avg_latency_ms: Average write latency
# - p95_latency_ms: 95th percentile
# - p99_latency_ms: 99th percentile
```

**Thresholds:**
```
avg_latency_ms < 100ms:   Normal
avg_latency_ms < 500ms:   Degraded
avg_latency_ms < 1000ms:  Warning
avg_latency_ms ≥ 1000ms:  Critical
```

**Bundle Size:**
```bash
# Check bundle sizes
ls -lh var/telemetry/bundle_*.ndjson

# Count events in current bundle
wc -l var/telemetry/bundle_latest.ndjson
```

**Actions:**
- Latency > 500ms: Rotate bundles
- Bundle > 50MB: Force rotation
- Many bundles: Archive old ones

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server-api/src/health.rs:310-363` - Telemetry health check
- `/Users/star/Dev/aos/crates/adapteros-telemetry/src/lib.rs` - Telemetry writer

### 6. Cleanup Metrics

**TTL Cleanup:**
```bash
# Via logs
grep "TTL cleanup" var/aos-cp.log | tail -10

# Track expired adapters
sqlite3 var/aos-cp.sqlite3 <<EOF
SELECT COUNT(*) as expired_count
FROM adapters
WHERE expires_at IS NOT NULL
  AND expires_at < strftime('%s', 'now');
EOF
```

**GC Metrics:**
```bash
# Via logs
grep -E "GC|garbage collection" var/aos-cp.log | tail -10

# Orphaned adapter count
sqlite3 var/aos-cp.sqlite3 <<EOF
SELECT COUNT(*) as orphaned_count
FROM adapters
WHERE current_state IN ('failed', 'loading')
  AND updated_at < strftime('%s', 'now') - 3600;
EOF
```

**Actions:**
- > 10 expired: Run cleanup
- > 5 orphaned: Run sweep
- Cleanup failures: Check logs

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:843-895` - TTL cleanup task

## Prometheus Metrics (UDS)

**Access metrics via Unix Domain Socket:**
```bash
# Read metrics
socat - UNIX-CONNECT:var/run/metrics.sock

# Expected format (Prometheus):
# adapteros_inference_requests_total 1234
# adapteros_memory_usage_bytes 30001651712
# adapteros_quarantine_active 0
```

**Available metrics:**
- `adapteros_inference_requests_total` - Total inference requests (counter)
- `adapteros_memory_usage_bytes` - Current memory usage (gauge)
- `adapteros_quarantine_active` - System quarantine status (gauge)

**Integration:**
```bash
# Export to file
socat - UNIX-CONNECT:var/run/metrics.sock > metrics.txt

# Parse with prometheus client
# (Requires prometheus client library)
```

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:647-704` - UDS metrics exporter
- `/Users/star/Dev/aos/crates/adapteros-telemetry/src/lib.rs` - Metrics exporter

## Monitoring Dashboards

### CLI Dashboard
```bash
#!/bin/bash
# dashboard.sh - Simple CLI dashboard

while true; do
  clear
  echo "=== AdapterOS Dashboard ==="
  echo ""

  echo "System Metrics:"
  curl -s http://localhost:8080/healthz/system-metrics | jq -r '"  Memory: " + (.details.memory_used_mb|tostring) + "/" + (.details.memory_total_mb|tostring) + " MB (" + (.details.headroom_pct|tostring) + "% free)"'

  echo ""
  echo "Database:"
  curl -s http://localhost:8080/healthz/db | jq -r '"  Latency: " + (.details.query_latency_ms|tostring) + "ms"'
  curl -s http://localhost:8080/healthz/db | jq -r '"  Pool: " + (.details.pool_active|tostring) + "/" + (.details.max_connections|tostring) + " (" + (.details.pool_saturation_pct|tostring) + "%)"'

  echo ""
  echo "Router:"
  curl -s http://localhost:8080/healthz/router | jq -r '"  Queue: " + (.details.queue_depth|tostring) + " requests"'
  curl -s http://localhost:8080/healthz/router | jq -r '"  Total: " + (.details.total_requests|tostring) + " requests"'

  echo ""
  echo "Adapters:"
  sqlite3 var/aos-cp.sqlite3 "SELECT current_state, COUNT(*) FROM adapters GROUP BY current_state" | while read line; do
    echo "  $line"
  done

  echo ""
  echo "Last updated: $(date)"
  sleep 5
done
```

### Watch Commands
```bash
# Memory pressure
watch -n 5 'curl -s http://localhost:8080/healthz/system-metrics | jq ".details | {used_mb, headroom_pct, pressure_level}"'

# Database performance
watch -n 10 'curl -s http://localhost:8080/healthz/db | jq ".details | {latency_ms: .query_latency_ms, saturation: .pool_saturation_pct}"'

# Overall health
watch -n 5 'curl -s http://localhost:8080/healthz/all | jq "{status: .overall_status, degraded: [.components[] | select(.status != \"healthy\") | .component]}"'
```

## Alerting Thresholds

### Critical Alerts (Immediate Action)
```
- Memory headroom < 15%
- Database pool saturation > 90%
- Router queue depth > 150
- Any component unhealthy
- Database query latency > 200ms
- > 5 adapters stuck in loading
```

### Warning Alerts (Monitor)
```
- Memory headroom < 20%
- Database pool saturation > 80%
- Router queue depth > 100
- Any component degraded
- Database query latency > 100ms
- > 3 adapters stuck in loading
- Telemetry latency > 500ms
```

### Informational Alerts (Track)
```
- Memory headroom < 25%
- Router confidence < 0.5 (frequent)
- Adapter load time > 3s
- GC collected > 10 adapters
```

## Metric Collection Script

```bash
#!/bin/bash
# collect-metrics.sh - Collect metrics snapshot

TIMESTAMP=$(date +%s)
OUTPUT="metrics-$TIMESTAMP.json"

echo "Collecting metrics snapshot..."

# Aggregate all health endpoints
echo "{" > $OUTPUT
echo "  \"timestamp\": $TIMESTAMP," >> $OUTPUT
echo "  \"date\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"," >> $OUTPUT

echo "  \"health\": {" >> $OUTPUT
curl -s http://localhost:8080/healthz/all >> $OUTPUT
echo "  }," >> $OUTPUT

echo "  \"adapters\": {" >> $OUTPUT
echo "    \"by_state\": [" >> $OUTPUT
sqlite3 -json var/aos-cp.sqlite3 "SELECT current_state, COUNT(*) as count FROM adapters GROUP BY current_state" >> $OUTPUT
echo "    ]," >> $OUTPUT
echo "    \"total\": $(sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*) FROM adapters")" >> $OUTPUT
echo "  }," >> $OUTPUT

echo "  \"telemetry\": {" >> $OUTPUT
echo "    \"bundles\": $(ls var/telemetry/*.ndjson 2>/dev/null | wc -l)," >> $OUTPUT
echo "    \"total_size_mb\": $(du -sm var/telemetry 2>/dev/null | cut -f1)" >> $OUTPUT
echo "  }" >> $OUTPUT

echo "}" >> $OUTPUT

echo "Metrics saved to $OUTPUT"
cat $OUTPUT | jq
```

## Historical Metrics

**Query historical adapter states:**
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
SELECT
  datetime(created_at, 'unixepoch') as created,
  datetime(updated_at, 'unixepoch') as updated,
  adapter_id,
  current_state,
  tier
FROM adapters
ORDER BY updated_at DESC
LIMIT 20;
EOF
```

**Query historical routing decisions:**
```bash
aosctl telemetry-list --event-type router.decision --limit 100 | \
  jq '.events[] | {timestamp, adapter_id, confidence}'
```

**Analyze adapter usage patterns:**
```bash
# Most frequently selected adapters
aosctl telemetry-list --event-type router.decision --limit 1000 | \
  jq -r '.events[].adapter_id' | \
  sort | uniq -c | sort -rn | head -10
```

## Related Runbooks

- [Health Check Failures](./health-check-failures.md)
- [Memory Pressure](./memory-pressure.md)
- [Database Failures](./database-failures.md)
- [Log Analysis](./log-analysis.md)

## Escalation Criteria

Escalate if:
- Multiple critical thresholds exceeded
- Metrics show degradation trend
- Unusual metric patterns
- See [Escalation Guide](./escalation.md)
