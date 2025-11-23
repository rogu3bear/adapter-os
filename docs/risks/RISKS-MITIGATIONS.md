# Risks & Mitigations: AdapterOS v0.3-alpha

**Version:** 1.0
**Date:** 2025-01-23
**Related:** [PRD-COMPLETION-V03-ALPHA.md](../PRD-COMPLETION-V03-ALPHA.md)

---

## Overview

This document identifies risks to AdapterOS v0.3-alpha completion and defines mitigation strategies. Risks are categorized by type (Technical, Schedule, Scope) and priority (Critical, High, Medium, Low).

**Risk Management:** Weekly review in all-hands sync, escalate Critical/High risks immediately.

---

## Critical Risks

### RISK-001: CoreML FFI Too Complex
**Category:** Technical
**Likelihood:** Medium
**Impact:** High (blocks Phase 1)
**Owner:** Team 1 Lead

**Description:**
CoreML FFI bridge (C1) requires Objective-C++ expertise and may have undocumented edge cases. Implementation may take longer than estimated (Week 2).

**Impact:**
- Delays Phase 1 completion
- Blocks Team 2 (Inference) start
- Cascades to entire timeline

**Mitigation:**
1. **Primary:** Start MLX backend (C2) in parallel Week 2
2. **Fallback:** Use Metal backend instead (H1 priority)
3. **Escalation:** Hire Objective-C++ contractor (Week 2 if needed)
4. **Contingency:** Document CoreML as "not supported" in v0.3, defer to v0.4

**Status:** **Active** - Monitor Week 2 progress

---

### RISK-002: Team 1 Delay Blocks Teams 2-3
**Category:** Schedule
**Likelihood:** Medium
**Impact:** Critical (affects critical path)
**Owner:** Engineering Lead

**Description:**
Teams 2 (Inference) and 3 (Training) are blocked until Team 1 completes at least one backend (Week 3). Any delay in Phase 1 cascades to entire timeline.

**Impact:**
- Week 4-7 critical path compressed
- Teams 2-3 idle (cost: 6-7 engineers)
- May miss Phase 2 gate (Week 7)

**Mitigation:**
1. **Stub Backend:** Team 6 creates stub backend for Teams 2-3 (Week 2)
2. **Parallel Work:** Teams 2-3 use stub for architecture work (Week 3)
3. **Buffer Time:** Build 1-week buffer into Phase 1 (extend to Week 4 if needed)
4. **Daily Sync:** Team 1 + 2 daily standup (spot issues early)

**Status:** **Monitoring** - Daily standup in place

---

### RISK-003: Metal Toolchain Download Fails
**Category:** Technical
**Likelihood:** Low
**Impact:** High (blocks build)
**Owner:** Team 7 Lead

**Description:**
`xcodebuild -downloadComponent MetalToolchain` depends on Apple servers. Download may fail due to network issues, Apple server downtime, or licensing problems.

**Impact:**
- Workspace build blocked
- 29 `lora-worker` tests fail
- Delays Week 1 milestone

**Mitigation:**
1. **Primary:** Automate download with retry logic (3 attempts)
2. **Fallback:** Manual download from Apple Developer portal
3. **Workaround:** MLX-only path (no Metal backend)
4. **Escalation:** Contact Apple Developer Support (if licensing issue)

**Status:** **Monitoring** - Retry logic implemented in Week 1

---

## High Risks

### RISK-004: Training Pipeline Scope Creep
**Category:** Scope
**Likelihood:** High
**Impact:** Medium (delays Phase 2)
**Owner:** Product Owner

**Description:**
Training pipeline (T1-T12) has 12 tasks. Teams may want to add advanced features (distributed training, model merging, etc.) beyond scope.

**Impact:**
- Week 6-7 timeline slips
- May miss Phase 2 gate
- Reduces time for Phase 3 (UI integration)

**Mitigation:**
1. **Strict Scope:** Basic LoRA only (rank ≤16, no advanced features)
2. **Backlog:** Defer advanced features to v0.4 (document in backlog)
3. **Gate Keeper:** Product owner approves any scope changes
4. **Time Boxing:** Hard stop at Week 7, ship what's done

**Status:** **Active** - Weekly scope review in all-hands

---

### RISK-005: UI-Backend Integration Gaps
**Category:** Technical
**Likelihood:** Medium
**Impact:** Medium (delays Phase 3)
**Owner:** Team 5 Lead

