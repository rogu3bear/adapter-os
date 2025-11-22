# Escalation Guide

When to escalate, severity levels, and escalation procedures.

## Overview

This guide defines when to escalate issues beyond operational runbooks to engineering teams, severity classification, and escalation procedures.

## Escalation Decision Tree

```
Issue Occurs
    │
    ├─→ Can be resolved by runbook? → YES → Follow runbook
    │                                       │
    ├─→ NO ────────────────────────────────┘
    │
    ├─→ Is service degraded/down? → YES → SEV1 (Critical)
    │                                      Escalate immediately
    │
    ├─→ Is data at risk? → YES → SEV2 (High)
    │                             Escalate within 15 minutes
    │
    ├─→ Is workaround available? → NO → SEV2 (High)
    │                                    Escalate within 15 minutes
    │
    ├─→ YES → SEV3 (Medium)
              Document and escalate within 4 hours
```

## Severity Levels

### SEV1 - Critical (P1)

**Definition:** Complete service outage or critical functionality unavailable

**Examples:**
- Control plane server won't start
- All inference requests failing
- Database corruption detected
- Complete memory exhaustion
- Security breach detected
- Data loss occurring

**Response Time:**
- Acknowledge: Immediate (< 5 minutes)
- Initial response: < 15 minutes
- Status updates: Every 30 minutes
- Resolution target: < 4 hours

**Escalation Process:**
1. Gather diagnostic bundle immediately
2. Notify on-call engineer via pager/phone
3. Create incident ticket (priority: Critical)
4. Begin incident response procedure
5. Notify stakeholders

**Do NOT wait for SEV1:**
- Multiple runbook procedures fail
- Service completely unavailable
- Active data corruption
- Security incident

### SEV2 - High (P2)

**Definition:** Major degradation or critical feature unavailable, no workaround

**Examples:**
- Memory pressure critical, cannot be resolved
- Database migration failures
- Adapter loading completely broken
- Health checks persistently unhealthy
- Multiple component failures
- Determinism violations
- Policy violations causing quarantine

**Response Time:**
- Acknowledge: < 15 minutes
- Initial response: < 30 minutes
- Status updates: Every hour
- Resolution target: < 24 hours

**Escalation Process:**
1. Document issue with diagnostic data
2. Create incident ticket (priority: High)
3. Notify on-call engineer via ticket system
4. Provide attempted resolution steps
5. Share diagnostic bundle

**Escalate to SEV2 if:**
- SEV3 issue not resolved within 8 hours
- Workaround stops working
- Issue spreading to other components
- Data integrity at risk

### SEV3 - Medium (P3)

**Definition:** Degraded functionality with workaround available

**Examples:**
- Single component degraded (with workaround)
- Performance degradation (< 50% impact)
- Non-critical adapter failures
- Elevated memory pressure (manageable)
- Configuration issues (workaround available)
- Router low confidence (backup adapters work)

**Response Time:**
- Acknowledge: < 4 hours (business hours)
- Initial response: < 8 hours
- Status updates: Daily
- Resolution target: < 1 week

**Escalation Process:**
1. Document issue thoroughly
2. Create ticket (priority: Medium)
3. Provide reproduction steps
4. Document workaround
5. Attach logs and diagnostic data

### SEV4 - Low (P4)

**Definition:** Minor issue, cosmetic, or feature request

**Examples:**
- Log message formatting
- CLI output formatting
- Documentation errors
- Performance optimization opportunities
- Feature enhancement requests

**Response Time:**
- Acknowledge: Best effort
- Resolution target: Next release

**Process:**
1. Create ticket
2. Provide description and context
3. No immediate escalation needed

## Escalation Triggers by Component

### Database
**SEV1:**
- Database corruption (integrity_check fails)
- Cannot start due to database errors
- Data loss detected

**SEV2:**
- Migration failures (cannot resolve)
- Persistent locked database
- Schema version mismatch (cannot migrate)
- Query latency > 1 second consistently

**SEV3:**
- Pool saturation > 80% consistently
- WAL file growing despite checkpoints
- Query latency > 200ms

### Memory/System Resources
**SEV1:**
- System unresponsive due to OOM
- Cannot free memory despite all procedures
- Memory leak causing repeated OOM

**SEV2:**
- Critical memory pressure persists > 30 minutes
- Adapter eviction thrashing
- Memory leak identified but cannot resolve

**SEV3:**
- Elevated memory pressure (manageable)
- Occasional eviction thrashing
- High memory usage with unknown cause

### Router/Inference
**SEV1:**
- No adapters can be loaded
- All inference requests failing
- Router completely non-functional

**SEV2:**
- > 50% of inference requests failing
- Queue depth > 200 persistently
- Router latency > 10 seconds

**SEV3:**
- Occasional inference failures
- Queue depth 100-200
- Router latency > 1 second

