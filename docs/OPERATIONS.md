# AdapterOS Production Operations Guide

**Complete operational guide for production deployment, monitoring, backup/restore, and disaster recovery.**

**Last Updated:** 2025-12-11
**Version:** 1.0
**Maintained By:** AdapterOS Operations Team

---

## Table of Contents

1. [Production Deployment Checklist](#production-deployment-checklist)
2. [Monitoring and Observability](#monitoring-and-observability)
3. [Backup and Restore Procedures](#backup-and-restore-procedures)
4. [System Metrics Reference](#system-metrics-reference)
5. [Operational Runbooks](#operational-runbooks)
6. [Capacity Planning](#capacity-planning)
7. [Maintenance Procedures](#maintenance-procedures)

---

## Production Deployment Checklist

### Pre-Deployment Requirements

- [ ] Prometheus configured with 15s scrape interval
- [ ] Grafana dashboard imported and validated
- [ ] AlertManager configured with escalation routes
- [ ] PagerDuty/Slack integration tested
- [ ] Backup scripts installed and tested
- [ ] Disaster recovery procedures documented
- [ ] On-call engineer trained on runbooks
- [ ] SLO targets documented and tracked
- [ ] Load testing completed with production workload
- [ ] Security review completed

### System Configuration

```toml
[system_metrics]
collection_interval_secs = 30
sampling_rate = 0.05  # 5% sampling per Telemetry Ruleset #9
enable_gpu_metrics = true
enable_disk_metrics = true
enable_network_metrics = true
retention_days = 30

[metrics]
enabled = true
bearer_token = "your_secure_token_here"
server_enabled = true
server_port = 9090
system_metrics_interval_secs = 30

[thresholds]
cpu_warning = 70.0
cpu_critical = 90.0
memory_warning = 80.0
memory_critical = 95.0
disk_warning = 85.0
disk_critical = 95.0
gpu_warning = 80.0
gpu_critical = 95.0
min_memory_headroom = 15.0
```

---

## Monitoring and Observability

### High-Level Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        Telemetry Sources                          │
├──────────────────────────────────────────────────────────────────┤
│
│ ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ │ adapteros-   │  │ adapteros-   │  │ adapteros-   │
│ │ lora-worker  │  │ lora-kernel- │  │ lora-         │
│ │              │  │ mtl          │  │ lifecycle    │
│ └──────┬───────┘  └──────┬───────┘  └──────┬───────┘
│        │                 │                 │
│        ├─► Kernel        ├─► GPU Memory   ├─► Adapter
│        │   Latency       │   Pressure     │   State
│        │                 │                │   Transitions
│        │                 ├─► Metal        │
│        │                 │   Panics       │
│        │                 │                │
│        └─────────┬───────┴────────┬───────┘
│                  │                │
│                  ▼                ▼
│        ┌────────────────────────────────────┐
│        │ adapteros-telemetry               │
│        │ MetricsCollector + Prometheus     │
│        │ - Histogram buckets               │
│        │ - Counter increments              │
│        │ - Gauge snapshots                 │
│        └────────────┬─────────────┬────────┘
│                     │             │
│        ┌────────────▼──┐   ┌──────▼─────────┐
│        │ Prometheus    │   │ UDS Exporter   │
│        │ :9090         │   │ (macOS only)   │
│        └────────────┬──┘   └────────────────┘
│                     │
│                     ▼
│        ┌──────────────────────────┐
│        │ Grafana Dashboard        │
│        │ - Real-time metrics      │
│        │ - Historical trends      │
│        │ - Alert status           │
│        └──────────────────────────┘
│
│        ┌──────────────────────────┐
│        │ Alert Manager            │
│        │ - Rule evaluation        │
│        │ - Escalation logic       │
│        │ - Pagerduty/Slack        │
│        └──────────────────────────┘
│
└──────────────────────────────────────────────────────────────────┘
```

### Core Metrics

#### 1. Inference Performance

**Metric:** `inference_latency_ms`
- **Type:** Histogram with buckets: [1, 5, 10, 25, 50, 100, 250, 500, 1000, 5000]
- **Labels:** `adapter_id`, `k`, `batch_size`, `backend`
- **Alert Threshold:**
  - WARNING: p99 > 300ms for 5 minutes
  - CRITICAL: p99 > 1000ms for 2 minutes

**Prometheus Query:**
```promql
# P99 latency trend
histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m]))

# Alert: P99 latency > 500ms
histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m])) > 500
```

#### 2. GPU Memory Management

**Metric:** `gpu_memory_pressure`
- **Type:** Gauge (0.0 - 1.0)
- **Alert Threshold:**
  - WARNING: > 0.75 for 5 minutes
  - CRITICAL: > 0.85 for 2 minutes

**Prometheus Query:**
```promql
# Current pressure
gpu_memory_pressure

# Alert when approaching threshold
gpu_memory_pressure > 0.85
```

#### 3. Hot-Swap Operations

**Metric:** `hotswap_latency_ms`
- **Type:** Histogram
- **Labels:** `operation`, `adapter_count`, `status`
- **Alert Threshold:**
  - WARNING: p95 > 100ms for 5 minutes
  - CRITICAL: p95 > 250ms for 2 minutes

#### 4. Determinism & Integrity

**Metric:** `determinism_violations_total`
- **Type:** Counter
- **Alert Threshold:** CRITICAL - Any violation (zero-tolerance)

#### 5. System Resources

**Metrics:**
- `cpu_usage_percent`: WARNING > 70%, CRITICAL > 90%
- `memory_usage_bytes`: WARNING > 80%, CRITICAL > 95%
- `disk_usage`: WARNING > 85%, CRITICAL > 95%

### Health Check Endpoint

**GET /healthz** - Enhanced health check with model runtime information:

```json
{
  "status": "healthy",
  "version": "1.2.3",
  "models": {
    "total_models": 5,
    "loaded_count": 3,
    "healthy": true,
    "inconsistencies_count": 0
  }
}
```

### Metrics Endpoint

**GET /metrics** - Prometheus/OpenMetrics export (requires Bearer token):

```bash
curl -H "Authorization: Bearer YOUR_TOKEN" http://localhost:8080/metrics
```

### Prometheus Configuration

Add to `prometheus.yml`:

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: adapteros
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'
    bearer_token: 'your_secure_token_here'
```

### Alert Rules

```yaml
groups:
  - name: adapteros.alerts
    rules:
      - alert: HighInferenceLatency
        expr: histogram_quantile(0.99, rate(inference_latency_ms_bucket[5m])) > 300
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High inference latency (p99 = {{ $value }}ms)"

      - alert: CriticalMemoryUsage
        expr: adapteros_memory_usage_percent > 95
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Memory usage above 95%"

      - alert: GPUMemoryPressureCritical
        expr: gpu_memory_pressure > 0.85
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "GPU memory pressure critical ({{ $value | humanizePercentage }})"

      - alert: DeterminismViolation
        expr: increase(determinism_violations_total[5m]) > 0
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "Determinism violation detected: {{ $labels.violation_type }}"
          action: "IMMEDIATE PAGE - Quarantine adapter {{ $labels.adapter_id }}"
```

### Grafana Dashboard

**Dashboard Panels:**

1. **Inference Latency** - P50, P95, P99 trends
2. **GPU Memory Pressure Gauge** - 0-1 scale with color coding
3. **Hot-Swap Success Rate** - Percentage of successful swaps
4. **Adapter Activation Heatmap** - Shows hot vs cold adapters
5. **Metal Kernel Execution Times** - Per-kernel type breakdown
6. **Determinism Violations Timeline** - Critical alert visualization
7. **System Resource Usage** - CPU, Memory, GPU combined view

### SLO Targets

| Objective | Target | Measurement |
|-----------|--------|-------------|
| **Availability** | 99.9% (43.2 min downtime/month) | Inference request success rate |
| **Inference Latency** | p99 < 300ms | Histogram from metrics collector |
| **GPU Memory Pressure** | < 80% (5-min average) | `gpu_memory_pressure` gauge |
| **Hot-Swap Success Rate** | > 99.5% | swap_count_total{status=success} / total |
| **Determinism Violations** | < 1 per month (critical) | Strict zero-tolerance SLO |
| **Router Latency** | p99 < 50ms | router_latency_ms_bucket |

---

## Backup and Restore Procedures

### Backup Strategy Overview

AdapterOS stores critical state in multiple locations requiring coordinated backup:

- **Control-plane DB:** `AOS_DATABASE_URL` (default `sqlite://var/aos-cp.sqlite3`)
- **KV store:** `AOS_KV_PATH` (default `var/aos-kv.redb`)
- **KV search indexes:** `AOS_KV_TANTIVY_PATH` (`var/aos-kv-index`) and `AOS_TANTIVY_PATH` (`var/aos-search`)
- **Adapters:** `AOS_ADAPTERS_DIR` (default `var/adapters/repo`)
- **Artifacts/logits:** `AOS_ARTIFACTS_DIR` (default `var/artifacts`)
- **Model cache + tokenizer:** `AOS_MODEL_CACHE_DIR` (default `var/model-cache`)
- **Control-plane config:** `AOS_CONFIG_PATH` (default `configs/cp.toml`)

### Key Management

Store the encryption key at `/etc/aos/backup.key` (mode 600):

```bash
sudo install -m 600 /dev/null /etc/aos/backup.key
sudo openssl rand -hex 64 | sudo tee /etc/aos/backup.key >/dev/null
```

### Automated Backup Script

Create `/opt/adapteros/scripts/backup/backup.sh`:

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

**Environment knobs:**
- `AOS_BACKUP_ROOT` (default `/var/backups/aos`)
- `AOS_DATA_ROOT` (default `<repo>/var`)
- `AOS_BACKUP_KEY_PATH` (default `/etc/aos/backup.key`)
- `AOS_BACKUP_RETENTION_DAYS` (default `7`)
- `AOS_BACKUP_OFFSITE_ROOT` (optional second rsync target)
- `AOS_BACKUP_REQUIRE_OFFSITE=1` (fail if offsite not available)
- `AOS_BACKUP_REQUIRE_SIGNING=1` (fail if signing not enabled)

### Database Restore Procedure

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

### Backup Verification

Run daily/CI:

```bash
/opt/adapteros/scripts/backup/verify-backups.sh
```

Checks:
- Decrypts latest backup
- Recomputes checksums
- Runs `PRAGMA integrity_check` on SQLite
- Verifies signature if `AOS_BACKUP_VERIFY_PUBKEY` is set

### Restore Testing (Weekly Drill)

```bash
/opt/adapteros/scripts/backup/test-restore.sh
```

What it does:
- Decrypts latest backup
- Integrity-checks SQLite
- Restores into temp directory
- Runs server health probe on port 18080
- Logs JSON; keeps restored data for inspection

### Cron Schedule

```bash
# Add to /etc/cron.daily/aos-backup
#!/bin/bash
/opt/adapteros/scripts/backup/backup.sh >> /var/log/adapteros/backup.log 2>&1

# Cron template: nightly backup 02:15 UTC, daily verify 09:00 UTC
15 2 * * * /opt/adapteros/scripts/backup/backup.sh >> /var/log/adapteros/backup.log 2>&1
0 9 * * * /opt/adapteros/scripts/backup/verify-backups.sh >> /var/log/adapteros/verify.log 2>&1
```

### Post-Restore Verification Checklist

- [ ] `sqlite3 <restored>/var/aos-cp.sqlite3 "PRAGMA integrity_check;"` returns `ok`
- [ ] KV files present (`aos-kv.redb`, indexes)
- [ ] Adapters directory populated
- [ ] `curl -fs http://127.0.0.1:18080/health` succeeds
- [ ] cp.toml matches expected tenant/egress settings

---

## System Metrics Reference

### Data Flow

```
System Resources → Collector → Policy Check → Telemetry → Database
                                    ↓
                              Alert/Incident
```

### Key Database Tables

- `system_metrics` - Real-time performance data (CPU, memory, GPU, network, disk)
- `system_health_checks` - Automated health status validation
- `threshold_violations` - Performance threshold breach detection
- `metrics_aggregations` - Pre-computed time-series summaries

### API Endpoints

**GET /v1/metrics/system** - Returns current system metrics:

```json
{
  "cpu_usage": 45.2,
  "memory_usage": 62.8,
  "active_workers": 3,
  "requests_per_second": 12.5,
  "avg_latency_ms": 24.3,
  "disk_usage": 23.1,
  "network_bandwidth": 1.2,
  "gpu_utilization": 15.5,
  "uptime_seconds": 86400,
  "process_count": 156,
  "load_average": {
    "load_1min": 1.2,
    "load_5min": 1.1,
    "load_15min": 1.0
  },
  "timestamp": 1640995200
}
```

### CLI Commands

```bash
# View current metrics
aosctl metrics show
aosctl metrics show --json

# View system health status
aosctl metrics health

# Show metrics history (last 24 hours)
aosctl metrics history
aosctl metrics history --hours 48 --limit 200

# Export metrics
aosctl metrics export --output metrics.json --format json --hours 24
aosctl metrics export --output metrics.csv --format csv --hours 168

# Check policy thresholds
aosctl metrics check
aosctl metrics violations
aosctl metrics violations --unresolved

# Configuration
aosctl metrics config --list
aosctl metrics config --key sampling_rate --value 0.1
aosctl metrics config --key cpu_warning --value 75.0
```

### Telemetry Events

#### system.metrics
```json
{
  "event_type": "system.metrics",
  "cpu_usage": 45.2,
  "memory_usage": 62.8,
  "disk_read_bytes": 1024000,
  "disk_write_bytes": 512000,
  "network_rx_bytes": 2048000,
  "network_tx_bytes": 1536000,
  "gpu_utilization": 15.5,
  "timestamp": 1640995200
}
```

#### system.threshold_violation
```json
{
  "event_type": "system.threshold_violation",
  "metric_name": "cpu_usage",
  "current_value": 95.0,
  "threshold_value": 90.0,
  "severity": "critical",
  "timestamp": 1640995200
}
```

---

## Operational Runbooks

### Runbook: High Inference Latency

**Alert:** `InferenceLatencyHigh` (p99 > 300ms)

**Diagnosis:**

1. Check router latency
   ```bash
   curl http://localhost:9090/api/v1/query?query=histogram_quantile(0.99, router_latency_ms_bucket)
   ```
   - If > 50ms: Router bottleneck
   - If < 10ms: Kernel bottleneck

2. Check GPU memory pressure
   ```bash
   curl http://localhost:9090/api/v1/query?query=gpu_memory_pressure
   ```
   - If > 0.85: Memory pressure causing slowdown

3. Profile kernel execution
   ```bash
   aosctl metrics get kernel_latency_ms --group-by kernel_type
   ```

**Resolution:**

**If Router Bottleneck:**
- Increase Q15 gate cache TTL
- Profile gate computation (compile with `--profile=release`)
- Consider using approximate nearest neighbor search

**If Kernel Bottleneck:**
- Profile Metal kernel with Instruments.app
- Check Metal command queue depth
- Verify no buffer memory issues (run `verify-gpu` endpoint)

**If Memory Pressure:**
- Trigger manual eviction: `aosctl lifecycle evict --count=3`
- Review router K selection (K=4 vs K=8)
- Check for pinned adapters: `aosctl db query "SELECT * FROM pinned_adapters"`

### Runbook: GPU Memory Pressure Critical

**Alert:** `GPUMemoryPressureCritical` (pressure > 0.85)

**Immediate Actions:**

1. Trigger auto-eviction (should happen automatically)
   ```bash
   aosctl lifecycle check-pressure
   ```

2. Check eviction status
   ```bash
   aosctl metrics get adapter_evictions_total --since=5m
   ```

3. Manual eviction if auto-eviction fails
   ```bash
   aosctl lifecycle evict --count=2 --strategy=lowest_activation_pct
   ```

**Diagnosis:**

1. Identify memory hog adapters
   ```bash
   aosctl db query \
     "SELECT adapter_id, vram_mb FROM adapters WHERE tenant_id = ? \
      ORDER BY vram_mb DESC LIMIT 10"
   ```

2. Check for memory leak
   ```bash
   curl "http://localhost:9090/api/v1/query?query=\
     (memory_usage_bytes{component='gpu_pool'} - \
      memory_usage_bytes offset 1h) / \
      memory_usage_bytes offset 1h"
   ```

3. Review buffer pool fragmentation
   ```bash
   aosctl metrics get gpu_memory_pool_fragmentation_ratio
   ```

**Resolution:**

**If Eviction Successful:**
- Monitor pressure for next 10 minutes
- If pressure stays low: resume normal operation
- If pressure increases again: investigate root cause

**If Eviction Fails:**
- Check pinned_adapters table
- Review pinning reasons (should be temporary, not permanent)
- Check for leaked reference counts: `aosctl worker debug refcounts`

**If Chronic High Pressure:**
- Reduce max_active_adapters in config
- Lower K-sparse size (K=4 instead of K=8)
- Increase total GPU memory (hardware upgrade)
- Profile memory usage per adapter type

### Runbook: Determinism Violation

**Alert:** `DeterminismViolation` (immediate page)

**Immediate Actions:**

1. Stop all inference (circuit breaker activated)
2. Quarantine affected adapter
   ```bash
   aosctl adapter quarantine --id <adapter_id>
   ```
3. Preserve GPU state for forensics
   ```bash
   aosctl worker debug dump --file /tmp/gpu_state.bin
   ```

**Investigation:**

1. Identify violation type
   ```bash
   curl "http://localhost:9090/api/v1/query?query=\
     increase(determinism_violations_total[5m]) by (violation_type)"
   ```

2. If `hash_mismatch`:
   ```bash
   aosctl db query \
     "SELECT * FROM determinism_log WHERE adapter_id = ? ORDER BY timestamp DESC LIMIT 10"
   ```
   - Compare expected_hash vs actual_hash in event logs

3. If `gpu_buffer_corruption`:
   - Check GPU memory for bit flips
   - Review Metal command queue for errors
   - Check for concurrent buffer access

**Recovery:**

1. Reload adapter from backup
   ```bash
   aosctl adapter reload --id <adapter_id> --from-registry
   ```

2. Run determinism test
   ```bash
   cargo test determinism_tests -- --nocapture
   ```

3. If still failing: escalate to architecture review

**Prevention:**
- Enable buffer fingerprinting on all swaps
- Increase checkpoint history limit (debug mode)
- Monitor GPU thermal throttling
- Schedule GPU memory test during low-traffic windows

### Runbook: High Hot-Swap Latency

**Alert:** `HotSwapLatencyHigh` (p95 > 100ms)

**Diagnosis:**

1. Identify operation bottleneck
   ```bash
   curl "http://localhost:9090/api/v1/query?query=\
     histogram_quantile(0.95, hotswap_latency_ms_bucket) by (operation)"
   ```

2. If `preload > 100ms`: Disk I/O
   ```bash
   iostat -d 1 5  # Check read throughput (MB/s)
   # Expected: > 200 MB/s for NVMe
   ```

3. If `swap > 50ms`: Checkpoint verification
   ```bash
   aosctl metrics get hotswap_latency_ms --filter operation=swap
   ```

**Resolution:**

**If Disk I/O Slow:**
- Check disk space (must be < 80% utilized): `df -h /var/lib/aos/adapters`
- Check for other processes reading disk: `lsof +D /var/lib/aos/adapters`
- Enable read-ahead cache in config

**If Swap Latency High:**
- Profile pointer flip and refcount updates
- Check for lock contention in `AdapterTable::swap()`
- Monitor `swap_concurrent_attempts_total`

**If Verify Latency High:**
- Consider sampling fewer buffer locations (2 instead of 3)
- Cache GPU fingerprints instead of recomputing
- Run verification asynchronously post-swap

---

## Capacity Planning

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

### Scaling Triggers

- **Memory Headroom** < 20%: Add RAM or reduce concurrent models
- **CPU Usage** > 80%: Add CPU cores or reduce load
- **Storage Usage** > 85%: Archive old telemetry or add storage
- **Queue Depth** > 100: Horizontal scaling or performance tuning

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

- [CLAUDE.md](../CLAUDE.md) - Project standards and development guide
- [DEPLOYMENT.md](DEPLOYMENT.md) - Complete deployment guide
- [POLICIES.md](POLICIES.md) - Policy enforcement and compliance
- [DATABASE.md](DATABASE.md) - Database documentation
- Prometheus Docs: https://prometheus.io/docs/
- Grafana Docs: https://grafana.com/docs/grafana/latest/

---

**MLNavigator Inc 2025-12-11**