**Description:**
429 UI components exist but many lack backend APIs (T4, T5, H6, etc.). Team 5 may discover gaps during integration (Week 8-10).

**Impact:**
- UI pages show errors/incomplete data
- May miss Phase 3 gate (Week 10)
- User experience degraded

**Mitigation:**
1. **API Workshop:** Teams 2/3 → Team 5 (Week 7) to document APIs
2. **Feature Flags:** Hide incomplete UI features behind flags
3. **Graceful Degradation:** UI shows "Coming soon" for missing features
4. **Prioritization:** Focus on top 10 most-used pages first

**Status:** **Monitoring** - Workshop scheduled Week 7

---

### RISK-006: Test Coverage Below Target
**Category:** Quality
**Likelihood:** Medium
**Impact:** Medium (blocks release)
**Owner:** Team 6 Lead

**Description:**
Target: ≥80% test coverage. Teams may deprioritize tests to meet feature deadlines.

**Impact:**
- Release gate blocked (Week 12)
- Bugs discovered late (costly to fix)
- Production instability

**Mitigation:**
1. **Coverage Tracking:** Codecov on every PR (fail if <80%)
2. **Test-First:** Write tests before code (TDD)
3. **Review Process:** PR approval requires tests
4. **Incentives:** Test coverage in sprint reports (gamification)

**Status:** **Active** - Codecov configured Week 1

---

### RISK-007: KMS Provider Implementation Delays
**Category:** Schedule
**Likelihood:** Low
**Impact:** Medium (nice-to-have for v0.3)
**Owner:** Team 4 Lead

**Description:**
KMS providers (S1-S5) require cloud accounts (AWS, GCP, Azure) and SDKs. Integration may take longer than estimated.

**Impact:**
- Security features incomplete
- Production deployment delayed (KMS required for prod)

**Mitigation:**
1. **Prioritize AWS:** Complete S1 (AWS KMS) first (Week 1-4)
2. **Defer Others:** S2-S5 lower priority, ship if time allows
3. **Mock Mode:** Allow mock KMS for development/testing
4. **Documentation:** Document KMS setup for production

**Status:** **Monitoring** - AWS KMS prioritized

---

## Medium Risks

### RISK-008: MLX Backend Dependency on Python
**Category:** Technical
**Likelihood:** Medium
**Impact:** Low (affects training only)
**Owner:** Team 1 Lead

**Description:**
MLX backend (C2/C3) requires MLX C++ library and Python bindings. Installation may fail on some systems.

**Impact:**
- GPU training unavailable (T10)
- Training slower (CPU-only)

**Mitigation:**
1. **Stub Fallback:** MLX backend has stub mode (no MLX library)
2. **Documentation:** Clear installation guide for MLX
3. **CI/CD:** Test with and without MLX (`--features real-mlx`)
4. **Fallback:** CPU-only training acceptable for v0.3

**Status:** **Low Priority** - Stub mode working

---

### RISK-009: Cypress E2E Tests Flaky
**Category:** Quality
**Likelihood:** High
**Impact:** Low (annoying but not blocking)
**Owner:** Team 5 Lead

**Description:**
Cypress E2E tests may be flaky due to timing issues, SSE race conditions, or environment differences.

**Impact:**
- CI/CD unreliable
- Developer frustration
- False negatives

**Mitigation:**
1. **Retries:** Cypress retry failed tests (3 attempts)
2. **Waits:** Use explicit waits (not hard-coded sleep)
3. **Isolation:** Each test resets database
4. **Debugging:** Record videos on failure

**Status:** **Monitoring** - Retry logic in place

---

### RISK-010: Late Bug Discoveries (Week 11-12)
**Category:** Quality
**Likelihood:** Medium
**Impact:** Medium (delays release)
**Owner:** Engineering Lead

**Description:**
Integration testing (Week 11) may uncover critical bugs that require code changes. Fixes may introduce regressions.

**Impact:**
- Release date slips (Week 12 → Week 13+)
- Quality compromised if rushed

**Mitigation:**
1. **Early Testing:** Start integration tests Week 8 (not Week 11)
2. **Triage:** Ruthlessly prioritize bugs (Critical → defer Medium/Low to v0.4)
3. **Bug Bash:** All-hands bug bash Week 10 (proactive)
4. **Buffer Time:** Plan for Week 13 buffer (not advertised)

**Status:** **Active** - Integration tests start Week 8

---

## Low Risks

