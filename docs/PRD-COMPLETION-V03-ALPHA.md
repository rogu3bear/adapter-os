# Product Requirements Document: AdapterOS v0.3-alpha Completion

**Version:** 1.0
**Date:** 2025-01-23
**Author:** AdapterOS Core Team
**Status:** Draft for Agent Team Review

---

## Executive Summary

### Objective
Complete AdapterOS v0.3-alpha by addressing 100+ incomplete features, stubs, and placeholders identified in the November 2025 codebase audit. Transform the current "builds but doesn't run" state into a fully functional ML inference and training platform for Apple Silicon.

### Scope
- **Timeline:** 12 weeks across 4 implementation phases
- **Team Size:** 15-19 engineers across 6-7 specialized teams
- **Tasks:** 70 actionable items grouped into 6 functional areas
- **Test Coverage Target:** ≥80% for production code, ≥95% for crypto
- **Performance Target:** 40+ tokens/sec inference, <5min training (small datasets)

### Current State
- ✅ **69 crates** compiling successfully (when Metal toolchain available)
- ✅ **189 REST API endpoints** defined
- ✅ **429 UI components** implemented
- ❌ **3 backends** (CoreML, MLX, Metal) all have critical gaps
- ❌ **Training pipeline** incomplete (dataset → train → package flow broken)
- ❌ **29 tests failing** in `adapteros-lora-worker`
- ❌ **KMS providers** all return mock implementations

### Success Metrics
1. **Technical:** All Phase 1-3 milestones achieved (backend functional, inference working, training operational)
2. **Quality:** ≥80% test coverage, <5 CRITICAL severity bugs at release
3. **Performance:** Inference ≥40 tok/s, training <5min for small datasets
4. **Integration:** 10+ end-to-end workflows functional in UI

---

## 1. Current State Assessment

### What's Working vs. Documented

| Component | Documentation Says | Reality |
|-----------|-------------------|---------|
| **CoreML Backend** | "Primary production backend, ANE acceleration" | FFI bridge not implemented (C1) |
| **MLX Backend** | "Production inference, training" | Stub with dummy data without `mlx` (C2/C3) |
| **Metal Backend** | "Deterministic GPU kernels, fallback" | Toolchain missing, can't compile (H1) |
| **Training Pipeline** | "5-step flow complete" | Trainer integration missing (T3-T4) |
| **KMS Providers** | "Cloud provider support" | All mocks (S1-S5) |
| **UI Components** | "React/TypeScript dashboard" | Data binding incomplete (U1-U15) |

### Critical Path Blockers

**BLOCKER 1: Metal Toolchain Missing**
- **Error:** "cannot execute tool 'metal' due to missing Metal Toolchain"
- **Impact:** Blocks workspace build, 29 test failures in `adapteros-lora-worker`
- **Solution:** `xcodebuild -downloadComponent MetalToolchain` (Team 1, Week 1)

**BLOCKER 2: No Functional Backend**
- **Issue:** All 3 backends (CoreML, MLX, Metal) have critical implementation gaps
- **Impact:** Cannot run inference end-to-end, blocks training pipeline
- **Solution:** Complete at least 1 backend (Team 1, Weeks 2-3)

**BLOCKER 3: Training Pipeline Broken**
- **Issue:** Dataset manager works, but trainer integration missing
- **Impact:** Cannot create adapters, core value proposition non-functional
- **Solution:** MicroLoRATrainer integration + .aos packaging (Team 3, Weeks 6-7)

**BLOCKER 4: UI-Backend Disconnect**
- **Issue:** 429 UI components exist but many show placeholder data
- **Impact:** Looks functional but unusable in practice
- **Solution:** Connect all UI pages to real APIs (Team 5, Weeks 8-10)

### Dependency Chain

```
External: Metal Toolchain Download
  ↓
Week 1: Fix adapteros-lora-kernel-mtl Build
  ↓
Weeks 2-3: Complete Backend (CoreML OR MLX OR Metal)
  ↓
Weeks 4-5: Inference Pipeline Integration
  ↓
Weeks 6-7: Training Pipeline Completion
  ↓
Weeks 8-10: UI Integration + Production Features
  ↓
Weeks 11-12: Testing & Release Prep
```

---

## 2. Feature Inventory

See [FEATURE-INVENTORY.md](features/FEATURE-INVENTORY.md) for detailed task breakdown.

### Summary by Functional Area

| Area | Tasks | Complexity | Team |
|------|-------|------------|------|
| **A. Core Backends** | 4 (C1-C4) | 3× XL, 1× L | Team 1 |
| **B. Inference Pipeline** | 8 (H1-H8) | 2× M, 4× S, 2× L | Team 2 |
| **C. Training Pipeline** | 12 (T1-T12) | 7× S, 3× M, 2× L | Team 3 |
| **D. Security & Crypto** | 9 (S1-S9) | 4× M, 4× L, 1× S | Team 4 |
| **E. UI Integration** | 15 (U1-U15) | 9× S, 5× M, 1× L | Team 5 |
| **F. API Endpoints** | 7 (A1-A7) | 3× S, 3× M, 1× L | Team 6 |
| **Total** | **55 tasks** | | **6 teams** |

