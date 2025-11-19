# Cleanup Procedures

TTL cleanup, orphaned adapters, and resource maintenance.

## Overview

AdapterOS includes automatic cleanup mechanisms for:
- Expired adapters (TTL-based)
- Orphaned adapters (failed/stuck states)
- Expired pins
- Old telemetry bundles
- Stale heartbeats

These procedures can also be run manually for maintenance.

## Automatic Cleanup Tasks

### TTL Cleanup Task

**Purpose:** Removes adapters that have exceeded their time-to-live

**Schedule:** Every 5 minutes (300 seconds)

**What it does:**
1. Finds adapters where `expires_at < current_time`
2. Deletes expired adapters from database
3. Cleans up associated files
4. Logs deletion events

**Related Code:**
```
/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:843-895
```

**View in logs:**
```bash
grep "TTL cleanup" var/aos-cp.log | tail -10
grep "expired adapter" var/aos-cp.log | tail -10
```

### Heartbeat Recovery Task

**Purpose:** Recovers adapters that haven't sent heartbeat in 5 minutes

**Schedule:** Every 5 minutes (300 seconds)

**What it does:**
1. Finds adapters with stale heartbeats (> 300s)
2. Transitions to recovery state
3. Cleans up resources
4. Logs recovery events

**Related Code:**
```
/Users/star/Dev/aos/crates/adapteros-server/src/main.rs:897-926
```

**View in logs:**
```bash
grep "Heartbeat recovery" var/aos-cp.log | tail -10
grep "stale adapters" var/aos-cp.log | tail -10
```

## Manual Cleanup Commands

### 1. Garbage Collection (GC)

**Purpose:** Clean up orphaned and expired adapters

**Command:**
```bash
aosctl maintenance gc
```

**With force (ignores some safety checks):**
```bash
aosctl maintenance gc --force
```

**What it does:**
- Removes orphaned adapters (stuck in loading/failed > 1 hour)
- Removes expired adapters (past TTL)
- Cleans up adapter files
- Updates database states

**Expected output:**
```
Found 3 orphaned adapters
Found 2 expired adapters
Cleaning up 5 adapters...
✓ Deleted adapter ephemeral_001 (expired)
✓ Deleted adapter temp_002 (orphaned)
✓ Deleted adapter test_003 (expired)
✓ Deleted adapter debug_004 (orphaned)
✓ Deleted adapter old_005 (expired)
GC complete: 5 adapters removed
```

### 2. Sweep Orphaned Adapters

**Purpose:** Find and clean up adapters in problematic states

**Command:**
```bash
aosctl maintenance sweep-orphaned
```

**With age threshold:**
```bash
aosctl maintenance sweep-orphaned --older-than 3600  # seconds
```

**What it does:**
- Finds adapters stuck in `loading` state > 1 hour
- Finds adapters stuck in `failed` state
- Finds adapters with missing files
- Transitions to `cold` or deletes

**Example:**
```bash
# Find adapters stuck in loading for 1+ hours
sqlite3 var/aos-cp.sqlite3 <<EOF
SELECT
  adapter_id,
  name,
  current_state,
  datetime(updated_at, 'unixepoch') as last_update,
  (strftime('%s', 'now') - updated_at) as age_seconds
FROM adapters
WHERE current_state = 'loading'
  AND (strftime('%s', 'now') - updated_at) > 3600;
EOF
```

### 3. Pin Cleanup

**Purpose:** Remove expired pins

**Command:**
```bash
aosctl maintenance cleanup-pins
```

**What it does:**
- Finds pins where `expires_at < current_time`
- Removes from `pinned_adapters` table
- Allows adapters to be evicted again

**Manual query:**
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
DELETE FROM pinned_adapters
WHERE expires_at IS NOT NULL
  AND expires_at < strftime('%s', 'now');
EOF
```

**List expired pins:**
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
.mode column
.headers on
SELECT
  tenant_id,
  adapter_id,
  datetime(expires_at, 'unixepoch') as expired_at,
  reason
FROM pinned_adapters
WHERE expires_at IS NOT NULL
  AND expires_at < strftime('%s', 'now');
EOF
```

### 4. Telemetry Cleanup

**Purpose:** Archive or delete old telemetry bundles

**Command:**
```bash
aosctl maintenance cleanup-telemetry --older-than 7d
```

**What it does:**
- Finds bundles older than specified age
- Optionally archives to separate location
- Deletes from active telemetry directory

**Manual cleanup:**
```bash
# List old telemetry bundles
find var/telemetry -name "bundle_*.ndjson" -mtime +7 -ls

# Archive old bundles
mkdir -p var/telemetry/archive
find var/telemetry -name "bundle_*.ndjson" -mtime +7 -exec mv {} var/telemetry/archive/ \;

# Delete archived bundles
rm -f var/telemetry/archive/bundle_*.ndjson
```

