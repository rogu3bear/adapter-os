# AdapterOS Disaster Recovery Guide

## Overview

This guide provides procedures for recovering AdapterOS systems from various disaster scenarios. Recovery procedures are categorized by severity and impact.

## Recovery Categories

### Category 1: Service Recovery (Low Impact)
- Service crashes or stops responding
- Single node failures
- Configuration corruption
- Expected downtime: < 15 minutes

### Category 2: Data Recovery (Medium Impact)
- Database corruption or loss
- Model file corruption
- Configuration loss
- Expected downtime: 30 minutes - 2 hours

### Category 3: System Recovery (High Impact)
- Complete system failure
- Multiple node failures
- Data center disaster
- Expected downtime: 2+ hours

## Category 1: Service Recovery

### Automated Service Recovery

Most service failures are handled automatically:

```bash
# Check service status
sudo systemctl status adapteros

# View recent logs
sudo journalctl -u adapteros -n 50 -f

# Restart service (if not auto-restarting)
sudo systemctl restart adapteros

# Verify recovery
curl http://localhost:8080/healthz
```

### Manual Service Recovery

If automatic recovery fails:

1. **Stop the service:**
   ```bash
   sudo systemctl stop adapteros
   ```

2. **Check system resources:**
   ```bash
   # Check disk space
   df -h

   # Check memory
   free -h

   # Check for zombie processes
   ps aux | grep defunct
   ```

3. **Clean up temporary files:**
   ```bash
   # Remove temporary files
   find /tmp -name "adapteros*" -type f -mtime +1 -delete

   # Clean model cache if corrupted
   rm -rf /var/lib/adapteros/cache/*
   ```

4. **Restart with verbose logging:**
   ```bash
   sudo systemctl start adapteros
   sudo journalctl -u adapteros -f
   ```

5. **Verify all endpoints:**
   ```bash
   curl http://localhost:8080/healthz
   curl http://localhost:8080/metrics
   ```

### Configuration Recovery

If configuration is corrupted:

1. **Restore from backup:**
   ```bash
   sudo cp /backup/latest/config/config.toml /etc/adapteros/config.toml
   ```

2. **Validate configuration:**
   ```bash
   adapteros --validate-config /etc/adapteros/config.toml
   ```

3. **Restart service:**
   ```bash
   sudo systemctl restart adapteros
   ```

## Category 2: Data Recovery

### Database Recovery

#### Point-in-Time Recovery

1. **Stop the service:**
   ```bash
   sudo systemctl stop adapteros
   ```

2. **Identify the recovery point:**
   ```bash
   # Check available backups
   ls -la /backup/

   # Check backup logs for last good backup
   grep "backup completed" /var/log/adapteros/backup.log | tail -5
   ```

3. **Restore database:**
   ```bash
   # Create recovery script
   cat > /tmp/recovery.sql << 'EOF'
   -- Recovery script for AdapterOS database
   -- Run this against a fresh database instance

   -- Stop all active connections
   SELECT pg_terminate_backend(pid)
   FROM pg_stat_activity
   WHERE datname = 'adapteros' AND pid <> pg_backend_pid();

   -- Drop and recreate database
   DROP DATABASE IF EXISTS adapteros;
   CREATE DATABASE adapteros OWNER adapteros;
   \c adapteros

   -- Restore from backup
   \i /backup/latest/database/adapteros.sql
   EOF

   # Execute recovery
   sudo -u postgres psql -f /tmp/recovery.sql
   ```

4. **Verify data integrity:**
   ```sql
   -- Run integrity checks
   SELECT schemaname, tablename, n_tup_ins, n_tup_upd, n_tup_del
   FROM pg_stat_user_tables
   ORDER BY n_tup_ins DESC LIMIT 10;

   -- Check for orphaned records
   SELECT COUNT(*) as orphaned_models
   FROM models m
   LEFT JOIN base_model_imports bmi ON m.id = bmi.id
   WHERE bmi.id IS NULL;
   ```

5. **Restart service:**
   ```bash
   sudo systemctl start adapteros
   ```

#### Incremental Recovery

If only recent data is lost:

1. **Identify the gap:**
   ```sql
   -- Find missing records
   SELECT MAX(created_at) as last_import FROM base_model_imports;
   SELECT MAX(updated_at) as last_model_update FROM models;
   ```

2. **Restore from incremental backups:**
   ```bash
   # Apply incremental backups in order
   for backup in /backup/incremental/*; do
       pg_restore -d adapteros "$backup"
   done
   ```

### Model File Recovery

1. **Identify corrupted models:**
   ```bash
   # Check model validation logs
   grep "corrupt\|invalid" /var/log/adapteros/*.log | tail -20
   ```

2. **Restore from backup:**
   ```bash
   # Stop service first
   sudo systemctl stop adapteros

   # Restore model files
   cd /var/lib/adapteros
   tar -xzf /backup/latest/models/models.tar.gz

   # Restore adapter files
   tar -xzf /backup/latest/models/adapters.tar.gz
   ```

3. **Validate restored files:**
   ```bash
   # Run model validation
   find /var/lib/adapteros/models -name "*.safetensors" -exec adapteros validate-model {} \;
   ```

4. **Restart service:**
   ```bash
   sudo systemctl start adapteros
   ```

## Category 3: System Recovery

### Complete System Recovery

#### From Bare Metal

1. **Prepare the system:**
   ```bash
   # Install base OS (Ubuntu/Debian)
   # Configure networking
   # Mount storage volumes
   ```

2. **Install AdapterOS:**
   ```bash
   # Install AdapterOS package
   sudo apt update
   sudo apt install adapteros

   # Mount backup storage
   sudo mount /dev/backup-volume /backup
   ```