**Deferred to v0.4:**
- G. Federation (4 tasks)
- H. Platform Stubs (14 tasks - Windows/Linux)

---

## 3. Team Structure

See [TEAM-CHARTERS.md](teams/TEAM-CHARTERS.md) for detailed team responsibilities.

### Team Summary

| Team | Size | Focus | Critical Path |
|------|------|-------|---------------|
| **Team 1: Backend Infrastructure** | 3 | CoreML/MLX/Metal backends | Yes (Weeks 1-3) |
| **Team 2: Inference Engine** | 3-4 | Inference pipeline, hot-swap, lifecycle | Yes (Weeks 4-5) |
| **Team 3: Training Pipeline** | 2-3 | Dataset → Train → Package workflow | Yes (Weeks 6-7) |
| **Team 4: Security & Crypto** | 2 | KMS providers, Secure Enclave | No (parallel) |
| **Team 5: Frontend Integration** | 2-3 | UI ↔ Backend connectivity | No (parallel) |
| **Team 6: API & Integration** | 2 | API handlers, integration tests | No (supports all) |
| **Team 7: Platform & Tooling** | 1-2 (optional) | Build, CI/CD, benchmarks | No (supports all) |

**Total:** 15-19 engineers

### Key Coordination Points
- **Daily Standups:** Teams 1 & 2 (critical path sync)
- **Weekly Syncs:** All teams (blocker resolution, handoffs)
- **Team 6 Role:** Integration testing coordinator, supports all teams

---

## 4. Implementation Phases

See [IMPLEMENTATION-PHASES.md](phases/IMPLEMENTATION-PHASES.md) for detailed timeline.

### Phase 1: Foundation (Weeks 1-3) - **CRITICAL PATH**

**Goal:** Unblock all downstream work

**Milestones:**
- ✅ Week 1: Metal toolchain installed, workspace builds clean
- ✅ Week 2-3: At least 1 backend functional (CoreML OR MLX)

**Teams Active:** 1 (critical), 4 (parallel), 6 (parallel), 7 (optional)

---

### Phase 2: Core Workflows (Weeks 4-7)

**Goal:** Inference + Training pipelines operational

**Milestones:**
- ✅ Week 5: Can run inference end-to-end (prompt → response)
- ✅ Week 7: Can train adapter (dataset → .aos file)

**Teams Active:** 1 (finish), 2 (critical), 3 (critical), 4-6 (parallel)

---

### Phase 3: Integration & Polish (Weeks 8-10)

**Goal:** UI connected, production-ready features

**Milestones:**
- ✅ Week 8: Training Jobs UI shows real data
- ✅ Week 10: All critical UI pages functional

**Teams Active:** 2-6 (all parallel)

---

### Phase 4: Testing & Hardening (Weeks 11-12)

**Goal:** Production quality, release readiness

**Milestones:**
- ✅ Week 11: Integration tests passing, stress tests complete
- ✅ Week 12: Release candidate ready

**Teams Active:** All teams (bug fixes, testing)

---

## 5. Verification Strategy

See [VERIFICATION-STRATEGY.md](testing/VERIFICATION-STRATEGY.md) for detailed test plans.

### Test Coverage Requirements

| Component | Unit Tests | Integration Tests | E2E Tests |
|-----------|-----------|-------------------|-----------|
| **Core Backends** | ≥80% | ≥70% | N/A |
| **Inference Pipeline** | ≥85% | ≥75% | ≥60% |
| **Training Pipeline** | ≥70% | ≥65% | ≥50% |
| **Security/Crypto** | ≥95% | ≥90% | ≥70% |
| **UI Components** | ≥60% | N/A | ≥50% (Cypress) |
| **API Handlers** | ≥80% | ≥80% | ≥60% |

### Acceptance Criteria (Phase-Level)

**Phase 1 Complete:**
- [ ] Workspace builds without errors (Metal toolchain working)
- [ ] At least 1 backend passes determinism tests
- [ ] Inference latency p95 <30ms (documented target: 24-28ms)

**Phase 2 Complete:**
- [ ] End-to-end inference test passes (prompt → router → adapters → response)
- [ ] Training test passes (dataset → train → package → register)
- [ ] 29 `lora-worker` tests now passing

**Phase 3 Complete:**
- [ ] All 25+ UI pages render without errors
- [ ] Real-time updates working (SSE for metrics/training)
- [ ] 10+ Cypress E2E tests passing

**Phase 4 Complete:**
- [ ] All 189 API endpoints documented in OpenAPI
- [ ] <5 CRITICAL severity bugs in release
- [ ] Performance benchmarks meet targets (40+ tok/s, <5min training)

---

## 6. Risks & Mitigations

