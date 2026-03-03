# Phase 9: Determinism and Compatibility Revalidation - Research

**Researched:** 2026-02-24
**Domain:** Determinism replay validation, OpenAI compatibility regression, CI merge-gate governance
**Confidence:** HIGH
**Status:** Executed and reconciled (historical planning research)

## Reconciled Execution State (2026-02-24)

This research document preserves planning-time deferred-suite framing. Phase 09 is complete and reconciled: DET-06 and API-07 passed; FFI-05 governance evidence was captured and then normalized to accepted external debt in Phase 11 closure artifacts.

## Summary

At planning time, Phase 9 was a closure phase for deferred verification and governance work, not an implementation-expansion phase. Phase 03 closeouts had explicitly skipped replay-heavy determinism reruns, and Phase 04 closeouts had explicitly skipped full OpenAI compatibility reruns plus OpenAPI regeneration/drift validation. Those deferred suites became first-class acceptance gates for v1.1.

The repository already had native command and suite anchors for this work: determinism replay and receipt harnesses in `tests/` plus `crates/adapteros-server-api/tests/replay_determinism_tests.rs`, OpenAI compatibility tests in `crates/adapteros-server-api/tests/`, OpenAPI drift guard in `scripts/ci/check_openapi_drift.sh`, and an existing push-triggered `ffi-asan` job in `.github/workflows/ci.yml`. The planning-time unresolved gap was governance: `ffi-asan` existed but required-check branch-protection enforcement was left as a manual closure item.

**Primary recommendation:** Split Phase 9 into three plans across two waves: run determinism and OpenAI revalidation in parallel (Wave 1), then close ASAN required-check governance using fresh revalidation evidence (Wave 2).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Reuse existing suites/scripts and avoid parallel verification harnesses.
- Close deferred replay determinism suites from Phase 03 summaries.
- Close deferred full OpenAI compatibility reruns and OpenAPI regeneration/drift checks from Phase 04 summaries.
- Close `FFI-05` with explicit branch-protection required-check governance for `ffi-asan`.
- Plan decomposition is fixed to three plans with explicit waves and dependencies.

### Claude's Discretion
- Exact ordering inside each plan as long as deferred suites and governance evidence are complete.
- Minimal corrective edits when reruns expose regressions.
- Final summary evidence format.

### Deferred Ideas (OUT OF SCOPE)
- New determinism or OpenAI feature development beyond what is needed to make deferred suites pass.
- CI workflow redesign outside `ffi-asan` governance closure.
- Phase 10 operations release-signoff work.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| DET-06 | Deferred replay determinism suites are re-run and pass on current workspace state | Phase 03 summaries list skipped suites (`determinism_core_suite`, `record_replay_receipt_harness`, `determinism_replay_harness`, `replay_determinism_tests`) and call out replay confidence as not re-proven. |
| API-07 | Full OpenAI compatibility suites (not only targeted subsets) are re-run and pass | Phase 04 summaries confirm only targeted tests were rerun and explicitly defer broader OpenAI suites and OpenAPI regeneration. Existing OpenAI tests are already centralized under `crates/adapteros-server-api/tests/`. |
| FFI-05 | ASAN FFI CI lane is confirmed enforceable at merge-gate level (required-check policy documented and active) | `.github/workflows/ci.yml` already defines `ffi-asan`; Phase 02 summary states required-check branch-protection policy remained a manual governance gap. |
</phase_requirements>

## Existing Native Patterns

### Pattern 1: Deferred-suite closure by rerunning canonical commands
- Prior closeouts captured exact skipped command sets, which can be replayed directly without creating new harnesses.
- This keeps evidence comparable with prior phase summaries.

### Pattern 2: OpenAPI contract parity via existing drift gate
- `scripts/ci/check_openapi_drift.sh` already wraps exporter execution and drift comparison against `docs/api/openapi.json`.
- Use this path instead of ad-hoc spec generation checks.

### Pattern 3: CI governance through branch-protection required checks
- Workflow jobs are necessary but not sufficient; enforceability requires required-status-check policy on the protected branch.
- Governance evidence should include both workflow/job definition and branch-protection check contexts.

## Gaps and Risks

