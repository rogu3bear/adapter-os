# PRD: Standardize partial_cmp Error Handling

**Status:** Draft  
**Last Updated:** 2026-01-05  
**Owner:** Engineering  
**Related Docs:** docs/DETERMINISM.md

## Problem / Motivation

`partial_cmp()` returns `None` for NaN values. Some call sites unwrap that `None`, which can panic and introduce nondeterministic behavior when NaN scores appear in sorting or selection logic. The codebase uses a mix of `unwrap()` and `unwrap_or(Ordering::Equal)`, so behavior is inconsistent.

## Goals

- Eliminate panic risk from `partial_cmp()` on NaN.
- Establish a single, documented NaN handling policy for float comparisons.
- Prevent regressions through a lightweight guard.

## Non-Goals

- Rewriting all float comparison logic to `total_cmp()`.
- Changing domain-specific scoring semantics beyond NaN handling.
- Large refactors across the router or scoring pipelines.

## Proposed Approach

1. Standardize on `partial_cmp(...).unwrap_or(Ordering::Equal)` for non-total ordering comparisons.
2. Document NaN handling policy: NaN comparisons resolve to `Ordering::Equal`, and stable tie-breakers determine final order.
3. Add a regression guard (CI script or lint) to block new `partial_cmp(...).unwrap()` usage.
4. (Optional) Introduce a helper trait (e.g., `SafePartialOrd`) for clarity and reuse in new code.

## Requirements

- **R1 (P0):** Replace `partial_cmp(...).unwrap()` usages with `unwrap_or(Ordering::Equal)`.
- **R2 (P1):** Document NaN handling policy in determinism docs.
- **R3 (P1):** Add a regression guard to prevent new unsafe unwraps.

## Acceptance Criteria

- `rg "partial_cmp.*\.unwrap\(\)" --type rust .` returns no results.
- NaN inputs do not panic in code paths using float comparisons.
- NaN handling policy is documented for developers.
- Regression guard is in place (script or lint).

## Test Plan

- Add or update unit tests in the affected modules to include NaN values in sorted/selected lists.
- Run targeted tests for impacted crates (examples):
  - `cargo test -p adapteros-system-metrics`
  - `cargo test -p adapteros-memory`
  - `cargo test -p adapteros-lora-worker`

## Rollout / Risk

- Low risk: only changes NaN fallback behavior in float comparisons.
- If NaNs are present, ordering becomes stable instead of panicking.
- No config changes required.

## Follow-up Tasks

1. Add a CI guard script (rg-based) to block `partial_cmp(...).unwrap()` regressions.
2. Add helper trait for safe comparisons if repeated usage becomes noisy.
3. Expand NaN handling tests in router and scoring paths.
