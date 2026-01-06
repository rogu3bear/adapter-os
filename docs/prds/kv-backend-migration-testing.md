# PRD: KV Backend Migration Testing

**Status:** Draft
**Last Updated:** 2026-01-05
**Owner:** Engineering

---

## Problem

The KV backend (redb) migration path (SqlOnly -> DualWrite -> KvPrimary) lacks a single end-to-end test that exercises the full workflow and validates rollback behavior under KV failures. Current tests cover individual behaviors but do not provide a cohesive migration workflow, and there is no CI job dedicated to KV migration coverage.

## Non-Goals

- Completing the full SqlOnly -> KvOnly cutover.
- Performance tuning beyond a baseline KV read benchmark.
- Reworking the storage architecture or migration tooling.

## Proposed Approach

1. **Migration workflow test (Phase 1):**
   - Add an integration test that walks SqlOnly -> DualWrite -> KvPrimary using the existing `Db` storage mode APIs and KV helpers.
   - Verify SqlOnly does not write to KV, DualWrite writes to both, and KvPrimary reads succeed for KV-backed data.

2. **Strict rollback test (Phase 1):**
   - Add a test-only KV backend wrapper that injects write failures.
   - Run DualWrite in strict mode and confirm SQL rollbacks when KV writes fail.

3. **CI coverage (Phase 2):**
   - Add or formalize a `kv-backend` feature flag in `adapteros-db` if needed.
   - Add a CI job that runs KV tests (including ignored tests, if any) with KV backend enabled.

4. **Benchmark (Phase 2):**
   - Extend the existing KV vs SQL benchmark to enforce a baseline threshold for adapter lookup.

## Acceptance Criteria

### Phase 1 (this PRD slice)
- Migration workflow test covers SqlOnly -> DualWrite -> KvPrimary behavior in a single test.
- Strict dual-write rollback test verifies SQL does not commit when KV writes fail.

### Phase 2 (follow-up tasks)
- `cargo test -p adapteros-db --features kv-backend -- --ignored` passes.
- CI runs KV migration tests on every PR.
- KV read benchmark is documented and tracked (target: <1ms for adapter lookup).

## Test Plan

- `cargo test -p adapteros-db --test atomic_dual_write_tests`
- `cargo test -p adapteros-db --test kv_integration`
- Future: `cargo test -p adapteros-db --features kv-backend -- --ignored`

## Rollout Plan

1. Land Phase 1 tests in default test suite.
2. Add CI job for KV migration tests behind feature gating.
3. Monitor test runtime and failure rate; adjust gating or timeouts as needed.
4. Add benchmark threshold and track regressions over time.

## Risks and Open Questions

- The repo currently lacks an explicit `kv-backend` feature flag; confirm intended gating strategy.
- Injected KV failure tests rely on a test-only backend wrapper; validate this aligns with storage error semantics.
