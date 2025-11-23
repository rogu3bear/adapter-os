# Team Charters: AdapterOS v0.3-alpha Completion

**Version:** 1.0
**Date:** 2025-01-23
**Related:** [PRD-COMPLETION-V03-ALPHA.md](../PRD-COMPLETION-V03-ALPHA.md)

---

## Overview

This document defines the responsibilities, deliverables, and coordination model for the 6-7 specialized teams completing AdapterOS v0.3-alpha.

**Total Team Size:** 15-19 engineers across 6 core teams + 1 optional support team
**Timeline:** 12 weeks (4 phases)
**Coordination Model:** See [AGENT-COORDINATION.md](AGENT-COORDINATION.md)

---

## Team 1: Backend Infrastructure

### Overview
**Size:** 3 engineers
**Timeline:** Weeks 1-3 (critical path), Weeks 4-12 (support/optimization)
**Priority:** CRITICAL - Blocks Teams 2 & 3

### Team Composition
- **Lead:** Senior Rust engineer with Metal/GPU experience
- **Engineer 2:** iOS/macOS engineer with CoreML/ANE expertise
- **Engineer 3:** ML engineer with MLX/Python FFI experience

### Deliverables

#### Critical (Weeks 1-3)
| ID | Task | Lines of Code | Complexity | Dependencies |
|----|------|---------------|------------|--------------|
| **C1** | CoreML FFI Bridge (`coreml_bridge.mm`) | ~500-800 | XL | Objective-C++, CoreML framework |
| **C2** | MLX Backend (real-mlx feature) | ~300-500 | L | MLX C++ library |
| **C3** | MLX C++ Wrapper (replace stubs) | ~600-800 | XL | MLX installation, Python bindings |
| **C4** | ANE Execution Path | ~400-600 | XL | CoreML MLProgram, Metal shaders |
| **H1** | Metal Kernel Compilation | ~100-200 | M | Xcode MetalToolchain |

**Total Estimated LOC:** ~1,900-2,900 lines

#### Success Criteria (Phase 1 Gate)
- [ ] At least 1 backend (CoreML OR MLX) passes determinism test
- [ ] Inference latency p95 <30ms (target: 24-28ms)
- [ ] Memory overhead ≤10% (documented: 8% acceptable)
- [ ] Workspace builds without Metal toolchain errors

### Skills Required
- **Must Have:**
  - Rust async/await, FFI patterns
  - Metal/GPU programming
  - macOS frameworks (CoreML, Metal)
  - Performance optimization

- **Nice to Have:**
  - MLX library experience
  - Neural Engine (ANE) optimization
  - Python bindings (PyO3)

### Coordination
- **Daily Standup:** With Team 2 (Inference Engine)
- **Weekly Sync:** With Team 6 (integration testing)
- **Handoff:** Week 3 → Team 2 (backend ready for integration)

### Blockers & Dependencies
- **External Dependency:** Metal Toolchain download (Week 1, Team 7 assists)
- **Blocks:** Teams 2 & 3 cannot proceed without at least 1 working backend

---

## Team 2: Inference Engine

### Overview
**Size:** 3-4 engineers
**Timeline:** Weeks 4-7 (critical path), Weeks 8-12 (optimization)
**Priority:** CRITICAL - Core value proposition

### Team Composition
- **Lead:** Senior Rust async engineer
- **Engineer 2:** GPU/Metal specialist
- **Engineer 3:** Testing engineer (integration/stress tests)
- **Engineer 4 (Optional):** Performance engineer

### Deliverables

#### Critical (Weeks 4-7)
| ID | Task | Lines of Code | Complexity | Dependencies |
|----|------|---------------|------------|--------------|
| **H2** | Router Integration Tests | ~200-300 | M | Backend (Team 1) |
| **H3** | K-Sparse Selection (test) | ~150-200 | S | None |
| **H4** | Hot-Swap Integration Tests | ~300-400 | M | Backend, lifecycle |
| **H5** | Memory Pressure Testing | ~200-300 | M | Metal runtime |
| **H6** | Streaming Inference (SSE) | ~250-350 | M | Backend |
| **H7** | Adapter Lifecycle Transitions | ~300-400 | M | Database, memory mgr |
| **H8** | Lifecycle Heartbeat Recovery | ~100-150 | S | Database triggers |

**Total Estimated LOC:** ~1,500-2,100 lines

#### Secondary (Weeks 8-12)
- Performance tuning (cache hit rates, VRAM optimization)
- Stress testing (1000 concurrent requests, 1000 hot-swaps)
- Profiling (Instruments, perf)

