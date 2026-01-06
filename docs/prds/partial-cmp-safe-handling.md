# PRD: Standardize partial_cmp Error Handling

**Status:** Draft
**Last Updated:** 2026-01-05
**Owner:** Engineering
**Related Docs:** `crates/adapteros-memory/src/unified_tracker.rs`

---

## 1. Summary

Some code paths use `partial_cmp().unwrap()` on floating point values, which can panic on NaN. This PRD defines a consistent safe comparison pattern to avoid runtime panics.

---

## 2. Problem Statement

`partial_cmp` returns `None` for NaN inputs, and `unwrap()` will panic. Inconsistent handling across the codebase introduces latent crash risks.

---

## 3. Goals

- Replace `partial_cmp().unwrap()` with a safe fallback ordering.
- Ensure sorting and selection functions handle NaN values without panicking.
- Prevent future regressions with linting guidance.

---

## 4. Non-Goals

- Switching to total ordering via `total_cmp` for all floats.
- Redesigning ranking or scoring semantics.

---

## 5. Proposed Approach

- Replace `partial_cmp(...).unwrap()` with `unwrap_or(Ordering::Equal)`.
- Update tests and benches to use the same safe pattern.
- Add documentation describing NaN handling policy.

---

## 6. Acceptance Criteria

- No `partial_cmp().unwrap()` remains in Rust sources.
- Code paths that sort or rank floats no longer panic on NaN.
- Optional linting guidance documented for future changes.

---

## 7. Test Plan

- Unit test that NaN values do not panic during sort/selection.
- Run existing benches/tests to validate no regressions.

---

## 8. Rollout Plan

1. Update all existing `partial_cmp` call sites.
2. Add documentation and linting guidance.
3. Monitor for any behavioral regressions in ranking outputs.

---

## 9. Follow-up Tasks (Tracked)

- TASK-1: Add clippy/lint guidance to flag `partial_cmp().unwrap()`.
  - Acceptance: linting docs include the safe comparison pattern.
