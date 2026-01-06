# Issue Tracker

## Summary
- **Total Issues:** 16 (3 local + 13 GitHub)
- **P0 (CI Blockers):** 3
- **P1 (Critical):** 4
- **P2 (Standard):** 7
- **P3 (Low Priority):** 2

## Issue Table

| ID | Title | Type | Priority | Status | Files |
|----|-------|------|----------|--------|-------|
| LOCAL-001 | Fix clippy bool_assert_comparison errors | chore | P0 | Queued | `crates/adapteros-config/tests/config_validation_tests.rs` |
| LOCAL-002 | Fix clippy field_reassign_with_default errors | chore | P0 | Queued | `crates/adapteros-config/src/model.rs` |
| LOCAL-003 | Fix non-exhaustive match in streaming_tests | bug | P0 | Queued | `crates/adapteros-api/tests/streaming_tests.rs` |
| #163 | Security: Hardcoded insecure JWT secret | security | P1 | Queued | `crates/adapteros-server/src/boot/security.rs` |
| #152 | Add skip-worker readiness mode | enhancement | P1 | Queued | `crates/adapteros-server-api/src/handlers/health.rs` |
| #151 | Expose GPU memory metrics via Worker API | enhancement | P1 | Queued | `crates/adapteros-server-api/src/handlers/capacity.rs` |
| #166 | Bug: partial_cmp().unwrap() can panic on NaN | bug | P2 | Queued | `crates/adapteros-lora-worker/src/filter_engine.rs` |
| #164 | Bug: Silent adapter filtering without logging | bug | P2 | Queued | `crates/adapteros-lora-router/src/policy_mask.rs` |
| #162 | Bug: Panic calls in policy evaluation | bug | P2 | Queued | `crates/adapteros-policy/src/packs/*.rs` |
| #160 | Audit: allow_silent_downgrade field exists | security | P2 | Queued | `crates/adapteros-policy/src/backend_policy.rs` |
| #155 | Persist config baseline for drift detection | enhancement | P2 | Queued | `crates/adapteros-server-api/src/handlers/runtime.rs` |
| #154 | Implement version-based training workflow | enhancement | P2 | Queued | `crates/adapteros-server-api/src/handlers/repos.rs` |
| #153 | Implement repository version timeline | enhancement | P2 | Queued | `crates/adapteros-server-api/src/handlers/repos.rs` |
| #150 | Track boot download MB in BootStateManager | enhancement | P2 | Queued | `crates/adapteros-server-api/src/handlers/streaming.rs` |
| #157 | feat(training): Add GPU gradient kernels | enhancement | P3 | Blocked | N/A - Requires significant architecture work |
| #161 | Design: Best-effort mode 40-70% confidence | design | P3 | Needs Discussion | N/A - Product decision required |

## Detailed Issue Notes

### LOCAL-001: Fix clippy bool_assert_comparison errors
**Acceptance:** `cargo clippy --workspace` passes without bool_assert_comparison errors
**Fix:** Replace `assert_eq!(x, true)` with `assert!(x)` and `assert_eq!(x, false)` with `assert!(!x)`

### LOCAL-002: Fix clippy field_reassign_with_default errors
**Acceptance:** `cargo clippy --workspace` passes without field_reassign_with_default errors
**Fix:** Use struct initialization syntax with `..Default::default()` instead of mutation

### LOCAL-003: Fix non-exhaustive match in streaming_tests
**Acceptance:** `cargo test -p adapteros-api` compiles and passes
**Fix:** Add `StreamEvent::Paused { .. }` arm to the match statement

### #163: Security - Hardcoded JWT secret
**Acceptance:** Server refuses to start if JWT secret matches placeholder pattern
**Fix:** Add startup validation that checks for "CHANGE_ME" in jwt_secret

### #152: Skip-worker readiness mode
**Acceptance:** New config option allows readiness check to skip worker validation
**Fix:** Add `ReadinessMode::Relaxed` variant and config option

### #151: GPU memory metrics
**Acceptance:** Capacity handler returns real GPU memory values
**Fix:** Add `memory_report()` method to Worker struct, wire through handlers

### #166: NaN panic in filter engine
**Acceptance:** median filter handles NaN values without panic
**Fix:** Use `total_cmp()` instead of `partial_cmp().unwrap()`, filter NaN values

### #164: Silent adapter filtering
**Acceptance:** Warning logged when allowlist references non-existent adapter
**Fix:** Add tracing::warn when adapter ID not found in registry

### #162: Panic in policy evaluation
**Acceptance:** No panic calls in policy evaluation hot paths
**Fix:** Replace `panic!()` with proper error handling

### #160: allow_silent_downgrade audit violation
**Acceptance:** Field either removed or startup validation added
**Fix:** Add runtime validation that refuses to start if field is true

### #155: Config drift detection
**Acceptance:** Config baseline persisted and compared at runtime
**Fix:** Store initial config snapshot, implement comparison logic

### #154: Version-based training workflow
**Acceptance:** Training endpoint returns 200 with job ID instead of 501
**Fix:** Implement the 3-step workflow (create draft, submit job, return ID)

### #153: Repository version timeline
**Acceptance:** Timeline endpoint returns actual version history
**Fix:** Implement `list_version_history_for_repo` in adapter_repositories.rs

### #150: Boot download MB tracking
**Acceptance:** Boot progress event includes actual download size
**Fix:** Add download counter to BootStateManager, accumulate during model loads

### #157: GPU gradient kernels (BLOCKED)
**Blocked Reason:** Requires significant architecture work including new Metal shaders
**Recommended Action:** Convert to well-scoped design document, defer to future sprint

### #161: Best-effort mode design (NEEDS DISCUSSION)
**Recommended Action:** Close as design discussion - product decision required, not a bug