### Adapter Lifecycle
**SEV1:**
- Adapter loading completely broken
- Critical adapters cannot be loaded
- Adapter corruption detected

**SEV2:**
- > 5 adapters stuck in loading
- Adapter state machine broken
- Lifecycle transitions failing

**SEV3:**
- 1-3 adapters stuck in loading
- Occasional lifecycle transition failures
- Slow adapter loads (> 5s)

### Security
**SEV1:**
- Security breach detected
- Unauthorized access confirmed
- Quarantine activated unexpectedly
- Policy violation with no resolution

**SEV2:**
- PF/firewall check failing
- Environment drift blocking startup
- Audit trail gaps detected
- Signature verification failing

**SEV3:**
- Environment drift detected (non-critical)
- Policy warnings
- Audit anomalies

## Diagnostic Bundle Collection

Before escalating SEV1 or SEV2, collect diagnostic bundle:

```bash
#!/bin/bash
# collect-diagnostic-bundle.sh

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BUNDLE="diagnostic-bundle-$TIMESTAMP.tar.gz"
BUNDLE_DIR="diagnostic-bundle-$TIMESTAMP"

mkdir -p "$BUNDLE_DIR"

echo "Collecting diagnostic data..."

# System information
echo "System info..." >> "$BUNDLE_DIR/collection.log"
uname -a > "$BUNDLE_DIR/system-info.txt"
sw_vers >> "$BUNDLE_DIR/system-info.txt" 2>/dev/null || true
system_profiler SPHardwareDataType >> "$BUNDLE_DIR/system-info.txt" 2>/dev/null || true

# Configuration
echo "Configuration..." >> "$BUNDLE_DIR/collection.log"
cp configs/cp.toml "$BUNDLE_DIR/" 2>/dev/null || true

# Logs
echo "Logs..." >> "$BUNDLE_DIR/collection.log"
cp var/aos-cp.log "$BUNDLE_DIR/" 2>/dev/null || true
tail -1000 var/aos-cp.log > "$BUNDLE_DIR/aos-cp-tail-1000.log" 2>/dev/null || true

# Database state
echo "Database state..." >> "$BUNDLE_DIR/collection.log"
sqlite3 var/aos-cp.sqlite3 <<EOF > "$BUNDLE_DIR/db-state.txt" 2>&1
.mode column
.headers on
SELECT name, sql FROM sqlite_master WHERE type='table';
SELECT COUNT(*) as adapter_count FROM adapters;
SELECT current_state, COUNT(*) FROM adapters GROUP BY current_state;
SELECT version, description FROM _sqlx_migrations ORDER BY version DESC LIMIT 5;
PRAGMA integrity_check;
EOF

# Health checks
echo "Health checks..." >> "$BUNDLE_DIR/collection.log"
curl -s http://localhost:8080/healthz/all > "$BUNDLE_DIR/health-all.json" 2>&1 || true

# Process information
echo "Process info..." >> "$BUNDLE_DIR/collection.log"
ps aux | grep aos > "$BUNDLE_DIR/processes.txt"
lsof -i :8080 > "$BUNDLE_DIR/port-8080.txt" 2>&1 || true
lsof var/aos-cp.sqlite3 > "$BUNDLE_DIR/db-locks.txt" 2>&1 || true

# Memory information
echo "Memory info..." >> "$BUNDLE_DIR/collection.log"
vm_stat > "$BUNDLE_DIR/vm-stat.txt" 2>&1 || true
top -l 1 -n 10 -o mem > "$BUNDLE_DIR/top-memory.txt" 2>&1 || true

# Disk information
echo "Disk info..." >> "$BUNDLE_DIR/collection.log"
df -h > "$BUNDLE_DIR/disk-usage.txt"
du -sh var/* > "$BUNDLE_DIR/var-sizes.txt" 2>/dev/null || true

# Adapter information
echo "Adapter info..." >> "$BUNDLE_DIR/collection.log"
aosctl adapter list --json > "$BUNDLE_DIR/adapters.json" 2>&1 || true

# Telemetry sample
echo "Telemetry..." >> "$BUNDLE_DIR/collection.log"
tail -100 var/telemetry/bundle_latest.ndjson > "$BUNDLE_DIR/telemetry-sample.ndjson" 2>&1 || true

# Environment
echo "Environment..." >> "$BUNDLE_DIR/collection.log"
env | grep -E "RUST_|AOS_|DATABASE_" > "$BUNDLE_DIR/environment.txt" 2>&1 || true

# Compress bundle
echo "Creating archive..."
tar czf "$BUNDLE" "$BUNDLE_DIR"
rm -rf "$BUNDLE_DIR"

echo "Diagnostic bundle created: $BUNDLE"
echo "Size: $(ls -lh $BUNDLE | awk '{print $5}')"
```

**Usage:**
```bash
./scripts/collect-diagnostic-bundle.sh
```