#### Success Criteria (Phase 2 Gate)
- [ ] End-to-end inference test passes (prompt → response)
- [ ] Hot-swap stress test: 1000 swaps, 0 failures
- [ ] Memory eviction triggers correctly at <15% headroom
- [ ] All lifecycle state transitions functional (Unloaded → Resident)
- [ ] 29 `lora-worker` tests now passing

### Skills Required
- **Must Have:**
  - Rust async/tokio
  - Concurrency testing (stress tests, race conditions)
  - Memory management (VRAM, eviction policies)

- **Nice to Have:**
  - Apple Instruments profiling
  - Distributed systems (for federation prep)

### Coordination
- **Daily Standup:** With Team 1 (backend integration blockers)
- **Weekly Sync:** With Team 3 (lifecycle integration), Team 5 (UI streaming)
- **Handoff:** Week 7 → Team 3 (inference pipeline ready for training)

### Blockers & Dependencies
- **Blocked By:** Team 1 (needs working backend)
- **Blocks:** Team 3 (training depends on inference pipeline)

---

## Team 3: Training Pipeline

### Overview
**Size:** 2-3 engineers
**Timeline:** Weeks 6-12
**Priority:** HIGH - Core value proposition

### Team Composition
- **Lead:** ML engineer with LoRA expertise
- **Engineer 2:** Backend Rust engineer
- **Engineer 3 (Optional):** Data engineer

### Deliverables

#### Critical (Weeks 6-9)
| ID | Task | Lines of Code | Complexity | Dependencies |
|----|------|---------------|------------|--------------|
| **T1** | Dataset Upload/Validation | ~150-200 | S | None |
| **T2** | Chunked Upload Handler | ~200-300 | M | Storage backend |
| **T3** | MicroLoRATrainer Integration | ~400-600 | L | Backend, optimizer |
| **T4** | Training Job Management | ~300-400 | M | Job scheduler |
| **T5** | Progress Tracking (SSE) | ~150-200 | S | SSE infrastructure |
| **T6** | Hyperparameter Templates | ~100-150 | S | None |
| **T7** | Model Packaging (.aos) | ~300-400 | M | Compression, validation |
| **T8** | Registry Integration | ~150-200 | S | Database |
| **T9** | Training Metrics Collection | ~200-300 | M | Telemetry, database |
| **T10** | GPU Training Support | ~250-350 | L | MLX backend (Team 1) |
| **T11** | Training UI Data Binding | ~200-300 | M | API handlers |
| **T12** | Training Templates UI | ~100-150 | S | API endpoint |

**Total Estimated LOC:** ~2,500-3,500 lines

#### Success Criteria (Phase 2 Gate)
- [ ] Can train adapter from dataset in <5 min (small dataset)
- [ ] Packaged .aos file loads and runs correctly
- [ ] Training metrics visible in UI (real-time)
- [ ] GPU utilization >80% during training

### Skills Required
- **Must Have:**
  - ML fundamentals (LoRA, fine-tuning)
  - Rust backend development
  - Dataset management (validation, chunking)

- **Nice to Have:**
  - PyTorch/MLX training
  - Distributed training (for federation prep)

### Coordination
- **Weekly Sync:** With Team 2 (inference integration), Team 5 (UI), Team 6 (API testing)
- **Handoff:** Week 9 → Team 5 (training UI ready for data binding)

### Blockers & Dependencies
- **Blocked By:** Team 2 (inference pipeline), Team 1 (MLX backend for GPU training)
- **Blocks:** None (Team 5 UI work can proceed in parallel with stubs)

---

## Team 4: Security & Crypto

### Overview
**Size:** 2 engineers
**Timeline:** Weeks 1-12 (parallel workstream)
**Priority:** HIGH - Production requirement

### Team Composition
- **Lead:** Security engineer with PKI/HSM experience
- **Engineer 2:** Cloud engineer (AWS/GCP/Azure SDKs)

### Deliverables

#### Critical (Weeks 1-8)
| ID | Task | Lines of Code | Complexity | Dependencies |
|----|------|---------------|------------|--------------|
| **S1** | AWS KMS Provider | ~300-400 | M | AWS SDK |
| **S2** | GCP KMS Provider | ~300-400 | M | GCP client libs |
| **S3** | Azure Key Vault | ~300-400 | M | Azure SDK |
| **S4** | HashiCorp Vault | ~250-350 | M | Vault client |
| **S5** | PKCS#11 HSM | ~400-600 | L | HSM drivers |
| **S6** | Secure Enclave SEP Attestation | ~300-500 | L | macOS Security framework |
| **S7** | Key Lifecycle Creation Date | ~50-100 | S | macOS Security API |
| **S8** | Rotation Daemon KMS | ~200-300 | M | KMS providers (S1-S5) |
| **S9** | Patch Crypto/Audit Modules | ~300-400 | M | Policy engine |

