# Phase 9 Execution: Integration Sheriff Closeout

## Objective
- Integrate Phases 4-8 outputs, resolve verification blockers, and publish final readiness status.

## Cross-Phase Conflict Matrix

| Conflict | Source phases | Resolution |
|---|---|---|
| Contract artifacts drifted during integration checks | Phase 7, Phase 9 | Regenerated via `scripts/contracts/generate_contract_artifacts.py`; canonical docs claims check now passes. |
| OpenAPI spec drift vs code annotations | Phase 9 | Reconciled via `scripts/ci/check_openapi_drift.sh --fix`; follow-up check now passes. |
| Readiness test target assumptions outdated | Phase 4 | Resolved by confirming strict boot transition order in `crates/adapteros-server-api/tests/readyz_failure_modes.rs` (`worker_discovery` before `ready`) and re-running readiness suites. |
| UI crate compile gate failing in active worktree | Phase 5 | Re-verified; `cargo check -p adapteros-ui` now passes. |
| Legacy docs heuristics conflict with canonical contract gate | Phase 7 | Resolved by updating docs policy-pack counts and adding README `alpha-v` marker; `scripts/validate-docs.sh --legacy` now passes. |
| Static minimal diagnostics endpoint drift (`/health` vs `/healthz`) | Phase 6 | Resolved by canonicalizing `crates/adapteros-server/static-minimal/api-test.html` to `/healthz`. |

## Final Verification Pack and Status

1. `scripts/contracts/check_docs_claims.sh`
- Status: PASS

2. `scripts/validate-docs.sh`
- Status: PASS (canonical mode)

3. `scripts/validate-docs.sh --legacy`
- Status: PASS

4. `cargo test -p adapteros-server-api --test replay_determinism_tests`
- Status: PASS (`32 passed`)

5. `cargo test -p adapteros-server-api --test readyz_failure_modes`
- Status: PASS (`10 passed`)

6. `cargo test -p adapteros-server-api --test health_readyz_timeout_tests`
- Status: PASS (`31 passed`)

7. `cargo check -p adapteros-ui`
- Status: PASS

8. `scripts/ci/check_openapi_drift.sh`
- Status: PASS (after applying `--fix` once)

## Release Readiness Memo (for this execution scope)
- Phase execution artifacts are complete and sequenced through Phase 9.
- Verification pack is fully green across docs contracts, OpenAPI drift, replay determinism, readiness suites, and UI compile gate.
- Remaining risk is reduced to normal codebase churn (no unresolved blocker from this phase sequence).
- No destructive operations were used; existing in-flight workspace changes were preserved.

## Phase 9 Completion
- [x] Cross-phase conflict report delivered.
- [x] Final verification pack executed.
- [x] Release-readiness memo delivered with all tracked blockers closed.
