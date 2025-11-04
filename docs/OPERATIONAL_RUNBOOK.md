# AdapterOS Operational Runbook

## Table of Contents

1. [Monitoring](#monitoring)
2. [Troubleshooting](#troubleshooting)
3. [Performance Issues](#performance-issues)
4. [Security Incidents](#security-incidents)
5. [System Recovery](#system-recovery)
6. [Circuit Breaker Management](#circuit-breaker-management)
7. [Retry Policy Operations](#retry-policy-operations)
8. [Incident Response Playbooks](#incident-response-playbooks)
9. [Operational Components Maintenance](#operational-components-maintenance)
10. [Maintenance Procedures](#maintenance-procedures)

## Monitoring

### Key Metrics to Monitor

#### System Health
- **CPU Usage**: Monitor per-core and overall CPU utilization
- **Memory Usage**: Track heap usage, RSS, and virtual memory
- **Disk I/O**: Monitor read/write operations and queue depth
- **Network I/O**: Track bandwidth usage and connection counts

#### Application Metrics
- **HTTP Request Latency**: P50, P95, P99 response times
- **Request Rate**: Requests per second by endpoint
- **Error Rate**: 4xx and 5xx response rates
- **Active Connections**: Current connection count

#### Model Runtime Metrics
- **Loaded Models**: Number of models currently in memory
- **Model Cache Hit Rate**: Cache effectiveness for lazy loading
- **Model Load Time**: Time to load models from disk
- **Inference Latency**: End-to-end inference response times

#### Security Metrics
- **Security Violations**: Count of path traversal attempts, size limit violations
- **Failed Authentications**: Authentication failure rates
- **Policy Violations**: Count of policy enforcement failures

### Alert Thresholds

```yaml
# Critical Alerts (Page immediately)
cpu_usage_percent: 90
memory_usage_percent: 95
disk_usage_percent: 95
error_rate_percent: 5
response_time_p99_ms: 5000

# Warning Alerts
cpu_usage_percent: 75
memory_usage_percent: 80
disk_usage_percent: 80
error_rate_percent: 1
response_time_p95_ms: 1000
```

### Monitoring Commands

```bash
# Check system resources
top -p $(pgrep -f adapteros)
htop
iostat -x 1
free -h

# Check application logs
journalctl -u adapteros -f
tail -f /var/log/adapteros/*.log

# Check metrics endpoint
curl http://localhost:9090/metrics

# Check health endpoint
curl http://localhost:8080/healthz
```

## Troubleshooting

### High CPU Usage

**Symptoms:**
- CPU usage > 80%
- Slow response times
- System becomes unresponsive

**Investigation Steps:**

1. **Check system load:**
   ```bash
   uptime
   top -b -n1 | head -20
   ```

2. **Identify CPU-intensive processes:**
   ```bash
   ps aux --sort=-%cpu | head -10
   ```

3. **Check for runaway threads:**
   ```bash
   kill -QUIT $(pgrep adapteros)  # Generate thread dump
   ```

4. **Monitor inference operations:**
   ```bash
   curl http://localhost:8080/metrics | grep inference
   ```

**Common Causes & Solutions:**

- **Memory pressure causing GC thrashing:**
  ```bash
  # Increase memory limits
  echo "vm.max_map_count=262144" >> /etc/sysctl.conf
  sysctl -p
  ```

- **Too many concurrent inferences:**
  ```bash
  # Reduce max concurrent requests
  # Update config: server.max_concurrent_requests = 50
  ```

- **Model loading storms:**
  ```bash
  # Enable lazy loading
  # Update config: mlx.lazy_loading = true
  ```

### High Memory Usage

**Symptoms:**
- Memory usage > 80%
- Frequent GC pauses
- OutOfMemory errors

**Investigation Steps:**

1. **Check memory usage:**
   ```bash
   free -h
   ps aux --sort=-%mem | head -10
   ```

2. **Monitor model cache:**
   ```bash
   curl http://localhost:8080/metrics | grep model_cache
   ```

3. **Check for memory leaks:**
   ```bash
   # Use jemalloc stats if available
   curl http://localhost:8080/debug/malloc_stats
   ```

**Solutions:**

- **Enable model eviction:**
  ```yaml
  mlx:
    lazy_loading: true
    max_cached_models: 3
    cache_eviction_policy: lru
  ```

- **Reduce model cache size:**
  ```yaml
  mlx:
    max_cached_models: 2
  ```

- **Force garbage collection:**
  ```bash
  kill -USR1 $(pgrep adapteros)
  ```

### Slow Response Times

**Symptoms:**
- P95 latency > 1000ms
- Request timeouts
- User complaints about slowness

**Investigation Steps:**

1. **Check latency percentiles:**
   ```bash
   curl http://localhost:8080/metrics | grep http_request_duration
   ```

2. **Identify slow endpoints:**
   ```bash
   # Check access logs for slow requests
   tail -f /var/log/adapteros/access.log | grep -E "[0-9]{4,}"
   ```

3. **Profile inference operations:**
   ```bash
   curl http://localhost:8080/debug/pprof/profile?seconds=30 > profile.pb.gz
   ```

**Solutions:**

- **Optimize model loading:**
  ```yaml
  mlx:
    lazy_loading: true
    cache_eviction_policy: lfu  # Use frequency-based eviction
  ```

- **Enable request retries:**
  ```yaml
  # Retries are automatically enabled for transient failures
  ```

- **Scale resources:**
  ```bash
  # Add more CPU cores or memory
  # Consider horizontal scaling
  ```

### Database Connection Issues

**Symptoms:**
- Database connection errors
- Slow queries
- Timeout errors

**Investigation Steps:**

1. **Check database connectivity:**
   ```bash
   # Test database connection
   timeout 5 telnet localhost 5432  # or appropriate port
   ```

2. **Monitor connection pool:**
   ```bash
   curl http://localhost:8080/metrics | grep db_connections
   ```

3. **Check for connection leaks:**
   ```sql
   SELECT count(*) as active_connections FROM pg_stat_activity
   WHERE datname = 'adapteros' AND state = 'active';
   ```

**Solutions:**

- **Increase connection pool size:**
  ```yaml
  db:
    max_connections: 20
  ```

- **Enable connection retry:**
  ```yaml
  # Database retries are automatically configured
  ```

- **Check database performance:**
  ```sql
  SELECT * FROM pg_stat_user_tables ORDER BY n_tup_ins DESC LIMIT 10;
  ```

## Performance Issues

### Model Loading Performance

**Problem:** Models take too long to load on first inference request.

**Solutions:**

1. **Pre-warm critical models:**
   ```bash
   # Disable lazy loading for critical models
   curl -X POST http://localhost:8080/models/critical-model/load
   ```

2. **Optimize storage:**
   ```bash
   # Use faster storage (NVMe, SSD)
   # Consider model compression
   ```

3. **Enable model caching:**
   ```yaml
   mlx:
     lazy_loading: true
     max_cached_models: 5
   ```

### Inference Performance

**Problem:** Inference requests are slow.

**Solutions:**

1. **Profile inference operations:**
   ```bash
   # Use built-in profiler
   curl http://localhost:8080/debug/pprof/profile > profile.pb.gz
   go tool pprof profile.pb.gz
   ```

2. **Optimize batch sizes:**
   ```yaml
   inference:
     max_batch_size: 32
     optimal_batch_size: 16
   ```

3. **Use Metal acceleration:**
   ```yaml
   mlx:
     default_backend: metal
   ```

### Memory Pressure

**Problem:** System runs out of memory under load.

**Solutions:**

1. **Configure memory limits:**
   ```yaml
   server:
     max_memory_gb: 16
     memory_headroom_pct: 15
   ```

2. **Enable model eviction:**
   ```yaml
   mlx:
     cache_eviction_policy: lru
     max_cached_models: 2
   ```

3. **Monitor memory usage:**
   ```bash
   # Set up memory monitoring alerts
   ```

## Security Incidents

### Path Traversal Attempts

**Detection:**
- Monitor security violation metrics
- Check logs for path traversal alerts

**Response:**
1. **Block suspicious IPs:**
   ```bash
   iptables -A INPUT -s SUSPICIOUS_IP -j DROP
   ```

2. **Review access patterns:**
   ```bash
   grep "path.*violation" /var/log/adapteros/security.log
   ```

3. **Update security rules if needed**

### Authentication Failures

**Detection:**
- Monitor failed authentication metrics
- Check for brute force patterns

**Response:**
1. **Implement rate limiting:**
   ```yaml
   security:
     auth_rate_limit_per_minute: 10
   ```

2. **Block suspicious IPs:**
   ```bash
   # Add to firewall
   ```

3. **Review authentication logs**

### Data Exfiltration

**Detection:**
- Monitor unusual data access patterns
- Check for large file downloads

**Response:**
1. **Enable egress monitoring**
2. **Review data access logs**
3. **Implement data loss prevention**

## System Recovery

### Service Crash Recovery

**Automatic Recovery:**
- Systemd will automatically restart crashed services
- Check service status: `systemctl status adapteros`

**Manual Recovery:**
```bash
# Restart service
sudo systemctl restart adapteros

# Check logs for crash reason
journalctl -u adapteros -n 100
```

### Database Recovery

**Connection Loss:**
```bash
# Check database status
sudo systemctl status postgresql

# Restart if needed
sudo systemctl restart postgresql

# Verify connectivity
psql -d adapteros -c "SELECT 1;"
```

**Data Corruption:**
```bash
# Stop service
sudo systemctl stop adapteros

# Restore from backup
# Contact DBA team

# Restart service
sudo systemctl start adapteros
```

### Model File Corruption

**Detection:**
- Model loading failures
- Inference errors

**Recovery:**
```bash
# Identify corrupted model
grep "model.*corrupt" /var/log/adapteros/*.log

# Restore from backup
cp /backup/models/good-model.bin /var/lib/adapteros/models/

# Restart service
sudo systemctl restart adapteros
```

## Maintenance Procedures

### Regular Maintenance Tasks

#### Daily Checks
```bash
# Check service status
systemctl status adapteros

# Check disk space
df -h /var/lib/adapteros

# Check log sizes
du -sh /var/log/adapteros/*

# Check metrics
curl http://localhost:8080/metrics | grep -E "(error|latency)"
```

#### Weekly Maintenance
```bash
# Rotate logs
logrotate /etc/logrotate.d/adapteros

# Clean old model cache
find /var/lib/adapteros/cache -mtime +7 -delete

# Update system packages
apt update && apt upgrade -y

# Restart service for updates
systemctl restart adapteros
```

#### Monthly Maintenance
```bash
# Full backup
./backup.sh full

# Security audit
# Review access logs for anomalies

# Performance review
# Analyze metrics trends
```

### Backup Procedures

#### Configuration Backup
```bash
# Backup configuration
cp /etc/adapteros/config.toml /backup/config/config.toml.$(date +%Y%m%d)

# Backup secrets
# Use secure key management system
```

#### Data Backup
```bash
# Database backup
pg_dump adapteros > /backup/database/adapteros.$(date +%Y%m%d).sql

# Model files backup
tar -czf /backup/models/models.$(date +%Y%m%d).tar.gz /var/lib/adapteros/models/
```

#### Log Backup
```bash
# Compress and archive old logs
find /var/log/adapteros -name "*.log" -mtime +30 -exec gzip {} \;
```

### Update Procedures

#### Minor Updates
```bash
# Update package
apt update && apt install adapteros

# Restart service
systemctl restart adapteros

# Verify functionality
curl http://localhost:8080/healthz
```

#### Major Updates
```bash
# Create full backup
./backup.sh full

# Update package
apt update && apt install adapteros

# Check configuration compatibility
adapteros --validate-config /etc/adapteros/config.toml

# Restart service
systemctl restart adapteros

# Monitor for issues
watch -n 10 'curl http://localhost:8080/healthz'
```

### Circuit Breaker Management

Circuit breakers protect AdapterOS services from cascading failures by automatically stopping requests to failing services. This section covers monitoring, reset procedures, and troubleshooting.

### Monitoring Circuit Breaker State

Circuit breakers are critical for maintaining service availability. Monitor them proactively to prevent service degradation.

#### Check Circuit Breaker Status Across Services

```bash
# Get detailed health metrics including circuit breaker states
curl -s http://localhost:8080/api/v1/monitoring/health-metrics | jq '.circuit_breakers'

# Example output:
{
  "database": {
    "state": "closed",
    "requests_total": 15420,
    "successes_total": 15415,
    "failures_total": 5,
    "opens_total": 2,
    "closes_total": 2,
    "half_opens_total": 1,
    "last_state_change": 1640995200
  },
  "external_api": {
    "state": "half_open",
    "requests_total": 892,
    "successes_total": 887,
    "failures_total": 5,
    "opens_total": 1,
    "closes_total": 0,
    "half_opens_total": 1,
    "last_state_change": 1640995300
  }
}
```

#### Real-time Circuit Breaker Monitoring

```bash
# Monitor circuit breaker state changes in real-time
curl -s http://localhost:8080/api/v1/monitoring/health-metrics | \
  jq -r '.circuit_breakers | to_entries[] | select(.value.state != "closed") | "\(.key): \(.value.state) (\(.value.failures_total)/\(.value.requests_total) failures)"'

# Set up alerting for open circuit breakers
watch -n 30 'curl -s http://localhost:8080/api/v1/monitoring/health-metrics | jq ".circuit_breakers | to_entries[] | select(.value.state == \"open\") | .key"'
```

### Circuit Breaker Reset Procedures

Circuit breakers automatically recover, but manual intervention may be needed in some cases.

#### Reset Circuit Breaker for Specific Service

```bash
# Reset database circuit breaker
curl -X POST http://localhost:8080/api/v1/admin/circuit-breakers/database/reset \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json"

# Reset all circuit breakers (use with caution)
curl -X POST http://localhost:8080/api/v1/admin/circuit-breakers/reset-all \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json"
```

#### Force Circuit Breaker State

```bash
# Force circuit breaker to closed state (emergency recovery)
curl -X POST http://localhost:8080/api/v1/admin/circuit-breakers/external_api/force-close \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"reason": "Emergency recovery after service restoration"}'
```

### Circuit Breaker Troubleshooting

#### High Failure Rate Investigation

**Symptoms:**
- Circuit breaker frequently transitions to open state
- High failure rates (>5%) for specific services
- Service unavailability during peak load

**Investigation Steps:**

1. **Check Recent Error Patterns:**
   ```bash
   # Query error logs for the affected service
   grep "circuit_breaker.*open" /var/log/adapteros/circuit_breaker.log | tail -20

   # Check detailed error metrics
   curl -s "http://localhost:8080/api/v1/monitoring/health-metrics?service=database&start_time=$(date -d '1 hour ago' +%s)" | \
     jq '.circuit_breakers.database'
   ```

2. **Analyze Failure Causes:**
   ```bash
   # Check database connection issues
   curl -s http://localhost:8080/api/v1/monitoring/health-metrics | jq '.dependencies.database'

   # Review recent failures
   grep "database.*error" /var/log/adapteros/server.log | tail -50
   ```

3. **Check System Resources:**
   ```bash
   # Monitor system load during failures
   top -b -n1 | head -10
   iostat -x 1 5
   free -h
   ```

**Common Causes & Solutions:**

- **Database Connection Pool Exhaustion:**
  ```yaml
  # Increase connection pool size
  database:
    max_connections: 50  # Increase from default
    connection_timeout_ms: 30000
  ```

- **Network Timeouts:**
  ```yaml
  # Adjust timeout settings
  circuit_breaker:
    database:
      timeout_ms: 10000  # Increase timeout
      failure_threshold: 3  # Reduce sensitivity
  ```

- **Service Overload:**
  ```yaml
  # Implement load shedding
  server:
    max_concurrent_requests: 1000  # Reduce concurrent load
    queue_size: 100
  ```

#### Circuit Breaker Recovery Procedures

**Automatic Recovery (Preferred):**
Circuit breakers automatically transition to half-open after the timeout period and test service recovery.

**Manual Recovery (When Automatic Fails):**

1. **Verify Service Health:**
   ```bash
   # Test service manually
   curl -f http://localhost:5432/health  # For database
   # or
   timeout 5 telnet localhost 5432
   ```

2. **Reset Circuit Breaker:**
   ```bash
   curl -X POST http://localhost:8080/api/v1/admin/circuit-breakers/database/reset
   ```

3. **Monitor Recovery:**
   ```bash
   watch -n 5 'curl -s http://localhost:8080/api/v1/monitoring/health-metrics | jq ".circuit_breakers.database.state"'
   ```

#### Threshold Tuning Guidelines

**Conservative Settings (High Availability):**
```yaml
circuit_breaker:
  failure_threshold: 10  # More tolerant of failures
  success_threshold: 5   # Require more successes
  timeout_ms: 120000     # Longer recovery time
```

**Aggressive Settings (High Performance):**
```yaml
circuit_breaker:
  failure_threshold: 3   # Quick to react
  success_threshold: 2   # Fast recovery
  timeout_ms: 30000      # Quick recovery attempts
```

**Service-Specific Tuning:**

- **Database Services:** Use conservative settings to prevent cascading failures
- **External APIs:** Use aggressive settings for faster failure detection
- **Cache Services:** Use conservative settings as they're often non-critical

## Retry Policy Operations

Retry policies automatically handle transient failures, but require monitoring and tuning for optimal performance.

### Analyzing Retry Patterns

#### Query Retry Metrics

```bash
# Get retry metrics across all services
curl -s http://localhost:8080/api/v1/monitoring/retry-metrics | jq '.retry_rates'

# Example output:
{
  "database": {
    "attempts_per_minute": 45.2,
    "success_rate": 0.87,
    "average_retry_count": 1.2,
    "circuit_breaker_trips": 2
  },
  "external_api": {
    "attempts_per_minute": 120.5,
    "success_rate": 0.92,
    "average_retry_count": 0.8,
    "circuit_breaker_trips": 0
  }
}
```

#### Real-time Retry Monitoring

```bash
# Monitor retry rates in real-time
watch -n 30 'curl -s http://localhost:8080/api/v1/monitoring/retry-metrics | \
  jq -r ".retry_rates | to_entries[] | \"\(.key): \(.value.attempts_per_minute) attempts/min, \(.value.success_rate * 100)% success\" "'
```

### Retry Policy Configuration

#### Service-Specific Retry Strategies

**Database Operations (Conservative):**
```yaml
retry:
  database:
    max_attempts: 3
    initial_delay: 200ms
    max_delay: 2s
    backoff_multiplier: 1.5
    jitter_factor: 0.2
```

**External API Calls (Fast Recovery):**
```yaml
retry:
  external_api:
    max_attempts: 5
    initial_delay: 100ms
    max_delay: 5s
    backoff_multiplier: 2.0
    jitter_factor: 0.1
```

**Cache Operations (Aggressive):**
```yaml
retry:
  cache:
    max_attempts: 2
    initial_delay: 50ms
    max_delay: 500ms
    backoff_multiplier: 2.0
    jitter_factor: 0.3
```

#### Backoff Algorithm Tuning

**Exponential Backoff:**
- Starts with `initial_delay`
- Multiplies by `backoff_multiplier` each retry
- Caps at `max_delay`
- Adds random jitter to prevent thundering herd

**Tuning Guidelines:**
- **Low Latency Services:** Shorter delays, higher jitter
- **High Throughput Services:** Longer delays, lower jitter
- **Rate-Limited Services:** Respect API rate limits with appropriate delays

#### Circuit Breaker Integration

Retry policies work with circuit breakers to provide layered failure protection:

```yaml
# Combined configuration
circuit_breaker:
  database:
    failure_threshold: 5
    timeout_ms: 60000

retry:
  database:
    max_attempts: 3
    initial_delay: 200ms
    # Retries stop when circuit breaker opens
```

**Integration Behavior:**
1. Retries attempt to recover from transient failures
2. Circuit breaker opens after repeated failures
3. Once open, retries are blocked until circuit recovers
4. This prevents wasted retry attempts during outages

## Incident Response Playbooks

### Circuit Breaker Cascade Failure

**Detection:**
- Multiple circuit breakers opening simultaneously
- Service unavailability across multiple components
- Alert: "Circuit Breaker Cascade Detected"

**Immediate Actions:**
1. **Stop All Traffic:**
   ```bash
   # Enable maintenance mode
   curl -X POST http://localhost:8080/api/v1/admin/maintenance/enable \
     -H "Authorization: Bearer $ADMIN_TOKEN"
   ```

2. **Identify Root Cause:**
   ```bash
   # Check system resources
   top -b -n1 | head -10
   iostat -x 1
   free -h

   # Check network connectivity
   ping -c 5 database.internal
   traceroute database.internal
   ```

3. **Isolate Failing Services:**
   ```bash
   # Force open problematic circuit breakers
   curl -X POST http://localhost:8080/api/v1/admin/circuit-breakers/problematic/force-open
   ```

**Recovery Steps:**
1. **Restore Individual Services:**
   ```bash
   # Test and restore services one by one
   systemctl restart adapteros-database
   sleep 30
   curl -f http://localhost:8080/healthz
   ```

2. **Gradual Traffic Restoration:**
   ```bash
   # Reset circuit breakers in order of dependency
   curl -X POST http://localhost:8080/api/v1/admin/circuit-breakers/database/reset
   sleep 60
   curl -X POST http://localhost:8080/api/v1/admin/circuit-breakers/cache/reset
   ```

3. **Disable Maintenance Mode:**
   ```bash
   curl -X POST http://localhost:8080/api/v1/admin/maintenance/disable
   ```

### Retry Storm Incident

**Detection:**
- Exponential increase in retry attempts
- System resource exhaustion (CPU, memory)
- Alert: "Retry Storm Detected"

**Immediate Actions:**
1. **Reduce Retry Frequency:**
   ```yaml
   # Emergency retry configuration
   retry:
     global:
       max_attempts: 1  # Disable retries temporarily
   ```

2. **Enable Circuit Breakers:**
   ```bash
   # Force all circuit breakers to conservative settings
   curl -X POST http://localhost:8080/api/v1/admin/circuit-breakers/conservative-mode
   ```

3. **Load Shedding:**
   ```bash
   # Reduce concurrent requests
   curl -X POST http://localhost:8080/api/v1/admin/config/update \
     -d '{"server.max_concurrent_requests": 100}'
   ```

**Investigation:**
1. **Identify Trigger:**
   ```bash
   # Check recent changes
   grep "config.*retry" /var/log/adapteros/audit.log | tail -20

   # Analyze retry patterns
   curl -s http://localhost:8080/api/v1/monitoring/retry-metrics | jq '.retry_rates'
   ```

2. **Check for External Issues:**
   ```bash
   # Network connectivity
   mtr database.internal

   # External service status
   curl -s https://status.external-api.com/api/v2/status.json
   ```

**Recovery:**
1. **Restore Normal Retry Settings:**
   ```yaml
   retry:
     global:
       max_attempts: 3
   ```

2. **Monitor Recovery:**
   ```bash
   watch -n 10 'curl -s http://localhost:8080/api/v1/monitoring/retry-metrics | jq ".retry_rates[].attempts_per_minute"'
   ```

### Circuit Breaker Stuck in Open State

**Detection:**
- Circuit breaker remains open despite service recovery
- Service unavailable after underlying issue resolved
- Manual intervention required

**Diagnosis:**
```bash
# Check circuit breaker state and last change
curl -s http://localhost:8080/api/v1/monitoring/health-metrics | \
  jq '.circuit_breakers[] | select(.state == "open") | {name: ., state, last_state_change}'

# Verify service health
curl -f http://database.internal:5432/health
```

**Recovery:**
1. **Manual Reset:**
   ```bash
   curl -X POST http://localhost:8080/api/v1/admin/circuit-breakers/database/reset
   ```

2. **Force Close (Emergency):**
   ```bash
   curl -X POST http://localhost:8080/api/v1/admin/circuit-breakers/database/force-close \
     -d '{"reason": "Service verified healthy, manual recovery"}'
   ```

3. **Monitor Success:**
   ```bash
   watch -n 5 'curl -s http://localhost:8080/api/v1/monitoring/health-metrics | jq ".circuit_breakers.database"'
   ```

## Operational Components Maintenance

### Circuit Breaker Maintenance

#### Weekly Maintenance Tasks

```bash
# Review circuit breaker metrics
curl -s http://localhost:8080/api/v1/monitoring/health-metrics | \
  jq '.circuit_breakers | to_entries[] | select(.value.opens_total > 0) | "\(.key): \(.value.opens_total) opens, \(.value.failures_total) failures"'

# Check for frequently failing services
curl -s http://localhost:8080/api/v1/monitoring/health-metrics | \
  jq '.circuit_breakers | to_entries[] | select(.value.failures_total / .value.requests_total > 0.05) | .key'
```

#### Monthly Maintenance Tasks

1. **Tune Circuit Breaker Thresholds:**
   ```bash
   # Analyze historical failure patterns
   curl -s "http://localhost:8080/api/v1/monitoring/health-metrics?period=30d" | \
     jq '.circuit_breakers | to_entries[] | {service: .key, failure_rate: (.value.failures_total / .value.requests_total)}'
   ```

2. **Update Circuit Breaker Configurations:**
   ```yaml
   # Adjust based on analysis
   circuit_breaker:
     high_failure_service:
       failure_threshold: 7  # Increase tolerance
     low_failure_service:
       failure_threshold: 3  # Keep sensitive
   ```

### Retry Policy Maintenance

#### Weekly Maintenance Tasks

```bash
# Review retry effectiveness
curl -s http://localhost:8080/api/v1/monitoring/retry-metrics | \
  jq '.retry_rates | to_entries[] | select(.value.success_rate < 0.8) | "\(.key): \(.value.success_rate * 100)% success rate"'

# Check for retry storms
curl -s http://localhost:8080/api/v1/monitoring/retry-metrics | \
  jq '.retry_rates | to_entries[] | select(.value.attempts_per_minute > 100) | "\(.key): \(.value.attempts_per_minute) attempts/min"'
```

#### Monthly Maintenance Tasks

1. **Optimize Retry Configurations:**
   ```bash
   # Analyze retry patterns over time
   curl -s "http://localhost:8080/api/v1/monitoring/retry-metrics?period=30d" | \
     jq '.retry_rates | to_entries[] | {service: .key, avg_retries: .value.average_retry_count, success_rate: .value.success_rate}'
   ```

2. **Update Backoff Strategies:**
   ```yaml
   # Adjust retry policies based on analysis
   retry:
     high_retry_service:
       max_attempts: 5  # Increase attempts for frequently failing service
       initial_delay: 500ms  # Slower backoff
     low_retry_service:
       max_attempts: 2  # Reduce attempts for reliable service
   ```

#### Configuration Validation

```bash
# Validate retry and circuit breaker configurations
curl -X POST http://localhost:8080/api/v1/admin/config/validate \
  -H "Content-Type: application/json" \
  -d @/etc/adapteros/config.yml
```

## Emergency Procedures

#### Service Unavailable
1. **Check service status:**
   ```bash
   systemctl status adapteros
   ```

2. **Check system resources:**
   ```bash
   top
   free -h
   df -h
   ```

3. **Restart service:**
   ```bash
   systemctl restart adapteros
   ```

4. **If restart fails, check logs:**
   ```bash
   journalctl -u adapteros -n 50
   ```

5. **Last resort - system reboot:**
   ```bash
   # Only if all else fails
   reboot
   ```

#### Data Loss
1. **Stop service immediately:**
   ```bash
   systemctl stop adapteros
   ```

2. **Assess damage:**
   ```bash
   # Check what data is missing
   ls -la /var/lib/adapteros/
   ```

3. **Restore from backup:**
   ```bash
   # Use latest backup
   ./restore.sh /backup/latest/
   ```

4. **Verify data integrity:**
   ```bash
   # Run validation checks
   adapteros --validate-data
   ```

5. **Restart service:**
   ```bash
   systemctl start adapteros
   ```

This runbook should be updated regularly as new issues are discovered and procedures are refined.
