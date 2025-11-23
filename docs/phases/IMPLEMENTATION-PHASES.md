# Implementation Phases: AdapterOS v0.3-alpha Completion

**Version:** 1.0
**Date:** 2025-01-23
**Related:** [PRD-COMPLETION-V03-ALPHA.md](../PRD-COMPLETION-V03-ALPHA.md)

---

## Overview

This document provides a week-by-week implementation timeline for completing AdapterOS v0.3-alpha over a 12-week period. The plan is organized into 4 phases with clear milestones and gating criteria.

**Timeline:** 12 weeks (January - April 2025)
**Teams:** 6-7 teams, 15-19 engineers
**Critical Path:** Weeks 1-7 (Backend → Inference → Training)

---

## Phase 1: Foundation (Weeks 1-3)

### Goal
Unblock all downstream work by completing at least one functional backend and fixing the build system.

### Critical Success Criteria
- ✅ Metal toolchain installed, workspace builds without errors
- ✅ At least 1 backend (CoreML OR MLX) passes determinism tests
- ✅ Inference latency p95 <30ms (documented target: 24-28ms)
- ✅ 29 failing `lora-worker` tests now passing

---

### Week 1: Build System & Toolchain

**Focus:** Fix blockers, set up infrastructure

#### Team 1: Backend Infrastructure (3 engineers)
**Tasks:**
- [ ] **H1:** Install Metal Toolchain (`xcodebuild -downloadComponent MetalToolchain`)
- [ ] Fix `adapteros-lora-kernel-mtl` build
- [ ] Start **C1:** CoreML FFI bridge research (architecture design)

**Deliverables:**
- Workspace builds clean
- Metal kernels compile to `.metallib`

**Blockers:**
- None (external dependency: Apple servers for toolchain download)

---

#### Team 6: API & Integration (2 engineers)
**Tasks:**
- [ ] Set up integration test framework
- [ ] Document all 189 API endpoints (baseline)
- [ ] Create test database fixtures

**Deliverables:**
- Integration test harness ready
- API documentation baseline

---

#### Team 7: Platform & Tooling (1-2 engineers, optional)
**Tasks:**
- [ ] Automate Metal Toolchain installation script
- [ ] Configure CI/CD to install toolchain
- [ ] Set up benchmark harness (Criterion)

**Deliverables:**
- One-command Metal setup
- CI builds pass

---

#### All Teams
**Tasks:**
- Onboarding, tooling setup
- Review codebase architecture
- Set up development environments

**Milestone 1.1 (Week 1 End):**
- ✅ Workspace builds without Metal errors
- ✅ All teams onboarded

---

### Week 2: Backend Implementation (CoreML)

**Focus:** Complete CoreML FFI bridge OR MLX backend

#### Team 1: Backend Infrastructure (3 engineers)
**Lead Focus: CoreML**
**Tasks:**
- [ ] **C1:** Implement `coreml_bridge.mm` (Objective-C++ FFI)
  - `coreml_load_model()`
  - `coreml_forward()`
  - `coreml_free_model()`
- [ ] Update `build.rs` to compile `.mm` files
- [ ] Wire FFI into Rust `CoreMLBackend::forward()`
- [ ] Test ANE detection

**Alternate Focus: MLX (if CoreML blocked)**
**Tasks:**
- [ ] **C2:** Enable `--features real-mlx`
- [ ] Link MLX C++ library
- [ ] Replace stub `MLXFFIModel::load()`

**Deliverables:**
- CoreML backend loads model (basic functionality)
- OR MLX backend with real-mlx feature works

**Blockers:**
- CoreML complexity may require Week 3 spillover

---

#### Team 4: Security & Crypto (2 engineers)
**Tasks:**
- [ ] **S1:** Start AWS KMS provider implementation
- [ ] **S7:** Implement key lifecycle creation date (quick win)

**Deliverables:**
- AWS KMS basic encrypt/decrypt working
- Creation date extraction complete

---

#### Team 6: API & Integration (2 engineers)
**Tasks:**
- [ ] Write integration tests for existing endpoints
- [ ] Document OpenAPI spec gaps

**Deliverables:**
- 50+ integration tests ready

---

**Milestone 1.2 (Week 2 End):**
- ✅ CoreML backend partially functional (model loads)
- ✅ OR MLX backend with `real-mlx` compiles

---

### Week 3: Backend Completion & ANE

**Focus:** Complete at least 1 backend end-to-end