### Gap A: Determinism reruns were deferred, not eliminated
- Risk: current code may have replay regressions masked by compile-only verification.
- Mitigation: rerun the full deferred deterministic matrix and lock command transcript in `09-01-SUMMARY.md`.

### Gap B: OpenAI confidence is currently subset-based
- Risk: compatibility claims can regress outside two targeted test binaries.
- Mitigation: run full OpenAI test matrix and enforce OpenAPI drift gate in `09-02`.

### Gap C: ASAN lane exists but merge policy may still be non-blocking
- Risk: sanitizer regressions can merge if branch protection does not require `ffi-asan` context.
- Mitigation: verify/update branch-protection contexts and record proof in `09-03`.

### Gap D: Check-context naming can vary by GitHub job/workflow rendering
- Risk: governance automation fails if context name is assumed incorrectly.
- Mitigation: discover actual contexts from GitHub API before asserting policy closure.

## Recommended Plan Split and Waves

| Plan | Wave | Depends On | Requirement Focus | Outcome |
|------|------|------------|-------------------|---------|
| 09-01 | 1 | [] | DET-06 | Deferred replay determinism suites re-run and passing on current workspace. |
| 09-02 | 1 | [] | API-07 | Full OpenAI suite rerun plus OpenAPI drift/regeneration checks passing. |
| 09-03 | 2 | ["09-01", "09-02"] | FFI-05 | `ffi-asan` required-check governance is active and evidenced for merges. |

## Suggested Verification Set (Smallest Relevant but Complete)

### DET-06 command set
```bash
cargo test --test determinism_core_suite canonical_hashing -- --test-threads=1
cargo test --test record_replay_receipt_harness -- --test-threads=1
cargo test -p adapter-os --no-default-features --test determinism_replay_harness -- --test-threads=1 --nocapture
cargo test -p adapteros-server-api --test replay_determinism_tests -- --test-threads=1
bash scripts/check_fast_math_flags.sh
```

### API-07 command set
```bash
cargo test -p adapteros-server-api --test openai_chat_completions_compat -- --test-threads=1 --nocapture
cargo test -p adapteros-server-api --test openai_chat_completions_streaming -- --test-threads=1 --nocapture
cargo test -p adapteros-server-api --test openai_embeddings_tests -- --test-threads=1 --nocapture
cargo test -p adapteros-server-api --test openai_models_list_test -- --test-threads=1 --nocapture
cargo test -p adapteros-server-api --test openai_error_format_tests -- --test-threads=1 --nocapture
cargo test -p adapteros-server-api --test streaming_infer test_openai_compatible_format -- --test-threads=1 --nocapture
cargo test -p adapteros-server-api --test streaming_adapter_integration test_openai_spec_compliance -- --test-threads=1 --nocapture
bash scripts/ci/check_openapi_drift.sh
cargo run --locked -p adapteros-server-api --bin export-openapi -- target/codegen/openapi.json
```

### FFI-05 governance command set
```bash
rg -n "ffi-asan|if: github.event_name == 'push'|sanitizer=address" .github/workflows/ci.yml
REPO="$(gh repo view --json nameWithOwner --jq '.nameWithOwner')"
BRANCH="$(gh repo view --json defaultBranchRef --jq '.defaultBranchRef.name')"
gh api "repos/${REPO}/branches/${BRANCH}/protection/required_status_checks" --jq '.contexts'
```

## Open Questions

1. **What is the exact required-check context string for `ffi-asan` in this repository?**
   - What we know: CI job name is `FFI AddressSanitizer (push)` and job id is `ffi-asan`.
   - What is unclear: required-status-check context label can differ from raw job id.
   - Recommendation: query branch-protection contexts directly via GitHub API and use returned value as source of truth.

2. **Does branch protection target `main` or another default branch in this repo?**
   - What we know: check commands and roadmap assume default branch protection exists.
   - What is unclear: target branch name can vary by repo settings.
   - Recommendation: resolve default branch via `gh repo view --json defaultBranchRef` before policy checks.

---

*Phase: 09-determinism-and-compatibility-revalidation*
*Research completed: 2026-02-24*
*Ready for planning: no (already executed and reconciled)*
