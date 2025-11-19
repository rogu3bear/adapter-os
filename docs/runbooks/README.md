# AdapterOS Operational Runbooks

Quick reference guide for common operational issues and procedures.

## Purpose

These runbooks provide step-by-step procedures for diagnosing and resolving common operational issues in AdapterOS. Each runbook is self-contained and includes:

- **Symptoms**: How to identify the issue
- **Root causes**: Common causes and failure modes
- **Fix procedures**: Specific commands and steps
- **Prevention**: How to avoid the issue
- **Related files**: Source code references for deeper investigation

## Quick Index

### Startup & Configuration
- [Startup Procedures](./startup-procedures.md) - Initial setup and first run
- [Startup Failures](./startup-failures.md) - Server won't start, missing config
- [Port Binding Conflicts](./port-binding-conflicts.md) - Port 8080 in use, PID lock issues

### Database Operations
- [Database Failures](./database-failures.md) - Migration errors, connection issues, WAL problems
- [Database Optimization](./database-optimization.md) - PRAGMA optimization, WAL checkpoints

### Resource Management
- [Memory Pressure](./memory-pressure.md) - High memory usage, adapter eviction
- [Cleanup Procedures](./cleanup-procedures.md) - TTL cleanup, orphaned adapters

### Monitoring & Diagnostics
- [Health Check Failures](./health-check-failures.md) - Component degraded/unhealthy status
- [Log Analysis](./log-analysis.md) - What to look for in logs
- [Metrics Review](./metrics-review.md) - Key operational metrics

### Escalation
- [Escalation Guide](./escalation.md) - When to escalate, severity levels

## Common Commands

```bash
# System health check
aosctl doctor

# Component-specific health
curl http://localhost:8080/healthz/db
curl http://localhost:8080/healthz/router
curl http://localhost:8080/healthz/system-metrics

# View logs
tail -f var/aos-cp.log

# Check memory pressure
aosctl status memory

# Database integrity check
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"

# Reset database (development only)
aosctl db reset

# List running processes
ps aux | grep aos
```

## Emergency Response Flow

1. **Identify Issue**
   - Check health endpoints: `/healthz/all`
   - Review recent logs: `tail -100 var/aos-cp.log`
   - Check system resources: `aosctl status system`

2. **Diagnose Root Cause**
   - Use appropriate runbook from index
   - Follow diagnostic steps
   - Gather evidence for escalation

3. **Apply Fix**
   - Follow runbook procedures
   - Verify fix with health checks
   - Document resolution

4. **Escalate if Needed**
   - See [Escalation Guide](./escalation.md)
   - Prepare diagnostic bundle
   - Follow severity protocols

## File Locations

```
/Users/star/Dev/aos/
├── var/
│   ├── aos-cp.sqlite3          # Main database
│   ├── aos-cp.log              # Application logs
│   ├── aos-cp.pid              # PID lock file
│   ├── baseline_fingerprint.json  # Environment baseline
│   └── telemetry/              # Telemetry bundles
├── configs/
│   └── cp.toml                 # Server configuration
└── crates/
    ├── adapteros-server/       # Main server code
    ├── adapteros-cli/          # CLI tool (aosctl)
    └── adapteros-db/           # Database layer
```

## Related Documentation

- [Production Operations Guide](../PRODUCTION_OPERATIONS.md)
- [Database Schema](../database-schema.md)
- [Health Check API](../api/health.md)
- [Verification Checklist](../VERIFICATION_CHECKLIST.md)

## Maintenance

These runbooks are maintained as part of the AdapterOS codebase. Update them when:
- New failure modes are discovered
- Procedures change
- File paths or commands are updated
- New operational features are added

Last updated: 2025-11-19