#### Team 1: Backend Infrastructure (3 engineers)
**Tasks:**
- [ ] **C1:** Complete CoreML FFI (forward pass, determinism)
- [ ] **C3:** Replace MLX C++ wrapper stubs (if MLX is chosen)
- [ ] **C4:** Implement ANE execution path OR document fallback
- [ ] **C2:** Complete MLX backend integration

**Decision Point:** Choose primary backend (CoreML recommended)

**Deliverables:**
- At least 1 backend passes full determinism test
- Forward pass returns correct tensor shapes
- Performance meets targets (p95 <30ms)

---

#### Team 4: Security & Crypto (2 engineers)
**Tasks:**
- [ ] **S1:** Complete AWS KMS provider
- [ ] **S6:** Start Secure Enclave SEP attestation

**Deliverables:**
- AWS KMS fully functional
- SEP attestation design complete

---

#### Team 6: API & Integration (2 engineers)
**Tasks:**
- [ ] Integration tests for backend
- [ ] Verify backend determinism via API

**Deliverables:**
- Backend integration tests passing

---

**Milestone 1.3 (Week 3 End) - PHASE 1 GATE:**
- ✅ At least 1 backend fully functional
- ✅ Determinism test passes (10 runs, identical outputs)
- ✅ Inference latency p95 <30ms
- ✅ 29 `lora-worker` tests passing
- ✅ Workspace builds clean on CI

**Gate Decision:** Proceed to Phase 2 ONLY if backend is functional.

---

## Phase 2: Core Workflows (Weeks 4-7)

### Goal
Build functional inference and training pipelines on top of the completed backend.

### Critical Success Criteria
- ✅ End-to-end inference works (prompt → response)
- ✅ End-to-end training works (dataset → .aos file)
- ✅ Hot-swap stress test passes (1000 swaps, 0 failures)
- ✅ Training job visible in UI with real-time updates

---

### Week 4: Inference Pipeline Integration

**Focus:** Integrate backend into full inference flow

#### Team 2: Inference Engine (3-4 engineers) - **CRITICAL PATH STARTS**
**Tasks:**
- [ ] **H2:** Router integration tests (K-sparse selection)
- [ ] **H3:** K-sparse selection unit tests
- [ ] Integrate backend into router
- [ ] Test end-to-end: prompt → router → adapters → response

**Deliverables:**
- Inference pipeline works end-to-end
- Router selects top-K adapters correctly

**Blockers:**
- Blocked by Team 1 (backend must be complete)

---

#### Team 1: Backend Infrastructure (3 engineers)
**Tasks:**
- [ ] Support Team 2 (backend integration issues)
- [ ] Performance tuning (cache optimization)
- [ ] Start alternative backend (if CoreML chosen, start MLX; if MLX chosen, start Metal)

**Deliverables:**
- Backend optimized
- Alternative backend 50% complete

---

#### Team 4: Security & Crypto (2 engineers)
**Tasks:**
- [ ] **S6:** Complete Secure Enclave SEP attestation
- [ ] **S2:** Start GCP KMS provider

**Deliverables:**
- SEP attestation working or fallback documented
- GCP KMS 30% complete

---

#### Team 6: API & Integration (2 engineers)
**Tasks:**
- [ ] **A6:** Start model import handler (depends on backend)
- [ ] Integration tests for inference API

**Deliverables:**
- Model import 50% complete

---

**Milestone 2.1 (Week 4 End):**
- ✅ Inference works end-to-end
- ✅ Router integration tests passing

---

### Week 5: Memory Management & Hot-Swap

**Focus:** Robust inference with memory management and hot-swap

#### Team 2: Inference Engine (3-4 engineers)
**Tasks:**
- [ ] **H4:** Hot-swap integration tests
- [ ] **H5:** Memory pressure tests
- [ ] **H7:** Adapter lifecycle transitions (start)

**Deliverables:**
- Hot-swap stress test passes (1000 iterations)
- Memory eviction triggers correctly

---

#### Team 1: Backend Infrastructure (3 engineers)
**Tasks:**
- [ ] Continue alternative backend
- [ ] Performance profiling (Instruments)

**Deliverables:**
- Alternative backend 80% complete

---

#### Team 4: Security & Crypto (2 engineers)
**Tasks:**
- [ ] **S2:** Complete GCP KMS provider
- [ ] **S8:** Start rotation daemon KMS

**Deliverables:**
- GCP KMS functional

---

**Milestone 2.2 (Week 5 End):**
- ✅ Hot-swap <100ms latency
- ✅ Memory eviction working

---

### Week 6: Training Pipeline Starts

**Focus:** Dataset management and training job infrastructure

