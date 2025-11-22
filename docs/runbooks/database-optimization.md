# Database Optimization

PRAGMA optimization, WAL checkpoints, and performance tuning.

## Overview

AdapterOS uses SQLite with WAL (Write-Ahead Logging) mode for the control plane database. Regular optimization ensures good performance and prevents issues like:

- WAL file growing too large
- Query performance degradation
- Fragmented database
- Slow checkpoint operations

## WAL Mode Configuration

**Current Settings:**
```sql
PRAGMA journal_mode;          -- Should return: wal
PRAGMA synchronous;           -- Should return: normal
PRAGMA wal_autocheckpoint;    -- Should return: 1000
```

**Verify WAL mode:**
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
PRAGMA journal_mode;
PRAGMA synchronous;
PRAGMA wal_autocheckpoint;
EOF
```

**Expected output:**
```
wal
1
1000
```

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs:33-56` - WAL mode configuration
- `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs:46` - Journal mode setup

## Regular Maintenance Tasks

### 1. WAL Checkpoint

**Purpose:** Moves WAL contents back to main database file

**When to run:**
- WAL file > 10MB
- During maintenance windows
- After bulk operations
- Before backups

**Commands:**
```bash
# Passive checkpoint (doesn't block)
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(PASSIVE);"

# Full checkpoint (waits for readers)
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(FULL);"

# Truncate checkpoint (resets WAL file)
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"
```

**Checkpoint types:**
- `PASSIVE`: Checkpoint as much as possible without blocking
- `FULL`: Wait for concurrent readers to finish
- `TRUNCATE`: Full checkpoint + reset WAL file to zero size

**Check WAL size:**
```bash
ls -lh var/aos-cp.sqlite3-wal

# Or
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint;" | head -1
```

**Automated checkpoint script:**
```bash
#!/bin/bash
# checkpoint.sh - Run WAL checkpoint

DB="var/aos-cp.sqlite3"
WAL_THRESHOLD_MB=10

# Check WAL size
WAL_SIZE=$(stat -f%z "$DB-wal" 2>/dev/null || echo 0)
WAL_SIZE_MB=$((WAL_SIZE / 1024 / 1024))

if [ $WAL_SIZE_MB -gt $WAL_THRESHOLD_MB ]; then
  echo "WAL file is ${WAL_SIZE_MB}MB, running checkpoint..."
  sqlite3 "$DB" "PRAGMA wal_checkpoint(TRUNCATE);"
  echo "Checkpoint complete"
else
  echo "WAL file is ${WAL_SIZE_MB}MB, no checkpoint needed"
fi
```

### 2. PRAGMA Optimize

**Purpose:** Updates query planner statistics for better query plans

**When to run:**
- Daily during low-traffic periods
- After large data changes
- After database restore
- When queries slow down

**Command:**
```bash
sqlite3 var/aos-cp.sqlite3 "PRAGMA optimize;"
```

**With analysis:**
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
PRAGMA analysis_limit=1000;
ANALYZE;
PRAGMA optimize;
EOF
```

**Automated optimize script:**
```bash
#!/bin/bash
# optimize.sh - Run database optimization

DB="var/aos-cp.sqlite3"

echo "Running PRAGMA optimize..."
sqlite3 "$DB" "PRAGMA optimize;"
echo "Optimization complete"

# Check statistics
echo "Analyzing statistics..."
sqlite3 "$DB" <<EOF
.mode column
.headers on
SELECT name, sql FROM sqlite_master
WHERE type='index' AND name LIKE 'sqlite_stat%';
EOF
```

### 3. Integrity Check

**Purpose:** Verifies database is not corrupted

**When to run:**
- Weekly as preventive maintenance
- After system crashes
- Before major upgrades
- After hardware issues

**Command:**
```bash
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"
```

**Expected output:** `ok`

**Full check:**
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
PRAGMA integrity_check;
PRAGMA foreign_key_check;
PRAGMA quick_check;
EOF
```

**Automated integrity check:**
```bash
#!/bin/bash
# integrity-check.sh - Verify database integrity

DB="var/aos-cp.sqlite3"
LOG="var/integrity-check-$(date +%Y%m%d).log"

echo "Running integrity check at $(date)" | tee $LOG

# Stop server for consistent check
echo "Stopping server..."
pkill -SIGTERM aos-cp
sleep 5

# Run checks
echo "=== Integrity Check ===" | tee -a $LOG
sqlite3 "$DB" "PRAGMA integrity_check;" | tee -a $LOG

echo "=== Foreign Key Check ===" | tee -a $LOG
sqlite3 "$DB" "PRAGMA foreign_key_check;" | tee -a $LOG

echo "=== Quick Check ===" | tee -a $LOG
sqlite3 "$DB" "PRAGMA quick_check;" | tee -a $LOG

# Restart server
echo "Starting server..."
./scripts/start_server.sh

echo "Check complete, log: $LOG"
```