## Escalation Checklist

Before escalating, ensure you have:

### SEV1 Escalation Checklist
- [ ] Verified issue is actually SEV1 (service down/critical)
- [ ] Attempted immediate mitigation (restart, rollback, etc.)
- [ ] Collected diagnostic bundle
- [ ] Documented timeline of events
- [ ] Noted all attempted resolution steps
- [ ] Identified impact scope (users, systems, data)
- [ ] Prepared for live troubleshooting session
- [ ] Notified stakeholders of outage

### SEV2 Escalation Checklist
- [ ] Attempted all relevant runbook procedures
- [ ] Documented failure of procedures
- [ ] Collected diagnostic bundle
- [ ] Identified workaround if available
- [ ] Estimated impact and urgency
- [ ] Prepared reproduction steps (if applicable)
- [ ] Gathered relevant logs and metrics
- [ ] Documented timeline

### SEV3 Escalation Checklist
- [ ] Thoroughly documented issue
- [ ] Attempted standard troubleshooting
- [ ] Identified workaround
- [ ] Collected relevant logs
- [ ] Prepared clear reproduction steps
- [ ] Noted frequency/pattern of issue
- [ ] Estimated business impact

## Escalation Contact Information

**During Business Hours:**
- Engineering ticket system: [Ticket URL]
- Engineering Slack channel: #aos-ops
- Email: eng@example.com

**After Hours (SEV1 only):**
- On-call pager: [Pager number/system]
- Emergency hotline: [Phone number]
- Backup contact: [Backup engineer contact]

**Escalation Path:**
1. On-call engineer (initial response)
2. Engineering manager (if no response in 30 min)
3. CTO/Director (for prolonged SEV1)

## Communication Templates

### SEV1 Initial Notification
```
SUBJECT: [SEV1] AdapterOS Critical Issue - [Brief Description]

IMPACT: [Describe impact - e.g., "All inference requests failing"]
START TIME: [When issue started]
CURRENT STATUS: [What's happening now]

ATTEMPTED MITIGATIONS:
- [List what you tried]
- [Include runbook references]

DIAGNOSTIC BUNDLE: [Path or attachment]

REQUESTING: Immediate engineering assistance

NEXT UPDATE: 30 minutes
```

### SEV2 Ticket Template
```
TITLE: [SEV2] [Component] - [Issue Summary]

SEVERITY: High (SEV2)

IMPACT:
- [Describe functional impact]
- [Workaround if available]

SYMPTOMS:
- [What you observed]
- [Relevant metrics/logs]

ATTEMPTED RESOLUTION:
- [Runbooks followed]
- [Commands executed]
- [Results observed]

DIAGNOSTIC DATA:
- Logs: [Attach or reference]
- Health checks: [Include output]
- Metrics: [Include relevant metrics]
- Diagnostic bundle: [Attach]

REPRODUCTION STEPS:
1. [Step 1]
2. [Step 2]
3. [Observe issue]

ADDITIONAL CONTEXT:
- [When it started]
- [What changed recently]
- [Frequency/pattern]
```

## Post-Escalation

### After Issue Resolution

1. **Document resolution:**
   - Root cause
   - Resolution steps
   - Time to resolution
   - Impact duration

2. **Update runbooks:**
   - Add new procedures if identified
   - Update existing procedures
   - Note what worked/didn't work

3. **Post-mortem (SEV1/SEV2):**
   - Timeline reconstruction
   - Root cause analysis
   - Action items for prevention
   - Process improvements

4. **Knowledge base:**
   - Create KB article for issue
   - Share learnings with team
   - Update training materials

### Prevent Future Escalations

**Add monitoring for:**
- Issues that required escalation
- Thresholds that predicted the issue
- Metrics that would have alerted earlier

**Update runbooks:**
- New resolution procedures
- Better diagnostic steps
- Clearer escalation triggers

**Improve automation:**
- Automate manual steps that worked
- Add health checks for gap areas
- Implement preventive measures

## Escalation Anti-Patterns

**DON'T:**
- Escalate without trying runbooks
- Escalate without diagnostic data
- Use wrong severity level to get faster response
- Escalate during business hours for non-urgent issues
- Skip documentation "to save time"

**DO:**
- Follow runbooks first
- Collect data before escalating
- Use appropriate severity
- Provide clear, concise information
- Document everything

## Related Runbooks

- [Startup Failures](./STARTUP-FAILURES.md)
- [Database Failures](./DATABASE-FAILURES.md)
- [Memory Pressure](./MEMORY-PRESSURE.md)
- [Health Check Failures](./HEALTH-CHECK-FAILURES.md)
- All other runbooks

## Feedback

These escalation procedures improve through feedback:
- What issues should have been escalated sooner?
- What information was missing?
- What diagnostic data would have helped?
- How can we prevent similar escalations?

Submit feedback to: [Feedback channel/email]
