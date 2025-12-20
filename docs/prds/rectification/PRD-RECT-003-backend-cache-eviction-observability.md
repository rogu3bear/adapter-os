# PRD-RECT-003: Backend Cache — Eviction Predictability + Observability

## Problem / Motivation

The worker has a per-process model handle cache (`ModelHandleCache`) intended to deduplicate loads and control memory pressure. Drift tracking marks eviction behavior and UI/telemetry exposure as **unverified** (`plan/drift-summary.md:11`).

This PRD defines the eviction/observability contract and adds the missing instrumentation/tests so we can demo and operate the cache confidently.

## Goals

- Make cache eviction **predictable and explainable** (clear ordering + reasons).
- Emit telemetry/metrics on:
  - load, reuse
  - eviction
  - eviction blocked (pinned/active)
  - eviction budget exceeded
- Add tests that verify eviction ordering and blocked-eviction behavior.

## Non-Goals

- Build a full UI page for cache internals.
- Introduce cross-process or control-plane global model cache changes.

## Requirements

### R1. Stable eviction policy

Document and enforce a stable eviction ordering that does not depend on hashmap iteration order.

Acceptable policy options:

- True LRU: `last_used_seq` (monotonic counter) ascending, then `access_count`, then `ModelKey` as a final tie-break.
- If keeping current “oldest loaded” policy, add deterministic tie-breakers (e.g., `ModelKey` ordering) and document why it’s sufficient.

### R2. Observability contract

Expose enough signals to answer:

- “What’s cached, how big, and why can’t it evict?”
- “What got evicted and why?”
- “Are we pinned/active constrained?”

This can be done via existing Prometheus metrics + structured telemetry events (preferred).

### R3. No behavioral regressions in determinism modes

Cache events must not affect inference determinism (telemetry side effects only).

## Acceptance Criteria

- Eviction ordering is deterministic given the same sequence of cache operations.
- Telemetry/metrics are emitted for eviction and “budget exceeded” conditions.
- Unit/integration tests cover:
  - eviction under memory pressure
  - eviction blocked by pinned entry
  - eviction blocked by active entry
  - stable tie-break behavior when timestamps/counters match
- `cargo test -p adapteros-lora-worker` passes.

## Test Plan

- Add `crates/adapteros-lora-worker/tests/model_handle_cache_eviction.rs` to simulate:
  - multiple entries with controlled “age” and access counts
  - pinned + active guards
  - forced eviction via a low `max_memory_bytes`
- Run:
  - `cargo test -p adapteros-lora-worker model_handle_cache_eviction -- --nocapture`

## Rollout / Risk

- Risk: changing eviction ordering could affect memory behavior in long-lived worker processes. Keep changes minimal and covered by tests.
- Ensure any additional telemetry is rate-limited or sampled if it can be triggered per-token.