#### Team 3: Training Pipeline (2-3 engineers) - **JOINS CRITICAL PATH**
**Tasks:**
- [ ] **T1:** Dataset upload/validation
- [ ] **T2:** Chunked upload handler
- [ ] **T4:** Training job management (database)

**Deliverables:**
- Dataset upload works
- Training jobs table populated

**Blockers:**
- None (can work in parallel with Team 2)

---

#### Team 2: Inference Engine (3-4 engineers)
**Tasks:**
- [ ] **H6:** Streaming inference (SSE)
- [ ] **H7:** Complete adapter lifecycle transitions
- [ ] **H8:** Lifecycle heartbeat recovery

**Deliverables:**
- SSE streaming works
- All lifecycle states functional

---

#### Team 4: Security & Crypto (2 engineers)
**Tasks:**
- [ ] **S8:** Complete rotation daemon KMS
- [ ] **S9:** Start patch crypto/audit modules

**Deliverables:**
- Rotation daemon tested

---

**Milestone 2.3 (Week 6 End):**
- ✅ Streaming inference working
- ✅ Datasets uploadable and validated

---

### Week 7: Training Integration

**Focus:** Complete training pipeline end-to-end

#### Team 3: Training Pipeline (2-3 engineers)
**Tasks:**
- [ ] **T3:** MicroLoRATrainer integration
- [ ] **T5:** Progress tracking (SSE)
- [ ] **T7:** Model packaging (.aos)
- [ ] **T8:** Registry integration
- [ ] Test end-to-end: dataset → train → package → register

**Deliverables:**
- Training works end-to-end
- .aos file created and loadable

**Blockers:**
- Needs MLX backend for GPU training (Team 1 alternative backend)

---

#### Team 2: Inference Engine (3-4 engineers)
**Tasks:**
- [ ] Performance tuning
- [ ] Stress testing (1000 concurrent requests)

**Deliverables:**
- Performance benchmarks documented

---

#### Team 6: API & Integration (2 engineers)
**Tasks:**
- [ ] **A6:** Complete model import handler
- [ ] Integration tests for training API

**Deliverables:**
- Model import working

---

**Milestone 2.4 (Week 7 End) - PHASE 2 GATE:**
- ✅ Inference pipeline complete (all H1-H8 tasks done)
- ✅ Training pipeline complete (T1-T8 done)
- ✅ Can train adapter in <5 min (small dataset)
- ✅ Packaged .aos file works

**Gate Decision:** Proceed to Phase 3 ONLY if core workflows are functional.

---

## Phase 3: Integration & Polish (Weeks 8-10)

### Goal
Connect UI to backend, complete production features, polish UX.

### Critical Success Criteria
- ✅ All 25+ UI pages functional with real data
- ✅ Real-time updates working (SSE for metrics/training)
- ✅ 10+ Cypress E2E tests passing
- ✅ KMS providers functional (2+ cloud providers)

---

### Week 8: UI Integration Starts

**Focus:** Connect UI pages to real APIs

#### Team 5: Frontend Integration (2-3 engineers) - **JOINS**
**Tasks:**
- [ ] **U1:** Training Jobs page data binding
- [ ] **U2:** Metrics dashboard real-time updates
- [ ] **U4:** Hot-swap UI live status
- [ ] **U9:** Router config UI

**Deliverables:**
- 4+ pages showing real data
- SSE updates working in UI

---

#### Team 3: Training Pipeline (2-3 engineers)
**Tasks:**
- [ ] **T6:** Hyperparameter templates
- [ ] **T9:** Training metrics collection
- [ ] **T10:** GPU training support
- [ ] **T11:** Training UI data binding (with Team 5)

**Deliverables:**
- GPU training works
- Training metrics in UI

---

#### Team 2: Inference Engine (3-4 engineers)
**Tasks:**
- [ ] Performance optimization
- [ ] Documentation (inference flow diagrams)

**Deliverables:**
- Performance tuned

---

#### Team 4: Security & Crypto (2 engineers)
**Tasks:**
- [ ] **S9:** Complete patch crypto/audit modules
- [ ] **S3:** Start Azure Key Vault (lower priority)

**Deliverables:**
- Patch modules functional

---

#### Team 6: API & Integration (2 engineers)
**Tasks:**
- [ ] **A1:** Process alerts handlers
- [ ] **A2:** Process anomalies handlers
- [ ] **A3:** Monitoring dashboards API

**Deliverables:**
- 3 API handlers complete

---

**Milestone 3.1 (Week 8 End):**
- ✅ Training Jobs UI shows real data
- ✅ Metrics dashboard real-time updates work

