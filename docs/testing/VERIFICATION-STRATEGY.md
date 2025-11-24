# Verification Strategy: AdapterOS v0.3-alpha

**Version:** 1.0
**Date:** 2025-01-23
**Related:** [PRD-COMPLETION-V03-ALPHA.md](../PRD-COMPLETION-V03-ALPHA.md)

---

## Overview

This document defines the testing and quality assurance strategy for AdapterOS v0.3-alpha completion, including test coverage requirements, testing levels, and acceptance criteria per team.

**Goal:** Ensure production-ready quality with ≥80% test coverage and comprehensive integration testing.

---

## Test Coverage Requirements

### By Component

| Component | Unit Tests | Integration Tests | E2E Tests | Coverage Target |
|-----------|-----------|-------------------|-----------|-----------------|
| **Core Backends** | ≥80% | ≥70% | N/A | 80% |
| **Inference Pipeline** | ≥85% | ≥75% | ≥60% | 85% |
| **Training Pipeline** | ≥70% | ≥65% | ≥50% | 70% |
| **Security/Crypto** | ≥95% | ≥90% | ≥70% | 95% |
| **UI Components** | ≥60% | N/A | ≥50% (Cypress) | 60% |
| **API Handlers** | ≥80% | ≥80% | ≥60% | 80% |

**Rationale:**
- **Crypto:** Highest coverage (95%) - security-critical code
- **Inference:** High coverage (85%) - core value proposition
- **UI:** Lower unit coverage (60%) - E2E tests more valuable
- **Training:** Moderate coverage (70%) - ML code harder to test

---

## Testing Levels

### 1. Unit Tests
**Scope:** Per-crate, fast (<1s per test)
**Run:** On every commit (pre-commit hook)
**Tools:** Rust `#[test]`, `cargo test`

**Example:**
```rust
#[test]
fn test_k_sparse_selection() {
    let router = Router::new(k=3);
    let gates = vec![0.9, 0.7, 0.5, 0.3, 0.1];
    let selected = router.select_top_k(&gates);
    assert_eq!(selected, vec![0, 1, 2]); // Indices of top 3
}
```

---

### 2. Integration Tests
**Scope:** Cross-crate, slower (1-10s per test)
**Run:** On every PR (GitHub Actions)
**Tools:** `tests/` directory, `cargo test --test`

**Example:**
```rust
#[tokio::test]
async fn test_inference_end_to_end() {
    let backend = setup_test_backend().await;
    let router = Router::new(k=3);
    let result = router.route_and_infer("Hello world", &backend).await;
    assert!(result.is_ok());
    assert!(result.unwrap().len() > 0);
}
```

---

### 3. E2E Tests (API)
**Scope:** Full system (backend + database), slowest (10-60s per test)
**Run:** Nightly (GitHub Actions)
**Tools:** `axum-test`, `sqlx` test database

**Example:**
```rust
#[tokio::test]
async fn test_training_workflow_e2e() {
    let app = setup_test_app().await;

    // Upload dataset
    let upload_resp = app.post("/v1/datasets/upload")
        .file("file", "test.jsonl")
        .await;
    assert_eq!(upload_resp.status(), 200);

    // Start training
    let train_resp = app.post("/v1/training/start")
        .json(&json!({"dataset_id": "..."}))
        .await;
    assert_eq!(train_resp.status(), 200);

    // Poll until complete
    // ...
}
```

---

### 4. E2E Tests (UI)
**Scope:** Full system (backend + UI), slowest (30-120s per test)
**Run:** On PR (GitHub Actions), nightly (full suite)
**Tools:** Cypress

**Example:**
```typescript
describe('Training Workflow', () => {
  it('should upload dataset and start training', () => {
    cy.visit('/training');
    cy.get('[data-testid="upload-dataset"]').attachFile('test.jsonl');
    cy.get('[data-testid="start-training"]').click();
    cy.contains('Training job started').should('be.visible');
  });
});
```

