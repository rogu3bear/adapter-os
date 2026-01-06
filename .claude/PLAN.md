# Execution Plan

## Strategy Overview

Work in priority-ordered batches. CI must stay green between batches.

## Batch 1: CI Blockers (P0)

**Goal:** Get baseline CI passing

1. **LOCAL-001**: Fix bool_assert_comparison in config_validation_tests.rs
2. **LOCAL-002**: Fix field_reassign_with_default in model.rs
3. **LOCAL-003**: Fix non-exhaustive match in streaming_tests.rs

**Validation:** `cargo clippy --workspace --all-targets --exclude adapteros-lora-mlx-ffi -- -D warnings` passes

**Commit:** `fix: resolve clippy and test compilation errors`

## Batch 2: Security Issues (P1)

**Goal:** Address security-related issues

4. **#163**: Add JWT secret placeholder validation at startup
5. **#160**: Add runtime validation for allow_silent_downgrade

**Validation:**
- Server refuses to start with placeholder JWT secret
- Server refuses to start if allow_silent_downgrade=true

**Commit:** `fix(security): validate JWT secret and audit compliance at startup`

## Batch 3: Core Bug Fixes (P2 Bugs)

**Goal:** Fix reliability bugs in critical paths

6. **#166**: Fix NaN panic in filter_engine.rs
7. **#164**: Add logging for missing adapters in policy_mask.rs
8. **#162**: Replace panic!() calls in policy evaluation

**Validation:** Tests pass, no panics in policy code

**Commit:** `fix: improve reliability of filter and policy code`

## Batch 4: API Enhancements (P1/P2)

**Goal:** Implement missing API features

9. **#152**: Add skip-worker readiness mode
10. **#151**: Expose GPU memory metrics

**Validation:** New config options work, metrics return real values

**Commit:** `feat(api): add skip-worker readiness mode and GPU metrics`

## Batch 5: Feature Stubs (P2 Enhancements)

**Goal:** Replace stub implementations

11. **#155**: Implement config drift detection
12. **#150**: Track boot download MB

**Validation:** Handlers return real data instead of stubs

**Commit:** `feat(api): implement config drift and boot progress tracking`

## Batch 6: Complex Handlers (P2)

**Goal:** Implement training/versioning handlers

13. **#154**: Implement version-based training workflow
14. **#153**: Implement repository version timeline

**Validation:** Endpoints return 200 with data instead of 501/empty

**Commit:** `feat(repos): implement version timeline and training workflow`

## Batch 7: Triage Remaining

15. **#157**: Close as "deferred" - requires significant architecture work, convert to design doc
16. **#161**: Close as "needs discussion" - product decision required

## Definition of Done

- All P0-P2 issues fixed with tests
- CI green locally
- No new warnings introduced
- PR(s) created with "Closes #N" links

## Risk Mitigation

- Run clippy + tests after each batch
- Atomic commits per batch for easy rollback
- Document any intentional behavioral changes