**Check telemetry size:**
```bash
du -sh var/telemetry/
du -sh var/telemetry/*.ndjson | sort -h | tail -10
```

### 5. Database Cleanup

**Purpose:** Clean up audit logs and old records

**Commands:**
```bash
# Clean old audit logs (> 90 days)
sqlite3 var/aos-cp.sqlite3 <<EOF
DELETE FROM audit_log
WHERE created_at < strftime('%s', 'now') - (90 * 86400);
EOF

# Clean old telemetry events (> 30 days)
sqlite3 var/aos-cp.sqlite3 <<EOF
DELETE FROM telemetry_events
WHERE timestamp < strftime('%s', 'now') - (30 * 86400);
EOF

# Clean deleted adapter records
sqlite3 var/aos-cp.sqlite3 <<EOF
DELETE FROM adapters
WHERE deleted_at IS NOT NULL
  AND deleted_at < strftime('%s', 'now') - (7 * 86400);
EOF
```

## Cleanup Procedures by Scenario

### Scenario 1: Low Disk Space

**Symptoms:**
- Disk usage > 90%
- "No space left on device" errors
- Database operations failing

**Cleanup procedure:**
```bash
# Step 1: Check disk usage
df -h .
du -sh var/*

# Step 2: Identify large files
du -sh var/* | sort -h | tail -10

# Step 3: Clean telemetry (usually largest)
du -sh var/telemetry/
aosctl maintenance cleanup-telemetry --older-than 3d

# Step 4: Clean adapters
aosctl maintenance gc --force

# Step 5: Clean database
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"

# Step 6: Archive old logs
gzip var/aos-cp.log.*
mv var/aos-cp.log.*.gz var/archive/

# Step 7: Verify space recovered
df -h .
```

### Scenario 2: Too Many Adapters

**Symptoms:**
- Hundreds of adapters in database
- Slow adapter queries
- Memory pressure from adapter metadata

**Cleanup procedure:**
```bash
# Step 1: Count adapters by state
sqlite3 var/aos-cp.sqlite3 <<EOF
SELECT current_state, COUNT(*) as count
FROM adapters
GROUP BY current_state
ORDER BY count DESC;
EOF

# Step 2: Identify expired ephemeral adapters
sqlite3 var/aos-cp.sqlite3 <<EOF
.mode column
.headers on
SELECT
  adapter_id,
  tier,
  datetime(created_at, 'unixepoch') as created,
  datetime(expires_at, 'unixepoch') as expires
FROM adapters
WHERE tier = 'ephemeral'
  AND (expires_at IS NULL OR expires_at < strftime('%s', 'now'))
ORDER BY created_at
LIMIT 20;
EOF

# Step 3: Delete old ephemeral adapters
aosctl maintenance gc --force

# Step 4: Delete test/development adapters
sqlite3 var/aos-cp.sqlite3 <<EOF
DELETE FROM adapters
WHERE name LIKE 'test_%'
   OR name LIKE 'dev_%'
   OR name LIKE 'tmp_%';
EOF

# Step 5: Verify adapter count reduced
aosctl adapter list | wc -l
```

### Scenario 3: Corrupted Adapter State

**Symptoms:**
- Adapters stuck in loading/transitioning
- Lifecycle state machine errors
- Cannot load or unload adapters

**Cleanup procedure:**
```bash
# Step 1: Find problematic adapters
sqlite3 var/aos-cp.sqlite3 <<EOF
.mode column
.headers on
SELECT
  adapter_id,
  current_state,
  datetime(updated_at, 'unixepoch') as last_update,
  (strftime('%s', 'now') - updated_at) as age_seconds
FROM adapters
WHERE current_state IN ('loading', 'transitioning', 'failed')
ORDER BY updated_at;
EOF

# Step 2: Force state reset
aosctl maintenance sweep-orphaned --force

# Step 3: Manual reset if needed
sqlite3 var/aos-cp.sqlite3 <<EOF
UPDATE adapters
SET current_state = 'cold',
    updated_at = strftime('%s', 'now')
WHERE current_state IN ('loading', 'transitioning')
  AND updated_at < strftime('%s', 'now') - 3600;
EOF

# Step 4: Verify states corrected
aosctl adapter list --state loading
aosctl adapter list --state transitioning
```

### Scenario 4: Memory Pressure from Pins

**Symptoms:**
- Too many pinned adapters
- Cannot evict to free memory
- Persistent memory pressure

