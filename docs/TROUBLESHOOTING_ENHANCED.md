# adapterOS Enhanced Troubleshooting Guide

> **Agent note:** Code is authoritative. Diagnostic commands and config paths may have changed. Re-verify before use. See [CANONICAL_SOURCES.md](CANONICAL_SOURCES.md) and [DOCS_AUDIT_2026-02-18.md](DOCS_AUDIT_2026-02-18.md).

**Comprehensive error diagnosis and resolution for adapterOS**  
**Last Updated:** 2026-02-18  
**Version:** 3.0  
**Maintained By:** adapterOS Support Team

---

## Table of Contents

- [Common Error Messages](#common-error-messages)
- [Database Issues](#database-issues)
- [Worker Connection Problems](#worker-connection-problems)
- [Authentication Failures](#authentication-failures)
- [Performance Issues](#performance-issues)
- [Decision Trees](#decision-trees)
- [Diagnostic Commands](#diagnostic-commands)

---

## Common Error Messages

### Error Catalog

This section provides specific error messages, their causes, and solutions.

#### Database Errors

##### "Database connection failed: unable to open database file"

**Error Code:** `E_DB_CONNECTION`
**Severity:** Critical
**Common Causes:**
- Database file doesn't exist
- Insufficient permissions
- Database file corrupted
- Path contains invalid characters (e.g., `/tmp`)

**Diagnosis:**
```bash
# Check if database file exists
ls -la var/aos-cp.sqlite3

# Check permissions
ls -ld var/
stat var/aos-cp.sqlite3

# Verify path is not in /tmp
grep -i "database.*tmp" configs/aos.toml

# Test connection
sqlite3 var/aos-cp.sqlite3 "SELECT 1;"
```

**Solutions:**
```bash
# Fix permissions
chmod 644 var/aos-cp.sqlite3
chmod 755 var/

# Move from invalid path
if [[ "$AOS_DATABASE_URL" == *"/tmp/"* ]]; then
  export AOS_DATABASE_URL="sqlite://var/aos-cp.sqlite3"
fi

# Recreate database (WARNING: data loss)
rm var/aos-cp.sqlite3
cargo sqlx migrate run
```

##### "Migration signature verification failed"

**Error Code:** `E_MIG_INVALID`
**Severity:** Critical
**Common Causes:**
- Migration file modified after signing
- `signatures.json` out of date
- Git checkout/merge conflict

**Diagnosis:**
```bash
# Check for modified migrations
git status migrations/

# Verify signature file
cat migrations/signatures.json | jq '.migrations | length'

# Check for merge conflicts
grep -r "<<<<<<" migrations/
```

**Solutions:**
```bash
# Option 1: Reset to canonical migrations
git checkout -- migrations/
git checkout -- migrations/signatures.json

# Option 2: Re-sign migrations (if you modified them intentionally)
./scripts/sign_migrations.sh

# Verify fix
cargo sqlx migrate info
```

##### "Foreign key constraint violation"

**Error Code:** `E_FK_VIOLATION`
**Severity:** High
**Common Causes:**
- Tenant ID mismatch
- Referenced adapter/model doesn't exist
- Cascade delete disabled

**Diagnosis:**
```bash
# Check foreign key enforcement
sqlite3 var/aos-cp.sqlite3 "PRAGMA foreign_keys;"
# Should return 1

# Find orphaned records
sqlite3 var/aos-cp.sqlite3 "
SELECT a.id, a.tenant_id
FROM adapters a
LEFT JOIN tenants t ON a.tenant_id = t.id
WHERE t.id IS NULL;"

# Check constraint violations in logs
grep "FOREIGN KEY constraint failed" var/logs/backend.log
```

**Solutions:**
```bash
# Enable foreign keys (should be automatic)
# Check crates/adapteros-db/src/lib.rs contains foreign_keys=true

# Delete orphaned records
sqlite3 var/aos-cp.sqlite3 "
DELETE FROM adapters
WHERE tenant_id NOT IN (SELECT id FROM tenants);"

# Verify tenant exists before creating adapter
curl -s http://localhost:8080/v1/tenants | jq '.[].id'
```

#### Worker Errors

##### "Worker socket not found"

**Error Code:** `E_WORKER_SOCKET`
**Severity:** Critical
**Common Causes:**
- Worker process not running
- Socket path in `/tmp` (rejected)
- Permissions issue
- Socket file stale

**Diagnosis:**
```bash
# Check worker process
ps aux | grep aos-worker

# Check socket files
ls -la var/run/aos/*/worker.sock

# Verify path not in /tmp
grep -i "socket.*tmp" configs/aos.toml var/logs/backend.log

# Check socket permissions
stat var/run/aos/*/worker.sock
```

**Solutions:**
```bash
# Start worker
./scripts/service-manager.sh start worker

# Remove stale socket
rm -f var/run/aos/*/worker.sock

# Fix socket path (if in /tmp)
export AOS_WORKER_SOCKET="var/run/aos/default/worker.sock"

# Verify connection
lsof var/run/aos/*/worker.sock
```

##### "UDS connection failed: Connection refused"

**Error Code:** `E_UDS_REFUSED`
**Severity:** Critical
**Common Causes:**
- Worker crashed
- Worker not accepting connections
- Socket buffer full
- Worker in initialization

**Diagnosis:**
```bash
# Check worker logs
tail -50 var/logs/worker.log

# Check for panics
grep -i "panic\|fatal" var/logs/worker.log

# Check worker status
./scripts/service-manager.sh status

# Monitor socket activity
lsof -c aos-worker | grep unix
```

**Solutions:**
```bash
# Restart worker
./scripts/service-manager.sh restart worker

# Check for OOM
dmesg | grep -i "out of memory"
vm_stat | grep "Pages active"

# Increase socket buffer (if needed)
# In configs/aos.toml:
# [worker]
# socket_buffer_size = 65536

# Monitor startup
tail -f var/logs/worker.log | grep -i "ready\|initialized"
```

##### "Worker not responding: timeout after 30s"

**Error Code:** `E_WORKER_TIMEOUT`
**Severity:** High
**Common Causes:**
- Worker busy with long-running inference
- Model loading in progress
- Memory pressure causing swap
- Backend deadlock

**Diagnosis:**
```bash
# Check worker CPU/memory
ps aux | grep aos-worker | awk '{print $3, $4, $11}'

# Check for swap usage
vm_stat | grep "Pages swapped"

# Check active requests
curl -s http://localhost:8080/v1/metrics/system | jq '.inference.active_count'

# Monitor worker responsiveness
time echo "ping" | socat - UNIX-CONNECT:var/run/aos/default/worker.sock
```

**Solutions:**
```bash
# Kill stuck worker (will auto-restart if managed)
pkill -9 aos-worker

# Reduce concurrent requests
# In configs/aos.toml:
# [worker]
# max_concurrent_requests = 4

# Increase timeout
# In configs/aos.toml:
# [worker]
# request_timeout_secs = 60

# Check for memory leak
watch -n 5 'ps aux | grep aos-worker'
```

#### Authentication Errors

##### "JWT signature verification failed"

**Error Code:** `E_AUTH_JWT_INVALID`
**Severity:** Medium
**Common Causes:**
- Token expired
- Wrong JWT secret
- Token malformed
- Clock skew

**Diagnosis:**
```bash
# Decode JWT (without verification)
echo "$TOKEN" | cut -d'.' -f2 | base64 -d 2>/dev/null | jq .

# Check expiration
echo "$TOKEN" | cut -d'.' -f2 | base64 -d 2>/dev/null | jq '.exp' | xargs -I {} date -r {}

# Verify secret
grep "JWT_SECRET" .env .env.local

# Check server time
date
```

**Solutions:**
```bash
# Use dev bypass (development only)
export AOS_DEV_NO_AUTH=1
./start up

# Generate new token
./aosctl auth login

# Set correct secret
export AOS_JWT_SECRET="your-secret-here"

# Fix clock skew
sudo ntpdate -u time.apple.com  # macOS
```

##### "Tenant isolation violation"

**Error Code:** `E_AUTHZ_TENANT`
**Severity:** Critical
**Common Causes:**
- Cross-tenant resource access attempt
- Admin permissions not set
- Token contains wrong tenant_id
- Security policy violated

**Diagnosis:**
```bash
# Check token claims
echo "$TOKEN" | cut -d'.' -f2 | base64 -d 2>/dev/null | jq '.tenant_id, .admin_tenants'

# Check resource ownership
sqlite3 var/aos-cp.sqlite3 "
SELECT id, tenant_id, name
FROM adapters
WHERE id = 'adapter-123';"

# Check security logs
grep "isolation.*violation" var/logs/backend.log | tail -20
```

**Solutions:**
```bash
# Use correct tenant token
./aosctl auth login --tenant "correct-tenant-id"

# Add admin permissions (if authorized)
# Modify JWT claims to include admin_tenants

# In development, use wildcard admin
# claims.admin_tenants = ["*"]

# Check policy enforcement
curl -s http://localhost:8080/v1/policies/isolation | jq .
```

#### Adapter Errors

##### "Adapter not loaded: current state is Cold"

**Error Code:** `E_ADAPTER_COLD`
**Severity:** Medium
**Common Causes:**
- Adapter evicted due to memory pressure
- Adapter never loaded
- Load operation failed
- Cache cleared

**Diagnosis:**
```bash
# Check adapter state
curl -s http://localhost:8080/v1/adapters/adapter-123 | jq '.status'

# Check memory headroom
curl -s http://localhost:8080/v1/metrics/system | jq '.memory.headroom_pct'

# Check eviction logs
grep -i "evict.*adapter-123" var/logs/backend.log

# Check load failures
grep -i "load.*adapter-123.*fail" var/logs/backend.log
```

**Solutions:**
```bash
# Load adapter
curl -X POST http://localhost:8080/v1/adapters/adapter-123/load

# Increase memory headroom
# In configs/aos.toml:
# [memory]
# min_headroom_pct = 15

# Evict other adapters
curl -X POST http://localhost:8080/v1/adapters/adapter-456/unload

# Check for file corruption
./aosctl adapter inspect var/adapters/adapter-123.aos
```

##### "Adapter hash mismatch: expected abc123, got def456"

**Error Code:** `E_ADAPTER_HASH`
**Severity:** Critical
**Common Causes:**
- File corruption
- Tampering attempt
- Incomplete upload
- Disk corruption

**Diagnosis:**
```bash
# Verify adapter file integrity
sha256sum var/adapters/adapter-123.aos

# Check disk for errors
df -h var/
fsck (on unmounted volume)

# Check upload logs
grep -i "upload.*adapter-123" var/logs/backend.log

# Verify manifest hash
./aosctl adapter inspect var/adapters/adapter-123.aos | jq '.manifest_hash'
```

**Solutions:**
```bash
# Re-upload adapter
rm var/adapters/adapter-123.aos
# Upload via UI or API

# Verify disk health
# Run disk utility on macOS or fsck on Linux

# If intentional update:
# Update manifest with new hash
./aosctl adapter update adapter-123 --recompute-hash

# Check for security breach
grep -i "tamper\|unauthorized" var/logs/backend.log
```

#### Performance Errors

##### "Resource exhaustion: memory usage 95%"

**Error Code:** `E_MEM_EXHAUSTED`
**Severity:** Critical
**Common Causes:**
- Too many loaded adapters
- Memory leak
- Large model loaded
- System memory low

**Diagnosis:**
```bash
# Check memory metrics
curl -s http://localhost:8080/v1/metrics/system | jq '.memory'

# Check loaded adapters
curl -s http://localhost:8080/v1/adapters | jq '[.[] | select(.status == "Loaded")] | length'

# Check system memory
free -h 2>/dev/null || vm_stat

# Check for leaks
watch -n 5 'ps aux | grep adapteros-server | awk "{print \$6}"'
```

**Solutions:**
```bash
# Immediate: evict cold adapters
curl -X POST http://localhost:8080/v1/adapters/evict?tier=cold

# Reduce adapter count
# In configs/aos.toml:
# [memory]
# max_adapters_per_tenant = 5

# Increase memory headroom
# [memory]
# min_headroom_pct = 20

# Restart if leak suspected
./start up
```

##### "Inference timeout: exceeded 30s"

**Error Code:** `E_INFER_TIMEOUT`
**Severity:** High
**Common Causes:**
- Model too large
- Backend stub active
- GPU not available
- Queue backlog

**Diagnosis:**
```bash
# Check inference metrics
curl -s http://localhost:8080/v1/metrics/system | jq '.inference'

# Check backend type
grep -i "backend.*initialized" var/logs/backend.log | tail -1

# Check for stub warnings
grep -i "stub.*active" var/logs/backend.log

# Check GPU
system_profiler SPDisplaysDataType | grep Metal
```

**Solutions:**
```bash
# Use real backend (not stub)
cargo build --release --features mlx-backend
export AOS_MODEL_BACKEND=mlx

# Increase timeout
# In configs/aos.toml:
# [inference]
# timeout_secs = 60

# Check queue depth
curl -s http://localhost:8080/v1/metrics/system | jq '.inference.queue_depth'

# Reduce batch size
# [inference]
# max_batch_size = 4
```

---

## Database Issues

### Decision Tree: Database Problems

```
Database Issue
│
├─ Can't connect to database?
│  ├─ File doesn't exist?
│  │  └─ Run migrations: cargo sqlx migrate run
│  ├─ Permission denied?
│  │  └─ Fix permissions: chmod 644 var/aos-cp.sqlite3
│  └─ Database locked?
│     └─ Kill duplicate processes: pkill adapteros-server
│
├─ Migration failed?
│  ├─ Signature mismatch?
│  │  └─ Reset migrations: git checkout -- migrations/
│  └─ Version conflict?
│     └─ Check version: sqlite3 var/aos-cp.sqlite3 "PRAGMA user_version;"
│
├─ Query performance slow?
│  ├─ Missing indexes?
│  │  └─ Check indexes: sqlite3 var/aos-cp.sqlite3 ".indexes"
│  ├─ WAL too large?
│  │  └─ Checkpoint: sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"
│  └─ Too many rows?
│     └─ Archive old data
│
└─ Data corruption?
   ├─ Check integrity: PRAGMA integrity_check
   └─ Restore from backup
```

### Common Database Queries

#### Check Database Health
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
-- Integrity check
PRAGMA integrity_check;

-- Database size
SELECT page_count * page_size as size_bytes FROM pragma_page_count(), pragma_page_size();

-- Table row counts
SELECT name, (SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=m.name) as count
FROM sqlite_master m WHERE type='table';

-- WAL status
PRAGMA wal_checkpoint;
EOF
```

#### Tenant Isolation Audit
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
-- Find orphaned adapters (tenant doesn't exist)
SELECT a.id, a.tenant_id
FROM adapters a
LEFT JOIN tenants t ON a.tenant_id = t.id
WHERE t.id IS NULL;

-- Cross-tenant references (should be empty)
SELECT a.id as adapter_id, a.tenant_id as adapter_tenant, s.tenant_id as stack_tenant
FROM adapter_stack_members asm
JOIN adapters a ON asm.adapter_id = a.id
JOIN adapter_stacks s ON asm.stack_id = s.id
WHERE a.tenant_id != s.tenant_id;
EOF
```

#### Performance Analysis
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
-- Slowest queries (if query logging enabled)
SELECT query, avg_time_ms, call_count
FROM query_performance
ORDER BY avg_time_ms DESC
LIMIT 10;

-- Largest tables
SELECT name,
       (SELECT COUNT(*) FROM pragma_table_info(m.name)) as column_count,
       (SELECT COUNT(*) FROM main.[name]) as row_count
FROM sqlite_master m
WHERE type='table'
ORDER BY row_count DESC;
EOF
```

### Database Optimization

#### Vacuum and Optimize
```bash
# Reclaim space
sqlite3 var/aos-cp.sqlite3 "VACUUM;"

# Analyze for query planner
sqlite3 var/aos-cp.sqlite3 "ANALYZE;"

# Checkpoint WAL
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"

# Check improvement
du -h var/aos-cp.sqlite3*
```

#### Index Health Check
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
-- List all indexes
.indexes

-- Find missing indexes (tables without indexes on foreign keys)
SELECT m.name, p.name as column
FROM sqlite_master m
JOIN pragma_table_info(m.name) p
WHERE m.type='table'
  AND p.name LIKE '%_id'
  AND NOT EXISTS (
    SELECT 1 FROM pragma_index_list(m.name) il
    JOIN pragma_index_info(il.name) ii
    WHERE ii.name = p.name
  );
EOF
```

---

## Worker Connection Problems

### Decision Tree: Worker Issues

```
Worker Connection Problem
│
├─ Worker process not running?
│  ├─ Check process: ps aux | grep aos-worker
│  ├─ Start worker: ./scripts/service-manager.sh start worker
│  └─ Check startup logs: tail -50 var/logs/worker.log
│
├─ Socket file missing?
│  ├─ Path in /tmp? (rejected)
│  │  └─ Fix path: export AOS_WORKER_SOCKET="var/run/aos/default/worker.sock"
│  ├─ Socket stale?
│  │  └─ Remove: rm var/run/aos/*/worker.sock
│  └─ Permissions?
│     └─ Fix: chmod 755 var/run/aos/
│
├─ Connection refused?
│  ├─ Worker crashed?
│  │  └─ Restart: ./scripts/service-manager.sh restart worker
│  ├─ Worker busy?
│  │  └─ Check CPU: ps aux | grep aos-worker
│  └─ Socket buffer full?
│     └─ Increase buffer size in config
│
├─ Request timeout?
│  ├─ Long-running inference?
│  │  └─ Increase timeout in config
│  ├─ Memory pressure?
│  │  └─ Check swap: vm_stat | grep swap
│  └─ Backend deadlock?
│     └─ Kill worker: pkill -9 aos-worker
│
└─ Determinism violation?
   ├─ Hash mismatch?
   │  └─ Verify adapter integrity
   └─ Replay failed?
      └─ Check manifest consistency
```

### Worker Diagnostic Commands

#### Worker Health Check
```bash
# Process status
ps aux | grep aos-worker

# Socket connectivity
ls -la var/run/aos/*/worker.sock
lsof var/run/aos/*/worker.sock

# Test connection
echo '{"method":"health_check"}' | socat - UNIX-CONNECT:var/run/aos/default/worker.sock

# Check logs
tail -100 var/logs/worker.log
grep -i "error\|fatal\|panic" var/logs/worker.log
```

#### Worker Performance Metrics
```bash
# CPU and memory
ps aux | grep aos-worker | awk '{print "CPU:", $3"%", "MEM:", $4"%", "RSS:", $6/1024"MB"}'

# Thread count
ps -M $(pgrep aos-worker) | wc -l

# File descriptors
lsof -p $(pgrep aos-worker) | wc -l

# Active connections
lsof -p $(pgrep aos-worker) | grep UNIX
```

#### Worker Restart Procedure
```bash
# Graceful stop (allows drain)
./scripts/service-manager.sh stop worker

# Wait for drain
timeout=30
while lsof var/run/aos/*/worker.sock 2>/dev/null; do
  echo "Waiting for connections to drain..."
  sleep 1
  timeout=$((timeout - 1))
  if [ $timeout -le 0 ]; then
    echo "Timeout - forcing stop"
    pkill -9 aos-worker
    break
  fi
done

# Start worker
./scripts/service-manager.sh start worker

# Verify startup
tail -f var/logs/worker.log | grep -i "ready\|initialized"
```

---

## Authentication Failures

### Decision Tree: Auth Problems

```
Authentication Failure
│
├─ Dev mode needed?
│  └─ Enable: AOS_DEV_NO_AUTH=1 ./start up
│
├─ JWT token invalid?
│  ├─ Token expired?
│  │  └─ Get new token: ./aosctl auth login
│  ├─ Wrong secret?
│  │  └─ Check: grep JWT_SECRET .env .env.local
│  └─ Token malformed?
│     └─ Decode: echo $TOKEN | cut -d'.' -f2 | base64 -d | jq .
│
├─ Permission denied?
│  ├─ Tenant isolation?
│  │  └─ Check token tenant_id matches resource
│  ├─ Missing admin permissions?
│  │  └─ Add to admin_tenants in JWT
│  └─ Policy violation?
│     └─ Check policy: curl http://localhost:8080/v1/policies
│
└─ Session expired?
   └─ Refresh: ./aosctl auth refresh
```

### Auth Diagnostic Commands

#### Decode and Inspect JWT
```bash
# Decode JWT header
echo "$TOKEN" | cut -d'.' -f1 | base64 -d 2>/dev/null | jq .

# Decode JWT payload
echo "$TOKEN" | cut -d'.' -f2 | base64 -d 2>/dev/null | jq .

# Check expiration
exp=$(echo "$TOKEN" | cut -d'.' -f2 | base64 -d 2>/dev/null | jq -r '.exp')
now=$(date +%s)
remaining=$((exp - now))
echo "Token expires in $remaining seconds"

# Check tenant claims
echo "$TOKEN" | cut -d'.' -f2 | base64 -d 2>/dev/null | jq '.tenant_id, .admin_tenants'
```

#### Verify Authentication Configuration
```bash
# Check JWT secret length
jwt_secret=$(grep "^AOS_JWT_SECRET=" .env.local | cut -d'=' -f2)
echo "JWT secret length: ${#jwt_secret} (production requires >= 64)"

# Check dev mode
grep "^AOS_DEV_NO_AUTH=" .env.local

# Check auth endpoints
curl -v http://localhost:8080/v1/auth/status

# Check session table
sqlite3 var/aos-cp.sqlite3 "
SELECT user_id, created_at, expires_at
FROM auth_sessions
WHERE expires_at > datetime('now')
ORDER BY created_at DESC
LIMIT 5;"
```

#### Test Tenant Isolation
```bash
# Get current tenant
tenant_id=$(echo "$TOKEN" | cut -d'.' -f2 | base64 -d 2>/dev/null | jq -r '.tenant_id')

# List tenant resources
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/adapters | jq "[.[] | {id, tenant_id}]"

# Try cross-tenant access (should fail)
other_tenant_adapter="adapter-from-different-tenant"
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/adapters/$other_tenant_adapter

# Check audit log
sqlite3 var/aos-cp.sqlite3 "
SELECT action, resource_id, result, created_at
FROM audit_log
WHERE user_id = '$tenant_id'
ORDER BY created_at DESC
LIMIT 10;"
```

---

## Performance Issues

### Decision Tree: Performance Problems

```
Performance Issue
│
├─ High latency?
│  ├─ Backend stub active?
│  │  └─ Enable real backend: cargo build --features mlx-backend
│  ├─ GPU not used?
│  │  └─ Check Metal: system_profiler SPDisplaysDataType | grep Metal
│  ├─ Queue backlog?
│  │  └─ Check queue depth: curl http://localhost:8080/v1/metrics/system | jq .queue_depth
│  └─ Slow database?
│     └─ Run ANALYZE: sqlite3 var/aos-cp.sqlite3 "ANALYZE;"
│
├─ High memory?
│  ├─ Too many adapters?
│  │  └─ Evict: curl -X POST http://localhost:8080/v1/adapters/evict
│  ├─ Memory leak?
│  │  └─ Monitor: watch 'ps aux | grep adapteros-server'
│  └─ Large model?
│     └─ Use quantized model
│
├─ High CPU?
│  ├─ Infinite loop?
│  │  └─ Check logs for circuit breaker
│  ├─ Too many concurrent requests?
│  │  └─ Reduce: max_concurrent_requests in config
│  └─ Backend thrashing?
│     └─ Check GPU utilization
│
└─ Disk I/O?
   ├─ WAL too large?
   │  └─ Checkpoint: PRAGMA wal_checkpoint(TRUNCATE)
   ├─ Logging too verbose?
   │  └─ Reduce: AOS_LOG_LEVEL=warn
   └─ Disk full?
      └─ Clean logs: find var/logs -mtime +7 -delete
```

### Performance Diagnostic Commands

#### Latency Analysis
```bash
# Check inference latency
curl -s http://localhost:8080/v1/metrics/system | jq '.inference | {
  avg: .avg_latency_ms,
  p50: .p50_latency_ms,
  p95: .p95_latency_ms,
  p99: .p99_latency_ms,
  active: .active_count,
  queue: .queue_depth
}'

# Benchmark single request
time curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "base",
    "messages": [{"role": "user", "content": "test"}],
    "max_tokens": 10
  }'

# Monitor continuous
watch -n 1 'curl -s http://localhost:8080/v1/metrics/system | jq .inference.p99_latency_ms'
```

#### Memory Analysis
```bash
# System memory
free -h 2>/dev/null || vm_stat | head -10

# Process memory
ps aux | grep adapteros | awk '{print $11, "RSS:", $6/1024"MB", "CPU:", $3"%"}'

# Memory pressure timeline
while true; do
  mem=$(curl -s http://localhost:8080/v1/metrics/system | jq -r '.memory.used_percent')
  echo "$(date +%H:%M:%S) Memory: $mem%"
  sleep 5
done

# Check for leaks
pid=$(pgrep adapteros-server)
while true; do
  rss=$(ps -o rss= -p $pid)
  echo "$(date +%H:%M:%S) RSS: $((rss/1024))MB"
  sleep 10
done
```

#### CPU Analysis
```bash
# Process CPU
ps aux | grep adapteros | awk '{print $3}' | awk '{sum+=$1} END {print "Total CPU:", sum"%"}'

# Top threads
top -l 1 -stats pid,cpu,command | grep adapteros

# Profile with dtrace (macOS, requires SIP disabled)
sudo dtrace -n 'profile-997 /execname == "adapteros-server"/ { @[ustack()] = count(); }'

# Check for CPU spin
while true; do
  cpu=$(ps aux | grep adapteros-server | awk '{print $3}' | head -1)
  echo "$(date +%H:%M:%S) CPU: $cpu%"
  sleep 1
done
```

#### Database Performance
```bash
# Query performance (if logging enabled)
sqlite3 var/aos-cp.sqlite3 "
SELECT query_type, avg_duration_ms, max_duration_ms, call_count
FROM query_performance
ORDER BY avg_duration_ms DESC
LIMIT 10;"

# WAL size
ls -lh var/aos-cp.sqlite3-wal

# Checkpoint and measure improvement
before=$(ls -l var/aos-cp.sqlite3-wal | awk '{print $5}')
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"
after=$(ls -l var/aos-cp.sqlite3-wal 2>/dev/null | awk '{print $5}')
echo "Reclaimed: $(( (before - after) / 1024 / 1024 ))MB"

# Index usage
sqlite3 var/aos-cp.sqlite3 "
SELECT name, tbl_name
FROM sqlite_master
WHERE type='index' AND sql IS NOT NULL
ORDER BY tbl_name;"
```

---

## Decision Trees

<a id="primary-diagnostic-decision-tree"></a>
<a id="master-diagnostic-decision-tree"></a>
### Primary Diagnostic Decision Tree

```
Problem Detected
│
├─ Service not responding?
│  ├─ Check health: curl http://localhost:8080/healthz
│  │  ├─ 000 (connection refused)
│  │  │  └─ Service not running → Start service
│  │  ├─ 500 (internal error)
│  │  │  └─ Check logs → Database/Worker issue
│  │  └─ 200 OK
│  │     └─ Check readyz → Boot phase issue
│  │
│  └─ Port conflict
│     └─ Stop process: lsof -ti:8080 | xargs kill
│
├─ Requests failing?
│  ├─ Authentication?
│  │  ├─ 401 Unauthorized → JWT invalid
│  │  └─ 403 Forbidden → Tenant isolation
│  │
│  ├─ Resource not found?
│  │  ├─ 404 Not Found → Check resource exists
│  │  └─ Adapter not loaded → Load adapter
│  │
│  └─ Internal error?
│     ├─ Database error → Check DB connection
│     ├─ Worker error → Check worker status
│     └─ Backend error → Check backend logs
│
├─ Performance degraded?
│  ├─ High latency?
│  │  ├─ Check backend type (stub vs real)
│  │  ├─ Check GPU usage
│  │  └─ Check queue depth
│  │
│  ├─ High memory?
│  │  ├─ Evict adapters
│  │  ├─ Check for leaks
│  │  └─ Reduce adapter count
│  │
│  └─ High CPU?
│     ├─ Check for infinite loops
│     └─ Reduce concurrent requests
│
└─ Data issues?
   ├─ Corruption?
   │  └─ PRAGMA integrity_check
   │
   ├─ Inconsistency?
   │  └─ Check tenant isolation
   │
   └─ Migration failed?
      └─ Verify signatures
```

### Quick Triage Decision Tree

```
1. Is the service running?
   NO  → Start service: ./start up
   YES → Continue to 2

2. Does /healthz return 200?
   NO  → Check database connection
   YES → Continue to 3

3. Does /readyz return 200?
   NO  → Check boot state: curl /readyz | jq .boot_state
   YES → Continue to 4

4. Can you authenticate?
   NO  → Use dev bypass: AOS_DEV_NO_AUTH=1
   YES → Continue to 5

5. Can you load an adapter?
   NO  → Check worker status
   YES → Continue to 6

6. Is inference fast (<500ms)?
   NO  → Check backend type (stub vs real)
   YES → System healthy
```

---

## Diagnostic Commands

### System Health Commands

```bash
# Full health check
curl -f http://localhost:8080/healthz && \
curl -f http://localhost:8080/readyz && \
echo "System healthy"

# Detailed status
curl -s http://localhost:8080/readyz | jq '{
  boot_state: .boot_state,
  worker_connected: .worker_connected,
  database_healthy: .database_healthy
}'

# Metrics summary
curl -s http://localhost:8080/v1/metrics/system | jq '{
  memory: .memory.used_percent,
  adapters_loaded: .adapters.loaded_count,
  inference_p99: .inference.p99_latency_ms,
  errors_1h: .errors.last_hour_count
}'
```

### Component Health Commands

```bash
# Database
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;" && \
sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*) FROM adapters;" && \
echo "Database healthy"

# Worker
ps aux | grep aos-worker && \
lsof var/run/aos/*/worker.sock && \
echo "Worker healthy"

# Backend
grep -i "backend.*initialized" var/logs/backend.log | tail -1
grep -i "stub" var/logs/backend.log | tail -1

# Disk space
df -h var/ | tail -1
du -sh var/{logs,adapters,aos-cp.sqlite3*}
```

### Log Analysis Commands

```bash
# Error summary (last hour)
grep ERROR var/logs/backend.log | \
  awk -F'ERROR' '{print $2}' | \
  cut -d':' -f1 | \
  sort | uniq -c | sort -nr

# Recent errors
tail -100 var/logs/backend.log | grep ERROR

# Specific error types
grep -i "database.*error" var/logs/backend.log | tail -10
grep -i "worker.*timeout" var/logs/backend.log | tail -10
grep -i "memory.*pressure" var/logs/backend.log | tail -10

# Error timeline
grep ERROR var/logs/backend.log | \
  awk '{print $1, $2}' | \
  cut -d'T' -f1,2 | \
  cut -d'+' -f1 | \
  uniq -c
```

### Performance Monitoring Commands

```bash
# Real-time metrics dashboard
watch -n 2 'curl -s http://localhost:8080/v1/metrics/system | jq "{
  timestamp: now | strftime(\"%H:%M:%S\"),
  memory_pct: .memory.used_percent,
  cpu_pct: .cpu.used_percent,
  adapters: .adapters.loaded_count,
  latency_p99: .inference.p99_latency_ms,
  queue: .inference.queue_depth,
  errors: .errors.last_minute_count
}"'

# Memory trend
for i in {1..60}; do
  mem=$(curl -s http://localhost:8080/v1/metrics/system | jq -r '.memory.used_percent')
  echo "$(date +%H:%M:%S) $mem%" >> /tmp/memory_trend.log
  sleep 5
done
cat /tmp/memory_trend.log

# Latency histogram
curl -s http://localhost:8080/v1/metrics/system | jq -r '
  .inference | {
    "p50": .p50_latency_ms,
    "p95": .p95_latency_ms,
    "p99": .p99_latency_ms,
    "max": .max_latency_ms
  } | to_entries | .[] | "\(.key): \(.value)ms"
'
```

### Diagnostic Report Generator

```bash
#!/bin/bash
# Generate comprehensive diagnostic report

echo "=== adapterOS Diagnostic Report ==="
echo "Generated: $(date)"
echo

echo "=== System Info ==="
uname -a
sw_vers 2>/dev/null || cat /etc/os-release
echo

echo "=== Service Status ==="
ps aux | grep adapteros
echo

echo "=== Health Checks ==="
curl -f http://localhost:8080/healthz && echo "Health: OK" || echo "Health: FAIL"
curl -f http://localhost:8080/readyz && echo "Ready: OK" || echo "Ready: FAIL"
echo

echo "=== Metrics ==="
curl -s http://localhost:8080/v1/metrics/system | jq .
echo

echo "=== Database Status ==="
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"
sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*) as adapter_count FROM adapters;"
sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*) as tenant_count FROM tenants;"
echo

echo "=== Worker Status ==="
lsof var/run/aos/*/worker.sock
echo

echo "=== Disk Usage ==="
df -h var/
du -sh var/*
echo

echo "=== Recent Errors ==="
tail -50 var/logs/backend.log | grep ERROR
echo

echo "=== End of Report ==="
```

---

## Quick Reference

### Common Commands by Scenario

#### Service Won't Start
```bash
# Check port
lsof -ti:8080 | xargs kill

# Check database
ls -la var/aos-cp.sqlite3
sqlite3 var/aos-cp.sqlite3 "SELECT 1;"

# Check migrations
cargo sqlx migrate info

# Start service
./start up
```

#### Adapter Won't Load
```bash
# Check memory
curl -s http://localhost:8080/v1/metrics/system | jq '.memory.headroom_pct'

# Check adapter state
curl -s http://localhost:8080/v1/adapters/ADAPTER_ID | jq '.status'

# Load adapter
curl -X POST http://localhost:8080/v1/adapters/ADAPTER_ID/load

# Check worker
ps aux | grep aos-worker
lsof var/run/aos/*/worker.sock
```

#### Slow Inference
```bash
# Check backend
grep -i "backend.*initialized" var/logs/backend.log | tail -1

# Check GPU
system_profiler SPDisplaysDataType | grep Metal

# Check queue
curl -s http://localhost:8080/v1/metrics/system | jq '.inference.queue_depth'

# Benchmark
time curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "base", "messages": [{"role": "user", "content": "test"}], "max_tokens": 10}'
```

#### Database Issues
```bash
# Check integrity
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"

# Checkpoint WAL
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"

# Vacuum
sqlite3 var/aos-cp.sqlite3 "VACUUM;"

# Analyze
sqlite3 var/aos-cp.sqlite3 "ANALYZE;"
```

---

## Additional Resources

- [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) - Main troubleshooting guide
- [MLX_TROUBLESHOOTING.md](./MLX_TROUBLESHOOTING.md) - MLX-specific issues
- [BOOT_TROUBLESHOOTING.md](./BOOT_TROUBLESHOOTING.md) - Boot sequence issues
- [ERRORS.md](./ERRORS.md) - Error handling reference
- [Runbooks](./runbooks/) - Production incident response
- [OPERATIONS.md](./OPERATIONS.md) - Operations guide
- [AGENTS.md](../AGENTS.md) - System architecture

---

**MLNavigator Inc © 2025**
