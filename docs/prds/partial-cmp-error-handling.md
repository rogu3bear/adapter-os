# PRD: Standardize `partial_cmp` Error Handling

## Problem
Some code paths call `partial_cmp().unwrap()` on floating-point values. If any value is NaN, the unwrap panics, which can crash production processes. The current usage is inconsistent across the codebase, making behavior hard to reason about and easy to regress.

## Non-goals
- Redesigning sort semantics for all floats beyond NaN-safe comparison.
- Replacing every float comparison with total ordering (e.g., `total_cmp`) without a determinism review.
- Changing ranking logic or business rules in existing algorithms.

## Proposed Approach
- Replace `partial_cmp().unwrap()` with `unwrap_or(Ordering::Equal)` (or a shared helper) to avoid panics on NaN.
- Introduce a lightweight guardrail (CI `rg` check or lint) to prevent new `partial_cmp().unwrap()` usage.
- Prioritize production code paths first, then tests/benches.
- Document the NaN handling policy in `docs/` (treat NaN comparisons as equal for ordering stability).

## Acceptance Criteria
- No `partial_cmp().unwrap()` in production crates (`crates/**/src`).
- A guardrail prevents regressions (`rg "partial_cmp.*\\.unwrap\\(\\)" --type rust` is empty in CI).
- At least one unit test covers NaN behavior in a production code path.
- Documentation describes NaN handling and rationale.

## Test Plan
- Add/extend unit tests to exercise NaN inputs without panicking.
- Run targeted tests for the impacted crate(s).
- Run CI grep check for `partial_cmp().unwrap()` once the guardrail is added.

## Rollout Plan
1. Phase 1 (this PR): Update one production call site + add NaN unit test (validate safe ordering).
2. Phase 2: Replace remaining production usages and add guardrail (CI grep or lint).
3. Phase 3: Update tests/benches and document the NaN ordering policy.