**Cleanup procedure:**
```bash
# Step 1: List pinned adapters
aosctl adapter list --pinned

# Step 2: Identify unnecessary pins
sqlite3 var/aos-cp.sqlite3 <<EOF
.mode column
.headers on
SELECT
  tenant_id,
  adapter_id,
  datetime(pinned_at, 'unixepoch') as pinned,
  datetime(expires_at, 'unixepoch') as expires,
  reason
FROM pinned_adapters
ORDER BY pinned_at;
EOF

# Step 3: Unpin non-critical adapters
aosctl unpin-adapter --tenant default --adapter <adapter-id>

# Step 4: Set TTLs on development pins
aosctl pin-adapter --tenant dev --adapter test-adapter \
  --ttl-hours 24 --reason "Testing" --replace

# Step 5: Clean expired pins
aosctl maintenance cleanup-pins

# Step 6: Verify memory improved
aosctl status memory
```

## Cleanup Scripts

### Daily Cleanup Script
```bash
#!/bin/bash
# daily-cleanup.sh - Run daily maintenance tasks

set -e

echo "=== Daily Cleanup $(date) ==="

# Expired adapters
echo "Cleaning expired adapters..."
aosctl maintenance gc

# Expired pins
echo "Cleaning expired pins..."
aosctl maintenance cleanup-pins

# Old telemetry (> 7 days)
echo "Cleaning old telemetry..."
aosctl maintenance cleanup-telemetry --older-than 7d

# WAL checkpoint
echo "Checkpointing database..."
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(PASSIVE);"

# Summary
echo ""
echo "=== Cleanup Summary ==="
echo "Adapters: $(aosctl adapter list | wc -l)"
echo "Disk usage: $(du -sh var/ | awk '{print $1}')"
echo "Database size: $(ls -lh var/aos-cp.sqlite3 | awk '{print $5}')"
echo "WAL size: $(ls -lh var/aos-cp.sqlite3-wal 2>/dev/null | awk '{print $5}' || echo '0')"
echo ""
```

### Weekly Cleanup Script
```bash
#!/bin/bash
# weekly-cleanup.sh - Run weekly aggressive maintenance

set -e

echo "=== Weekly Cleanup $(date) ==="

# Stop server for maintenance
echo "Stopping server..."
pkill -SIGTERM aos-cp
sleep 10

# Aggressive GC
echo "Running aggressive GC..."
aosctl maintenance gc --force

# Sweep orphaned
echo "Sweeping orphaned adapters..."
aosctl maintenance sweep-orphaned

# Clean old audit logs (> 90 days)
echo "Cleaning old audit logs..."
sqlite3 var/aos-cp.sqlite3 <<EOF
DELETE FROM audit_log
WHERE created_at < strftime('%s', 'now') - (90 * 86400);
EOF

# Optimize database
echo "Optimizing database..."
sqlite3 var/aos-cp.sqlite3 "PRAGMA optimize;"

# Checkpoint WAL
echo "Truncating WAL..."
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"

# Restart server
echo "Starting server..."
./scripts/start_server.sh

echo "Weekly cleanup complete"
```

## Monitoring Cleanup Operations

**Track cleanup metrics:**
```bash
# Adapters cleaned per day
grep "Deleted adapter" var/aos-cp.log | \
  cut -d' ' -f1 | cut -d'T' -f1 | \
  uniq -c

# GC frequency
grep "GC collected" var/aos-cp.log | tail -20

# Cleanup failures
grep -E "Failed to delete|cleanup failed" var/aos-cp.log | tail -10
```

**Cleanup dashboard:**
```bash
#!/bin/bash
# cleanup-dashboard.sh

echo "=== Cleanup Dashboard ==="
echo ""

echo "Adapter counts:"
sqlite3 var/aos-cp.sqlite3 <<EOF
SELECT current_state, COUNT(*) FROM adapters GROUP BY current_state;
EOF

echo ""
echo "Expired adapters:"
sqlite3 var/aos-cp.sqlite3 <<EOF
SELECT COUNT(*) FROM adapters
WHERE expires_at IS NOT NULL AND expires_at < strftime('%s', 'now');
EOF

echo ""
echo "Orphaned adapters:"
sqlite3 var/aos-cp.sqlite3 <<EOF
SELECT COUNT(*) FROM adapters
WHERE current_state IN ('loading', 'failed')
  AND updated_at < strftime('%s', 'now') - 3600;
EOF

echo ""
echo "Pinned adapters:"
sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*) FROM pinned_adapters;"

echo ""
echo "Telemetry bundles:"
ls -1 var/telemetry/*.ndjson 2>/dev/null | wc -l
echo "Telemetry size: $(du -sh var/telemetry | awk '{print $1}')"

echo ""
echo "Recent cleanup operations:"
grep -E "GC|TTL cleanup|Deleted adapter" var/aos-cp.log | tail -5
```

## Related Runbooks

- [Memory Pressure](./memory-pressure.md)
- [Database Optimization](./database-optimization.md)
- [Database Failures](./database-failures.md)

## Escalation Criteria

Escalate if:
- Cleanup operations fail repeatedly
- Disk space cannot be recovered
- Orphaned adapters cannot be cleaned
- Database cleanup causes corruption
- See [Escalation Guide](./escalation.md)