**Total Estimated LOC:** ~2,400-3,450 lines

#### Prioritization
1. **Week 1-4:** S1 (AWS KMS) + S6 (Secure Enclave)
2. **Week 5-8:** S2 (GCP KMS) + S8 (Rotation Daemon)
3. **Week 9-12:** S3-S5 (Azure, Vault, PKCS#11) + S9 (Patch modules)

#### Success Criteria
- [ ] 2+ KMS providers functional (AWS + GCP minimum)
- [ ] SEP attestation works on macOS or graceful fallback
- [ ] Key rotation daemon tested (90-day rotation)
- [ ] No TODO/FIXME in crypto code
- [ ] ≥95% test coverage for crypto code

### Skills Required
- **Must Have:**
  - Cryptography fundamentals (PKI, HSM, key management)
  - Cloud provider SDKs (AWS/GCP/Azure)
  - macOS Security framework

- **Nice to Have:**
  - FIPS 140-2 compliance
  - Hardware security modules (HSM)

### Coordination
- **Weekly Sync:** With Team 6 (integration testing), all teams (security reviews)
- **Independent:** No critical path dependencies

### Blockers & Dependencies
- **Blocked By:** None (independent workstream)
- **Blocks:** None (nice-to-have for v0.3, required for production)

---

## Team 5: Frontend Integration

### Overview
**Size:** 2-3 engineers
**Timeline:** Weeks 3-12
**Priority:** MEDIUM - User experience

### Team Composition
- **Lead:** Senior React/TypeScript engineer
- **Engineer 2:** Full-stack engineer
- **Engineer 3 (Optional):** UX engineer

### Deliverables

#### Critical (Weeks 8-10)
| ID | Task | Lines of Code | Complexity | Dependencies |
|----|------|---------------|------------|--------------|
| **U1** | Training Jobs Page Data Binding | ~150-200 | S | T4, T5 |
| **U2** | Metrics Dashboard Real-time Updates | ~300-400 | M | SSE, telemetry |
| **U3** | Adapter Detail Lifecycle Visualization | ~250-350 | M | H7, database |
| **U4** | Hot-Swap UI Live Status | ~100-150 | S | H4 |
| **U5** | Policy Management UI | ~200-300 | M | Policy API |
| **U6** | Golden Runs UI | ~200-300 | M | Golden run logic |
| **U7** | Replay UI | ~150-200 | M | Replay system |
| **U8** | Telemetry Viewer | ~200-300 | M | Telemetry API |
| **U9** | Router Config UI | ~100-150 | S | Router API |
| **U10** | System Metrics GPU Stats | ~150-200 | M | GPU monitoring |
| **U11** | Alerts/Notifications | ~200-300 | M | Alerting backend |
| **U12** | Git Integration Page | ~150-200 | M | Git subsystem |
| **U13** | Contacts/Discovery Pages | ~100-150 | S | Discovery protocol |
| **U14** | Federation UI | ~300-400 | L | Federation backend |
| **U15** | Base Models Page | ~200-300 | M | Model import logic |

**Total Estimated LOC:** ~2,750-3,900 lines

#### Success Criteria (Phase 3 Gate)
- [ ] All 25+ pages render without console errors
- [ ] 10+ Cypress E2E tests passing
- [ ] No mock data in production build
- [ ] Real-time updates working (SSE for metrics/training)
- [ ] API errors display user-friendly messages

### Skills Required
- **Must Have:**
  - React/TypeScript
  - SSE/WebSocket real-time updates
  - Cypress E2E testing

- **Nice to Have:**
  - D3.js/Recharts (data visualization)
  - Accessibility (WCAG 2.1 AA)

### Coordination
- **Weekly Sync:** With all backend teams (API integration)
- **Daily Standup:** Internal team (UI consistency)

### Blockers & Dependencies
- **Blocked By:** Teams 2, 3, 4 (API endpoints must exist)
- **Blocks:** None (can work with stub APIs in parallel)

---

## Team 6: API & Integration

### Overview
**Size:** 2 engineers
**Timeline:** Weeks 1-12 (supports all teams)
**Priority:** HIGH - Quality gatekeeper

### Team Composition
- **Lead:** Backend Rust engineer
- **Engineer 2:** DevOps/Testing engineer

### Deliverables

#### Critical (Weeks 1-12)
| ID | Task | Lines of Code | Complexity | Dependencies |
|----|------|---------------|------------|--------------|
| **A1** | Process Alerts Handlers | ~150-200 | S | Alerting backend |
| **A2** | Process Anomalies Handlers | ~200-300 | M | Anomaly detection |
| **A3** | Monitoring Dashboards API | ~200-300 | M | Dashboard config |
| **A4** | Database Incidents Module | ~100-150 | S | Incident schema |
| **A5** | Chunked Upload Handler | ~200-300 | M | Storage backend |
| **A6** | Model Import Handler | ~300-400 | L | MLX backend |
| **A7** | Unwired Handlers (9 modules) | ~400-600 | M | Various |

**Total Estimated LOC:** ~1,550-2,250 lines

#### Additional Responsibilities
- **Integration Test Suite:** 100+ tests covering all 189 endpoints
- **OpenAPI Validation:** Ensure API spec matches implementation
- **Performance Testing:** API response times p95 <200ms
- **RBAC Enforcement:** Verify 40 permissions × 5 roles

#### Success Criteria
- [ ] 189 endpoints documented in OpenAPI
- [ ] 100+ integration tests passing
- [ ] API response times p95 <200ms
- [ ] RBAC enforced on all protected endpoints

### Skills Required
- **Must Have:**
  - Rust backend (Axum framework)
  - Integration testing (test infrastructure)
  - API design (REST, OpenAPI)

- **Nice to Have:**
  - Performance testing (load testing, profiling)
  - CI/CD (GitHub Actions)

### Coordination
- **Daily Standup:** With critical path team (Team 1 or 2)
- **Weekly Sync:** With all teams (integration testing coordination)
- **Role:** Supports all teams, owns integration testing

### Blockers & Dependencies
- **Blocked By:** All teams (needs domain logic from Teams 2-5)
- **Blocks:** None (enables all teams via integration tests)

---

## Team 7: Platform & Tooling (Optional)

### Overview
**Size:** 1-2 engineers
**Timeline:** Weeks 1-12 (support role)
**Priority:** LOW - Nice to have

### Team Composition
- **Lead:** DevOps engineer
- **Engineer 2 (Optional):** Rust tooling engineer

### Deliverables

#### Week 1 (Critical)
- Metal Toolchain automation (`xcodebuild -downloadComponent MetalToolchain`)
- CI pipeline fixes (GitHub Actions)

#### Weeks 2-12 (Support)
- Test infrastructure (parallel test execution)
- Benchmark harness (Criterion with historical tracking)
- Build optimization (caching, incremental builds)
- Developer tooling (`make` shortcuts, scripts)

#### Success Criteria
- [ ] Metal toolchain setup automated (one-command)
- [ ] CI builds <10 minutes (currently: 15-20 min)
- [ ] Benchmarks run nightly with trend analysis

### Skills Required
- **Must Have:**
  - CI/CD (GitHub Actions, caching)
  - Shell scripting (Bash, Make)
  - Rust tooling (Cargo, rustc)

- **Nice to Have:**
  - Docker/containers
  - Monitoring (Prometheus, Grafana)

### Coordination
- **Weekly Sync:** With all teams (tooling support)
- **Role:** Unblocks teams, improves developer experience

### Blockers & Dependencies
- **Blocked By:** None
- **Blocks:** None (supports all teams)

---

## Summary Table

| Team | Size | Weeks Active | LOC Estimate | Critical Path |
|------|------|--------------|--------------|---------------|
| **Team 1: Backend** | 3 | 1-12 (critical 1-3) | 1,900-2,900 | Yes |
| **Team 2: Inference** | 3-4 | 4-12 (critical 4-7) | 1,500-2,100 | Yes |
| **Team 3: Training** | 2-3 | 6-12 | 2,500-3,500 | Yes |
| **Team 4: Security** | 2 | 1-12 | 2,400-3,450 | No |
| **Team 5: Frontend** | 2-3 | 3-12 | 2,750-3,900 | No |
| **Team 6: API** | 2 | 1-12 | 1,550-2,250 | No |
| **Team 7: Platform** | 1-2 (optional) | 1-12 | ~500-1,000 | No |
| **Total** | **15-19** | **12 weeks** | **13,100-19,100** | **3 teams** |

---

**Document Control:**
- **Version:** 1.0
- **Last Updated:** 2025-01-23
- **Next Review:** Week 2 (after Phase 1 kickoff)
- **Related:** [AGENT-COORDINATION.md](AGENT-COORDINATION.md), [PRD](../PRD-COMPLETION-V03-ALPHA.md)