### RISK-011: Secure Enclave Not Available
**Category:** Technical
**Likelihood:** Low
**Impact:** Low (nice-to-have for v0.3)
**Owner:** Team 4 Lead

**Description:**
Secure Enclave (S6) requires M1/M2/M3 Macs. May not work on Intel Macs or older systems.

**Impact:**
- SEP attestation unavailable
- Fallback to software crypto

**Mitigation:**
1. **Graceful Fallback:** Detect SEP availability, fallback if not available
2. **Documentation:** Document SEP requirement (M1+ only)
3. **CI/CD:** Test on both M-series and Intel Macs

**Status:** **Low Priority** - Fallback implemented

---

### RISK-012: Documentation Outdated
**Category:** Quality
**Likelihood:** High
**Impact:** Low (developer experience)
**Owner:** All Teams

**Description:**
Code changes faster than documentation updates. CLAUDE.md, README.md may become stale.

**Impact:**
- Developer confusion
- Onboarding friction

**Mitigation:**
1. **Update Protocol:** PR approval requires doc updates
2. **Weekly Review:** Check CLAUDE.md accuracy in all-hands
3. **Final Pass:** Documentation review Week 12

**Status:** **Active** - PR checklist includes docs

---

## Risk Dashboard

**Weekly Tracking:**

| Risk ID | Category | Likelihood | Impact | Status | Owner | Last Updated |
|---------|----------|-----------|--------|--------|-------|--------------|
| RISK-001 | Technical | Medium | High | Active | Team 1 | 2025-01-23 |
| RISK-002 | Schedule | Medium | Critical | Monitoring | Eng Lead | 2025-01-23 |
| RISK-003 | Technical | Low | High | Monitoring | Team 7 | 2025-01-23 |
| RISK-004 | Scope | High | Medium | Active | Product | 2025-01-23 |
| RISK-005 | Technical | Medium | Medium | Monitoring | Team 5 | 2025-01-23 |
| RISK-006 | Quality | Medium | Medium | Active | Team 6 | 2025-01-23 |
| RISK-007 | Schedule | Low | Medium | Monitoring | Team 4 | 2025-01-23 |
| RISK-008 | Technical | Medium | Low | Low Priority | Team 1 | 2025-01-23 |
| RISK-009 | Quality | High | Low | Monitoring | Team 5 | 2025-01-23 |
| RISK-010 | Quality | Medium | Medium | Active | Eng Lead | 2025-01-23 |
| RISK-011 | Technical | Low | Low | Low Priority | Team 4 | 2025-01-23 |
| RISK-012 | Quality | High | Low | Active | All Teams | 2025-01-23 |

**Risk Status:**
- **Active:** Actively mitigating (4 risks)
- **Monitoring:** Watching closely (5 risks)
- **Low Priority:** Accepted risk (3 risks)

---

## Contingency Plans

### If Phase 1 Slips (Week 3 → Week 4)
1. **Compress Phase 2:** Reduce Week 4-7 to 3 weeks (parallel work)
2. **Defer Features:** Move T10 (GPU training) to Phase 3
3. **Add Resources:** Hire contractor for Team 1 (CoreML expert)

### If Phase 2 Slips (Week 7 → Week 8)
1. **Reduce Scope:** Ship basic training only (T1-T8, defer T9-T12)
2. **Extend Timeline:** Week 12 → Week 13 (1-week buffer)
3. **Parallel Work:** Team 5 starts UI with stub APIs (Week 7)

### If Critical Bugs Found (Week 11-12)
1. **Triage:** Defer Medium/Low bugs to v0.4
2. **Hotfix:** Create release branch, fix Critical bugs only
3. **Delay Release:** Week 12 → Week 13 (max 1-week delay acceptable)

---

## Escalation Triggers

**Immediate Escalation (to Engineering Lead):**
- Any Critical risk becomes "Likely" or "High Impact"
- Phase gate missed by >1 week
- >10 Critical bugs discovered
- Team member unavailable >3 days

**Stakeholder Escalation (to Product Owner):**
- Scope changes requested
- Timeline extension >2 weeks
- Resource needs (hire contractor, etc.)
- Feature prioritization conflicts

---

**Document Control:**
- **Version:** 1.0
- **Last Updated:** 2025-01-23
- **Next Review:** Weekly (all-hands sync)
- **Related:** [IMPLEMENTATION-PHASES.md](../phases/IMPLEMENTATION-PHASES.md), [PRD](../PRD-COMPLETION-V03-ALPHA.md)
