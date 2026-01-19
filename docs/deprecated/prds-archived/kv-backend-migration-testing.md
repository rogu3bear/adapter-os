# PRD: Complete KV Backend Migration Testing

**Status:** Draft
**Last Updated:** 2026-01-05
**Owner:** Engineering
**Related Docs:** `crates/adapteros-db/tests/atomic_dual_write_tests.rs`, `.github/workflows/ci.yml`

---

## 1. Summary

The KV backend (redb) is intended to support a phased migration from SQLite, but end-to-end tests for the migration workflow are ignored. This PRD defines the test coverage and CI wiring required to validate SqlOnly -> DualWrite -> KvPrimary transitions.

---

## 2. Problem Statement

Migration to the KV backend lacks integration tests for atomic dual-write behavior and KV fallback. As a result, regressions can slip into production without confidence in the migration workflow.

---

## 3. Goals

- Enable deterministic integration tests for each storage mode.
- Validate atomic rollback when KV writes fail.
- Verify KV read fallback behavior during KvPrimary mode.
- Ensure CI executes KV backend tests.

---

## 4. Non-Goals

- Production rollout of KvPrimary mode.
- Performance tuning of redb beyond baseline benchmarks.
- Replacing existing SQLite tests.

---

## 5. Proposed Approach

- Un-ignore KV migration tests and run them behind `kv-backend` feature.
- Add a test helper to set up temporary KV stores for dual-write tests.
- Add explicit failure injection hooks for KV write errors (test-only).
- Add a CI job that runs KV tests on supported runners.

---

## 6. Acceptance Criteria

- `cargo test -p adapteros-db --features kv-backend -- --ignored` passes in CI.
- Migration workflow test covers SqlOnly, DualWrite (best-effort and strict), and KvPrimary.
- Atomic rollback is verified when KV writes fail.
- KV fallback reads are exercised when KV is unavailable.

---

## 7. Test Plan

- Integration test for full migration workflow with a temp redb store.
- Negative test for KV write failure to ensure SQL rollback.
- Benchmark test to compare KV read latency against SQLite (target <1 ms for adapter lookup).

---

## 8. Rollout Plan

1. Land test-only KV failure injection and helper utilities.
2. Enable KV migration tests in CI with feature flag.
3. Monitor CI stability and adjust timeouts/fixtures if needed.

---

## 9. Follow-up Tasks (Tracked)

- TASK-1: Implement migration workflow integration test.
  - Acceptance: asserts all four migration phases and data consistency.
- TASK-2: Add KV failure injection for dual-write strict mode.
  - Acceptance: SQL rollback verified on simulated KV error.
- TASK-3: Add CI job for `kv-backend` tests.
  - Acceptance: CI runs tests on every PR with feature enabled.
