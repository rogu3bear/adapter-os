# AdapterOS Operational Runbooks

**Complete operational procedures for common maintenance, troubleshooting, and management tasks in AdapterOS production environments.**

---

## Table of Contents

- [System Startup and Shutdown](#system-startup-and-shutdown)
- [Adapter Management](#adapter-management)
- [Training Operations](#training-operations)
- [Performance Monitoring](#performance-monitoring)
- [Backup and Recovery](#backup-and-recovery)
- [Security Operations](#security-operations)
- [Incident Response](#incident-response)
- [Maintenance Procedures](#maintenance-procedures)

---

## System Startup and Shutdown

### Normal System Startup

**Estimated Time:** 5-10 minutes

#### Prerequisites
- PostgreSQL database is running
- All configuration files are in place
- Required directories exist with correct permissions

#### Procedure

1. **Verify Prerequisites**
   ```bash
   # Check database connectivity
   psql -h localhost -U adapteros -d adapteros_prod -c "SELECT version();"

   # Verify configuration files
   ls -la configs/production.toml
   ls -la var/jwt_*.pem

   # Check directory permissions
   ls -ld /var/lib/adapteros/
   ls -ld /var/log/adapteros/
   ```

2. **Start Database (if not running)**
   ```bash
   # Start PostgreSQL
   brew services start postgresql@15

   # Verify database is ready
   pg_isready -h localhost -p 5432
   ```

3. **Initialize System (first-time only)**
   ```bash
   # Run database migrations
   ./target/release/aosctl db migrate

   # Initialize tenant
   ./target/release/aosctl init-tenant --id production --uid 1000 --gid 1000

   # Import base model
   ./target/release/aosctl import-model \
     --name qwen2.5-7b-instruct \
     --weights models/qwen2.5-7b-mlx/weights.safetensors \
     --config models/qwen2.5-7b-mlx/config.json \
     --tokenizer models/qwen2.5-7b-mlx/tokenizer.json
   ```

4. **Start AdapterOS Server**
   ```bash
   # Start in background
   nohup ./target/release/adapteros-server \
     --config configs/production.toml \
     > /var/log/adapteros/server.log 2>&1 &

   # Capture PID
   echo $! > /var/run/adapteros.pid
   ```

5. **Verify Startup**
   ```bash
   # Check process is running
   ps aux | grep adapteros-server

   # Check health endpoint
   curl -f http://localhost:8080/healthz

   # Check readiness
   curl -f http://localhost:8080/readyz

   # Verify metrics endpoint
   curl -f http://localhost:8080/metrics
   ```

6. **Monitor Initial Logs**
   ```bash
   # Check for startup errors
   tail -f /var/log/adapteros/server.log | head -50

   # Verify adapter loading
   grep "Loaded adapter" /var/log/adapteros/server.log
   ```

#### Success Criteria
- Server responds to health checks (HTTP 200)
- No critical errors in logs
- Metrics endpoint returns data
- At least one adapter is loaded

#### Rollback Procedure
If startup fails:
```bash
# Stop server if running
kill $(cat /var/run/adapteros.pid)

# Check logs for errors
tail -100 /var/log/adapteros/server.log

# Verify database state
psql -d adapteros_prod -c "SELECT COUNT(*) FROM adapters;"
```

### Emergency Shutdown

**Estimated Time:** 1-2 minutes

#### Procedure

1. **Graceful Shutdown (Recommended)**
   ```bash
   # Send SIGTERM for graceful shutdown
   kill -TERM $(cat /var/run/adapteros.pid)

   # Wait up to 30 seconds for graceful shutdown
   timeout 30 tail -f /var/log/adapteros/server.log | grep -q "Shutdown complete"
   ```

2. **Force Shutdown (if graceful fails)**
   ```bash
   # Send SIGKILL if graceful shutdown doesn't complete
   kill -KILL $(cat /var/run/adapteros.pid)

   # Clean up PID file
   rm -f /var/run/adapteros.pid
   ```

3. **Verify Shutdown**
   ```bash
   # Confirm no processes remain
   ps aux | grep adapteros-server | grep -v grep || echo "No processes found"

   # Check for core dumps
   find /var/lib/adapteros -name "core.*" -type f
   ```

---

## Adapter Management

### Registering a New Adapter

**Estimated Time:** 10-15 minutes

#### Prerequisites
- Adapter file (.aos format) is available
- Manifest information is prepared
- Sufficient disk space for adapter storage

#### Procedure

1. **Prepare Adapter Information**
   ```bash
   # Validate adapter file
   ls -lh custom_adapter.aos

   # Extract manifest (if available)
   ./target/release/aosctl adapter inspect custom_adapter.aos
   ```

2. **Register Adapter via API**
   ```bash
   # Get authentication token
   TOKEN=$(curl -s -X POST http://localhost:8080/api/v1/auth/login \
     -H "Content-Type: application/json" \
     -d '{"email":"admin@example.com","password":"password"}' \
     | jq -r '.token')

   # Register adapter
   curl -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "manifest": {
         "name": "Custom Python Adapter",
         "description": "Fine-tuned for Python development tasks",
         "base_model": "qwen2.5-7b-instruct",
         "rank": 16,
         "tags": ["python", "development"],
         "metadata": {
           "training_dataset": "python-code-corpus-v3",
           "epochs": 3,
           "learning_rate": 0.0001
         }
       }
     }' \
     http://localhost:8080/api/v1/adapters
   ```

3. **Upload Adapter Weights**
   ```bash
   # Extract adapter ID from registration response
   ADAPTER_ID="custom_adapter_abc123"

   # Upload weights
   curl -X PUT \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/octet-stream" \
     --data-binary @custom_adapter.aos \
     http://localhost:8080/api/v1/adapters/$ADAPTER_ID/upload
   ```

4. **Verify Registration**
   ```bash
   # Check adapter is listed
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/adapters | jq '.[] | select(.id == "'$ADAPTER_ID'")'

   # Verify adapter is available for inference
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/adapters/$ADAPTER_ID
   ```

5. **Monitor Initial Performance**
   ```bash
   # Run test inference
   curl -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "prompt": "Hello world in Python",
       "adapters": ["'$ADAPTER_ID'"],
       "max_tokens": 50
     }' \
     http://localhost:8080/api/v1/inference/chat
   ```

#### Success Criteria
- Adapter appears in adapter list
- Upload completes without errors
- Test inference succeeds
- Performance metrics are collected

### Adapter Eviction and Cleanup

**Estimated Time:** 5-10 minutes

#### Prerequisites
- Memory pressure detected or manual cleanup needed
- System has adapters that can be safely evicted

#### Procedure

1. **Check Current Memory Usage**
   ```bash
   # Get system metrics
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system | jq '.memory'
   ```

2. **Identify Eviction Candidates**
   ```bash
   # List loaded adapters with usage stats
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/adapters | jq '.[] | {id, name, last_used: .metrics.last_used}'

   # Check adapter memory usage
   for adapter in $(curl -s -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/v1/adapters | jq -r '.[].id'); do
     echo "Adapter: $adapter"
     curl -s -H "Authorization: Bearer $TOKEN" \
       http://localhost:8080/api/v1/metrics/adapters/$adapter | jq '.memory_usage_mb'
   done
   ```

3. **Trigger Manual Eviction (if needed)**
   ```bash
   # Evict specific adapter
   curl -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"adapter_id": "old_adapter_id"}' \
     http://localhost:8080/api/v1/system/evict-adapter
   ```

4. **Verify Memory Recovery**
   ```bash
   # Check memory usage after eviction
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system | jq '.memory.headroom_pct'

   # Confirm adapter is no longer loaded
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/adapters | jq '.[] | select(.id == "evicted_adapter_id")' || echo "Adapter evicted"
   ```

---

## Training Operations

### Starting a Training Job

**Estimated Time:** 15-30 minutes

#### Prerequisites
- Training dataset is prepared and accessible
- Base model is available
- Sufficient compute resources available

#### Procedure

1. **Prepare Training Dataset**
   ```bash
   # Validate dataset format
   head -5 /data/training/python-code.jsonl | jq .

   # Count training samples
   wc -l /data/training/python-code.jsonl

   # Verify dataset accessibility
   ls -lh /data/training/
   ```

2. **Check System Resources**
   ```bash
   # Verify available memory
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system | jq '.memory'

   # Check current training queue
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/training/jobs | jq '.[] | select(.status == "running")'
   ```

3. **Submit Training Job**
   ```bash
   # Start training via API
   curl -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "name": "Python Expert Adapter v2",
       "base_model": "qwen2.5-7b-instruct",
       "dataset": {
         "path": "/data/training/python-code.jsonl",
         "format": "jsonl",
         "size": 100000
       },
       "config": {
         "rank": 16,
         "epochs": 3,
         "batch_size": 4,
         "learning_rate": 0.0001,
         "lora_alpha": 32,
         "warmup_steps": 100
       },
       "validation": {
         "split_ratio": 0.1,
         "metrics": ["perplexity", "accuracy", "loss"]
       },
       "checkpointing": {
         "interval_steps": 500,
         "save_best_only": true
       }
     }' \
     http://localhost:8080/api/v1/training/jobs
   ```

4. **Monitor Training Progress**
   ```bash
   # Get training job ID from response
   JOB_ID="train_20251103_003"

   # Monitor progress
   watch -n 30 'curl -s -H "Authorization: Bearer '$TOKEN'" \
     http://localhost:8080/api/v1/training/jobs/'$JOB_ID' | jq ".progress, .status, .current_epoch, .metrics"'
   ```

5. **Monitor System Resources During Training**
   ```bash
   # Watch resource usage
   while true; do
     curl -s -H "Authorization: Bearer $TOKEN" \
       http://localhost:8080/api/v1/metrics/system | jq '.memory.used_bytes / .memory.total_bytes * 100'
     sleep 60
   done
   ```

#### Success Criteria
- Training job starts successfully
- Progress increases steadily
- No out-of-memory errors
- Validation metrics improve over time

### Canceling a Training Job

**Estimated Time:** 2-5 minutes

#### Procedure

1. **Identify Running Job**
   ```bash
   # List running training jobs
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/training/jobs | jq '.[] | select(.status == "running")'
   ```

2. **Cancel Job**
   ```bash
   JOB_ID="train_20251103_003"

   curl -X POST \
     -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/training/jobs/$JOB_ID/cancel
   ```

3. **Verify Cancellation**
   ```bash
   # Check job status
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/training/jobs/$JOB_ID | jq '.status'

   # Verify resources are freed
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system | jq '.memory'
   ```

---

## Performance Monitoring

### Setting Up Monitoring Dashboard

**Estimated Time:** 10-15 minutes

#### Prerequisites
- Prometheus and Grafana are installed
- AdapterOS metrics endpoint is accessible

#### Procedure

1. **Configure Prometheus Scraping**
   ```yaml
   # Add to prometheus.yml
   scrape_configs:
     - job_name: 'adapteros'
       static_configs:
         - targets: ['localhost:8080']
       metrics_path: '/metrics'
       scrape_interval: 15s
   ```

2. **Import Grafana Dashboard**
   ```bash
   # Import dashboard JSON
   curl -X POST \
     -H "Content-Type: application/json" \
     -d @docs/monitoring/adapteros-dashboard.json \
     http://localhost:3000/api/dashboards/import
   ```

3. **Configure Alert Rules**
   ```yaml
   # Add to alerting rules
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

     - alert: HighInferenceLatency
       expr: histogram_quantile(0.95, adapteros_inference_latency_seconds) > 0.2
       for: 2m
       labels:
         severity: warning
       annotations:
         summary: "AdapterOS p95 inference latency > 200ms"
   ```

4. **Set Up Log Aggregation**
   ```bash
   # Configure log shipping to Elasticsearch or similar
   cat > /etc/rsyslog.d/adapteros.conf << EOF
   if $programname == 'adapteros-server' then /var/log/adapteros/adapteros.log
   & stop
   EOF

   # Restart rsyslog
   sudo systemctl restart rsyslog
   ```

### Investigating Performance Issues

**Estimated Time:** 15-30 minutes

#### Procedure

1. **Gather System Metrics**
   ```bash
   # Get current system state
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system

   # Check adapter performance
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/adapters | jq '.[] | {id, metrics: .metrics}'
   ```

2. **Analyze Request Patterns**
   ```bash
   # Check request queue depth
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system | jq '.inference.queue_depth'

   # Analyze recent requests
   tail -100 /var/log/adapteros/server.log | grep "inference_request"
   ```

3. **Check Resource Utilization**
   ```bash
   # Memory usage over time
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system | jq '.memory'

   # CPU usage (if available)
   top -p $(pgrep adapteros-server) -n 1
   ```

4. **Identify Bottlenecks**
   ```bash
   # Check for memory pressure
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system | jq '.memory.headroom_pct'

   # Check adapter loading times
   grep "adapter_load" /var/log/adapteros/server.log | tail -20
   ```

5. **Apply Performance Tuning**
   ```bash
   # Adjust worker count if CPU-bound
   # Edit configs/production.toml
   workers = 12  # Increase from 8

   # Restart server
   systemctl restart adapteros-server

   # Or reduce K value if memory-bound
   curl -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"k_sparse": 2}' \
     http://localhost:8080/api/v1/system/config
   ```

---

## Backup and Recovery

### Database Backup

**Estimated Time:** 5-15 minutes (depending on database size)

#### Prerequisites
- Sufficient disk space for backup
- Database is running and accessible
- Backup directory exists

#### Procedure

1. **Prepare Backup Directory**
   ```bash
   # Create backup directory
   BACKUP_DIR="/backup/adapteros/$(date +%Y%m%d_%H%M%S)"
   mkdir -p $BACKUP_DIR

   # Check available space
   df -h $BACKUP_DIR
   ```

2. **Create Database Backup**
   ```bash
   # Stop adapteros-server for consistent backup (optional, hot backup preferred)
   systemctl stop adapteros-server

   # Create PostgreSQL backup
   pg_dump -h localhost -U adapteros -d adapteros_prod \
     --format=custom \
     --compress=9 \
     --file=$BACKUP_DIR/database.backup \
     --verbose

   # Restart server
   systemctl start adapteros-server
   ```

3. **Backup Configuration and Keys**
   ```bash
   # Backup configuration files
   cp configs/production.toml $BACKUP_DIR/
   cp -r var/ $BACKUP_DIR/

   # Encrypt sensitive files
   openssl enc -aes-256-cbc -salt \
     -in $BACKUP_DIR/var/jwt_private.pem \
     -out $BACKUP_DIR/jwt_private.pem.enc \
     -k $(openssl rand -base64 32)

   # Remove unencrypted private key
   rm $BACKUP_DIR/var/jwt_private.pem
   ```

4. **Backup Adapters and Models**
   ```bash
   # Backup adapter directory
   cp -r /var/lib/adapteros/adapters $BACKUP_DIR/

   # Backup models
   cp -r models/ $BACKUP_DIR/
   ```

5. **Create Backup Manifest**
   ```bash
   cat > $BACKUP_DIR/manifest.txt << EOF
   Backup created: $(date)
   Database version: $(psql -h localhost -U adapteros -d adapteros_prod -t -c "SELECT version();")
   AdapterOS version: $(./target/release/adapteros-server --version)
   Files included:
   - database.backup (PostgreSQL dump)
   - production.toml (configuration)
   - var/ (JWT keys and secrets)
   - adapters/ (adapter weights)
   - models/ (base models)
   EOF
   ```

6. **Verify Backup Integrity**
   ```bash
   # List backup contents
   find $BACKUP_DIR -type f -exec ls -lh {} \;

   # Test database backup
   pg_restore --list $BACKUP_DIR/database.backup | head -10

   # Check backup size
   du -sh $BACKUP_DIR
   ```

7. **Archive and Store**
   ```bash
   # Create compressed archive
   tar czf ${BACKUP_DIR}.tar.gz -C $(dirname $BACKUP_DIR) $(basename $BACKUP_DIR)

   # Move to long-term storage
   mv ${BACKUP_DIR}.tar.gz /backup/archive/

   # Clean up temporary directory
   rm -rf $BACKUP_DIR
   ```

#### Success Criteria
- Backup completes without errors
- All critical files are included
- Backup size is reasonable
- Archive is stored securely

### Database Recovery

**Estimated Time:** 15-30 minutes

#### Prerequisites
- Valid backup archive exists
- System has sufficient resources
- Recovery environment is prepared

#### Procedure

1. **Prepare Recovery Environment**
   ```bash
   # Stop adapteros-server
   systemctl stop adapteros-server

   # Create recovery directory
   RECOVERY_DIR="/tmp/adapteros_recovery"
   mkdir -p $RECOVERY_DIR

   # Extract backup
   tar xzf /backup/archive/adapteros_20251103_120000.tar.gz -C $RECOVERY_DIR
   ```

2. **Restore Database**
   ```bash
   # Create fresh database (if needed)
   createdb -h localhost -U postgres adapteros_recovery

   # Restore from backup
   pg_restore -h localhost -U adapteros -d adapteros_recovery \
     --clean --if-exists \
     --verbose \
     $RECOVERY_DIR/database.backup
   ```

3. **Restore Configuration and Keys**
   ```bash
   # Restore configuration
   cp $RECOVERY_DIR/production.toml configs/

   # Decrypt and restore private key
   openssl enc -d -aes-256-cbc \
     -in $RECOVERY_DIR/jwt_private.pem.enc \
     -out var/jwt_private.pem \
     -k $(cat /path/to/encryption_key)

   # Restore public key
   cp $RECOVERY_DIR/var/jwt_public.pem var/
   ```

4. **Restore Adapters and Models**
   ```bash
   # Restore adapter weights
   cp -r $RECOVERY_DIR/adapters/* /var/lib/adapteros/adapters/

   # Restore models
   cp -r $RECOVERY_DIR/models/* models/
   ```

5. **Update Database Connection**
   ```bash
   # Temporarily point to recovery database
   sed -i 's/adapteros_prod/adapteros_recovery/' configs/production.toml
   ```

6. **Start and Verify Recovery**
   ```bash
   # Start server
   ./target/release/adapteros-server --config configs/production.toml

   # Check health
   curl -f http://localhost:8080/healthz

   # Verify adapters are loaded
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/adapters | jq length
   ```

7. **Switch to Production Database**
   ```bash
   # Stop server
   systemctl stop adapteros-server

   # Drop old database and rename recovery
   psql -h localhost -U postgres -c "DROP DATABASE adapteros_prod;"
   psql -h localhost -U postgres -c "ALTER DATABASE adapteros_recovery RENAME TO adapteros_prod;"

   # Update configuration
   sed -i 's/adapteros_recovery/adapteros_prod/' configs/production.toml

   # Restart server
   systemctl start adapteros-server
   ```

---

## Security Operations

### JWT Key Rotation

**Estimated Time:** 10-15 minutes

#### Prerequisites
- Current keys are backed up
- System can tolerate brief service interruption
- All clients can handle new tokens

#### Procedure

1. **Generate New Key Pair**
   ```bash
   # Generate Ed25519 keypair
   openssl genpkey -algorithm Ed25519 -out var/jwt_private_new.pem
   openssl pkey -in var/jwt_private_new.pem -pubout -out var/jwt_public_new.pem

   # Set restrictive permissions
   chmod 600 var/jwt_private_new.pem
   chmod 644 var/jwt_public_new.pem
   ```

2. **Configure Dual Key Validation**
   ```toml
   # Temporarily add to configs/production.toml
   [jwt.rotation]
   old_public_key_file = "var/jwt_public.pem"
   new_private_key_file = "var/jwt_private_new.pem"
   new_public_key_file = "var/jwt_public_new.pem"
   migration_period_hours = 24
   ```

3. **Restart Server**
   ```bash
   # Restart with new configuration
   systemctl restart adapteros-server

   # Verify server starts successfully
   curl -f http://localhost:8080/healthz
   ```

4. **Test New Key Issuance**
   ```bash
   # Login with new key (should work)
   NEW_TOKEN=$(curl -s -X POST http://localhost:8080/api/v1/auth/login \
     -H "Content-Type: application/json" \
     -d '{"email":"admin@example.com","password":"password"}' \
     | jq -r '.token')

   # Verify new token works
   curl -H "Authorization: Bearer $NEW_TOKEN" \
     http://localhost:8080/api/v1/adapters | head -1
   ```

5. **Verify Old Token Still Works**
   ```bash
   # Test old token still valid during migration
   curl -H "Authorization: Bearer $OLD_TOKEN" \
     http://localhost:8080/api/v1/adapters | head -1
   ```

6. **Complete Migration**
   ```bash
   # After migration period, update to single key
   mv var/jwt_private_new.pem var/jwt_private.pem
   mv var/jwt_public_new.pem var/jwt_public.pem

   # Remove old keys
   rm var/jwt_private_old.pem var/jwt_public_old.pem

   # Update configuration
   sed -i '/\[jwt.rotation\]/,/migration_period_hours/d' configs/production.toml

   # Restart server
   systemctl restart adapteros-server
   ```

7. **Backup New Keys**
   ```bash
   # Create encrypted backup
   tar czf - var/jwt_*.pem | \
     openssl enc -aes-256-cbc -salt -out /backup/jwt_keys_$(date +%Y%m%d).tar.gz.enc
   ```

#### Success Criteria
- New tokens are issued with new key
- Old tokens still work during migration
- Server restarts without issues
- All API calls continue to work

### Certificate Renewal

**Estimated Time:** 5-10 minutes

#### Prerequisites
- Certificate is nearing expiration
- Domain validation is available
- Backup of current certificate exists

#### Procedure

1. **Check Current Certificate**
   ```bash
   # Check expiration date
   openssl x509 -in /etc/letsencrypt/live/adapteros.example.com/cert.pem -text | grep "Not After"

   # Test certificate validity
   openssl s_client -connect adapteros.example.com:443 -servername adapteros.example.com < /dev/null 2>/dev/null | openssl x509 -noout -dates
   ```

2. **Renew Certificate**
   ```bash
   # Stop web server temporarily (if using standalone challenge)
   systemctl stop nginx

   # Renew certificate
   certbot certonly --standalone -d adapteros.example.com

   # Restart web server
   systemctl start nginx
   ```

3. **Verify New Certificate**
   ```bash
   # Check new expiration date
   openssl x509 -in /etc/letsencrypt/live/adapteros.example.com/cert.pem -text | grep "Not After"

   # Test SSL connection
   curl -v https://adapteros.example.com/healthz 2>&1 | grep "SSL certificate verify ok"
   ```

4. **Update Certificate in Configuration**
   ```bash
   # If certificate paths changed, update nginx config
   cat > /etc/nginx/sites-available/adapteros << EOF
   server {
       listen 443 ssl http2;
       server_name adapteros.example.com;

       ssl_certificate /etc/letsencrypt/live/adapteros.example.com/fullchain.pem;
       ssl_certificate_key /etc/letsencrypt/live/adapteros.example.com/privkey.pem;

       # ... rest of config
   }
   EOF

   # Reload nginx
   nginx -t && systemctl reload nginx
   ```

---

## Incident Response

### High Memory Usage Incident

**Estimated Time:** 10-20 minutes

#### Detection
- Memory usage > 85%
- Alert triggered from monitoring
- System performance degradation

#### Immediate Response

1. **Assess Current State**
   ```bash
   # Get detailed memory metrics
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system | jq '.memory'

   # Check adapter memory usage
   for adapter in $(curl -s -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/v1/adapters | jq -r '.[].id'); do
     echo "Adapter: $adapter"
     curl -s -H "Authorization: Bearer $TOKEN" \
       http://localhost:8080/api/v1/metrics/adapters/$adapter | jq '.memory_usage_mb'
   done
   ```

2. **Reduce Memory Pressure**
   ```bash
   # Reduce K-sparse value temporarily
   curl -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"k_sparse": 2}' \
     http://localhost:8080/api/v1/system/config

   # Wait for system to stabilize
   sleep 30

   # Check memory improvement
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/metrics/system | jq '.memory.headroom_pct'
   ```

3. **Evict Unused Adapters**
   ```bash
   # Identify least recently used adapters
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:8080/api/v1/adapters | jq '.[] | {id, last_used: .metrics.last_used}' | sort_by(.last_used)

   # Evict oldest adapter
   curl -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"adapter_id": "old_adapter_id"}' \
     http://localhost:8080/api/v1/system/evict-adapter
   ```

4. **Monitor Recovery**
   ```bash
   # Watch memory usage
   watch -n 10 'curl -s -H "Authorization: Bearer '$TOKEN'" \
     http://localhost:8080/api/v1/metrics/system | jq ".memory.headroom_pct"'
   ```

#### Post-Incident Analysis

1. **Analyze Root Cause**
   ```bash
   # Check logs for memory spikes
   grep "memory.*high\|eviction" /var/log/adapteros/server.log | tail -50

   # Analyze request patterns
   grep "inference_request" /var/log/adapteros/server.log | tail -100 | cut -d' ' -f1 | uniq -c
   ```

2. **Implement Prevention**
   ```toml
   # Update production.toml
   [memory]
   min_headroom_pct = 20  # Increase from 15%
   max_adapters_per_tenant = 15  # Reduce from 20

   [eviction_policy]
   cold_threshold_mins = 30  # Reduce from 60
   ```

### Service Unavailability Incident

**Estimated Time:** 5-15 minutes

#### Detection
- Health check failures
- Alert from monitoring system
- User reports of service unavailability

#### Immediate Response

1. **Check Service Status**
   ```bash
   # Check if process is running
   ps aux | grep adapteros-server

   # Check health endpoints
   curl -f http://localhost:8080/healthz || echo "Health check failed"
   curl -f http://localhost:8080/readyz || echo "Readiness check failed"
   ```

2. **Check System Resources**
   ```bash
   # Check system resources
   df -h /var/lib/adapteros
   free -h

   # Check database connectivity
   psql -h localhost -U adapteros -d adapteros_prod -c "SELECT 1;" || echo "Database connection failed"
   ```

3. **Review Recent Logs**
   ```bash
   # Check recent errors
   tail -50 /var/log/adapteros/server.log | grep -i error

   # Check for crashes or panics
   grep -i "panic\|crash\|segmentation fault" /var/log/adapteros/server.log
   ```

4. **Attempt Service Restart**
   ```bash
   # Try graceful restart
   systemctl restart adapteros-server

   # Wait for startup
   sleep 30

   # Check if service recovered
   curl -f http://localhost:8080/healthz && echo "Service recovered"
   ```

5. **Escalate if Needed**
   ```bash
   # If restart fails, check for:
   # - Disk space issues
   # - Database corruption
   # - Configuration errors
   # - Hardware failures

   # Collect diagnostic information
   tar czf diagnostic_$(date +%Y%m%d_%H%M%S).tar.gz \
     /var/log/adapteros/server.log \
     configs/production.toml \
     /var/lib/adapteros/
   ```

---

## Maintenance Procedures

### Log Rotation

**Estimated Time:** 2-5 minutes

#### Prerequisites
- Logrotate is installed
- Sufficient disk space for rotated logs

#### Procedure

1. **Configure Log Rotation**
   ```bash
   # Create logrotate configuration
   cat > /etc/logrotate.d/adapteros << EOF
   /var/log/adapteros/*.log {
       daily
       rotate 30
       compress
       delaycompress
       missingok
       notifempty
       create 0644 adapteros adapteros
       postrotate
           systemctl reload adapteros-server
       endscript
   }
   EOF
   ```

2. **Test Log Rotation**
   ```bash
   # Test configuration
   logrotate -d /etc/logrotate.d/adapteros

   # Force rotation for testing
   logrotate -f /etc/logrotate.d/adapteros
   ```

3. **Verify Rotation**
   ```bash
   # Check rotated files
   ls -la /var/log/adapteros/

   # Verify server still logging
   logger -t adapteros-test "Test log entry"
   tail -1 /var/log/adapteros/server.log
   ```

### Database Maintenance

**Estimated Time:** 15-30 minutes

#### Prerequisites
- Database backup completed
- Maintenance window scheduled
- System can tolerate brief downtime

#### Procedure

1. **Pre-Maintenance Backup**
   ```bash
   # Create maintenance backup
   MAINTENANCE_BACKUP="/backup/adapteros/maintenance_$(date +%Y%m%d_%H%M%S).sql"
   pg_dump -h localhost -U adapteros adapteros_prod > $MAINTENANCE_BACKUP
   ```

2. **Analyze Database**
   ```sql
   -- Connect to database
   psql -h localhost -U adapteros adapteros_prod

   -- Analyze table bloat
   SELECT schemaname, tablename, n_dead_tup, n_live_tup
   FROM pg_stat_user_tables
   ORDER BY n_dead_tup DESC;

   -- Check index usage
   SELECT indexname, idx_scan, idx_tup_read, idx_tup_fetch
   FROM pg_stat_user_indexes
   ORDER BY idx_scan DESC;
   ```

3. **Perform Vacuum and Analyze**
   ```sql
   -- Vacuum analyze all tables
   VACUUM ANALYZE;

   -- Reindex if needed
   REINDEX DATABASE adapteros_prod;
   ```

4. **Update Statistics**
   ```sql
   -- Update table statistics
   ANALYZE;

   -- Check autovacuum settings
   SHOW autovacuum;
   ```

5. **Post-Maintenance Verification**
   ```sql
   -- Verify database integrity
   SELECT COUNT(*) FROM adapters;
   SELECT COUNT(*) FROM training_jobs;
   SELECT COUNT(*) FROM inference_requests LIMIT 1;
   ```

6. **Update Maintenance Logs**
   ```bash
   # Log maintenance completion
   echo "Database maintenance completed at $(date)" >> /var/log/adapteros/maintenance.log

   # Check database size after maintenance
   psql -h localhost -U adapteros adapteros_prod -c "SELECT pg_size_pretty(pg_database_size('adapteros_prod'));"
   ```

---

**Last Updated:** 2025-01-15
**Version:** 1.0
**Maintained By:** AdapterOS Operations Team

This runbook provides comprehensive procedures for operating AdapterOS in production. Regular review and updates based on operational experience are recommended.
