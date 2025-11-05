# AdapterOS Troubleshooting Guide

**Comprehensive troubleshooting guide for common AdapterOS deployment, configuration, and runtime issues.**

---

## Table of Contents

- [Quick Diagnosis](#quick-diagnosis)
- [Startup Issues](#startup-issues)
- [Authentication Problems](#authentication-problems)
- [Performance Issues](#performance-issues)
- [Memory Issues](#memory-issues)
- [Database Issues](#database-issues)
- [Network Issues](#network-issues)
- [Adapter Issues](#adapter-issues)
- [Training Issues](#training-issues)
- [Monitoring Issues](#monitoring-issues)
- [Security Issues](#security-issues)

---

## Quick Diagnosis

### System Health Check

Run this first when investigating any issue:

```bash
# Check if service is running
ps aux | grep adapteros-server

# Check health endpoints
curl -f http://localhost:8080/healthz && echo "✓ Health OK" || echo "✗ Health FAIL"
curl -f http://localhost:8080/readyz && echo "✓ Ready OK" || echo "✗ Ready FAIL"

# Check recent logs
tail -20 /var/log/adapteros/server.log

# Check system resources
df -h /var/lib/adapteros
free -h

# Check database connectivity
psql -h localhost -U adapteros -d adapteros_prod -c "SELECT 1;" && echo "✓ DB OK" || echo "✗ DB FAIL"
```

### Log Analysis Commands

```bash
# Show last 50 lines with timestamps
tail -50 /var/log/adapteros/server.log | cut -d' ' -f1-3

# Search for errors in last hour
grep "$(date -d '1 hour ago' '+%Y-%m-%d %H')" /var/log/adapteros/server.log | grep -i error

# Count errors by type
grep -i error /var/log/adapteros/server.log | cut -d' ' -f4- | sort | uniq -c | sort -nr

# Show memory-related warnings
grep -i "memory\|eviction\|headroom" /var/log/adapteros/server.log | tail -10
```

---

## Startup Issues

### Server Won't Start

**Symptoms:**
- Service fails to start
- No process visible in `ps`
- Systemd reports failure

**Diagnosis:**
```bash
# Check systemd status
systemctl status adapteros-server

# Check for configuration errors
./target/release/adapteros-server --config configs/production.toml --dry-run

# Validate configuration file
cat configs/production.toml | grep -v '^#' | grep -v '^$'

# Check file permissions
ls -la configs/production.toml
ls -la var/jwt_*.pem
```

**Common Solutions:**

1. **Configuration File Issues**
   ```bash
   # Check TOML syntax
   python3 -c "import tomllib; tomllib.load(open('configs/production.toml', 'rb'))"

   # Validate required fields
   grep -E "^(database|server|security)" configs/production.toml
   ```

2. **Database Connection Issues**
   ```bash
   # Test database connection
   psql -h localhost -U adapteros -d adapteros_prod -c "SELECT version();"

   # Check database is running
   pg_isready -h localhost -p 5432

   # Verify database exists
   psql -h localhost -U postgres -l | grep adapteros_prod
   ```

3. **Permission Issues**
   ```bash
   # Check adapter directory permissions
   ls -ld /var/lib/adapteros/
   ls -ld /var/lib/adapteros/adapters/

   # Check log directory permissions
   ls -ld /var/log/adapteros/

   # Fix permissions if needed
   sudo chown -R adapteros:adapteros /var/lib/adapteros/
   sudo chown -R adapteros:adapteros /var/log/adapteros/
   ```

4. **Port Binding Issues**
   ```bash
   # Check if port is already in use
   netstat -tlnp | grep :8080

   # Try different port
   sed -i 's/port = 8080/port = 8081/' configs/production.toml
   ```

### Adapter Loading Failures

**Symptoms:**
- Server starts but adapters fail to load
- Warnings about missing adapters in logs
- Inference requests fail

**Diagnosis:**
```bash
# Check adapter directory
ls -la /var/lib/adapteros/adapters/

# Check for adapter manifest files
find /var/lib/adapteros/adapters/ -name "*.json" | head -5

# Check adapter loading logs
grep "adapter.*load\|adapter.*error" /var/log/adapteros/server.log | tail -10

# Verify adapter file integrity
for adapter in /var/lib/adapteros/adapters/*.aos; do
  echo "Checking $adapter..."
  file "$adapter"
done
```

**Solutions:**

1. **Re-import Base Model**
   ```bash
   # Check if base model exists
   ls -la models/qwen2.5-7b-mlx/

   # Re-import if missing
   ./target/release/aosctl import-model \
     --name qwen2.5-7b-instruct \
     --weights models/qwen2.5-7b-mlx/weights.safetensors \
     --config models/qwen2.5-7b-mlx/config.json \
     --tokenizer models/qwen2.5-7b-mlx/tokenizer.json
   ```

2. **Fix Adapter File Corruption**
   ```bash
   # Identify corrupted adapters
   for adapter in /var/lib/adapteros/adapters/*.aos; do
     if ! ./target/release/aosctl adapter inspect "$adapter" >/dev/null 2>&1; then
       echo "Corrupted: $adapter"
       mv "$adapter" "${adapter}.corrupted"
     fi
   done

   # Re-upload corrupted adapters
   ```

---

## Authentication Problems

### Login Failures

**Symptoms:**
- Users can't log in
- "Invalid credentials" errors
- JWT token generation fails

**Diagnosis:**
```bash
# Test login endpoint
curl -v -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@example.com","password":"password"}'

# Check JWT key files
ls -la var/jwt_*.pem

# Validate key permissions
stat var/jwt_private.pem
stat var/jwt_public.pem

# Check authentication logs
grep -i "auth\|login\|jwt" /var/log/adapteros/server.log | tail -10
```

**Solutions:**

1. **JWT Key Issues**
   ```bash
   # Regenerate keys if corrupted
   openssl genpkey -algorithm Ed25519 -out var/jwt_private_new.pem
   openssl pkey -in var/jwt_private_new.pem -pubout -out var/jwt_public_new.pem

   # Update configuration
   sed -i 's|private_key_file = "var/jwt_private.pem"|private_key_file = "var/jwt_private_new.pem"|' configs/production.toml
   sed -i 's|public_key_file = "var/jwt_public.pem"|public_key_file = "var/jwt_public_new.pem"|' configs/production.toml

   # Restart server
   systemctl restart adapteros-server

   # Replace old keys after testing
   mv var/jwt_private_new.pem var/jwt_private.pem
   mv var/jwt_public_new.pem var/jwt_public.pem
   ```

2. **User Database Issues**
   ```sql
   -- Check if user exists
   SELECT id, email, role FROM users WHERE email = 'admin@example.com';

   -- Reset password if needed (for development only)
   UPDATE users SET password_hash = '$2b$10$...' WHERE email = 'admin@example.com';
   ```

### Invalid Token Errors

**Symptoms:**
- API requests fail with 401 Unauthorized
- Tokens work initially but expire quickly

**Diagnosis:**
```bash
# Decode JWT token to check expiry
echo "your.jwt.token" | cut -d'.' -f2 | base64 -d 2>/dev/null | jq '.exp, .iat'

# Check token expiry configuration
grep "token_expiry\|jwt" configs/production.toml

# Check system time sync
date
curl -s http://worldtimeapi.org/api/timezone/Etc/UTC.txt | grep "UTC:"
```

**Solutions:**

1. **Time Synchronization Issues**
   ```bash
   # Sync system time
   sudo ntpdate pool.ntp.org

   # Enable NTP
   sudo systemctl enable ntpd
   sudo systemctl start ntpd
   ```

2. **Token Expiry Configuration**
   ```toml
   # Increase token expiry for development
   [auth]
   token_expiry_hours = 24

   # Or decrease for production security
   [auth]
   token_expiry_hours = 8
   ```

---

## Performance Issues

### High Latency

**Symptoms:**
- Inference requests take longer than expected
- p95 latency > 200ms
- User complaints about slow responses

**Diagnosis:**
```bash
# Check current latency metrics
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/metrics/system | jq '.inference.avg_latency_ms'

# Check queue depth
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/metrics/system | jq '.inference.queue_depth'

# Monitor request patterns
tail -f /var/log/adapteros/server.log | grep "inference_request"

# Check system load
uptime
top -b -n1 | head -20
```

**Solutions:**

1. **Increase Worker Threads**
   ```toml
   # Update production.toml
   [server]
   workers = 16  # Increase from 8
   ```

2. **Reduce K-Sparse Value**
   ```bash
   # Temporarily reduce adapter count per request
   curl -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"k_sparse": 2}' \
     http://localhost:8080/api/v1/system/config
   ```

3. **Optimize Adapter Selection**
   ```bash
   # Check which adapters are most used
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/adapters | jq '.[] | {id, activations: .metrics.total_activations}' | sort_by(.activations) | reverse
   ```

### Low Throughput

**Symptoms:**
- System handles fewer requests than expected
- Tokens/sec below target
- Queue depth consistently high

**Diagnosis:**
```bash
# Check throughput metrics
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/metrics/system | jq '.inference.tokens_per_sec'

# Monitor CPU usage
top -p $(pgrep adapteros-server) -n 1

# Check memory usage
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/metrics/system | jq '.memory.used_bytes / .memory.total_bytes'

# Check for bottlenecks
iostat -x 1 5
```

**Solutions:**

1. **CPU Bottleneck**
   ```bash
   # Check current worker count
   ps aux | grep adapteros-server | wc -l

   # Increase workers if CPU < 80%
   ```

2. **Memory Bottleneck**
   ```bash
   # Check memory headroom
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system | jq '.memory.headroom_pct'

   # Evict unused adapters
   curl -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"adapter_id": "least_used_adapter"}' \
     http://localhost:8080/api/v1/system/evict-adapter
   ```

---

## Memory Issues

### Out of Memory Errors

**Symptoms:**
- System crashes with OOM
- Adapters fail to load
- Memory usage > 95%

**Diagnosis:**
```bash
# Check current memory usage
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/metrics/system | jq '.memory'

# Check system memory
free -h

# Check adapter memory usage
for adapter in $(curl -s -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/v1/adapters | jq -r '.[].id'); do
  echo "Adapter: $adapter"
  curl -s -H "Authorization: Bearer $TOKEN" \
    http://localhost:8080/api/v1/metrics/adapters/$adapter | jq '.memory_usage_mb'
done

# Check memory-related logs
grep -i "memory\|eviction\|oom" /var/log/adapteros/server.log | tail -20
```

**Solutions:**

1. **Immediate Memory Relief**
   ```bash
   # Reduce K-sparse value
   curl -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"k_sparse": 1}' \
     http://localhost:8080/api/v1/system/config

   # Evict largest adapters
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/adapters | jq '.[] | {id, memory: .metrics.memory_usage_mb}' | sort_by(.memory) | reverse | head -3
   ```

2. **Permanent Configuration Changes**
   ```toml
   # Update production.toml
   [memory]
   min_headroom_pct = 20  # Increase from 15%
   max_adapters_per_tenant = 10  # Reduce from 20

   [router]
   k_sparse = 2  # Reduce from 3
   ```

3. **System Memory Tuning**
   ```bash
   # Enable swap if needed (temporary)
   sudo fallocate -l 8G /swapfile
   sudo chmod 600 /swapfile
   sudo mkswap /swapfile
   sudo swapon /swapfile

   # Add to /etc/fstab for persistence
   echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
   ```

### Memory Leaks

**Symptoms:**
- Memory usage gradually increases over time
- System requires periodic restarts
- Performance degrades over time

**Diagnosis:**
```bash
# Monitor memory usage over time
while true; do
  date
  curl -s -H "Authorization: Bearer $TOKEN" \
    http://localhost:8080/api/v1/metrics/system | jq '.memory.used_bytes'
  sleep 300
done

# Check for memory leaks in logs
grep -i "leak\|alloc\|dealloc" /var/log/adapteros/server.log

# Profile memory usage
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/debug/pprof/heap > heap.prof
```

**Solutions:**

1. **Restart Services**
   ```bash
   # Graceful restart
   systemctl restart adapteros-server

   # Monitor memory after restart
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system | jq '.memory.used_bytes'
   ```

2. **Check for Memory Fragmentation**
   ```bash
   # Monitor adapter loading/unloading
   grep "adapter.*load\|adapter.*unload\|eviction" /var/log/adapteros/server.log | tail -20

   # Check eviction policy effectiveness
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system | jq '.adapters.loaded_count'
   ```

---

## Database Issues

### Connection Failures

**Symptoms:**
- Database connection errors
- Service starts but API calls fail
- "connection refused" errors

**Diagnosis:**
```bash
# Test database connectivity
psql -h localhost -U adapteros -d adapteros_prod -c "SELECT 1;"

# Check PostgreSQL status
sudo systemctl status postgresql@15

# Check database logs
tail -20 /usr/local/var/log/postgres.log

# Check connection pool
psql -h localhost -U adapteros -d adapteros_prod -c "SELECT * FROM pg_stat_activity WHERE datname = 'adapteros_prod';"
```

**Solutions:**

1. **Restart Database**
   ```bash
   sudo systemctl restart postgresql@15

   # Wait for startup
   sleep 10

   # Test connection
   psql -h localhost -U adapteros -d adapteros_prod -c "SELECT 1;"
   ```

2. **Check Connection Limits**
   ```sql
   -- Check current connections
   SELECT count(*) FROM pg_stat_activity WHERE datname = 'adapteros_prod';

   -- Check max connections
   SHOW max_connections;

   -- Check connection pool settings in AdapterOS config
   grep "pool_size" configs/production.toml
   ```

3. **Database Configuration Issues**
   ```bash
   # Check database URL in config
   grep "database.*url" configs/production.toml

   # Test with different connection string
   psql "postgresql://adapteros:password@localhost/adapteros_prod" -c "SELECT 1;"
   ```

### Query Performance Issues

**Symptoms:**
- Database queries are slow
- API responses delayed
- High database CPU usage

**Diagnosis:**
```sql
-- Check slow queries
SELECT query, total_time, calls, mean_time
FROM pg_stat_statements
ORDER BY mean_time DESC
LIMIT 10;

-- Check table bloat
SELECT schemaname, tablename, n_dead_tup, n_live_tup,
       n_dead_tup::float / (n_live_tup + n_dead_tup) * 100 as bloat_ratio
FROM pg_stat_user_tables
WHERE n_dead_tup > 0
ORDER BY bloat_ratio DESC;

-- Check index usage
SELECT indexname, idx_scan, idx_tup_read, idx_tup_fetch
FROM pg_stat_user_indexes
WHERE idx_scan = 0
ORDER BY idx_tup_read DESC;
```

**Solutions:**

1. **Database Maintenance**
   ```sql
   -- Vacuum analyze
   VACUUM ANALYZE;

   -- Reindex unused indexes
   REINDEX DATABASE adapteros_prod;
   ```

2. **Query Optimization**
   ```sql
   -- Create missing indexes
   CREATE INDEX CONCURRENTLY ON rag_documents USING hnsw (embedding vector_cosine_ops);
   CREATE INDEX CONCURRENTLY ON inference_requests (created_at);
   ```

3. **Connection Pool Tuning**
   ```toml
   # Update production.toml
   [db]
   pool_size = 30  # Increase if needed
   max_lifetime = 1800  # 30 minutes
   ```

---

## Network Issues

### Connection Refused

**Symptoms:**
- Cannot connect to service
- "connection refused" errors
- Port appears closed

**Diagnosis:**
```bash
# Check if service is listening
netstat -tlnp | grep :8080

# Check firewall rules
sudo pfctl -s rules | grep 8080

# Test local connectivity
curl -v http://localhost:8080/healthz

# Check network interface
ip addr show | grep inet
```

**Solutions:**

1. **Firewall Configuration**
   ```bash
   # Check Packet Filter rules
   sudo pfctl -s rules

   # Add rule for port 8080
   echo "pass in on en0 proto tcp to port 8080" | sudo pfctl -a adapteros -f -

   # Reload rules
   sudo pfctl -f /etc/pf.conf
   ```

2. **Service Binding**
   ```toml
   # Check server configuration
   [server]
   bind_address = "0.0.0.0"  # Allow external connections
   port = 8080
   ```

### SSL/TLS Issues

**Symptoms:**
- HTTPS connections fail
- Certificate validation errors
- Mixed content warnings

**Diagnosis:**
```bash
# Test SSL connection
openssl s_client -connect localhost:8443 -servername localhost

# Check certificate validity
openssl x509 -in /etc/ssl/certs/adapteros.crt -text | grep -A2 "Validity"

# Test with curl
curl -v https://localhost:8443/healthz
```

**Solutions:**

1. **Certificate Issues**
   ```bash
   # Renew certificate
   certbot renew

   # Restart web server
   sudo systemctl restart nginx

   # Check certificate chain
   openssl verify -CAfile /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/adapteros.crt
   ```

2. **SSL Configuration**
   ```nginx
   # Nginx SSL configuration
   server {
       listen 443 ssl http2;
       server_name adapteros.example.com;

       ssl_certificate /etc/letsencrypt/live/adapteros.example.com/fullchain.pem;
       ssl_certificate_key /etc/letsencrypt/live/adapteros.example.com/privkey.pem;

       ssl_protocols TLSv1.2 TLSv1.3;
       ssl_ciphers ECDHE-RSA-AES256-GCM-SHA512:DHE-RSA-AES256-GCM-SHA512;
       ssl_prefer_server_ciphers off;
   }
   ```

---

## Adapter Issues

### Adapter Loading Errors

**Symptoms:**
- Adapters fail to load at startup
- "adapter corrupted" errors
- Missing adapter warnings

**Diagnosis:**
```bash
# Check adapter files
ls -la /var/lib/adapteros/adapters/

# Test adapter integrity
for adapter in /var/lib/adapteros/adapters/*.aos; do
  echo "Testing $adapter..."
  ./target/release/aosctl adapter inspect "$adapter" || echo "FAILED: $adapter"
done

# Check loading logs
grep "adapter.*load\|adapter.*error\|adapter.*fail" /var/log/adapteros/server.log | tail -20
```

**Solutions:**

1. **Re-upload Corrupted Adapters**
   ```bash
   # Identify corrupted adapters
   find /var/lib/adapteros/adapters/ -name "*.aos" -exec ./target/release/aosctl adapter inspect {} \; 2>&1 | grep -v "OK" | cut -d: -f1

   # Remove corrupted files
   # Re-upload from backup or original source
   ```

2. **Check File Permissions**
   ```bash
   # Fix adapter file permissions
   sudo chown -R adapteros:adapteros /var/lib/adapteros/adapters/
   sudo chmod 644 /var/lib/adapteros/adapters/*.aos
   ```

### Router Selection Issues

**Symptoms:**
- Wrong adapters selected for requests
- Poor inference quality
- Router calibration errors

**Diagnosis:**
```bash
# Check router configuration
grep -A10 "\[router\]" configs/production.toml

# Check router metrics
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/metrics/system | jq '.router'

# Check router logs
grep -i "router\|select\|calibrat" /var/log/adapteros/server.log | tail -20

# Test router selection
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Test prompt",
    "adapters": ["auto"],
    "debug": true
  }' \
  http://localhost:8080/api/v1/inference/chat
```

**Solutions:**

1. **Router Recalibration**
   ```bash
   # Trigger router recalibration
   curl -X POST \
     -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/system/router-recalibrate
   ```

2. **Update Router Configuration**
   ```toml
   # Adjust router settings
   [router]
   k_sparse = 3
   entropy_floor = 0.05  # Increase for more diverse selection
   gate_quant = "q15"
   ```

---

## Training Issues

### Training Job Failures

**Symptoms:**
- Training jobs fail to start
- Jobs crash during training
- Poor training metrics

**Diagnosis:**
```bash
# Check training job status
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/training/jobs | jq '.[] | {id, status, error}'

# Check training logs
tail -50 /var/log/adapteros/training.log

# Check system resources during training
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/metrics/system

# Validate training dataset
head -5 /data/training/dataset.jsonl | jq .
wc -l /data/training/dataset.jsonl
```

**Solutions:**

1. **Resource Issues**
   ```bash
   # Check available memory for training
   free -h

   # Reduce batch size if OOM
   sed -i 's/batch_size = 8/batch_size = 4/' training_config.json
   ```

2. **Dataset Issues**
   ```bash
   # Validate JSONL format
   while read -r line; do
     echo "$line" | jq . || echo "Invalid JSON: $line"
   done < /data/training/dataset.jsonl | head -5

   # Check dataset size
   ls -lh /data/training/dataset.jsonl
   ```

### Training Performance Issues

**Symptoms:**
- Training is very slow
- GPU utilization is low
- Loss not decreasing

**Diagnosis:**
```bash
# Monitor training progress
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/training/jobs/train_123 | jq '.progress, .metrics, .current_epoch'

# Check system resource usage
top -p $(pgrep adapteros-server) -n 1

# Check Metal GPU usage (on macOS)
sudo powermetrics --samplers gpu_power | grep -A5 "GPU"

# Check training configuration
cat training_config.json | jq '.config'
```

**Solutions:**

1. **Optimize Training Configuration**
   ```json
   {
     "config": {
       "batch_size": 8,
       "gradient_accumulation_steps": 2,
       "learning_rate": 0.00005,
       "warmup_steps": 100,
       "weight_decay": 0.01,
       "max_grad_norm": 1.0
     }
   }
   ```

2. **Hardware Acceleration Issues**
   ```bash
   # Check Metal framework availability
   system_profiler SPDisplaysDataType | grep Metal

   # Verify Metal kernel compilation
   ls -la metal/kernels.metallib
   ```

---

## Monitoring Issues

### Metrics Not Appearing

**Symptoms:**
- Prometheus can't scrape metrics
- Grafana dashboards show no data
- `/metrics` endpoint returns errors

**Diagnosis:**
```bash
# Test metrics endpoint
curl -f http://localhost:8080/metrics | head -10

# Check metrics configuration
grep -A5 "\[telemetry\]" configs/production.toml

# Check Prometheus configuration
cat /etc/prometheus/prometheus.yml | grep -A5 adapteros

# Test Prometheus targets
curl http://localhost:9090/api/v1/targets | jq '.data.activeTargets[] | select(.labels.job == "adapteros")'
```

**Solutions:**

1. **Enable Metrics in Configuration**
   ```toml
   [telemetry]
   enabled = true
   prometheus_port = 9090
   json_output = "/var/log/adapteros/telemetry.jsonl"
   ```

2. **Fix Prometheus Configuration**
   ```yaml
   scrape_configs:
     - job_name: 'adapteros'
       static_configs:
         - targets: ['localhost:8080']
       metrics_path: '/metrics'
       scrape_interval: 15s
   ```

### Alert Not Firing

**Symptoms:**
- Alerts don't trigger when conditions are met
- Alertmanager not sending notifications

**Diagnosis:**
```bash
# Check alert rules
cat /etc/prometheus/alerts.yml

# Test alert conditions manually
curl -s http://localhost:8080/metrics | grep adapteros_memory_usage | awk '{print $2}'

# Check Alertmanager status
curl http://localhost:9093/api/v2/alerts

# Check alert logs
tail -20 /var/log/prometheus/alertmanager.log
```

**Solutions:**

1. **Fix Alert Rules**
   ```yaml
   groups:
   - name: adapteros
     rules:
     - alert: HighMemoryUsage
       expr: adapteros_memory_usage_bytes / adapteros_memory_total_bytes > 0.85
       for: 5m
       labels:
         severity: warning
       annotations:
         summary: "AdapterOS memory usage above 85%"
   ```

2. **Configure Alertmanager**
   ```yaml
   route:
     group_by: ['alertname']
     group_wait: 10s
     group_interval: 10s
     repeat_interval: 1h
     receiver: 'email'
   receivers:
   - name: 'email'
     email_configs:
     - to: 'ops@company.com'
       from: 'alerts@company.com'
   ```

---

## Security Issues

### Unauthorized Access Attempts

**Symptoms:**
- Multiple failed login attempts
- Suspicious IP addresses in logs
- Brute force attack indicators

**Diagnosis:**
```bash
# Check authentication logs
grep -i "login\|auth\|unauthorized" /var/log/adapteros/server.log | tail -50

# Check failed login attempts
grep "invalid credentials" /var/log/adapteros/server.log | cut -d' ' -f1 | uniq -c | sort -nr

# Check rate limiting
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/metrics/system | jq '.security.rate_limited_requests'
```

**Solutions:**

1. **Enable Rate Limiting**
   ```toml
   [security.rate_limiting]
   enabled = true
   requests_per_minute = 30
   burst = 10
   ```

2. **Configure Account Lockout**
   ```toml
   [auth]
   max_login_attempts = 5
   lockout_duration_minutes = 30
   ```

3. **IP-based Blocking**
   ```bash
   # Check for suspicious IPs
   grep "POST /api/v1/auth/login" /var/log/adapteros/server.log | awk '{print $1}' | sort | uniq -c | sort -nr | head -10

   # Add to firewall if needed
   sudo pfctl -t blocked_ips -T add suspicious.ip.address
   ```

### Certificate Expiry

**Symptoms:**
- SSL certificate warnings
- HTTPS connections failing
- Certificate expiry alerts

**Diagnosis:**
```bash
# Check certificate expiry
openssl x509 -in /etc/ssl/certs/adapteros.crt -text | grep -A2 "Validity"

# Calculate days until expiry
openssl x509 -in /etc/ssl/certs/adapteros.crt -enddate -noout | cut -d= -f2 | xargs -I {} date -d {} +%s | awk '{print int(($1 - systime()) / 86400)}'

# Check certificate chain
openssl verify -CAfile /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/adapteros.crt
```

**Solutions:**

1. **Renew Certificate**
   ```bash
   # Using Let's Encrypt
   certbot renew --cert-name adapteros.example.com

   # Manual renewal
   openssl req -new -key adapteros.key -out adapteros.csr
   # Submit CSR to CA and install new certificate
   ```

2. **Update Certificate Configuration**
   ```bash
   # Reload web server configuration
   sudo systemctl reload nginx

   # Check certificate is loaded
   curl -v https://adapteros.example.com/healthz 2>&1 | grep "SSL certificate verify ok"
   ```

---

## Getting Help

### When to Escalate

**Critical Issues (Escalate Immediately):**
- Service completely down
- Data loss or corruption
- Security breach
- Production impacting performance issues

**High Priority Issues (Escalate within 1 hour):**
- Partial service degradation
- Authentication system failures
- Memory usage > 95%
- Training job failures

**Normal Issues (Escalate within 4 hours):**
- Performance degradation < 50%
- Monitoring alerts
- Configuration issues

### Diagnostic Information to Collect

When escalating issues, always include:

```bash
# System information
uname -a
sw_vers  # macOS
free -h
df -h

# Service status
systemctl status adapteros-server
ps aux | grep adapteros

# Recent logs
tail -100 /var/log/adapteros/server.log

# Configuration (redact secrets)
grep -v "password\|secret\|key" configs/production.toml

# Current metrics
curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/v1/metrics/system

# Database status
psql -h localhost -U adapteros -d adapteros_prod -c "SELECT version();"
psql -h localhost -U adapteros -d adapteros_prod -c "SELECT COUNT(*) FROM adapters;"
```

---

**Last Updated:** 2025-01-15
**Version:** 1.0
**Maintained By:** AdapterOS Support Team

This troubleshooting guide should resolve most common AdapterOS issues. For issues not covered here, please escalate with complete diagnostic information.