---

### Week 9: More UI Integration

**Focus:** Complete remaining UI pages

#### Team 5: Frontend Integration (2-3 engineers)
**Tasks:**
- [ ] **U3:** Adapter detail lifecycle visualization
- [ ] **U5:** Policy management UI
- [ ] **U7:** Replay UI
- [ ] **U8:** Telemetry viewer
- [ ] **U10:** System metrics GPU stats

**Deliverables:**
- 5+ more pages functional

---

#### Team 3: Training Pipeline (2-3 engineers)
**Tasks:**
- [ ] **T12:** Training templates UI (with Team 5)
- [ ] Performance tuning (training speed)

**Deliverables:**
- Training templates UI complete

---

#### Team 4: Security & Crypto (2 engineers)
**Tasks:**
- [ ] **S3:** Complete Azure Key Vault
- [ ] **S4:** Start HashiCorp Vault (lower priority)

**Deliverables:**
- Azure Key Vault functional

---

#### Team 6: API & Integration (2 engineers)
**Tasks:**
- [ ] **A4:** Database incidents module
- [ ] **A5:** Chunked upload handler
- [ ] **A7:** Wire unwired handlers

**Deliverables:**
- 3+ API handlers complete

---

**Milestone 3.2 (Week 9 End):**
- ✅ 15+ UI pages functional
- ✅ Most API gaps filled

---

### Week 10: Polish & E2E Testing

**Focus:** Complete all UI integration, E2E tests

#### Team 5: Frontend Integration (2-3 engineers)
**Tasks:**
- [ ] **U6:** Golden runs UI
- [ ] **U11:** Alerts/notifications
- [ ] **U12:** Git integration page
- [ ] **U13:** Contacts/discovery pages
- [ ] **U15:** Base models page
- [ ] Cypress E2E tests (10+ scenarios)

**Deliverables:**
- All critical UI pages complete
- E2E tests passing

---

#### All Teams
**Tasks:**
- Bug fixes from integration testing
- Documentation updates
- Performance profiling

**Deliverables:**
- Bug backlog under control (<10 CRITICAL bugs)

---

**Milestone 3.3 (Week 10 End) - PHASE 3 GATE:**
- ✅ All 25+ UI pages render without errors
- ✅ 10+ Cypress E2E tests passing
- ✅ No mock data in production build
- ✅ Real-time updates working
- ✅ 2+ KMS providers functional

**Gate Decision:** Proceed to Phase 4 (testing) ONLY if integration is complete.

---

## Phase 4: Testing & Hardening (Weeks 11-12)

### Goal
Production-ready quality: comprehensive testing, bug fixes, performance validation.

### Critical Success Criteria
- ✅ All 189 API endpoints documented in OpenAPI
- ✅ <5 CRITICAL severity bugs
- ✅ Performance benchmarks meet targets (40+ tok/s, <5min training)
- ✅ ≥80% test coverage for production code

---

### Week 11: Integration Testing & Bug Fixes

**Focus:** Comprehensive integration testing, bug bash

#### Team 6: API & Integration (2 engineers) - **LEADS**
**Tasks:**
- [ ] Run full integration test suite (100+ tests)
- [ ] OpenAPI spec validation
- [ ] Performance testing (API response times)
- [ ] Load testing (100 concurrent users)

**Deliverables:**
- Integration test report
- Performance report

---

#### Team 2: Inference Engine (3-4 engineers)
**Tasks:**
- [ ] Stress testing (1000 concurrent requests, 1000 hot-swaps)
- [ ] Memory leak detection (Valgrind, Instruments)
- [ ] Profiling (Instruments, perf)

**Deliverables:**
- Stress test report
- Performance tuning complete

---

#### Team 3: Training Pipeline (2-3 engineers)
**Tasks:**
- [ ] Training stress tests (10 concurrent jobs)
- [ ] GPU utilization profiling

**Deliverables:**
- Training performance report

---

#### Team 5: Frontend Integration (2-3 engineers)
**Tasks:**
- [ ] E2E test suite expansion (20+ tests)
- [ ] Accessibility audit (WCAG 2.1 AA)
- [ ] Responsive design testing

**Deliverables:**
- E2E test report
- Accessibility report

---

#### All Teams
**Tasks:**
- Bug fixes from testing
- Code review cleanup
- TODO/FIXME removal

**Deliverables:**
- Bug count <10 CRITICAL
- No TODO/FIXME in production code paths

---

