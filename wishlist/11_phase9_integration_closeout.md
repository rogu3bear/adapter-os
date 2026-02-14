# Phase 9 Execution: Integration Sheriff Closeout

## Objective
- Integrate Phases 4-8 outputs, resolve verification blockers, and publish final readiness status.

## Cross-Phase Conflict Matrix

| Conflict | Source phases | Resolution |
|---|---|---|
| Contract artifacts drifted during integration checks | Phase 7, Phase 9 | Regenerated via `scripts/contracts/generate_contract_artifacts.py`; canonical docs claims check now passes. |
| OpenAPI spec drift vs code annotations | Phase 9 | Reconciled via `scripts/ci/check_openapi_drift.sh --fix`; follow-up check now passes. |
| Readiness test target assumptions outdated | Phase 4 | Recorded integration risk: `readyz_failure_modes` and `health_readyz_timeout_tests` fixtures/signatures are stale against current handler contracts. |
| UI crate compile gate failing in active worktree | Phase 5 | Recorded integration risk: unrelated active UI edits currently break `cargo check -p adapteros-ui`; parity artifacts still completed. |
| Legacy docs heuristics conflict with canonical contract gate | Phase 7 | `scripts/validate-docs.sh` consolidated to canonical default; legacy mode remains optional (`--legacy`). |

## Final Verification Pack and Status

1. `scripts/contracts/check_docs_claims.sh`
- Status: PASS

2. `scripts/validate-docs.sh`
- Status: PASS (canonical mode)

3. `scripts/ci/check_openapi_drift.sh`
- Status: PASS (after applying `--fix` once)

4. `cargo test -p adapteros-server-api --test replay_determinism_tests`
- Status: PASS (`32 passed`)

5. `cargo test -p adapteros-server-api --test readyz_failure_modes`
- Status: FAIL (test signature drift: `ready` now requires `Query<ReadyzQuery>`)

6. `cargo test -p adapteros-server-api --test health_readyz_timeout_tests`
- Status: FAIL (`ReadyzResponse` fixture missing `canary` field)

7. `cargo check -p adapteros-ui`
- Status: FAIL (pre-existing compile issues in active UI files: `pages/admin/org.rs`, `pages/admin/mod.rs`, `pages/models.rs`)

## Release Readiness Memo (for this execution scope)
- Phase execution artifacts are complete and sequenced through Phase 9.
- Canonical docs/API contract gates pass after reconciliation.
- Residual engineering risk remains in currently failing readiness/UI test surfaces that are outside this phase artifact change set.
- No destructive operations were used; existing in-flight workspace changes were preserved.

## Phase 9 Completion
- [x] Cross-phase conflict report delivered.
- [x] Final verification pack executed.
- [x] Release-readiness memo delivered with residual risks.