### 4. VACUUM

**Purpose:** Rebuilds database to reclaim space and defragment

**When to run:**
- After deleting large amounts of data
- When free pages > 25% of database
- Monthly maintenance
- When file size bloated

**⚠️ WARNING:** VACUUM requires exclusive lock and can be slow

**Command:**
```bash
# Stop server first
pkill -SIGTERM aos-cp
sleep 5

# Run vacuum
sqlite3 var/aos-cp.sqlite3 "VACUUM;"

# Restart server
./scripts/start_server.sh
```

**Check free pages:**
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
.mode column
.headers on
SELECT
  page_count,
  freelist_count,
  (freelist_count * 100.0 / page_count) as free_pct
FROM pragma_page_count, pragma_freelist_count;
EOF
```

**If free_pct > 25%, consider VACUUM**

### 5. Index Maintenance

**Purpose:** Rebuild indexes for optimal performance

**When to run:**
- After bulk updates
- When queries slow down
- After VACUUM
- Monthly maintenance

**Commands:**
```bash
# List indexes
sqlite3 var/aos-cp.sqlite3 <<EOF
.mode column
.headers on
SELECT name, tbl_name, sql
FROM sqlite_master
WHERE type='index';
EOF

# Rebuild specific index
sqlite3 var/aos-cp.sqlite3 "REINDEX idx_adapters_state;"

# Rebuild all indexes
sqlite3 var/aos-cp.sqlite3 "REINDEX;"
```

**Analyze index usage:**
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
.mode column
.headers on
SELECT
  idx,
  stat
FROM sqlite_stat1
ORDER BY idx;
EOF
```

## Performance Tuning

### Connection Pool Settings

**Current settings (in code):**
```rust
max_connections: 20
busy_timeout: 30s
statement_cache_capacity: 100
```

**To adjust (requires code change):**
- Increase max_connections if pool saturation > 80%
- Increase busy_timeout if locks frequent
- Increase cache if many repeated queries

**Related Files:**
- `/Users/star/Dev/aos/crates/adapteros-db/src/lib.rs:39-53` - Pool configuration

### Query Optimization

**Slow query detection:**
```bash
# Enable query logging (requires code change)
RUST_LOG=sqlx=debug cargo run --bin aos-cp

# Analyze slow queries in logs
grep "query" var/aos-cp.log | grep -E "[0-9]{3,}ms"
```

**Common optimization techniques:**
1. Add indexes for frequently queried columns
2. Use EXPLAIN QUERY PLAN to analyze queries
3. Avoid full table scans
4. Use prepared statements (default in sqlx)

**Example analysis:**
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
EXPLAIN QUERY PLAN
SELECT * FROM adapters
WHERE current_state = 'hot' AND tenant_id = 'default';
EOF
```

### Pragma Tuning

**Cache size:**
```bash
# Check current cache size
sqlite3 var/aos-cp.sqlite3 "PRAGMA cache_size;"

# Set cache size (in KB, negative = KB)
sqlite3 var/aos-cp.sqlite3 "PRAGMA cache_size = -64000;"  # 64MB

# Make persistent
sqlite3 var/aos-cp.sqlite3 "PRAGMA cache_size = -64000; VACUUM;"
```

**Temp store:**
```bash
# Use memory for temp tables
sqlite3 var/aos-cp.sqlite3 "PRAGMA temp_store = MEMORY;"
```

**Page size (requires VACUUM):**
```bash
# Check current page size
sqlite3 var/aos-cp.sqlite3 "PRAGMA page_size;"

# Change page size (4096 is good default)
sqlite3 var/aos-cp.sqlite3 "PRAGMA page_size = 4096; VACUUM;"
```

## Maintenance Schedule

### Daily Tasks (Automated)
```cron
# 2 AM daily - WAL checkpoint
0 2 * * * /path/to/checkpoint.sh

# 3 AM daily - Optimize
0 3 * * * /path/to/optimize.sh
```

### Weekly Tasks (Manual)
```bash
# Sunday 2 AM - Integrity check
0 2 * * 0 /path/to/integrity-check.sh
```

### Monthly Tasks (Planned Maintenance)
```bash
# First Sunday of month - VACUUM
# Run during maintenance window
# 1. Stop traffic
# 2. Stop server
# 3. Run VACUUM
# 4. Restart server
# 5. Verify health
```

## Monitoring Database Health

**Database metrics dashboard:**
```bash
#!/bin/bash
# db-metrics.sh - Database health metrics

DB="var/aos-cp.sqlite3"

echo "=== Database Metrics ==="
echo ""

echo "File sizes:"
ls -lh "$DB"* | awk '{print "  " $9, $5}'
echo ""

echo "WAL mode:"
sqlite3 "$DB" "PRAGMA journal_mode;"
echo ""

