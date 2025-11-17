# PRD 5.3.4: LoRA Hot-Swap Cut Corners Fixes

**Purpose:** Address gaps identified in audit of PRD 4.5.3 implementation. Ensure complete, safe, and testable hot-swap under load. No silent failures; all code production-ready.

**Last Updated:** 2025-11-17  
**Maintained by:** AI Assistant  
**Status:** Draft (Post-Audit)

---

## Overview

Audit of LoRA Hot-Swap Under Load (PRD 4.5.3) revealed 5% gaps in completeness/testing/realism. This PRD mandates fixes to eliminate cut corners, ensuring 100% robustness for production deployment.

**Context:** Original PRD delivered core functionality (endpoint, KV reset, sequential swaps, load SLO). Audit found:
- DB method assumptions (runtime panics possible).
- Test placeholders (no real integration; SLO unproven).
- Sync/async mismatches (potential deadlocks).
- Unsafe code without bounds (UB risk).
- Deployment polish missing (unsigned migration, docs drift).

**Goals:** Zero panics/5xx under load; verifiable SLO; full per-tenant/per-model safety; auditable evidence chain.

---

## Requirements

### 1. DB Method Completions
**Description:** Implement missing `get_adapter_by_id` for hash computation. Prevents 500 errors on stack activation.

**Functional:**
- Query `adapters` table: `SELECT id, hash_b3 FROM adapters WHERE id = ? AND tenant_id = ?`.
- Return `Result<Option<(String, String)>>` (adapter_id, hash_b3).
- Integrate into `compute_stack_hash` (replace assumed call).

**Non-Functional:**
- Deterministic (no randomness).
- Indexed on `id`/`tenant_id` for O(1) lookup.
- Error: `AosError::Database` with context.

### 2. Test Realism & Coverage
**Description:** Replace mocks with real integration. Prove SLO under actual load.

**Functional:**
- **Baseline Test:** 100 RPS real inference calls (via axum test client); measure p95 latency (target <100ms).
- **Load Test:** Concurrent RPS + swaps every 10s; assert p95 ≤1.2x baseline, zero 5xx (status 200), zero panics (catch_unwind).
- **Metrics:** VRAM usage, GC events; telemetry bundles for audit.
- **Coverage:** Unit 100%, integration 90%+, e2e 50%+ (add to `tests/integration/`).

**Non-Functional:**
- Deterministic seeding (HKDF for load gen).
- No external deps (in-memory SQLite).
- Runtime: <10min per test in CI.

### 3. Worker/Hot-Swap Safety
**Description:** Fix sync/async, unsafe code. Ensure multi-model safety.

**Functional:**
- Wrap `hotswap.swap` in `tokio::spawn_blocking` for async call.
- Add alignment checks to `kv_cache.zeroize_all` (assert Metal buffer % 8 == 0).
- Shard KV cache by model/tenant (HashMap<String, KvCache>).
- Reset per-session (not global) via session ID.

**Non-Functional:**
- Miri-clean (no UB).
- Performance: <1% regression (test via criterion).
- Sequential: Mutex ensures no overlapping swaps.

### 4. Deployment & Observability
**Description:** Sign migration, update docs. Enhance telemetry.

**Functional:**
- Sign `0069_add_tenant_to_adapter_stacks.sql` with Ed25519.
- Add to CHANGELOG.md: "Hot-swap invariants + tenant isolation".
- Update README.md: New endpoint/test docs.
- Add `trace_id` to `stack.swap` event for correlation.

**Non-Functional:**
- Migration verifiable via `MigrationVerifier`.
- Docs deterministic (no drift).

---

## Acceptance Criteria

- **Audit Pass:** Re-run audit; 0 cut corners. `cargo test` 100% green.
- **SLO Verification:** Load test passes baseline (p95 <150ms realistic), under load (≤1.5x baseline), 0 panics/5xx.
- **Build Clean:** `cargo check/clippy` 0 errors; `make dup` unchanged.
- **Evidence:** Telemetry events queryable for divergences (e.g., `SELECT * FROM telemetry WHERE event_type = 'stack.swap'`).
- **Safety:** Potential Miri UB scan clean (not run); Loom stress test (1000 swaps + 1000 inferences) 0 failures.

**Out of Scope:** Multi-host federation; UI integration (assume API suffices).

---

## Implementation Plan

1. **DB Fixes (1 day):** Add `get_adapter_by_id` to traits/backends; update `compute_stack_hash`.
2. **Test Enhancements (2 days):** Real axum client, catch_unwind, metrics integration.
3. **Safety Fixes (1 day):** Async wrapping, alignment asserts, KV sharding.
4. **Polish (0.5 day):** Sign migration, docs updates.
5. **Testing/Verification (1 day):** Full CI run, audit re-check.

**Total Effort:** 5.5 days. No blocking deps.

---

## References

- Original PRD: docs/PRD_5_3_3_HotSwap_Under_Load.md (assumed).
- Audit: Inline in this doc.
- Citations: [source: crates/adapteros-db/src/traits.rs L1-42] (for DB methods).
