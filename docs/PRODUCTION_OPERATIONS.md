# Production Operations Guide

**Complete guide for operating AdapterOS in production, including backup/restore, disaster recovery, and capacity planning.**

---

## Table of Contents

1. [Backup and Restore](#backup-and-restore)
2. [Disaster Recovery](#disaster-recovery)
3. [Capacity Planning](#capacity-planning)
4. [Monitoring and Alerting](#monitoring-and-alerting)
5. [Maintenance Procedures](#maintenance-procedures)

---

## Backup and Restore

AdapterOS stores critical state in multiple locations that require coordinated backup strategies.

### Database Backup

The primary state store is the SQLite database containing:
- Adapter metadata and configurations
- Model registrations and provenance
- Policy enforcement state
- Telemetry and audit logs
- User sessions and authentication state

#### Daily Backup Script

Create `/usr/local/bin/aos-backup.sh`:

```bash
#!/bin/bash
# AdapterOS Production Backup Script
set -euo pipefail

BACKUP_DIR="/var/backups/adapteros"
DB_PATH="/var/lib/adapteros/aos-cp.sqlite3"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Create backup directory
mkdir -p "$BACKUP_DIR"

# Database backup with integrity check
echo "Backing up database..."
sqlite3 "$DB_PATH" ".backup '$BACKUP_DIR/aos-db-$TIMESTAMP.sqlite3'"
sqlite3 "$DB_PATH" "PRAGMA integrity_check;" > "$BACKUP_DIR/integrity-$TIMESTAMP.txt"

# Compress backup
tar czf "$BACKUP_DIR/aos-backup-$TIMESTAMP.tar.gz" \
    -C "$BACKUP_DIR" \
    "aos-db-$TIMESTAMP.sqlite3" \
    "integrity-$TIMESTAMP.txt"

# Cleanup old backups (keep last 30 days)
find "$BACKUP_DIR" -name "aos-backup-*.tar.gz" -mtime +30 -delete
find "$BACKUP_DIR" -name "aos-db-*.sqlite3" -mtime +7 -delete
find "$BACKUP_DIR" -name "integrity-*.txt" -mtime +7 -delete

echo "Backup completed: $BACKUP_DIR/aos-backup-$TIMESTAMP.tar.gz"
```

#### Database Restore Procedure

**Critical: Stop all AdapterOS services before restore**

```bash
# 1. Stop services
sudo systemctl stop adapteros-cp

# 2. Backup current database (safety measure)
cp /var/lib/adapteros/aos-cp.sqlite3 /var/lib/adapteros/aos-cp.sqlite3.pre-restore

# 3. Restore from backup
BACKUP_FILE="/var/backups/adapteros/aos-backup-20250101_120000.tar.gz"
cd /tmp
tar xzf "$BACKUP_FILE"
cp aos-db-20250101_120000.sqlite3 /var/lib/adapteros/aos-cp.sqlite3

# 4. Verify integrity
sqlite3 /var/lib/adapteros/aos-cp.sqlite3 "PRAGMA integrity_check;"

# 5. Fix permissions
chown adapteros:adapteros /var/lib/adapteros/aos-cp.sqlite3

# 6. Restart services
sudo systemctl start adapteros-cp

# 7. Verify system health
sleep 30
sudo systemctl status adapteros-cp
```

### Model Directory Backup

Models and adapters are stored in the adapters directory:

```bash
# Model directory structure
/var/lib/adapteros/
├── adapters/
│   ├── model1.safetensors
│   ├── model1.json
│   ├── adapter1.safetensors
│   └── adapter1.json
└── aos-cp.sqlite3
```

#### Model Backup Strategy

```bash
#!/bin/bash
# Model directory backup script

MODEL_DIR="/var/lib/adapteros/adapters"
BACKUP_DIR="/var/backups/adapteros/models"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Create backup with hardlinks for efficiency
mkdir -p "$BACKUP_DIR/$TIMESTAMP"
cp -rl "$MODEL_DIR"/* "$BACKUP_DIR/$TIMESTAMP/"

# Create compressed archive
tar czf "$BACKUP_DIR/models-$TIMESTAMP.tar.gz" \
    -C "$BACKUP_DIR" \
    "$TIMESTAMP"

# Cleanup old backups
find "$BACKUP_DIR" -name "models-*.tar.gz" -mtime +30 -delete
find "$BACKUP_DIR" -maxdepth 1 -type d -mtime +7 -exec rm -rf {} \;

echo "Model backup completed: $BACKUP_DIR/models-$TIMESTAMP.tar.gz"
```

#### Model Restore Procedure

```bash
# 1. Stop services
sudo systemctl stop adapteros-cp

# 2. Backup current models (safety measure)
mv /var/lib/adapteros/adapters /var/lib/adapteros/adapters.pre-restore

# 3. Restore from backup
BACKUP_FILE="/var/backups/adapteros/models/models-20250101_120000.tar.gz"
cd /var/lib/adapteros
tar xzf "$BACKUP_FILE"
mv "20250101_120000" adapters

# 4. Fix permissions
chown -R adapteros:adapteros adapters

# 5. Restart services
sudo systemctl start adapteros-cp

# 6. Re-register models (if database was also restored)
# Models will be auto-discovered, but manual verification recommended
aosctl models list
```

### Configuration Backup

```bash
# Configuration files to backup
/etc/adapteros/config.toml
/etc/systemd/system/adapteros-cp.service
/var/lib/adapteros/jwt_keys/
/etc/pf.anchors/adapteros  # If using PF rules
```

### Automated Backup Setup

#### Cron Jobs for Daily Backups

```bash
# Add to /etc/cron.daily/aos-backup
#!/bin/bash
/usr/local/bin/aos-backup.sh >> /var/log/adapteros/backup.log 2>&1
/usr/local/bin/aos-model-backup.sh >> /var/log/adapteros/model-backup.log 2>&1
```

#### Backup Verification

Use the provided verification script to validate backup integrity:

```bash
# Verify all backups
./scripts/verify-backups.sh

# Test restore procedures (non-destructive)
./scripts/test-restore.sh
```

The verification script performs comprehensive checks:

- **Database integrity**: Validates SQLite database integrity on the backup file itself
- **Archive completeness**: Ensures all expected files are present in backups
- **Permissions**: Verifies backup files have appropriate security settings
- **Freshness**: Checks that backups are recent (within 24 hours)

##### Backup Testing

Before relying on backups in production, perform regular restore testing:

```bash
# Monthly restore testing (safe, non-destructive)
./scripts/test-restore.sh

# This validates:
# - Backup files are readable and complete
# - Database integrity is maintained
# - Model files can be extracted
# - Scripts have valid syntax
```

##### Backup Monitoring

Monitor backup success through logs and alerts:

```bash
# Check recent backup logs
tail -f /var/log/adapteros/backup.log
tail -f /var/log/adapteros/model-backup.log

# Verify backups exist and are recent
find /var/backups/adapteros -name "aos-backup-*.tar.gz" -mtime -1 | wc -l
find /var/backups/adapteros -name "models-*.tar.gz" -mtime -1 | wc -l
```

---

## Disaster Recovery

Procedures for recovering from catastrophic failures where model directories or databases are lost.

### Scenario 1: Models Directory Lost

**Impact**: Models and adapters unavailable, but database metadata intact.

**Recovery Time**: 2-4 hours depending on model sizes.

#### Recovery Procedure

1. **Assess Damage**
   ```bash
   # Check what models are registered in database
   sqlite3 /var/lib/adapteros/aos-cp.sqlite3 "
   SELECT name, path, size_bytes
   FROM models
   WHERE status = 'active';"
   ```

2. **Prepare Recovery Environment**
   ```bash
   # Create temporary models directory
   mkdir -p /tmp/adapteros-recovery/models

   # Get list of required models
   sqlite3 /var/lib/adapteros/aos-cp.sqlite3 "
   SELECT DISTINCT name, path
   FROM models
   WHERE status = 'active';" > /tmp/required-models.txt
   ```

3. **Restore from Backup**
   ```bash
   # If recent backup available
   LATEST_BACKUP=$(ls -t /var/backups/adapteros/models/models-*.tar.gz | head -1)
   if [ -n "$LATEST_BACKUP" ]; then
       cd /var/lib/adapteros
       tar xzf "$LATEST_BACKUP"
       mv models-restored adapters
       chown -R adapteros:adapteros adapters
   fi
   ```

4. **Re-download Missing Models**
   ```bash
   # For models not in backup, re-download from source
   while read -r name path; do
       if [ ! -f "/var/lib/adapteros/adapters/$(basename "$path")" ]; then
           echo "Re-downloading $name..."
           # Use original download command or source
           aosctl model download "$name"
       fi
   done < /tmp/required-models.txt
   ```

5. **Verify and Restart**
   ```bash
   # Verify model integrity
   aosctl models verify

   # Restart services
   sudo systemctl restart adapteros-cp

   # Test inference
   aosctl inference test --model qwen2.5-7b-instruct
   ```

### Scenario 2: Database Corruption

**Impact**: All state lost, models intact.

**Recovery Time**: 1-2 hours.

#### Recovery Procedure

1. **Stop Services Immediately**
   ```bash
   sudo systemctl stop adapteros-cp
   ```

2. **Attempt Database Repair**
   ```bash
   # Try SQLite recovery
   sqlite3 /var/lib/adapteros/aos-cp.sqlite3 ".recover" > /tmp/db-recover.sql

   # If recovery succeeds, recreate database
   if [ -s /tmp/db-recover.sql ]; then
       mv /var/lib/adapteros/aos-cp.sqlite3 /var/lib/adapteros/aos-cp.sqlite3.corrupted
       sqlite3 /var/lib/adapteros/aos-cp.sqlite3 < /tmp/db-recover.sql
   fi
   ```

3. **Restore from Backup**
   ```bash
   # Use latest database backup
   LATEST_DB_BACKUP=$(ls -t /var/backups/adapteros/aos-db-*.sqlite3 | head -1)

   if [ -n "$LATEST_DB_BACKUP" ]; then
       cp "$LATEST_DB_BACKUP" /var/lib/adapteros/aos-cp.sqlite3
       chown adapteros:adapteros /var/lib/adapteros/aos-cp.sqlite3
   fi
   ```

4. **Re-register Models**
   ```bash
   # Auto-discover models in adapters directory
   aosctl models rediscover

   # Verify registration
   aosctl models list
   ```

5. **Restore Configuration**
   ```bash
   # Restore JWT keys and config if needed
   cp /var/backups/adapteros/config/production.toml /etc/adapteros/config.toml
   ```

### Scenario 3: Complete System Loss

**Impact**: Both database and models lost.

**Recovery Time**: 4-8 hours.

#### Recovery Procedure

1. **Reinstall AdapterOS**
   ```bash
   # Use installer or manual installation
   make installer
   sudo installer run
   ```

2. **Restore Latest Backups**
   ```bash
   # Restore database
   LATEST_DB=$(ls -t /var/backups/adapteros/aos-db-*.sqlite3 | head -1)
   cp "$LATEST_DB" /var/lib/adapteros/aos-cp.sqlite3

   # Restore models
   LATEST_MODELS=$(ls -t /var/backups/adapteros/models-*.tar.gz | head -1)
   cd /var/lib/adapteros
   tar xzf "$LATEST_MODELS"
   ```

3. **Reinitialize Missing Components**
   ```bash
   # Reinitialize tenants and users
   aosctl tenant init --id production

   # Verify system health
   aosctl health check
   ```

### Recovery Testing

#### Monthly Disaster Recovery Validation

Use the provided restore testing script for regular validation:

```bash
# Safe, non-destructive testing
./scripts/test-restore.sh

# This validates backup integrity and extractability without affecting production
```

#### Quarterly Disaster Recovery Drill

For comprehensive disaster recovery testing, perform controlled drills:

```bash
#!/bin/bash
# Comprehensive disaster recovery testing script

echo "🧪 Starting Comprehensive Disaster Recovery Test"

# Prerequisites: Ensure backups exist
if ! ./scripts/verify-backups.sh >/dev/null 2>&1; then
    echo "❌ Backups not ready for testing"
    exit 1
fi

# 1. Test backup integrity (already done above)
echo "✅ Backup integrity verified"

# 2. Test restore procedures in isolated environment
if ./scripts/test-restore.sh >/dev/null 2>&1; then
    echo "✅ Restore procedures validated"
else
    echo "❌ Restore procedures failed"
    exit 1
fi

# 3. Documented Recovery Time Objective (RTO) Test
# Measure time to complete restore procedures
START_TIME=$(date +%s)

# Simulate database restore
echo "Testing database restore procedure..."
# (Actual restore commands would go here, measured for time)

END_TIME=$(date +%s)
RESTORE_TIME=$((END_TIME - START_TIME))

if [ $RESTORE_TIME -lt 300 ]; then  # Less than 5 minutes
    echo "✅ Recovery Time Objective met: ${RESTORE_TIME}s"
else
    echo "⚠️  Recovery Time Objective exceeded: ${RESTORE_TIME}s"
fi

echo "✅ Disaster Recovery Test Completed"
echo "📊 Test Results:"
echo "   - Backup integrity: ✅"
echo "   - Restore procedures: ✅"
echo "   - Recovery time: ${RESTORE_TIME}s"
```

#### Recovery Testing Checklist

- [ ] **Backup Verification**: Run `./scripts/verify-backups.sh` monthly
- [ ] **Restore Testing**: Run `./scripts/test-restore.sh` monthly
- [ ] **Full Drill**: Complete disaster recovery drill quarterly
- [ ] **RTO Validation**: Measure and validate recovery time objectives
- [ ] **Documentation Update**: Update procedures based on test findings

---

## Capacity Planning

Guidelines for sizing AdapterOS deployments based on workload requirements.

### Memory Requirements

#### Base Model Memory Usage

| Model | Size | Memory (int4) | Memory (fp16) | Recommended RAM |
|-------|------|---------------|---------------|-----------------|
| Qwen 2.5 7B | 7B params | ~5GB | ~14GB | 16GB+ |
| Qwen 2.5 14B | 14B params | ~10GB | ~28GB | 32GB+ |
| Qwen 2.5 32B | 32B params | ~22GB | ~64GB | 64GB+ |

#### Adapter Memory Overhead

- **Per Adapter**: 16-64MB depending on rank
- **K=3 Selection**: 48-192MB total per request
- **Memory Headroom**: Minimum 15% reserved (Memory Ruleset #12)

#### Total Memory Calculation

```
Total RAM = Base Model Memory + (K × Adapter Memory) + Headroom + System Overhead

Example for Qwen 2.5 7B with K=3:
Total RAM = 5GB + (3 × 64MB) + 15% + 2GB = ~8.5GB minimum
```

### Storage Requirements

#### Database Storage

- **Base Size**: 100MB initial
- **Per Model**: +50MB metadata
- **Per Adapter**: +10MB metadata
- **Telemetry Growth**: +500MB/month (high usage)
- **Total Estimate**: 1-5GB/year depending on usage

#### Model Storage

- **Base Models**: 5-20GB per model (quantized)
- **Adapters**: 50-500MB per adapter
- **Growth Rate**: +1-10GB/month depending on training
- **Backup Storage**: 2× primary storage for retention

### CPU Requirements

#### Inference Performance

- **M1/M2 Chip**: 8-12 cores recommended
- **M3 Max**: 16+ cores optimal
- **Concurrent Requests**: 1 core per simultaneous inference
- **Batch Processing**: CPU-bound for small batches

#### Background Tasks

- **Database Operations**: 2-4 cores
- **Telemetry Processing**: 1-2 cores
- **Monitoring**: 1 core
- **Total CPU Cores**: 8-16 recommended for production

### Network Requirements

#### Bandwidth

- **Model Downloads**: 100-500MB/minute during initial setup
- **Adapter Downloads**: 10-50MB/minute
- **Telemetry Upload**: 1-10MB/hour (compressed)
- **API Traffic**: 1-10MB/hour depending on usage

#### Latency Requirements

- **Intra-cluster**: <1ms for multi-node deployments
- **Client API**: <100ms p95 for inference requests
- **Database**: <10ms for metadata queries

### Scaling Formulas

#### Horizontal Scaling

```python
def calculate_node_count(total_qps, avg_latency_ms, target_p95_ms=100):
    """
    Calculate required nodes for inference workload

    Args:
        total_qps: Total queries per second
        avg_latency_ms: Average inference latency in milliseconds
        target_p95_ms: Target p95 latency

    Returns:
        Required node count
    """
    # Account for queueing theory and variability
    effective_qps_per_node = 1000 / (avg_latency_ms * 1.5)  # Conservative estimate
    return max(1, ceil(total_qps / effective_qps_per_node))
```

#### Memory Scaling

```python
def calculate_memory_requirement(model_size_gb, k_sparse=3, adapter_size_mb=64, headroom_pct=0.15):
    """
    Calculate total memory requirement

    Args:
        model_size_gb: Base model size in GB
        k_sparse: K-sparse routing parameter
        adapter_size_mb: Average adapter size in MB
        headroom_pct: Required memory headroom percentage

    Returns:
        Total memory requirement in GB
    """
    adapter_total_gb = (k_sparse * adapter_size_mb) / 1024
    headroom_gb = (model_size_gb + adapter_total_gb) * headroom_pct
    system_overhead_gb = 2  # OS and runtime overhead

    return model_size_gb + adapter_total_gb + headroom_gb + system_overhead_gb
```

### Capacity Planning Checklist

#### Pre-Deployment Assessment

- [ ] **Workload Analysis**
  - Peak QPS requirements
  - Average/max request size
  - Model selection patterns
  - Adapter usage patterns

- [ ] **Resource Sizing**
  - RAM: `calculate_memory_requirement(model_size, k_sparse)`
  - CPU: `min(physical_cores, concurrent_requests)`
  - Storage: `1.5 × (models + adapters + telemetry)`
  - Network: `10 × peak_qps` bandwidth

- [ ] **Growth Planning**
  - 6-month usage projections
  - Model addition schedule
  - Data retention policies
  - Backup storage requirements

#### Production Monitoring

```bash
# Memory headroom monitoring
aosctl metrics show | grep memory_headroom

# CPU utilization tracking
aosctl metrics history --hours 24 --metric cpu_usage

# Storage growth monitoring
du -sh /var/lib/adapteros/
df -h /var/lib/adapteros
```

#### Scaling Triggers

- **Memory Headroom** < 20%: Add RAM or reduce concurrent models
- **CPU Usage** > 80%: Add CPU cores or reduce load
- **Storage Usage** > 85%: Archive old telemetry or add storage
- **Queue Depth** > 100: Horizontal scaling or performance tuning

### Example Deployments

#### Small Production (Development Teams)

- **Users**: 50 developers
- **QPS**: 10-50 requests/second
- **Models**: 2-3 base models + 20 adapters
- **Hardware**: M3 Max (16 cores, 32GB RAM)
- **Storage**: 500GB SSD

#### Medium Production (Engineering Organization)

- **Users**: 500+ engineers
- **QPS**: 50-200 requests/second
- **Models**: 5-10 base models + 100+ adapters
- **Hardware**: 3× M3 Max nodes (48 cores, 96GB RAM total)
- **Storage**: 2TB NVMe per node

#### Large Production (Enterprise)

- **Users**: 5000+ users
- **QPS**: 200-1000+ requests/second
- **Models**: 20+ base models + 1000+ adapters
- **Hardware**: 10+ node cluster with shared storage
- **Storage**: 10TB+ distributed storage

---

## Monitoring and Alerting

### Key Metrics to Monitor

#### System Health

```bash
# Memory headroom (critical alert if < 20%)
aosctl metrics show --field memory_headroom_pct

# CPU usage (warning if > 70%, critical if > 90%)
aosctl metrics show --field cpu_usage

# Disk usage (warning if > 85%, critical if > 95%)
aosctl metrics show --field disk_usage
```

#### Application Metrics

```bash
# Inference latency (p95 should be < 100ms)
aosctl metrics show --field inference_latency_p95

# Request success rate (should be > 99.9%)
aosctl metrics show --field request_success_rate

# Active adapters in memory
aosctl metrics show --field active_adapters_count
```

### Alert Configuration

#### Prometheus Alerting Rules

```yaml
groups:
  - name: adapteros.alerts
    rules:
      - alert: HighMemoryUsage
        expr: adapteros_memory_usage_percent > 85
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Memory usage above 85%"

      - alert: CriticalMemoryUsage
        expr: adapteros_memory_usage_percent > 95
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Memory usage above 95%"

      - alert: HighInferenceLatency
        expr: histogram_quantile(0.95, adapteros_inference_duration_seconds) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "P95 inference latency > 100ms"
```

### Log Monitoring

#### Critical Log Patterns

```bash
# Monitor for critical errors
journalctl -u adapteros-cp -f | grep -E "(ERROR|CRITICAL|FATAL)"

# Memory pressure warnings
journalctl -u adapteros-cp -f | grep "memory.*headroom"

# Policy violations
journalctl -u adapteros-cp -f | grep "policy.*violation"
```

---

## Maintenance Procedures

### Routine Maintenance

#### Weekly Tasks

1. **Backup Verification**
   ```bash
   # Test backup integrity
   ./scripts/verify-backups.sh

   # Verify backup restore capability
   ./scripts/test-restore.sh --dry-run
   ```

2. **Database Maintenance**
   ```bash
   # Run SQLite optimization
   sqlite3 /var/lib/adapteros/aos-cp.sqlite3 "VACUUM;"

   # Check for corruption
   sqlite3 /var/lib/adapteros/aos-cp.sqlite3 "PRAGMA integrity_check;"
   ```

3. **Log Rotation**
   ```bash
   # Rotate application logs
   sudo logrotate /etc/logrotate.d/adapteros

   # Archive old telemetry bundles
   find /var/lib/adapteros/telemetry -name "*.jsonl" -mtime +30 -exec gzip {} \;
   ```

#### Monthly Tasks

1. **Performance Benchmarking**
   ```bash
   # Run inference benchmarks
   ./scripts/benchmark-inference.sh --model qwen2.5-7b-instruct

   # Check memory usage patterns
   ./scripts/analyze-memory-usage.sh --month $(date +%Y-%m)
   ```

2. **Security Updates**
   ```bash
   # Update Rust dependencies
   cargo update --workspace

   # Rebuild with latest security patches
   cargo build --release

   # Update system packages
   sudo apt update && sudo apt upgrade -y  # or equivalent
   ```

3. **Capacity Review**
   ```bash
   # Review usage patterns
   ./scripts/capacity-review.sh --months 3

   # Update capacity planning based on growth
   ./scripts/update-capacity-plan.sh
   ```

### Emergency Maintenance

#### High Memory Usage Response

```bash
# 1. Check current status
aosctl metrics show

# 2. Identify memory hogs
aosctl adapters list --sort memory_usage

# 3. Reduce K-sparse if needed
aosctl config set router.k_sparse 2  # Temporarily reduce from 3

# 4. Evict unused adapters
aosctl adapters evict --cold-lru --count 5

# 5. Monitor for recovery
watch aosctl metrics show --field memory_headroom_pct
```

#### Database Performance Issues

```bash
# 1. Check database performance
sqlite3 /var/lib/adapteros/aos-cp.sqlite3 "
EXPLAIN QUERY PLAN SELECT * FROM telemetry_events
WHERE timestamp > strftime('%s', 'now', '-1 hour');"

# 2. Add missing indices if needed
sqlite3 /var/lib/adapteros/aos-cp.sqlite3 "
CREATE INDEX IF NOT EXISTS idx_telemetry_timestamp
ON telemetry_events(timestamp);"

# 3. Compact database
sqlite3 /var/lib/adapteros/aos-cp.sqlite3 "VACUUM;"
```

---

## References

- [DEPLOYMENT.md](DEPLOYMENT.md) - Complete deployment guide
- [PRODUCTION_READINESS.md](PRODUCTION_READINESS.md) - Production readiness checklist
- [system-metrics.md](system-metrics.md) - Monitoring and metrics guide
- [database-schema/README.md](database-schema/README.md) - Database documentation
- [Memory Ruleset #12](POLICIES.md) - Memory management policies

---

**Version:** 1.0  
**Last Updated:** 2025-11-03  
**Maintained By:** AdapterOS Operations Team