See [RISKS-MITIGATIONS.md](risks/RISKS-MITIGATIONS.md) for detailed risk analysis.

### Top 5 Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| **CoreML FFI too complex** | Medium | High | Fallback to Metal backend |
| **Team 1 delay blocks Teams 2-3** | Medium | Critical | Stub backends for parallel progress |
| **Metal toolchain download fails** | Low | High | MLX-only path (no Metal) |
| **Training pipeline scope creep** | High | Medium | Strict scope: basic LoRA only |
| **UI-backend integration gaps** | Medium | Medium | Feature flags for incomplete features |

---

## 7. Success Metrics

### Technical Metrics
- [ ] All Phase 1-3 milestones achieved on schedule
- [ ] ≥80% test coverage for production code
- [ ] <5 CRITICAL severity bugs at release
- [ ] 0 runtime panics in production code paths

### Performance Metrics
- [ ] Inference: ≥40 tokens/sec on M3 Max (baseline: 45 tok/s from README)
- [ ] Memory overhead: ≤10% (documented: 8% router overhead acceptable)
- [ ] Training: <5 minutes for small datasets (1000 examples)
- [ ] Hot-swap: <100ms latency (documented target)

### Integration Metrics
- [ ] 10+ end-to-end workflows functional in UI
- [ ] All 189 API endpoints respond correctly
- [ ] Real-time updates working (SSE streams)
- [ ] RBAC enforced on all protected endpoints

### User Experience Metrics
- [ ] All 25+ UI pages load without console errors
- [ ] API errors display user-friendly messages
- [ ] Page load <2s, interactions <100ms

---

## 8. Out of Scope (Deferred to v0.4)

The following items identified in the audit are **intentionally deferred**:

1. **Federation System (G1-G4):** Peer discovery, tick ledger sync, adapter replication, cross-node verification
2. **Platform Stubs (H1-H14):** Windows/Linux compatibility (macOS-only for v0.3)
3. **Advanced UI Features:** Workspaces, tutorials, journeys (unwired handlers)
4. **Experimental Features:** Domain adapters beyond basic functionality

**Rationale:** Focus v0.3-alpha on core value proposition (inference + training on macOS). Federation and cross-platform support are v0.4 goals.

---

## 9. Dependencies & Assumptions

### External Dependencies
- **Metal Toolchain:** Requires macOS + Xcode Command Line Tools
- **MLX Library:** Optional, only for `mlx` feature
- **Cloud SDKs:** AWS/GCP/Azure for KMS providers (Team 4)

### Assumptions
1. All teams have macOS development machines (M1/M2/M3)
2. Access to test infrastructure (CI/CD, database, storage)
3. Stakeholder availability for weekly demos
4. No major scope changes during 12-week timeline

---

## 10. Communication Plan

### Cadence
- **Daily:** Team 1 & 2 standup (critical path sync)
- **Weekly:** All-hands sync (progress, blockers, demos)
- **Bi-weekly:** Stakeholder demo (Weeks 2, 4, 6, 8, 10, 12)
- **Ad-hoc:** Blocker escalation (Slack channel)

### Artifacts
- **Weekly:** Sprint reports (progress against todo list)
- **Bi-weekly:** Demo videos (key features working)
- **End-of-phase:** Phase completion reports (milestones achieved)

---

## 11. Next Steps

### Immediate Actions (Week 0 - Pre-Kickoff)
1. **Review & Approve PRD:** Stakeholder sign-off
2. **Staff Teams:** Recruit 15-19 engineers
3. **Set Up Infrastructure:** CI/CD, test databases, Slack channels
4. **Create Tracking:** GitHub projects or Jira with 70 tasks

### Week 1 Kickoff
1. **Team 1:** Start Metal toolchain setup
2. **Team 6:** Set up integration test framework
3. **Team 7:** Configure CI pipeline
4. **All Teams:** Onboarding, tooling setup

---

## Appendices

### A. Related Documents
- [AUDIT_UNFINISHED_FEATURES.md](../AUDIT_UNFINISHED_FEATURES.md) - Original audit (100+ items)
- [CLAUDE.md](../CLAUDE.md) - Developer guide
- [README.md](../README.md) - Project overview
- [docs/ARCHITECTURE_INDEX.md](ARCHITECTURE_INDEX.md) - Architecture documentation

### B. References
- **FEATURE-INVENTORY.md:** Detailed task breakdown with acceptance criteria
- **TEAM-CHARTERS.md:** Team responsibilities and coordination model
- **IMPLEMENTATION-PHASES.md:** Week-by-week execution timeline
- **VERIFICATION-STRATEGY.md:** Test plans and QA procedures
- **RISKS-MITIGATIONS.md:** Risk register with mitigation strategies
---

**Document Control:**
- **Version:** 1.0 (Initial Draft)
- **Last Updated:** 2025-01-23
- **Next Review:** Week 4 (mid-Phase 2 checkpoint)
- **Approval Required:** Engineering Lead, Product Owner
MLNavigator Inc 2025-12-04.
