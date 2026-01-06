# PRD: Heap Observer Allocator Hooks for Accurate Fragmentation

**Status:** Draft
**Last Updated:** 2026-01-05
**Owner:** Engineering
**Related Docs:**
- crates/adapteros-memory/src/heap_observer.rs
- crates/adapteros-memory/src/heap_observer_impl.mm
- crates/adapteros-memory/GUIDE_HEAP_OBSERVER.md
- docs/METAL_BACKEND.md

---

## 1. Summary

The Metal heap observer currently estimates internal fragmentation with a fixed percentage, which can mask real fragmentation behavior. This PRD defines a staged plan to capture allocator-level data so fragmentation metrics reflect actual free-block distribution and per-allocation waste, while keeping non-macOS behavior stable.

---

## 2. Problem Statement

Fragmentation is reported using a hardcoded estimate (15% in Rust, 5% in ObjC++), which is not tied to real allocator state. This undermines memory pressure diagnostics, pool optimization decisions, and operational confidence in fragmentation metrics.

---

## 3. Goals

1. Replace the hardcoded internal fragmentation estimate with real data derived from allocator hooks.
2. Keep fragmentation metrics deterministic and low overhead.
3. Preserve existing behavior on non-macOS targets with a safe fallback.
4. Expose accurate metrics through existing FFI surfaces and Rust wrappers.

---

## 4. Non-Goals

- Rewriting the Metal heap observer architecture or its FFI surface.
- Changing the allocator strategy used by Metal or the GPU memory pool.
- Adding new runtime dependencies or external libraries.

---

## 5. Proposed Approach

### 5.1 Allocation Hook Data

- Extend allocation records to include both requested size and actual allocated size.
- Use allocator callbacks (or Metal hook data) to capture requested size per allocation.
- Compute internal fragmentation as:
  - `sum(actual_size - requested_size) / sum(actual_size)`

### 5.2 Free-Block Metrics

- Continue deriving external fragmentation from free-block gaps.
- For macOS, also query `MTLHeap.maxAvailableSizeWithAlignment` to confirm the largest contiguous block.
- Provide a small `FreeBlockStats` struct for downstream consumers.

### 5.3 FFI and Rust Integration

- Add FFI-safe structures or fields to transmit the new metrics.
- Update Rust wrappers in `heap_observer.rs` and `ffi_wrapper.rs` to surface accurate values.
- Keep the existing FFI API stable when possible; add versioned fields if needed.

### 5.4 Fallback Behavior

- On non-macOS, or when hooks are unavailable, retain the existing estimate and log a warning that it is an estimate.

---

## 6. Work Breakdown (Follow-Up Tasks)

Each task should be a separate PR with targeted scope.

### Task A: Allocation Hook Plumbing

**Acceptance Criteria:**
- Allocation records store both requested and actual size.
- Internal fragmentation uses the delta between requested and actual sizes.
- Unit tests cover the new calculation with synthetic allocations.

### Task B: Free-Block Metrics Validation

**Acceptance Criteria:**
- Largest free block is validated against `maxAvailableSizeWithAlignment` on macOS.
- Free-block count and sizes are exposed via a stable struct.
- Tests verify block counting and largest-block logic in mock mode.

### Task C: FFI Surface Updates

**Acceptance Criteria:**
- FFI structures include new fields without breaking ABI compatibility.
- Rust wrapper surfaces the new metrics.
- Size/alignment tests validate the updated struct layout.

### Task D: Non-macOS Fallback

**Acceptance Criteria:**
- Non-macOS builds compile and return the existing estimate.
- Warning log emitted once per process when estimate is used.

---

## 7. Acceptance Criteria

- Internal fragmentation is computed from allocator hook data (not a fixed percentage).
- Fragmentation metrics remain within [0.0, 1.0] and do not panic.
- macOS behavior uses real heap data; non-macOS uses a documented estimate.
- Existing heap observer tests pass, with new tests for the updated logic.

---

## 8. Test Plan

- Unit tests for internal fragmentation calculation with synthetic allocations.
- Existing `metal_heap_tests` cover fragmentation detection with mock data.
- macOS-only integration tests (`--ignored`) validate the Metal path.

Suggested commands:
- `cargo test -p adapteros-memory --lib heap_observer`
- `cargo test -p adapteros-memory --test metal_heap_tests -- --ignored --nocapture`

---

## 9. Rollout Plan

1. Land allocation hook plumbing behind the existing heap observer config flag.
2. Ship the new metrics in dev builds first; compare with estimated values.
3. Enable in production once metrics stabilize and no regressions are found.

---

## 10. Risks and Open Questions

- How to reliably capture requested size for all allocation paths.
- ABI compatibility when extending FFI structures.
- Performance overhead when querying Metal heap stats frequently.

