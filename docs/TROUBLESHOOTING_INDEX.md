# AdapterOS Troubleshooting Index

**Complete guide to troubleshooting resources and tools**

**Last Updated:** 2025-12-20
**Version:** 1.0

---

## Overview

This index provides a navigable guide to all troubleshooting resources in the AdapterOS documentation. Use this as your starting point when encountering issues.

---

## Quick Navigation by Problem Type

### Service Issues
- **Service won't start** → [Boot Troubleshooting](./BOOT_TROUBLESHOOTING.md)
- **Port conflicts** → [Troubleshooting Guide - Network Issues](./TROUBLESHOOTING.md#network-and-port-issues)
- **Service crashes** → [Worker Crash Runbook](./runbooks/WORKER_CRASH.md)

### Database Issues
- **Connection failures** → [Enhanced Troubleshooting - Database](./TROUBLESHOOTING_ENHANCED.md#database-issues)
- **Migration errors** → [Boot Troubleshooting - Migration Failures](./BOOT_TROUBLESHOOTING.md#migration-signature-invalid)
- **Performance problems** → [Tenant Query Performance](./runbooks/TENANT_QUERY_PERFORMANCE_INCIDENT.md)
- **Disk space** → [Disk Full Runbook](./runbooks/DISK_FULL.md)

### Worker Issues
- **Worker not responding** → [Enhanced Troubleshooting - Worker](./TROUBLESHOOTING_ENHANCED.md#worker-connection-problems)
- **Socket errors** → [Troubleshooting Guide - Runtime Issues](./TROUBLESHOOTING.md#runtime-issues)
- **Worker crashes** → [Worker Crash Runbook](./runbooks/WORKER_CRASH.md)

### Authentication Issues
- **JWT errors** → [Enhanced Troubleshooting - Auth](./TROUBLESHOOTING_ENHANCED.md#authentication-failures)
- **Tenant isolation** → [Authentication Guide](./AUTHENTICATION.md)
- **Dev bypass** → [Troubleshooting Guide - Common Issues](./TROUBLESHOOTING.md#dev-bypass-button-not-visible)

### Performance Issues
- **High latency** → [Inference Latency Spike Runbook](./runbooks/INFERENCE_LATENCY_SPIKE.md)
- **High memory** → [Memory Pressure Runbook](./runbooks/MEMORY_PRESSURE.md)
- **Slow database** → [DB Optimization Rollout](./runbooks/DB_OPTIMIZATION_ROLLOUT.md)

### Backend-Specific Issues
- **MLX problems** → [MLX Troubleshooting](./MLX_TROUBLESHOOTING.md)
- **Metal issues** → [Metal Backend Guide](./METAL_BACKEND.md)
- **CoreML problems** → [CoreML Backend Guide](./COREML_BACKEND.md)

### Determinism Issues
- **Hash mismatches** → [Determinism Violation Runbook](./runbooks/DETERMINISM_VIOLATION.md)
- **Replay failures** → [Determinism Guide](./DETERMINISM.md)

---

## Documentation Hierarchy

### Level 1: Quick Reference
**For:** Immediate problem resolution
**Documents:**
- [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) - Main troubleshooting guide with common issues
- [Quick Diagnostics Script](../scripts/diagnose.sh) - Automated health check

### Level 2: Detailed Guides
**For:** Comprehensive problem investigation
**Documents:**
- [TROUBLESHOOTING_ENHANCED.md](./TROUBLESHOOTING_ENHANCED.md) - Error catalog, decision trees, diagnostic commands
- [MLX_TROUBLESHOOTING.md](./MLX_TROUBLESHOOTING.md) - MLX backend specific issues
- [BOOT_TROUBLESHOOTING.md](./BOOT_TROUBLESHOOTING.md) - Boot sequence failures

### Level 3: Production Runbooks
**For:** Production incident response
**Documents:**
- [Runbooks README](./runbooks/README.md) - Runbook index
- [Worker Crash](./runbooks/WORKER_CRASH.md)
- [Determinism Violation](./runbooks/DETERMINISM_VIOLATION.md)
- [Inference Latency Spike](./runbooks/INFERENCE_LATENCY_SPIKE.md)
- [Memory Pressure](./runbooks/MEMORY_PRESSURE.md)
- [Disk Full](./runbooks/DISK_FULL.md)

### Level 4: Reference Documentation
**For:** Understanding system behavior
**Documents:**
- [ERRORS.md](./ERRORS.md) - Error handling patterns and types
- [ARCHITECTURE.md](./ARCHITECTURE.md) - System architecture
- [DATABASE.md](./DATABASE.md) - Database schema and operations
- [OPERATIONS.md](./OPERATIONS.md) - Operational procedures

---

## Troubleshooting Workflow

### Step 1: Quick Diagnosis

Run the automated diagnostic tool:
```bash
./scripts/diagnose.sh
```

This generates a comprehensive report covering:
- System health
- Service status
- Database integrity
- Worker connectivity
- Recent errors
- Recommendations

### Step 2: Identify Problem Category

Use the decision tree from [TROUBLESHOOTING_ENHANCED.md](./TROUBLESHOOTING_ENHANCED.md#master-diagnostic-decision-tree):

1. **Service not responding?** → Check health endpoints
2. **Requests failing?** → Check authentication and resources
3. **Performance degraded?** → Check latency, memory, CPU
4. **Data issues?** → Check database integrity

### Step 3: Consult Specific Guide

Based on your problem category, consult the appropriate guide:

#### For Development Issues
- [TROUBLESHOOTING.md](./TROUBLESHOOTING.md)
- [TROUBLESHOOTING_ENHANCED.md](./TROUBLESHOOTING_ENHANCED.md)
- [MLX_TROUBLESHOOTING.md](./MLX_TROUBLESHOOTING.md) (if using MLX)

#### For Production Issues
- [Runbooks](./runbooks/) directory
- [OPERATIONS.md](./OPERATIONS.md)

### Step 4: Execute Diagnostic Commands

Each guide provides specific commands. Common starting points:

#### Health Check
```bash
curl -f http://localhost:8080/healthz
curl -f http://localhost:8080/readyz
```

#### System Metrics
```bash
curl -s http://localhost:8080/api/v1/metrics/system | jq .
```

#### Recent Errors
```bash
tail -50 var/aos-cp.log | grep ERROR
```

#### Database Integrity
```bash
sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"
```

### Step 5: Apply Solution

Follow the solution steps from the relevant guide. Always:
1. Verify you're in the correct environment
2. Back up data if performing destructive operations
3. Document commands executed
4. Verify the fix worked

### Step 6: Prevent Recurrence

After resolving the issue:
1. Check if monitoring should be added
2. Update documentation if gaps found
3. Share learnings with team
4. Create postmortem for production issues

---

## Tools and Scripts

### Diagnostic Tools

| Tool | Purpose | Documentation |
|------|---------|---------------|
| `./scripts/diagnose.sh` | Comprehensive health check | [Script source](../scripts/diagnose.sh) |
| `./aosctl diag` | Quick diagnostics via CLI | [CLI Guide](./CLI_GUIDE.md) |
| `./scripts/service-manager.sh` | Service management | [Operations Guide](./OPERATIONS.md) |

### Monitoring Commands

| Command | Purpose |
|---------|---------|
| `curl http://localhost:8080/healthz` | Basic health check |
| `curl http://localhost:8080/readyz` | Readiness check |
| `curl http://localhost:8080/api/v1/metrics/system` | System metrics |
| `./aosctl metrics show` | Formatted metrics display |
| `./aosctl status` | Overall system status |

### Database Tools

| Command | Purpose |
|---------|---------|
| `sqlite3 var/aos-cp.sqlite3 "PRAGMA integrity_check;"` | Integrity check |
| `sqlite3 var/aos-cp.sqlite3 "PRAGMA wal_checkpoint(TRUNCATE);"` | WAL checkpoint |
| `sqlite3 var/aos-cp.sqlite3 "VACUUM;"` | Reclaim space |
| `sqlite3 var/aos-cp.sqlite3 "ANALYZE;"` | Update statistics |

### Log Analysis

| Command | Purpose |
|---------|---------|
| `tail -100 var/aos-cp.log \| grep ERROR` | Recent errors |
| `grep -i "memory\|eviction" var/aos-cp.log` | Memory issues |
| `grep -i "determinism\|hash.*mismatch" var/aos-cp.log` | Determinism violations |
| `grep -i "panic\|fatal" var/logs/worker.log` | Worker crashes |

---

## Common Error Messages Reference

### Quick Lookup Table

| Error Message | Document | Section |
|--------------|----------|---------|
| "Database connection failed" | [Enhanced](./TROUBLESHOOTING_ENHANCED.md) | Database Errors |
| "Migration signature verification failed" | [Enhanced](./TROUBLESHOOTING_ENHANCED.md) | Database Errors |
| "Worker socket not found" | [Enhanced](./TROUBLESHOOTING_ENHANCED.md) | Worker Errors |
| "JWT signature verification failed" | [Enhanced](./TROUBLESHOOTING_ENHANCED.md) | Auth Errors |
| "Adapter not loaded" | [Enhanced](./TROUBLESHOOTING_ENHANCED.md) | Adapter Errors |
| "Resource exhaustion" | [Memory Pressure Runbook](./runbooks/MEMORY_PRESSURE.md) | - |
| "Inference timeout" | [Latency Spike Runbook](./runbooks/INFERENCE_LATENCY_SPIKE.md) | - |
| "MLX stub implementation active" | [MLX Troubleshooting](./MLX_TROUBLESHOOTING.md) | Backend Reports Stub Mode |
| "Metal kernels not found" | [Main Troubleshooting](./TROUBLESHOOTING.md) | Missing Metal Shaders |
| "Port already in use" | [Boot Troubleshooting](./BOOT_TROUBLESHOOTING.md) | Port Already in Use |

---

## Decision Trees Index

### Available Decision Trees

1. **Master Diagnostic Decision Tree** - [TROUBLESHOOTING_ENHANCED.md](./TROUBLESHOOTING_ENHANCED.md#master-diagnostic-decision-tree)
   - Overall problem triage
   - Routes to specific problem areas

2. **Database Decision Tree** - [TROUBLESHOOTING_ENHANCED.md](./TROUBLESHOOTING_ENHANCED.md#decision-tree-database-problems)
   - Connection issues
   - Migration problems
   - Performance issues
   - Corruption

3. **Worker Decision Tree** - [TROUBLESHOOTING_ENHANCED.md](./TROUBLESHOOTING_ENHANCED.md#decision-tree-worker-issues)
   - Process status
   - Socket problems
   - Connection issues
   - Timeouts

4. **Auth Decision Tree** - [TROUBLESHOOTING_ENHANCED.md](./TROUBLESHOOTING_ENHANCED.md#decision-tree-auth-problems)
   - JWT issues
   - Permissions
   - Tenant isolation

5. **Performance Decision Tree** - [TROUBLESHOOTING_ENHANCED.md](./TROUBLESHOOTING_ENHANCED.md#decision-tree-performance-problems)
   - Latency
   - Memory
   - CPU
   - Disk I/O

6. **Boot Diagnosis Flowchart** - [BOOT_TROUBLESHOOTING.md](./BOOT_TROUBLESHOOTING.md#quick-diagnosis-flowchart)
   - Startup failures
   - Early boot failures
   - Boot phase failures

7. **System Health Flowchart** - [TROUBLESHOOTING.md](./TROUBLESHOOTING.md#diagnostic-flowchart)
   - Service running
   - Health endpoints
   - Backend issues

---

## Production Runbook Quick Reference

### Severity-Based Response

#### SEV-1: Critical (Immediate Response)
- [Worker Crash](./runbooks/WORKER_CRASH.md) - Worker down, all inference failing
- [Determinism Violation](./runbooks/DETERMINISM_VIOLATION.md) - Security/audit breach

#### SEV-2: High (< 15 minutes)
- [Inference Latency Spike](./runbooks/INFERENCE_LATENCY_SPIKE.md) - P99 > 500ms
- [Memory Pressure](./runbooks/MEMORY_PRESSURE.md) - Memory > 85%
- [Disk Full](./runbooks/DISK_FULL.md) - Disk > 90%

#### SEV-3: Medium (< 1 hour)
- Single adapter failures
- Elevated error rates
- Non-critical warnings

### Runbook Structure

Each runbook follows this structure:
1. **Symptoms** - Observable indicators
2. **Diagnosis** - Investigation steps
3. **Resolution** - Immediate fixes
4. **Prevention** - Root cause fixes
5. **Escalation** - When to involve others

---

## Learning Path

### New Users
1. Start with [QUICKSTART.md](./QUICKSTART.md)
2. Read [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) overview
3. Familiarize with [diagnostic script](../scripts/diagnose.sh)
4. Practice with [decision trees](./TROUBLESHOOTING_ENHANCED.md#decision-trees)

### Developers
1. Review [TROUBLESHOOTING.md](./TROUBLESHOOTING.md)
2. Study [TROUBLESHOOTING_ENHANCED.md](./TROUBLESHOOTING_ENHANCED.md) error catalog
3. Understand [ERRORS.md](./ERRORS.md) error handling patterns
4. Read [BOOT_TROUBLESHOOTING.md](./BOOT_TROUBLESHOOTING.md) for startup issues

### SREs/Operators
1. Master [Production Runbooks](./runbooks/)
2. Study [OPERATIONS.md](./OPERATIONS.md)
3. Review [DEPLOYMENT.md](./DEPLOYMENT.md)
4. Understand [ARCHITECTURE.md](./ARCHITECTURE.md)

### Backend Developers
1. Read backend-specific guides:
   - [MLX_TROUBLESHOOTING.md](./MLX_TROUBLESHOOTING.md)
   - [METAL_BACKEND.md](./METAL_BACKEND.md)
   - [COREML_BACKEND.md](./COREML_BACKEND.md)
2. Review [DETERMINISM.md](./DETERMINISM.md)
3. Study [MLX_GUIDE.md](./MLX_GUIDE.md)

---

## Contributing to Troubleshooting Docs

### When to Update

Update documentation when:
1. You encounter a new error not documented
2. You find a better solution to an existing problem
3. A documented solution no longer works
4. You receive feedback from users

### How to Update

1. Identify the appropriate document
2. Follow the existing structure and format
3. Add specific error messages and codes
4. Include diagnostic commands
5. Provide clear, actionable solutions
6. Test your changes
7. Update this index if adding new content

### Documentation Standards

- **Error Messages**: Include exact error text
- **Commands**: Provide copy-paste ready commands
- **Solutions**: List steps in order with verification
- **Examples**: Show expected output
- **Cross-references**: Link to related docs

---

## Getting Help

### Self-Service Resources

1. Run diagnostics: `./scripts/diagnose.sh`
2. Search this index for your problem
3. Follow the decision tree
4. Check error message catalog
5. Review recent logs

### Escalation Path

If self-service doesn't resolve the issue:

#### Development Environment
1. Check GitHub Issues
2. Post in team Slack
3. Contact maintainers

#### Production Environment
1. Follow severity-based response
2. Use appropriate runbook
3. Escalate per [Runbooks README](./runbooks/README.md)
4. Create incident postmortem

---

## Frequently Asked Questions

### Q: Where do I start when something goes wrong?

Run `./scripts/diagnose.sh` first. It will identify most common issues and provide recommendations.

### Q: How do I know which guide to use?

Use the [Quick Navigation by Problem Type](#quick-navigation-by-problem-type) section above.

### Q: What's the difference between troubleshooting guides and runbooks?

- **Troubleshooting Guides**: For development and general issues
- **Runbooks**: For production incidents with time-sensitive response

### Q: How do I report a new error not in the documentation?

1. Capture the full error message and context
2. Run the diagnostic script
3. Create a GitHub issue with the report
4. Update documentation once resolved

### Q: Can I use these troubleshooting docs for production?

Yes. The [runbooks](./runbooks/) directory is specifically designed for production use.

---

## Document Maintenance

**Owners:** AdapterOS Support Team
**Review Cadence:** Monthly
**Last Review:** 2025-12-20

**Update Triggers:**
- New error patterns discovered
- System architecture changes
- After major incidents
- User feedback

---

## Related Documentation

- [AGENTS.md](../AGENTS.md) - System architecture and invariants
- [README.md](./README.md) - Documentation index
- [OPERATIONS.md](./OPERATIONS.md) - Operations guide
- [DEPLOYMENT.md](./DEPLOYMENT.md) - Deployment procedures
- [TESTING.md](./TESTING.md) - Testing guide

---

**MLNavigator Inc © 2025**
