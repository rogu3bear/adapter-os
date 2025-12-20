# AdapterOS Production Runbooks

**Operational runbooks for incident response and troubleshooting in production AdapterOS environments.**

**Last Updated:** 2025-12-15
**Version:** 1.0
**Maintained By:** AdapterOS SRE Team

---

## Overview

This directory contains actionable runbooks for AdapterOS production operations. Each runbook follows a consistent structure:

- **Symptoms** - Observable indicators (alerts, metrics, user reports)
- **Diagnosis** - Step-by-step investigation procedures
- **Resolution** - Immediate fixes and root cause prevention
- **Escalation** - When to involve senior engineers or notify stakeholders

## Available Runbooks

### Critical Incidents

1. **[Worker Crash](./WORKER_CRASH.md)** - Worker process failures, 503 errors, socket issues
2. **[Determinism Violation](./DETERMINISM_VIOLATION.md)** - Hash mismatches, replay failures, audit violations

### Performance Issues

3. **[Inference Latency Spike](./INFERENCE_LATENCY_SPIKE.md)** - P99 latency > 500ms, slow responses
4. **[Memory Pressure](./MEMORY_PRESSURE.md)** - High memory usage, adapter eviction failures

### Resource Issues

5. **[Disk Full](./DISK_FULL.md)** - Database write failures, log rotation issues, WAL growth

---

## Quick Reference

### Severity Levels

| Level | Response Time | Examples |
|-------|--------------|----------|
| **SEV-1** | Immediate | Worker down, all inference failing, data corruption |
| **SEV-2** | < 15 min | High latency affecting users, memory pressure critical |
| **SEV-3** | < 1 hour | Single adapter failures, elevated error rates |
| **SEV-4** | < 4 hours | Performance degradation, non-critical warnings |

### Essential Commands

```bash
# Health check
curl -f http://localhost:8080/healthz && echo "✓ OK" || echo "✗ FAIL"

# System metrics
aosctl metrics show
aosctl metrics show --json

# Worker status
ps aux | grep aos-worker
lsof var/run/aos/*/worker.sock

# Memory usage
free -h 2>/dev/null || vm_stat

# Recent logs
tail -100 var/aos-cp.log | grep ERROR
tail -100 var/aos-worker.log | grep -i "panic\|fatal"

# Database status
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"
sqlite3 var/aos-cp.sqlite3 "SELECT COUNT(*) FROM adapters;"

# Disk space
df -h var/
du -sh var/logs var/aos-cp.sqlite3*
```

### Common Alerts

| Alert | Runbook | Severity |
|-------|---------|----------|
| `WorkerCrashed` | [WORKER_CRASH.md](./WORKER_CRASH.md) | SEV-1 |
| `DeterminismViolation` | [DETERMINISM_VIOLATION.md](./DETERMINISM_VIOLATION.md) | SEV-1 |
| `InferenceLatencyHigh` | [INFERENCE_LATENCY_SPIKE.md](./INFERENCE_LATENCY_SPIKE.md) | SEV-2 |
| `MemoryPressureCritical` | [MEMORY_PRESSURE.md](./MEMORY_PRESSURE.md) | SEV-2 |
| `DiskUsageCritical` | [DISK_FULL.md](./DISK_FULL.md) | SEV-2 |

---

## Incident Response Process

### 1. Acknowledge Alert
- Acknowledge in PagerDuty/AlertManager
- Join incident Slack channel
- Update incident status page

### 2. Triage
- Determine severity level
- Identify affected runbook
- Gather initial diagnostics

### 3. Investigate
- Follow runbook diagnosis steps
- Document findings in incident ticket
- Collect logs and metrics

### 4. Mitigate
- Apply quick fix from runbook
- Verify service restoration
- Monitor for stability (15+ minutes)

### 5. Resolve
- Implement root cause fix
- Update monitoring/alerts if needed
- Schedule follow-up review

### 6. Document
- Complete incident postmortem
- Update runbook if gaps found
- Share learnings with team

---

## Escalation Paths

### On-Call Engineer
- First responder for all alerts
- Follows runbooks for standard incidents
- Escalates SEV-1/SEV-2 to senior engineer

### Senior Engineer
- Consulted for SEV-1/SEV-2 incidents
- Approves production changes
- Coordinates cross-team response

### Engineering Manager
- Notified for SEV-1 incidents
- Coordinates customer communication
- Approves emergency rollbacks

### Security Team
- Paged for determinism violations
- Reviews policy enforcement failures
- Approves quarantine actions

---

## Before You Start

### Prerequisites
- Access to production environment
- `aosctl` CLI installed and configured
- Database credentials in environment
- PagerDuty/Slack access

### Environment Setup
```bash
# Load environment
cd /path/to/adapter-os
source .env
source .env.local

# Verify access
aosctl config show
sqlite3 var/aos-cp.sqlite3 "SELECT 1;"

# Check current status
curl http://localhost:8080/healthz
aosctl metrics show
```

### Safety Checklist
- [ ] Verify you're in correct environment (staging vs production)
- [ ] Review recent changes in deployment log
- [ ] Check if backup is recent (< 24 hours)
- [ ] Notify team before destructive operations
- [ ] Document all commands executed

---

## Troubleshooting Tips

### Log Analysis
```bash
# Errors in last hour
grep ERROR var/aos-cp.log | tail -100

# Worker crashes
grep -i "panic\|fatal\|crashed" var/aos-worker.log

# Memory warnings
grep -i "memory\|eviction\|headroom" var/aos-cp.log

# Determinism violations
grep -i "determinism\|hash.*mismatch\|replay.*fail" var/aos-cp.log
```

### Metrics Queries
```bash
# Current system state
aosctl metrics show --json | jq '{
  memory: .memory.used_percent,
  adapters: .adapters.loaded_count,
  latency_p99: .inference.p99_latency_ms
}'

# History (last 4 hours)
aosctl metrics history --hours 4 --limit 100

# Violations
aosctl metrics violations --unresolved
```

### Database Queries
```bash
# Active adapters by tenant
sqlite3 var/aos-cp.sqlite3 "
SELECT tenant_id, COUNT(*) as adapter_count,
       SUM(CASE WHEN status='Loaded' THEN 1 ELSE 0 END) as loaded_count
FROM adapters
GROUP BY tenant_id;"

# Recent failures
sqlite3 var/aos-cp.sqlite3 "
SELECT created_at, error_type, COUNT(*) as count
FROM telemetry_events
WHERE event_type='error'
  AND created_at > datetime('now', '-1 hour')
GROUP BY error_type
ORDER BY count DESC
LIMIT 10;"
```

---

## Additional Resources

- [OPERATIONS.md](../OPERATIONS.md) - Full operations guide
- [TROUBLESHOOTING.md](../TROUBLESHOOTING.md) - Development troubleshooting
- [DEPLOYMENT.md](../DEPLOYMENT.md) - Production deployment
- [POLICIES.md](../POLICIES.md) - Policy enforcement
- [CLAUDE.md](../../CLAUDE.md) - System architecture and invariants

---

## Runbook Maintenance

**Owners:** SRE Team
**Review Cadence:** Monthly
**Update Triggers:**
- After each SEV-1 or SEV-2 incident
- When new monitoring/alerts are added
- After major version upgrades

**Contributing:**
1. Test changes in staging environment
2. Get peer review from SRE team
3. Update version number and date
4. Announce changes in #sre-announcements

---

**MLNavigator Inc © 2025-12-15**
