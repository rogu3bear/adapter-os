# Runbook: Disk Full

**Scenario:** Disk space exhausted, causing write failures

**Severity:** SEV-2 (15-minute response time, escalates to SEV-1 if database affected)

**Last Updated:** 2025-12-15

---

## Symptoms

### Alert Indicators
- **Alert:** `DiskUsageHigh` (disk > 85% full for 10+ minutes)
- **Alert:** `DiskUsageCritical` (disk > 95% full for 2+ minutes)
- **Alert:** `DiskFullImminent` (< 1GB free space)
- **Prometheus Query:** `disk_usage_percent{path="/var"} > 90`

### User Reports
- Training jobs failing with "No space left on device"
- Chat sessions not saving
- "Upload failed" errors when registering adapters
- Database write errors

### System Indicators
- SQLite errors: "database or disk is full"
- Log rotation failures
- WAL file not truncating
- Telemetry bundles not being created
- Backup failures

---

## Diagnosis Steps

### 1. Verify Disk Usage

```bash
# Check overall disk space
df -h

# Focus on AdapterOS data directory
df -h var/

# Check inode usage (can cause "disk full" even with space)
df -i var/

# Get detailed usage breakdown
du -sh var/*/ | sort -hr | head -20
```

**Critical Thresholds:**
- **85-90%:** Warning - cleanup recommended
- **90-95%:** High - cleanup required
- **95-100%:** Critical - immediate cleanup

**If > 95%:** Escalate to SEV-1, proceed immediately to Quick Fix
**If 90-95%:** Continue diagnosis to identify largest consumers

### 2. Identify Largest Space Consumers

```bash
# Check each major directory
echo "=== Database files ==="
du -sh var/aos-cp.sqlite3*
ls -lh var/aos-cp.sqlite3*

echo "=== Logs ==="
du -sh var/logs/
ls -lh var/logs/ | head -20

echo "=== Adapters ==="
du -sh var/adapters/
ls -lh var/adapters/repo/ | head -20

echo "=== Model cache ==="
du -sh var/model-cache/
du -sh var/model-cache/*/

echo "=== Telemetry ==="
du -sh var/telemetry/
ls -lh var/telemetry/ | head -20

echo "=== Artifacts ==="
du -sh var/artifacts/
ls -lh var/artifacts/ | head -20

echo "=== KV store ==="
du -sh var/aos-kv.redb
du -sh var/aos-kv-index/

echo "=== Temporary files ==="
du -sh var/tmp 2>/dev/null || echo "No var/tmp directory"
```

**Common Large Consumers:**
- **Database WAL:** `aos-cp.sqlite3-wal` (can grow to several GB)
- **Logs:** `var/logs/` (grows 100MB-1GB per day under load)
- **Telemetry bundles:** `var/telemetry/` (can accumulate quickly)
- **Model cache:** `var/model-cache/` (5-20GB per model)
- **Adapters:** `var/adapters/` (50-500MB per adapter)

### 3. Check for Specific Issues

**Database WAL Growth:**
```bash
# Check WAL file size
ls -lh var/aos-cp.sqlite3-wal

# Check if WAL checkpoint is running
lsof var/aos-cp.sqlite3-wal

# Check recent checkpoints in logs
grep -i "wal.*checkpoint" var/aos-cp.log | tail -10
```

**If WAL > 1GB:** Database not checkpointing properly (see Resolution)

**Log Rotation Issues:**
```bash
# Check if logrotate is working
ls -lt var/logs/ | head -20

# Look for .gz files (rotated logs)
ls -lh var/logs/*.gz

# Check logrotate config
cat /etc/logrotate.d/adapteros 2>/dev/null || echo "No logrotate config"

# Check for rotation errors
grep -i "logrotate" /var/log/syslog 2>/dev/null | tail -10
```

**Telemetry Accumulation:**
```bash
# Count telemetry bundles
find var/telemetry/ -name "*.jsonl" | wc -l

# Check oldest bundle
find var/telemetry/ -name "*.jsonl" | head -1 | xargs ls -lh

# Check if cleanup job is running
grep -i "telemetry.*cleanup" var/aos-cp.log | tail -10
```

---

## Resolution

### Quick Fix: Emergency Space Reclamation (> 95% Full)

**Immediate Actions (in order of safety):**

