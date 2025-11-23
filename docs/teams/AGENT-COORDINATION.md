# Agent Coordination Guide

**Version:** 1.0
**Date:** 2025-01-23
**Related:** [TEAM-CHARTERS.md](TEAM-CHARTERS.md), [PRD](../PRD-COMPLETION-V03-ALPHA.md)

---

## Overview

This document defines inter-team communication protocols, dependency management, and coordination mechanisms for AdapterOS v0.3-alpha completion.

**Teams:** 6 core + 1 optional (15-19 engineers)
**Duration:** 12 weeks across 4 phases

---

## Daily Standups

### Critical Path Teams (Weeks 1-7)

**Participants:** Teams 1 & 2 (Backend + Inference)
**Time:** 9:00 AM daily (15 minutes)
**Format:** Async-first (Slack thread), sync if blockers

**Template:**
```
Yesterday: Completed C1 (CoreML FFI bridge - 80%)
Today: Finish C1, start C2 (MLX backend)
Blockers: None
Help Needed: None
```

**Escalation:** If blocked >24 hours → tag @engineering-lead

---

### Full Team Standups (Weeks 8-12)

**Participants:** All teams
**Time:** 9:30 AM Monday/Wednesday/Friday (20 minutes)
**Format:** Rotating presenter (1 team per day)

**Monday:** Sprint planning, blocker triage
**Wednesday:** Mid-week progress check, demos
**Friday:** Week wrap-up, retrospective

---

## Weekly Syncs

### All-Hands Sync

**Participants:** All teams + stakeholders
**Time:** Tuesday 2:00 PM (60 minutes)
**Agenda:**
1. Phase progress update (10 min)
2. Team demos (30 min - 3 teams × 10 min each)
3. Blocker resolution (15 min)
4. Next week planning (5 min)

**Rotating Demo Schedule:**
- Week 1-3 (Phase 1): Teams 1, 6, 7
- Week 4-7 (Phase 2): Teams 1, 2, 3
- Week 8-10 (Phase 3): Teams 3, 5, 6
- Week 11-12 (Phase 4): All teams (bug fixes)

---

### Bi-Weekly Stakeholder Demo

**Participants:** Engineering leads + product owner + stakeholders
**Time:** Every other Friday 3:00 PM (60 minutes)
**Schedule:** Weeks 2, 4, 6, 8, 10, 12

**Format:**
1. Phase completion review (15 min)
2. Live demo (30 min)
3. Metrics review (10 min)
4. Q&A (5 min)

**Deliverables:**
- Demo video (recorded)
- Sprint report (GitHub project snapshot)
- Metrics dashboard (test coverage, performance)

---

## Dependency Matrix

### Team Dependencies

```
Team 1 (Backend) ──────> Team 2 (Inference) ──────> Team 3 (Training)
        │                       │                           │
        │                       │                           │
        ↓                       ↓                           ↓
    Team 6 (API) ←──────────────────────────────────> Team 5 (UI)
                                │
                                ↓
                           Team 4 (Security)
```

**Critical Path:** Team 1 → Team 2 → Team 3 (sequential)
**Parallel:** Teams 4, 5, 6, 7 (independent workstreams)

---

### Handoff Protocols

#### Week 3: Team 1 → Team 2
**Trigger:** Phase 1 Gate passed (backend functional)
**Handoff:**
- Backend integration guide (API docs)
- Determinism test suite (verify backend)
- Performance benchmarks (baseline metrics)

**Meeting:** 1-hour handoff session (architecture walkthrough)

---

#### Week 7: Team 2 → Team 3
**Trigger:** Phase 2 Gate passed (inference working)
**Handoff:**
- Inference pipeline integration guide
- Lifecycle manager API docs
- Training job scheduler skeleton

**Meeting:** 1-hour handoff session (integration points)

---

#### Week 8: Teams 2/3 → Team 5
**Trigger:** Phase 3 start (UI integration)
**Handoff:**
- API endpoint documentation (OpenAPI spec)
- SSE event schemas (real-time updates)
- Authentication guide (JWT, RBAC)

**Meeting:** 2-hour workshop (API integration patterns)

---

## Communication Channels

### Slack Channels

| Channel | Purpose | Participants |
|---------|---------|--------------|
| `#aos-v03-alpha` | General coordination, announcements | All teams |
| `#aos-critical-path` | Daily sync (Teams 1-3) | Teams 1, 2, 3 |
| `#aos-blockers` | Escalation (response <4 hours) | All teams, leads |
| `#aos-demos` | Share progress, screenshots | All teams |
| `#aos-infra` | CI/CD, tooling, environment | Team 7, all teams |

**Response SLAs:**
- `#aos-blockers`: <4 hours
- `#aos-critical-path`: <2 hours
- `#aos-v03-alpha`: <24 hours

---

### GitHub Issues