echo "Page stats:"
sqlite3 "$DB" <<EOF
SELECT
  'Total pages: ' || page_count,
  'Free pages: ' || freelist_count,
  'Free %: ' || ROUND(freelist_count * 100.0 / page_count, 2)
FROM pragma_page_count, pragma_freelist_count;
EOF
echo ""

echo "Integrity:"
sqlite3 "$DB" "PRAGMA quick_check;"
echo ""

echo "Last optimize:"
stat -f "  %Sm" -t "%Y-%m-%d %H:%M:%S" "$DB"
```

**Track metrics over time:**
```bash
#!/bin/bash
# db-metrics-log.sh - Log metrics to CSV

DB="var/aos-cp.sqlite3"
LOG="var/db-metrics.csv"

TIMESTAMP=$(date +%s)
FILE_SIZE=$(stat -f%z "$DB")
WAL_SIZE=$(stat -f%z "$DB-wal" 2>/dev/null || echo 0)
PAGE_COUNT=$(sqlite3 "$DB" "SELECT page_count FROM pragma_page_count;")
FREE_COUNT=$(sqlite3 "$DB" "SELECT freelist_count FROM pragma_freelist_count;")

echo "$TIMESTAMP,$FILE_SIZE,$WAL_SIZE,$PAGE_COUNT,$FREE_COUNT" >> $LOG
```

## Backup Before Optimization

**Always backup before major operations:**
```bash
#!/bin/bash
# backup-before-optimize.sh

DB="var/aos-cp.sqlite3"
BACKUP_DIR="var/backups"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

mkdir -p "$BACKUP_DIR"

echo "Creating backup..."

# Stop server
pkill -SIGTERM aos-cp
sleep 5

# Backup database
cp "$DB" "$BACKUP_DIR/aos-cp-$TIMESTAMP.sqlite3"
cp "$DB-wal" "$BACKUP_DIR/aos-cp-$TIMESTAMP.sqlite3-wal" 2>/dev/null || true
cp "$DB-shm" "$BACKUP_DIR/aos-cp-$TIMESTAMP.sqlite3-shm" 2>/dev/null || true

# Verify backup
sqlite3 "$BACKUP_DIR/aos-cp-$TIMESTAMP.sqlite3" "PRAGMA integrity_check;" > /dev/null
if [ $? -eq 0 ]; then
  echo "Backup verified: $BACKUP_DIR/aos-cp-$TIMESTAMP.sqlite3"
else
  echo "ERROR: Backup verification failed"
  exit 1
fi

# Restart server
./scripts/start_server.sh
```

## Troubleshooting Performance Issues

### Issue: Slow Queries

**Diagnosis:**
```bash
# Check query plan
sqlite3 var/aos-cp.sqlite3 <<EOF
EXPLAIN QUERY PLAN
SELECT * FROM adapters WHERE tenant_id = 'default';
EOF
```

**Fix:**
```bash
# Add missing index
sqlite3 var/aos-cp.sqlite3 <<EOF
CREATE INDEX IF NOT EXISTS idx_adapters_tenant
ON adapters(tenant_id);
EOF

# Rebuild statistics
sqlite3 var/aos-cp.sqlite3 "ANALYZE; PRAGMA optimize;"
```

### Issue: Large WAL File

**Diagnosis:**
```bash
ls -lh var/aos-cp.sqlite3-wal
```

**Fix:**
```bash
# Force checkpoint
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"

# Verify reduced
ls -lh var/aos-cp.sqlite3-wal
```

### Issue: Database Bloat

**Diagnosis:**
```bash
sqlite3 var/aos-cp.sqlite3 <<EOF
SELECT
  'DB size: ' || (page_count * page_size / 1024 / 1024) || ' MB',
  'Free pages: ' || freelist_count,
  'Wasted: ' || (freelist_count * page_size / 1024 / 1024) || ' MB'
FROM pragma_page_count, pragma_page_size, pragma_freelist_count;
EOF
```

**Fix:**
```bash
# Run VACUUM (requires downtime)
pkill -SIGTERM aos-cp
sleep 5
sqlite3 var/aos-cp.sqlite3 "VACUUM;"
./scripts/start_server.sh
```

## Related Runbooks

- [Database Failures](./DATABASE-FAILURES.md)
- [Cleanup Procedures](./CLEANUP-PROCEDURES.md)
- [Metrics Review](./METRICS-REVIEW.md)

## Related Documentation

- [Database Schema](../DATABASE-SCHEMA.md)
- [Production Operations](../PRODUCTION_OPERATIONS.md)

## Escalation Criteria

Escalate if:
- VACUUM fails or takes > 1 hour
- Integrity check reports corruption
- Performance degrades despite optimization
- WAL file cannot be checkpointed
- See [Escalation Guide](./ESCALATION.md)