```bash
# 1. Compress old logs (safe, reversible)
find var/logs/ -name "*.log" -mtime +7 -exec gzip {} \;
df -h var/  # Check if space freed

# 2. Delete old telemetry bundles (safe, can regenerate)
find var/telemetry/ -name "*.jsonl" -mtime +30 -delete
df -h var/  # Check if space freed

# 3. Checkpoint and truncate WAL (safe, but locks DB briefly)
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"
df -h var/  # Check if space freed

# 4. Delete old compressed logs (safe, historical data only)
find var/logs/ -name "*.gz" -mtime +30 -delete
df -h var/  # Check if space freed

# 5. Clean old training artifacts (safe if models already packaged)
find var/artifacts/ -type f -mtime +60 -delete
df -h var/  # Check if space freed

# 6. If still critical, delete temp files (safe)
rm -rf var/tmp/* 2>/dev/null
df -h var/  # Check if space freed
```

**Target:** Get disk usage below 85% before investigating root cause

### Root Cause Fix: Database WAL Management

**If WAL File Too Large (> 500MB):**

```bash
# 1. Check database integrity first
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"
# Expected: "ok"

# 2. Stop control plane (ensures clean checkpoint)
pkill -f adapteros-server

# 3. Force checkpoint and truncate
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"

# 4. Verify WAL reduced
ls -lh var/aos-cp.sqlite3-wal
# Should be small (< 100MB) or deleted

# 5. Configure auto-checkpoint more aggressively
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_autocheckpoint=1000;"
# Checkpoints every 1000 pages (~4MB) instead of default

# 6. Restart control plane
./start up

# 7. Monitor WAL size
watch -n 300 'ls -lh var/aos-cp.sqlite3-wal'
```

**Permanent Fix (add to startup script):**
```bash
# In scripts/startup.sh or systemd service
sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_autocheckpoint=1000;" || true
```

### Root Cause Fix: Log Management

**Configure Log Rotation:**

```bash
# Create logrotate config
sudo tee /etc/logrotate.d/adapteros <<'EOF'
/var/lib/adapteros/logs/*.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    create 0644 adapteros adapteros
    sharedscripts
    postrotate
        pkill -HUP -f adapteros-server || true
    endscript
}
EOF

# Test logrotate config
sudo logrotate -d /etc/logrotate.d/adapteros

# Force rotation now
sudo logrotate -f /etc/logrotate.d/adapteros

# Verify rotation worked
ls -lh var/logs/
```

**Alternative: Manual Cleanup Script**
```bash
# Create cleanup script
cat > scripts/cleanup-logs.sh <<'EOF'
#!/bin/bash
# AdapterOS Log Cleanup Script

LOG_DIR="var/logs"
RETENTION_DAYS=30

# Compress logs older than 7 days
find "$LOG_DIR" -name "*.log" -mtime +7 -exec gzip {} \;

# Delete compressed logs older than retention
find "$LOG_DIR" -name "*.gz" -mtime +$RETENTION_DAYS -delete

# Delete empty log files
find "$LOG_DIR" -name "*.log" -size 0 -delete

echo "Log cleanup completed: $(date)"
df -h var/
EOF

chmod +x scripts/cleanup-logs.sh

# Add to cron (daily at 2 AM)
(crontab -l 2>/dev/null; echo "0 2 * * * /path/to/adapter-os/scripts/cleanup-logs.sh") | crontab -
```

### Root Cause Fix: Telemetry Cleanup

**Automated Telemetry Archival:**

```bash
# Create telemetry cleanup script
cat > scripts/cleanup-telemetry.sh <<'EOF'
#!/bin/bash
# AdapterOS Telemetry Cleanup Script

TELEMETRY_DIR="var/telemetry"
ARCHIVE_DIR="var/telemetry-archive"
RETENTION_DAYS=30
ARCHIVE_RETENTION_DAYS=90

mkdir -p "$ARCHIVE_DIR"

# Compress bundles older than retention
find "$TELEMETRY_DIR" -name "*.jsonl" -mtime +$RETENTION_DAYS -exec gzip {} \;

# Move compressed bundles to archive
find "$TELEMETRY_DIR" -name "*.jsonl.gz" -exec mv {} "$ARCHIVE_DIR/" \;

# Delete old archives
find "$ARCHIVE_DIR" -name "*.jsonl.gz" -mtime +$ARCHIVE_RETENTION_DAYS -delete

echo "Telemetry cleanup completed: $(date)"
du -sh "$TELEMETRY_DIR" "$ARCHIVE_DIR"
EOF

chmod +x scripts/cleanup-telemetry.sh

# Add to cron (daily at 3 AM)
(crontab -l 2>/dev/null; echo "0 3 * * * /path/to/adapter-os/scripts/cleanup-telemetry.sh") | crontab -
```