---

### 5. Stress Tests
**Scope:** Load testing, performance validation
**Run:** Weekly, before release
**Tools:** Custom Rust harness, Apache Bench

**Example:**
```rust
#[test]
fn stress_test_hot_swap() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        for i in 0..1000 {
            swap_adapter("adapter-1", "adapter-2").await;
            assert_latency_under_ms(100);
        }
    });
}
```

---

## Acceptance Criteria by Team

### Team 1: Backend Infrastructure

**Phase 1 (Week 3):**
- [ ] At least 1 backend passes determinism test (10 runs, identical outputs)
- [ ] Inference latency p95 <30ms (target: 24-28ms from README)
- [ ] Memory overhead ≤10% (documented: 8% acceptable)
- [ ] Unit test coverage ≥80% for kernel FFI

**Tests Required:**
1. Determinism test: Same input → Same output (10 iterations)
2. Performance test: Latency p95 <30ms (1000 requests)
3. Memory test: Overhead ≤10% (profile with Instruments)
4. ANE detection test: Verify ANE vs GPU fallback

---

### Team 2: Inference Engine

**Phase 2 (Week 7):**
- [ ] End-to-end inference test passes (prompt → response)
- [ ] Hot-swap stress test: 1000 swaps, 0 failures, p95 <100ms
- [ ] Memory eviction: Triggers at <15% headroom
- [ ] All lifecycle states functional (Unloaded → Resident)
- [ ] Unit test coverage ≥85%

**Tests Required:**
1. E2E inference: Full pipeline (prompt → router → adapters → response)
2. Hot-swap stress: 1000 iterations, measure latency
3. Memory pressure: Load until <15%, verify eviction
4. Lifecycle: Test all state transitions
5. Concurrency: 100 concurrent requests, no deadlocks

---

### Team 3: Training Pipeline

**Phase 2 (Week 7):**
- [ ] Can train adapter from dataset in <5 min (small dataset: 1000 examples)
- [ ] Packaged .aos file loads and runs correctly
- [ ] Training metrics visible in UI (real-time)
- [ ] GPU utilization >80% during training
- [ ] Unit test coverage ≥70%

**Tests Required:**
1. E2E training: Dataset → Train → Package → Register
2. Performance: Training time <5 min for 1000 examples
3. GPU utilization: Monitor with `nvidia-smi` or MLX profiler
4. .aos round-trip: Write → Read → Verify integrity
5. Metrics: Verify telemetry events emitted

---

### Team 4: Security & Crypto

**Phase 3 (Week 10):**
- [ ] 2+ KMS providers functional (AWS + GCP minimum)
- [ ] SEP attestation works on macOS or graceful fallback
- [ ] Key rotation tested (90-day rotation)
- [ ] No TODO/FIXME in crypto code
- [ ] Unit test coverage ≥95%

**Tests Required:**
1. KMS encrypt/decrypt: Round-trip with real AWS/GCP
2. SEP attestation: Generate key → Attest → Verify chain
3. Rotation daemon: Set 1-min rotation, verify keys rotated
4. Error handling: Network failures, rate limits
5. Security audit: External or internal review

---

### Team 5: Frontend Integration

**Phase 3 (Week 10):**
- [ ] All 25+ pages render without console errors
- [ ] 10+ Cypress E2E tests passing
- [ ] No mock data in production build
- [ ] Real-time updates working (SSE for metrics/training)
- [ ] API errors display user-friendly messages

**Tests Required:**
1. Smoke tests: All pages load (Cypress)
2. E2E workflows: Login → Dashboard → Training → Inference
3. Real-time: SSE updates reflect in UI <500ms
4. Error handling: API 4xx/5xx → User-friendly message
5. Accessibility: WCAG 2.1 AA compliance (axe-core)

---

### Team 6: API & Integration