**Milestone 4.1 (Week 11 End):**
- ✅ Integration tests passing
- ✅ Stress tests passing
- ✅ Bug count <10 CRITICAL

---

### Week 12: Release Prep & Documentation

**Focus:** Final polish, documentation, release candidate

#### All Teams
**Tasks:**
- [ ] Final bug fixes
- [ ] Update CLAUDE.md (mark In Development items as Complete)
- [ ] Update README.md (status badges, benchmarks)
- [ ] Generate changelog
- [ ] Create release notes
- [ ] Tag v0.3-alpha release candidate

**Deliverables:**
- Release candidate tagged
- Documentation updated
- Changelog complete

---

#### Team 6: API & Integration (2 engineers)
**Tasks:**
- [ ] Verify all 189 endpoints in OpenAPI spec
- [ ] API documentation review
- [ ] Create Postman collection (optional)

**Deliverables:**
- OpenAPI spec published

---

#### Team 7: Platform & Tooling (1-2 engineers)
**Tasks:**
- [ ] Benchmark harness report (historical trends)
- [ ] CI/CD optimization report
- [ ] Developer onboarding guide

**Deliverables:**
- Infrastructure documentation

---

**Milestone 4.2 (Week 12 End) - RELEASE:**
- ✅ v0.3-alpha release candidate ready
- ✅ All 189 API endpoints documented
- ✅ <5 CRITICAL bugs
- ✅ Performance benchmarks met
- ✅ ≥80% test coverage
- ✅ Documentation complete

**Release Decision:** Ship v0.3-alpha

---

## Critical Path Summary

```
Week 1: Metal Toolchain
  ↓
Weeks 2-3: Backend (Team 1)
  ↓
Weeks 4-5: Inference Pipeline (Team 2)
  ↓
Weeks 6-7: Training Pipeline (Team 3)
  ↓
Weeks 8-10: UI Integration (Team 5) + API (Team 6)
  ↓
Weeks 11-12: Testing & Release
```

**Critical Path Teams:** 1, 2, 3 (sequential), then all teams converge.

---

## Parallel Workstreams

**Security (Team 4):** Weeks 1-12 (independent, non-blocking)
**Frontend (Team 5):** Weeks 8-12 (depends on APIs from Teams 2-3)
**API (Team 6):** Weeks 1-12 (supports all teams, integration testing)
**Platform (Team 7):** Weeks 1-12 (optional, supports all teams)

---

## Risk Management

### Week 1 Risks
- **Risk:** Metal toolchain download fails
- **Mitigation:** Manual download, or MLX-only path

### Week 2-3 Risks
- **Risk:** CoreML FFI too complex
- **Mitigation:** Fallback to Metal or MLX backend

### Week 4-5 Risks
- **Risk:** Team 1 delay blocks Team 2
- **Mitigation:** Team 2 uses stub backend for parallel progress

### Week 6-7 Risks
- **Risk:** Training integration issues
- **Mitigation:** Focus on basic LoRA only, defer advanced features

### Week 8-10 Risks
- **Risk:** UI-backend integration gaps
- **Mitigation:** Feature flags for incomplete features

### Week 11-12 Risks
- **Risk:** Late bug discoveries
- **Mitigation:** Triage ruthlessly, defer non-critical to v0.4

---

## Weekly Cadence

**Monday:**
- Team standups (15 min each)
- Critical path sync (Team 1 + 2, Weeks 1-7)

**Wednesday:**
- All-hands sync (30 min)
- Demo (rotating teams)

**Friday:**
- Sprint reports (each team)
- Blocker escalation
- Week retrospective

**Bi-weekly (Weeks 2, 4, 6, 8, 10, 12):**
- Stakeholder demo (1 hour)
- Phase completion review

---

## Tracking & Reporting

**GitHub Projects:**
- 70 tasks from FEATURE-INVENTORY.md
- Columns: Backlog, In Progress, In Review, Done
- Labels: critical-path, backend, frontend, security, api, blocked

**Slack Channels:**
- `#aos-v03-alpha` - General coordination
- `#aos-critical-path` - Teams 1-3 daily sync
- `#aos-blockers` - Escalation

**Dashboards:**
- Test coverage (Codecov)
- Build status (GitHub Actions)
- Performance benchmarks (Criterion)

---

**Document Control:**
- **Version:** 1.0
- **Last Updated:** 2025-01-23
- **Next Review:** Week 4 (after Phase 2 kickoff)
- **Related:** [FEATURE-INVENTORY.md](../features/FEATURE-INVENTORY.md), [PRD](../PRD-COMPLETION-V03-ALPHA.md)