**Configure Telemetry Retention in Code:**
```toml
# configs/cp.toml
[telemetry]
retention_days = 30
auto_cleanup_enabled = true
cleanup_interval_hours = 24
max_bundle_size_mb = 100
```

### Root Cause Fix: Adapter Storage Management

**If Adapters Consuming Too Much Space:**

```bash
# 1. Identify large or unused adapters
sqlite3 var/aos-cp.sqlite3 "
SELECT adapter_id, name, file_size_mb, last_used_at,
       CASE WHEN status='Quarantined' THEN 'Can Delete' ELSE status END as recommendation
FROM adapters
WHERE file_size_mb > 100
   OR last_used_at < datetime('now', '-90 days')
   OR status='Quarantined'
ORDER BY file_size_mb DESC
LIMIT 20;"

# 2. Delete quarantined adapters (safe)
sqlite3 var/aos-cp.sqlite3 "
SELECT file_path FROM adapters WHERE status='Quarantined';" | while read path; do
    rm -f "$path"
    echo "Deleted quarantined adapter: $path"
done

# 3. Archive old adapters (> 90 days unused)
mkdir -p var/adapters-archive
sqlite3 var/aos-cp.sqlite3 "
SELECT file_path FROM adapters
WHERE last_used_at < datetime('now', '-90 days')
  AND status != 'Active';" | while read path; do
    if [ -f "$path" ]; then
        mv "$path" var/adapters-archive/
        echo "Archived: $path"
    fi
done

# 4. Update database
sqlite3 var/aos-cp.sqlite3 "
UPDATE adapters
SET status='Archived', file_path=REPLACE(file_path, 'repo', 'archive')
WHERE last_used_at < datetime('now', '-90 days')
  AND status != 'Active';"

# 5. Verify space freed
du -sh var/adapters/
du -sh var/adapters-archive/
```

### Root Cause Fix: Model Cache Optimization

**If Model Cache Too Large:**

```bash
# 1. List models and sizes
du -sh var/model-cache/*/
ls -lh var/model-cache/*/model.safetensors

# 2. Identify unused models
# (Check which models are referenced in configs/cp.toml)
grep "base_model" configs/cp.toml

# 3. Delete unused models (CAUTION)
# Only delete if you're sure it's not needed
rm -rf var/model-cache/unused-model-name/

# 4. Use quantized models (smaller)
# int4 quantized models are 4x smaller than fp16
# Edit configs/cp.toml:
# [model]
# quantization = "int4"  # Instead of "fp16"

# 5. Share model cache across tenants (if multi-tenant)
# Use symlinks instead of copies
```

---

## Validation

After cleanup, verify sufficient free space:

```bash
# 1. Check overall disk usage (should be < 85%)
df -h var/

# 2. Check inode usage (should be < 80%)
df -i var/

# 3. Verify key directories
du -sh var/logs/ var/telemetry/ var/adapters/ var/model-cache/

# 4. Test database writes
sqlite3 var/aos-cp.sqlite3 "CREATE TABLE test_write (id INT); DROP TABLE test_write;"
echo "Database write test: $?"

# 5. Test log writes
echo "Test log entry: $(date)" >> var/aos-cp.log
tail -1 var/aos-cp.log

# 6. Monitor disk usage trend
watch -n 300 'df -h var/'
```

**Success Criteria:**
- Disk usage < 85%
- WAL file < 100MB
- Logs rotating properly
- Telemetry cleanup running
- Database writes succeeding
- No "disk full" errors in logs

---

## Root Cause Prevention

### Post-Incident Actions

1. **Implement Disk Monitoring Dashboard:**
   ```yaml
   # Grafana dashboard panels:
   - Disk Usage Gauge (0-100%)
   - Disk Free Space (GB)
   - WAL File Size Trend
   - Log Directory Size Trend
   - Telemetry Directory Size Trend
   - Largest Files/Directories (table)
   ```

2. **Set Up Automated Cleanup Jobs:**
   ```bash
   # Crontab schedule for cleanup
   0 2 * * * /opt/adapteros/scripts/cleanup-logs.sh
   0 3 * * * /opt/adapteros/scripts/cleanup-telemetry.sh
   0 4 * * * sqlite3 /var/lib/adapteros/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"
   0 5 * * 0 /opt/adapteros/scripts/cleanup-old-adapters.sh  # Weekly
   ```