**Phase 4 (Week 12):**
- [ ] 189 endpoints documented in OpenAPI
- [ ] 100+ integration tests passing
- [ ] API response times p95 <200ms
- [ ] RBAC enforced on all protected endpoints
- [ ] Unit test coverage ≥80%

**Tests Required:**
1. OpenAPI validation: All endpoints match spec
2. Integration suite: 100+ tests covering all endpoints
3. Performance: p95 <200ms (load test with 100 concurrent users)
4. RBAC: Verify 40 permissions × 5 roles enforced
5. Audit logs: Verify all operations logged

---

## Pre-Release Gates

### Phase 1 Gate (Week 3)
- [ ] Workspace builds clean on CI
- [ ] At least 1 backend functional
- [ ] 29 `lora-worker` tests passing

**Decision:** Proceed to Phase 2 ONLY if backend is functional.

---

### Phase 2 Gate (Week 7)
- [ ] Inference pipeline complete (H1-H8)
- [ ] Training pipeline complete (T1-T8)
- [ ] Integration tests passing

**Decision:** Proceed to Phase 3 ONLY if core workflows work.

---

### Phase 3 Gate (Week 10)
- [ ] UI integration complete (U1-U15)
- [ ] 10+ E2E tests passing
- [ ] Real-time updates working

**Decision:** Proceed to Phase 4 (testing) ONLY if integration is complete.

---

### Release Gate (Week 12)
- [ ] All 189 API endpoints documented
- [ ] <5 CRITICAL severity bugs
- [ ] ≥80% test coverage (production code)
- [ ] Performance benchmarks met:
  - Inference: ≥40 tok/s
  - Training: <5 min (small datasets)
  - Hot-swap: <100ms
- [ ] Documentation updated:
  - CLAUDE.md (mark In Development → Complete)
  - README.md (status badges, benchmarks)
  - Changelog generated

**Decision:** Ship v0.3-alpha release candidate.

---

## Regression Prevention

### Continuous Monitoring
1. **Test Coverage:** ✅ Track per PR via Codecov (configured in CI)
   - Coverage thresholds enforced per component (80/85/95%)
   - Script: `scripts/check_coverage.py`
   - Workflow: `.github/workflows/integration-tests.yml` (coverage job)
2. **Benchmark Regression:** ✅ Criterion baseline comparison, fail if >10% slower
   - Script: `scripts/compare_benchmarks.py`
   - Workflow: `.github/workflows/performance-regression.yml`
3. **Memory Leaks:** Valgrind on Linux, Instruments on macOS (weekly)
4. **Fuzz Testing:** Existing `fuzz/` crate (nightly)
5. **E2E Testing:** ✅ Cypress tests automated in CI
   - Workflow: `.github/workflows/e2e-ui-tests.yml`
6. **Stress Testing:** ✅ Weekly automated stress tests
   - Workflow: `.github/workflows/stress-tests.yml`
   - Script: `scripts/collect_stress_results.sh`

### Pre-Commit Hooks
**Status:** ✅ Implemented

Pre-commit hooks are now available. Install with:
```bash
./scripts/setup_pre_commit.sh
```

The hook performs:
- Format check: `cargo fmt --all -- --check`
- Lint check: `cargo clippy --workspace -- -D warnings`
- Fast unit tests: `cargo test --workspace --lib --quiet`

**Location:** `scripts/pre-commit-template` (template), `.git/hooks/pre-commit` (installed)

**Bypass:** `git commit --no-verify` (not recommended)

---

## Test Infrastructure

### GitHub Actions Workflows
1. **CI:** On every commit
   - Build workspace
   - Run unit tests
   - Run linters

2. **Integration:** On PR
   - Run integration tests
   - Check test coverage (fail if <80%)

3. **E2E:** Nightly
   - Run E2E API tests
   - Run Cypress tests
   - Run stress tests

4. **Performance:** Weekly
   - Run benchmark suite (Criterion)
   - Generate performance report