**Labels:**
- `critical-path` - Blocks downstream work
- `backend` - Team 1
- `inference` - Team 2
- `training` - Team 3
- `security` - Team 4
- `frontend` - Team 5
- `api` - Team 6
- `blocked` - Waiting on another team
- `bug` - Defect
- `enhancement` - New feature

**Milestones:**
- Phase 1 (Week 3)
- Phase 2 (Week 7)
- Phase 3 (Week 10)
- Release (Week 12)

**Issue Template:**
```markdown
## Description
Clear description of task

## Team
Team X (e.g., Team 1: Backend)

## Dependencies
- Blocked by: #123
- Blocks: #456

## Acceptance Criteria
- [ ] Criterion 1
- [ ] Criterion 2

## Test Plan
- [ ] Unit tests
- [ ] Integration tests
```

---

## Shared Resources

### Test Infrastructure

**Owned By:** Team 6 (API & Integration)
**Shared With:** All teams

**Resources:**
- Test database (SQLite, reset between runs)
- CI/CD pipelines (GitHub Actions)
- Benchmark harness (Criterion)
- E2E test framework (axum-test, Cypress)

**Access:**
- All teams can add tests
- Team 6 maintains infrastructure

---

### Documentation

**Owned By:** Team 6
**Shared With:** All teams

**Locations:**
- `/docs/PRD-COMPLETION-V03-ALPHA.md` - Main PRD
- `/docs/ARCHITECTURE_INDEX.md` - Architecture docs
- `/docs/CLAUDE.md` - Developer guide (updated by all teams)
- `/docs/README.md` - Docs navigation

**Update Protocol:**
- Update CLAUDE.md when patterns change
- Document new APIs in OpenAPI spec
- Update README.md for major features

---

### Staging Environments

**Owned By:** Team 7 (Platform & Tooling)
**Shared With:** All teams

**Environments:**
- `dev` - Latest main branch (auto-deploy)
- `staging` - Phase gate branches (manual deploy)
- `prod` - Release candidates only

**Access:**
- All teams can deploy to `dev`
- Team leads can deploy to `staging`
- Product owner deploys to `prod`

---

## Blocker Escalation

### Levels

**Level 1: Team-Internal**
- **Resolution Time:** <4 hours
- **Process:** Team standup, internal discussion

**Level 2: Cross-Team**
- **Resolution Time:** <1 day
- **Process:** Post in `#aos-blockers`, tag affected teams

**Level 3: Engineering Lead**
- **Resolution Time:** <2 days
- **Process:** Tag @engineering-lead, schedule sync meeting

**Level 4: Stakeholder Decision**
- **Resolution Time:** <3 days
- **Process:** Escalate to product owner (scope/priority decisions)

---

### Escalation Template

```markdown
**Blocker ID:** BLOCK-001
**Team:** Team 2 (Inference)
**Blocked On:** Team 1 (Backend completion)
**Impact:** Cannot start Week 4 tasks
**Started:** 2025-01-15
**Escalation Level:** 2 (Cross-Team)
**Action Required:** Team 1 provide backend stub for parallel work
**Owner:** @team-1-lead
```

---

## Meeting Protocols

### General Rules
1. **Agenda Required:** No agenda → meeting cancelled
2. **Timeboxed:** Strict end time, use parking lot for overflow
3. **Action Items:** Every meeting ends with action items (owner + due date)
4. **Recording:** All-hands and stakeholder demos recorded

### Async-First
- Use Slack threads for quick decisions
- Use GitHub issues for task tracking
- Use Loom videos for demos (when sync not needed)

### Sync When Needed
- Architecture decisions
- Blocker resolution
- Handoffs between teams

---

## Coordination Checklist

### Week 1 (Onboarding)
- [ ] All teams set up Slack, GitHub, environments
- [ ] Team leads review PRD and team charters
- [ ] Kickoff meeting (all teams)

### Week 3 (Phase 1 Gate)
- [ ] Team 1 → Team 2 handoff meeting
- [ ] Phase 1 completion demo
- [ ] Blocker triage (any Phase 2 blockers?)

### Week 7 (Phase 2 Gate)
- [ ] Team 2 → Team 3 handoff meeting
- [ ] Teams 2/3 → Team 5 API workshop
- [ ] Phase 2 completion demo

### Week 10 (Phase 3 Gate)
- [ ] All teams → Integration testing coordination
- [ ] Phase 3 completion demo
- [ ] Bug bash (all teams)

### Week 12 (Release)
- [ ] Final demo (all teams)
- [ ] Release notes review
- [ ] Retrospective (what went well, what to improve)

---

**Document Control:**
- **Version:** 1.0
- **Last Updated:** 2025-01-23
- **Next Review:** Week 2 (after first sprint)
- **Related:** [TEAM-CHARTERS.md](TEAM-CHARTERS.md), [PRD](../PRD-COMPLETION-V03-ALPHA.md)