3. **Restore configuration:**
   ```bash
   sudo mkdir -p /etc/adapteros
   sudo cp /backup/latest/config/* /etc/adapteros/

   # Edit configuration for new environment if needed
   sudo vi /etc/adapteros/config.toml
   ```

4. **Restore database:**
   ```bash
   # Create database
   sudo -u postgres createdb adapteros
   sudo -u postgres psql -c "GRANT ALL PRIVILEGES ON DATABASE adapteros TO adapteros;"

   # Restore from backup
   gunzip < /backup/latest/database/adapteros.sql.gz | sudo -u postgres psql -d adapteros
   ```

5. **Restore model files:**
   ```bash
   sudo mkdir -p /var/lib/adapteros
   cd /var/lib/adapteros

   sudo tar -xzf /backup/latest/models/models.tar.gz
   sudo tar -xzf /backup/latest/models/adapters.tar.gz
   sudo tar -xzf /backup/latest/models/artifacts.tar.gz
   ```

6. **Start services:**
   ```bash
   sudo systemctl enable adapteros
   sudo systemctl start adapteros
   ```

7. **Verify recovery:**
   ```bash
   # Check all endpoints
   curl http://localhost:8080/healthz
   curl http://localhost:8080/metrics

   # Test basic functionality
   curl http://localhost:8080/v1/models
   ```

### Multi-Node Recovery

For clustered deployments:

1. **Identify primary node failure:**
   ```bash
   # Check cluster status (if applicable)
   adapteros cluster status
   ```

2. **Failover to secondary node:**
   ```bash
   # Trigger failover
   adapteros cluster failover --to node-2
   ```

3. **Recover primary node:**
   ```bash
   # Follow single-node recovery procedure
   # Then rejoin cluster
   adapteros cluster join node-1
   ```

4. **Resync data:**
   ```bash
   # Ensure data consistency across nodes
   adapteros cluster sync
   ```

## Recovery Validation

### Automated Validation

```bash
#!/bin/bash
# recovery_validation.sh - Validate system recovery

echo "Running recovery validation..."

# Test service health
if ! curl -f http://localhost:8080/healthz >/dev/null 2>&1; then
    echo "ERROR: Service health check failed"
    exit 1
fi

# Test database connectivity
if ! psql -d adapteros -c "SELECT 1;" >/dev/null 2>&1; then
    echo "ERROR: Database connectivity check failed"
    exit 1
fi

# Test model loading
if ! curl -f http://localhost:8080/v1/models >/dev/null 2>&1; then
    echo "ERROR: Model API check failed"
    exit 1
fi

# Test inference (if models are loaded)
if curl http://localhost:8080/v1/models 2>/dev/null | grep -q '"status":"loaded"'; then
    # Run a simple inference test
    curl -X POST http://localhost:8080/v1/inference \
        -H "Content-Type: application/json" \
        -d '{"model":"test-model","prompt":"test"}' >/dev/null 2>&1 || true
fi

echo "Recovery validation completed successfully"
```

### Manual Validation Checklist

- [ ] Service starts without errors
- [ ] All API endpoints respond
- [ ] Database connections work
- [ ] Model files are accessible
- [ ] Configuration is valid
- [ ] Metrics are being collected
- [ ] Logs are being written
- [ ] Authentication works
- [ ] Basic inference requests succeed

## Prevention Measures

### Proactive Measures

1. **Regular Backups:**
   - Schedule automated daily backups
   - Test backup restoration monthly
   - Store backups offsite or in different AZ

2. **Monitoring:**
   - Set up comprehensive monitoring
   - Configure alerts for early warning signs
   - Monitor backup success/failure

3. **High Availability:**
   - Deploy in redundant configuration
   - Use load balancers
   - Implement circuit breakers

4. **Testing:**
   - Regular disaster recovery drills
   - Chaos engineering exercises
   - Load testing under failure conditions

### Backup Strategy

#### Backup Types
- **Full Backup:** Complete system state (daily)
- **Incremental Backup:** Changes since last backup (hourly)
- **Configuration Backup:** System configuration (weekly)
- **Log Backup:** Application logs (daily)

#### Retention Policy
- Daily backups: 7 days
- Weekly backups: 4 weeks
- Monthly backups: 12 months
- Offsite copies: Indefinite

#### Testing Schedule
- Backup integrity: Daily
- Restore procedure: Weekly
- Full disaster recovery: Monthly
- Cross-region recovery: Quarterly

## Communication Plan

### During Incident
1. **Assess Impact:** Determine affected systems and users
2. **Notify Stakeholders:** Update incident response team
3. **Set Expectations:** Provide ETA for resolution
4. **Regular Updates:** Keep stakeholders informed

### Post-Incident
1. **Root Cause Analysis:** Document what went wrong
2. **Lessons Learned:** Identify improvements
3. **Action Items:** Assign responsibility for fixes
4. **Report:** Document incident and resolution

## Contact Information

### Emergency Contacts
- **Primary On-Call:** +1-555-0100
- **Secondary On-Call:** +1-555-0101
- **Infrastructure Team:** infra@company.com
- **Database Team:** dba@company.com

### Escalation Path
1. Individual contributor level
2. Team lead
3. Engineering manager
4. Director
5. VP/Executive level

## Appendices

### Recovery Time Objectives (RTO)
- Category 1: 15 minutes
- Category 2: 2 hours
- Category 3: 8 hours

### Recovery Point Objectives (RPO)
- Database: 1 hour
- Model files: 24 hours
- Configuration: 24 hours
- Logs: 24 hours

### Backup Storage Requirements
- Daily full backup: ~100GB
- Incremental backup: ~10GB
- Total retention: ~2TB

This disaster recovery guide should be reviewed quarterly and updated as systems and procedures evolve.