3. **Configure Disk Space Alerts:**
   ```yaml
   # Prometheus alerts
   groups:
     - name: adapteros.disk
       rules:
         - alert: DiskUsageHigh
           expr: disk_usage_percent{path="/var"} > 85
           for: 10m
           labels:
             severity: warning
           annotations:
             summary: "Disk usage high ({{ $value }}%)"
             runbook: "docs/runbooks/DISK_FULL.md"

         - alert: DiskUsageCritical
           expr: disk_usage_percent{path="/var"} > 95
           for: 2m
           labels:
             severity: critical
           annotations:
             summary: "Disk critically full ({{ $value }}%)"
             action: "Immediate cleanup required"

         - alert: WALFileLarge
           expr: file_size_bytes{file="aos-cp.sqlite3-wal"} > 1073741824
           for: 5m
           labels:
             severity: warning
           annotations:
             summary: "Database WAL file > 1GB"
             action: "Checkpoint and truncate WAL"
   ```

4. **Capacity Planning:**
   ```bash
   # Estimate growth rate
   # Run weekly, track results
   cat > scripts/disk-growth-report.sh <<'EOF'
   #!/bin/bash
   echo "=== Disk Growth Report $(date) ==="
   echo "Current usage: $(df -h var/ | awk 'NR==2 {print $5}')"
   echo ""
   echo "Directory sizes:"
   du -sh var/*/ | sort -hr
   echo ""
   echo "Growth estimate (30 days):"
   # Compare to last month's report if available
   # Calculate daily growth rate
   EOF
   ```

### Configuration Best Practices

**Database Settings:**
```sql
-- Set in startup script
PRAGMA wal_autocheckpoint=1000;  -- Checkpoint every 4MB
PRAGMA journal_size_limit=67108864;  -- Max 64MB journal
PRAGMA temp_store=memory;  -- Use RAM for temp tables
```

**Retention Policies:**
```toml
# configs/cp.toml
[retention]
log_retention_days = 30
telemetry_retention_days = 30
artifact_retention_days = 60
adapter_archive_days = 90

[cleanup]
enable_auto_cleanup = true
cleanup_schedule = "0 2 * * *"  # 2 AM daily
compress_before_delete = true
```

**Storage Quotas:**
```toml
# configs/cp.toml
[storage]
max_log_size_mb = 1000
max_telemetry_size_mb = 5000
max_adapter_size_mb = 10000
disk_warning_threshold_pct = 85
disk_critical_threshold_pct = 95
```

---

## Escalation

### Escalate to Senior Engineer If:
- Disk > 95% for > 30 minutes despite cleanup
- Database corruption suspected
- Cleanup scripts failing repeatedly
- Root cause unclear (unexpected growth)

### Escalate to Engineering Manager If:
- SEV-1 upgrade (database writes failing)
- Service outage due to disk space
- Requires emergency capacity expansion
- Customer data at risk

### Notify Platform/Infrastructure Team If:
- Need storage expansion (hardware)
- Filesystem issues (corruption, mount problems)
- NFS/network storage problems
- Disk I/O performance degradation

### Notify Database Team If:
- WAL file growth unexplained
- Database performance issues
- Index bloat suspected
- Vacuum/optimize needed

---

## Notes

**Disk Usage Patterns:**
- **Normal:** 1-5% growth per week
- **High Load:** 10-20% growth per week
- **Runaway:** > 30% growth per week (investigate immediately)

**Common Space Hogs (in order):**
1. Database WAL file (can grow unbounded if checkpoint fails)
2. Logs (100MB-1GB per day under load)
3. Telemetry bundles (accumulate quickly)
4. Model cache (5-20GB per model)
5. Adapters (50-500MB each, but many adapters)

**Safe to Delete:**
- Compressed logs (`.gz`) older than retention period
- Telemetry bundles older than 30 days
- Quarantined adapters
- Temp files in `var/tmp/`
- Archived artifacts (if models already packaged)

**DO NOT Delete:**
- Active database files (`.sqlite3`, `-shm`, `-wal`)
- Current/recent logs (last 7 days)
- Loaded adapters
- Base model files
- KV store files (`.redb`, index directories)

**Recovery Time:**
- Emergency cleanup: 5-10 minutes
- Full cleanup + monitoring: 30-60 minutes
- Automated cleanup setup: 2-4 hours

---

**Owner:** SRE Team
**Last Incident:** [Link to most recent postmortem]
**Related Runbooks:** [WORKER_CRASH.md](./WORKER_CRASH.md), [MEMORY_PRESSURE.md](./MEMORY_PRESSURE.md)
