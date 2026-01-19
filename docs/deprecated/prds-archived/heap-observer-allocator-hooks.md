# PRD: Heap Observer Allocator Hooks for Accurate Fragmentation

**Status:** Draft
**Last Updated:** 2026-01-05
**Owner:** Engineering
**Related Docs:** `crates/adapteros-memory/src/heap_observer.rs`, `crates/adapteros-lora-kernel-mtl/src/gpu_memory_pool.rs`

---

## 1. Summary

Heap fragmentation telemetry currently relies on a fixed 15% waste estimate. This PRD defines allocator hook instrumentation to compute real free-block statistics and report accurate fragmentation ratios for Metal heaps.

---

## 2. Problem Statement

The heap observer reports internal fragmentation using a hardcoded percentage. This masks real memory pressure patterns and prevents reliable diagnostics during Metal heap exhaustion scenarios.

---

## 3. Goals

- Track free-block count, total free bytes, and largest block size per heap.
- Compute fragmentation ratios from actual heap state.
- Expose metrics via FFI-safe structs for ObjC/FFI consumers.

---

## 4. Non-Goals

- Replacing Metal heap allocation strategy.
- Building a full allocator or defragmentation system.
- Adding non-macOS GPU implementations.

---

## 5. Proposed Approach

- Introduce a `FreeBlockStats` struct in `heap_observer.rs` (FFI-safe).
- Add a Metal heap query function to compute largest contiguous free block size.
- Replace the hardcoded 15% estimate with derived fragmentation metrics.
- Provide a non-macOS fallback that preserves existing behavior.

---

## 6. Acceptance Criteria

- Fragmentation ratio derived from actual heap state on macOS.
- FFI-safe stats struct available to ObjC integration.
- Non-macOS builds fall back to current estimate without failures.
- Hook overhead <1us per query in steady state.

---

## 7. Test Plan

- Unit test `FreeBlockStats::fragmentation_ratio` for edge cases.
- Integration test against a simulated heap state (mocked stats input).
- Mac-only test validating `max_available_size_with_alignment` usage.

---

## 8. Rollout Plan

1. Add stats struct and fallback implementation.
2. Wire Metal heap queries and update metrics reporting.
3. Validate overhead and publish updated telemetry schema.

---

## 9. Follow-up Tasks (Tracked)

- TASK-1: Add `FreeBlockStats` and fragmentation ratio helpers.
  - Acceptance: unit tests cover single block and empty heap cases.
- TASK-2: Integrate Metal heap query for max contiguous block.
  - Acceptance: stats report changes as allocations grow.
- TASK-3: Expose stats via FFI boundary.
  - Acceptance: ObjC header includes stats struct and fields.
