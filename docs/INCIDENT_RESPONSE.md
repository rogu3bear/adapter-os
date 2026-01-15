# Incident Response Plan

**Document Version:** 1.0
**Last Updated:** 2026-01-14
**Status:** Draft
**Maintained by:** adapterOS Security Team

---

## Table of Contents

1. [Overview](#overview)
2. [Incident Classification](#incident-classification)
3. [Response Procedures](#response-procedures)
4. [Evidence Preservation](#evidence-preservation)
5. [Communication Templates](#communication-templates)
6. [Post-Incident Review](#post-incident-review)
7. [Contact Matrix](#contact-matrix)

---

## Overview

This document defines procedures for responding to security incidents in adapterOS deployments. It covers detection, containment, eradication, recovery, and lessons learned.

### Scope

This IRP applies to:
- adapterOS control plane servers
- Worker nodes and inference engines
- Database systems (SQLite, KV stores)
- Network infrastructure
- User data and tenant information

### Objectives

1. Minimize impact of security incidents
2. Preserve evidence for investigation
3. Restore normal operations quickly
4. Prevent recurrence
5. Meet compliance notification requirements

---

## Incident Classification

### Severity Levels

| Level | Name | Description | Response Time |
|-------|------|-------------|---------------|
| SEV-1 | Critical | Data breach, system compromise, service down | 15 minutes |
| SEV-2 | High | Active attack, significant degradation | 1 hour |
| SEV-3 | Medium | Suspicious activity, minor impact | 4 hours |
| SEV-4 | Low | Policy violation, no immediate impact | 24 hours |

### Incident Categories

| Category | Examples |
|----------|----------|
| **Data Breach** | Unauthorized data access, exfiltration |
| **System Compromise** | Malware, unauthorized access, privilege escalation |
| **Service Disruption** | DoS, resource exhaustion, outage |
| **Policy Violation** | Unauthorized configuration change, access abuse |
| **Crypto Failure** | Key compromise, signature failure, determinism violation |

---

## Response Procedures

### Phase 1: Detection & Triage (0-15 min)

1. **Identify the incident**
   ```bash
   # Check recent audit logs
   ./aosctl diag --system

   # Review security events
   sqlite3 var/db/adapteros.sqlite3 \
     "SELECT * FROM audit_logs WHERE created_at > datetime('now', '-1 hour') ORDER BY created_at DESC LIMIT 100"
   ```

2. **Classify severity** using the matrix above

3. **Assign Incident Commander (IC)**
   - SEV-1/SEV-2: Senior engineer or security lead
   - SEV-3/SEV-4: On-call engineer

4. **Create incident channel** (Slack/Teams/IRC)

### Phase 2: Containment (15-60 min)

**For System Compromise:**
```bash
# Isolate affected node
./aosctl node drain <node-id>

# Revoke all active sessions for affected tenant
./aosctl auth revoke-all --tenant <tenant-id>

# Block suspicious IP
./aosctl security ip-block <ip-address>
```

**For Data Breach:**
```bash
# Disable affected tenant
./aosctl tenant disable <tenant-id>

# Rotate affected keys
./aosctl crypto rotate-keys --force
```

**For Service Disruption:**
```bash
# Scale down affected services
./aosctl worker stop --all

# Enable maintenance mode
./aosctl server maintenance --enable
```

### Phase 3: Eradication (1-4 hours)

1. **Identify root cause**
   ```bash
   # Generate diagnostic bundle
   ./aosctl diag --bundle ./incident_$(date +%Y%m%d_%H%M%S).zip --full-db

   # Analyze telemetry
   ./aosctl verify telemetry --bundle-dir ./incident_bundle/telemetry
   ```

2. **Remove threat**
   - Patch vulnerability
   - Remove malware/backdoor
   - Reset compromised credentials

3. **Verify eradication**
   - Run security scan
   - Validate system integrity
   - Confirm no persistence mechanisms

### Phase 4: Recovery (4-24 hours)

1. **Restore services**
   ```bash
   # Verify system health
   ./aosctl doctor

   # Run preflight checks
   ./aosctl preflight

   # Re-enable services
   ./aosctl server maintenance --disable
   ```

2. **Monitor for recurrence**
   - Enhanced logging for 72 hours
   - Alert thresholds lowered
   - Manual review of suspicious activity

3. **Communicate status**
   - Update stakeholders
   - Notify affected users (if required)

### Phase 5: Lessons Learned (24-72 hours)

1. **Conduct post-incident review**
2. **Document timeline and actions**
3. **Identify improvements**
4. **Update runbooks and procedures**

---

## Evidence Preservation

### Critical: Preserve Before Modifying

Before any remediation, capture:

```bash
# 1. Full diagnostic bundle
./aosctl diag --bundle ./evidence_$(date +%Y%m%d_%H%M%S).zip --full-db

# 2. System state snapshot
./aosctl status --json > evidence_status.json

# 3. Process list
ps auxww > evidence_processes.txt

# 4. Network connections
netstat -an > evidence_network.txt
lsof -i > evidence_open_files.txt

# 5. Filesystem timeline (if available)
ls -laR /var/adapteros > evidence_filesystem.txt
```

### Evidence Chain of Custody

1. **Label evidence** with:
   - Incident ID
   - Collection timestamp
   - Collector name
   - Hash (SHA-256)

2. **Compute hashes immediately**
   ```bash
   shasum -a 256 evidence_*.* > evidence_hashes.txt
   ```

3. **Store securely**
   - Encrypted storage
   - Access logging enabled
   - Separate from production systems

4. **Document transfers**
   - Who, when, why for each access
   - Sign-off required

### Retention Requirements

| Evidence Type | Retention | Compliance Driver |
|---------------|-----------|-------------------|
| Audit logs | 7 years | SOC 2, HIPAA |
| Diagnostic bundles | 3 years | General |
| Incident reports | 7 years | SOC 2 |
| Communication logs | 3 years | General |

---

## Communication Templates

### Internal Escalation (SEV-1/SEV-2)

```
INCIDENT ALERT - [SEV-X] - [Brief Description]

Time Detected: [YYYY-MM-DD HH:MM UTC]
Incident Commander: [Name]
Status: [Investigating/Contained/Resolved]

Summary:
[1-2 sentence description of the incident]

Impact:
- Affected Systems: [list]
- Affected Tenants: [count or list]
- Service Status: [operational/degraded/down]

Current Actions:
- [Action 1]
- [Action 2]

Next Update: [Time]

Incident Channel: #incident-YYYYMMDD-XXX
```

### Customer Notification (Data Breach)

```
Security Notification - [Organization Name]

Dear [Customer],

We are writing to inform you of a security incident that may have affected your data.

What Happened:
[Brief, factual description]

What Information Was Involved:
[Types of data potentially affected]

What We Are Doing:
[Actions taken and planned]

What You Can Do:
[Recommended customer actions]

For More Information:
[Contact information]

We sincerely apologize for any inconvenience this may cause.

[Signature]
```

### Regulatory Notification (GDPR - 72 hours)

```
Data Breach Notification to Supervisory Authority

Organization: [Legal entity name]
DPO Contact: [Name, email, phone]
Notification Date: [Date]

Nature of Breach:
[Categories and approximate number of data subjects]
[Categories and approximate number of records]

Likely Consequences:
[Assessment of potential impact]

Measures Taken:
[Actions to address breach and mitigate effects]

[Authorized signature]
```

---

## Post-Incident Review

### Review Meeting Agenda

1. **Timeline reconstruction** (15 min)
   - Detection → Containment → Eradication → Recovery

2. **What went well** (10 min)
   - Effective responses
   - Tools/processes that helped

3. **What could improve** (15 min)
   - Gaps identified
   - Process breakdowns
   - Tool limitations

4. **Action items** (10 min)
   - Assign owners
   - Set deadlines
   - Schedule follow-ups

### Post-Incident Report Template

```markdown
# Incident Report: [INCIDENT-YYYY-XXX]

## Summary
- **Severity**: SEV-X
- **Duration**: [Start] to [End] ([X hours])
- **Impact**: [Brief description]
- **Root Cause**: [One sentence]

## Timeline
| Time (UTC) | Event |
|------------|-------|
| HH:MM | [Event description] |

## Root Cause Analysis
[Detailed explanation]

## Impact Assessment
- Users affected: [X]
- Data exposed: [Yes/No, details]
- Service downtime: [X minutes/hours]

## Response Effectiveness
- Detection time: [X minutes]
- Containment time: [X minutes]
- Resolution time: [X hours]

## Action Items
| Item | Owner | Due Date | Status |
|------|-------|----------|--------|
| [Action] | [Name] | [Date] | [Status] |

## Lessons Learned
1. [Lesson 1]
2. [Lesson 2]
```

---

## Contact Matrix

### Internal Escalation

| Role | Primary | Backup | Contact |
|------|---------|--------|---------|
| Incident Commander | [Name] | [Name] | [Phone/Slack] |
| Security Lead | [Name] | [Name] | [Phone/Slack] |
| Engineering Lead | [Name] | [Name] | [Phone/Slack] |
| Legal/Compliance | [Name] | [Name] | [Phone/Slack] |
| Communications | [Name] | [Name] | [Phone/Slack] |

### External Contacts

| Organization | Purpose | Contact |
|--------------|---------|---------|
| Legal Counsel | Breach notification | [Contact info] |
| Cyber Insurance | Claims | [Contact info] |
| Law Enforcement | Criminal activity | FBI IC3, local PD |
| PR/Communications | Public statements | [Contact info] |

### Regulatory Contacts (if applicable)

| Regulation | Authority | Notification Timeline |
|------------|-----------|----------------------|
| GDPR | Supervisory Authority | 72 hours |
| HIPAA | HHS OCR | 60 days |
| PCI DSS | Card brands | Varies |
| State breach laws | State AG | Varies by state |

---

## Appendix: Quick Reference

### SEV-1 Checklist

- [ ] Assign Incident Commander
- [ ] Create incident channel
- [ ] Capture evidence (before changes)
- [ ] Contain threat
- [ ] Notify leadership
- [ ] Begin root cause analysis
- [ ] Prepare customer communication (if needed)
- [ ] Document all actions with timestamps

### Key Commands

```bash
# Emergency diagnostics
./aosctl diag --bundle ./emergency.zip --full-db

# Check system health
./aosctl doctor

# View recent security events
./aosctl audit query --type security --since "1 hour ago"

# Revoke all sessions
./aosctl auth revoke-all

# Enable maintenance mode
./aosctl server maintenance --enable
```

---

*This document should be reviewed and updated quarterly, and after every SEV-1 or SEV-2 incident.*