### Test Database
- **SQLite in-memory:** For fast unit tests
- **Persistent SQLite:** For integration tests (reset between tests)
- **Fixtures:** Seed data for reproducible tests

### Mock Backends
- **Stub backends:** For Linux CI (no Metal/CoreML)
- **Deterministic responses:** Same input → Same output

---

## Verification Checklist

### Backend Features (C1-C4, H1)
- [ ] Unit tests pass: `cargo test -p adapteros-lora-kernel-coreml`
- [ ] Integration test: Load model → Run inference → Verify determinism
- [ ] Benchmark: Tokens/sec ≥40 on M3 Max
- [ ] Memory: VRAM usage ≤18GB (K=5 config)
- [ ] Attestation: Backend determinism validation passes

### Inference Pipeline (H2-H8)
- [ ] E2E test: Prompt → Router → Adapters → Response
- [ ] Hot-swap stress: 1000 swaps, 0 panics
- [ ] Memory pressure: Auto-eviction at <15% headroom
- [ ] Lifecycle: All state transitions verified
- [ ] Heartbeat: Stale adapter reset after 5-min timeout
- [ ] Streaming: SSE events received
- [ ] Concurrency: 100 concurrent requests, no deadlocks

### Training Pipeline (T1-T12)
- [ ] E2E test: Dataset upload → Train → Package → Register
- [ ] Performance: Training <5 min (small dataset)
- [ ] .aos packaging: Round-trip test passes
- [ ] UI: Training jobs page shows real data
- [ ] Metrics: Loss/accuracy logged and queryable
- [ ] GPU: Utilization >80% during training

### Security (S1-S9)
- [ ] KMS test: AWS + GCP encrypt/decrypt works
- [ ] SEP attestation: Works on macOS or graceful fallback
- [ ] Rotation: Keys rotated on schedule
- [ ] Audit: All crypto operations logged
- [ ] Policy: Patch crypto validates against 28 packs

### UI Integration (U1-U15)
- [ ] Cypress E2E: All pages load without errors
- [ ] Data binding: UI shows real backend data (no mocks)
- [ ] Real-time: SSE updates <500ms
- [ ] Error handling: API errors → User-friendly messages
- [ ] Responsive: Works on 1920x1080, 1366x768, 2880x1800
- [ ] Accessibility: WCAG 2.1 AA (axe-core passes)
- [ ] Performance: Page load <2s, interactions <100ms

### API Endpoints (A1-A7)
- [ ] OpenAPI: All 189 endpoints match spec
- [ ] Integration: All endpoints return correct status codes
- [ ] RBAC: Permissions enforced (40 permissions × 5 roles)
- [ ] Audit: All operations logged
- [ ] Performance: p95 <200ms
- [ ] Error handling: 4xx/5xx include structured errors

---

## Test Metrics Dashboard

**Track Weekly:**
- Test coverage % (by crate)
- Test pass rate (%)
- Failed test count
- Skipped/ignored test count
- Performance benchmarks (tokens/sec, latency)
- Bug count by severity (CRITICAL, HIGH, MEDIUM, LOW)

**Report Format:**
| Week | Coverage | Pass Rate | Failed | Critical Bugs | Perf (tok/s) |
|------|----------|-----------|--------|---------------|--------------|
| 3    | 65%      | 95%       | 12     | 2             | 38           |
| 7    | 75%      | 97%       | 6      | 1             | 42           |
| 10   | 82%      | 98%       | 3      | 0             | 45           |
| 12   | 85%      | 99%       | 1      | 0             | 47           |

---

**Document Control:**
- **Version:** 1.0
- **Last Updated:** 2025-01-23
- **Next Review:** Week 4 (after Phase 2 kickoff)
- **Related:** [FEATURE-INVENTORY.md](../features/FEATURE-INVENTORY.md), [PRD](../PRD-COMPLETION-V03-ALPHA.md)
