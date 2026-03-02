# Phase 8: Observability and Runtime Evidence Closure - Research

**Researched:** 2026-02-24
**Domain:** Streaming inference drain/error semantics and runtime evidence revalidation
**Confidence:** HIGH
**Status:** Executed and reconciled (historical planning research)

## Reconciled Execution State (2026-02-24)

This research document preserves planning-time gap framing. Phase 08 is complete, including post-closeout rectification of runtime fidelity and lifecycle evidence; see `08-03-SUMMARY.md`, `08-VERIFICATION.md`, and `.planning/milestones/v1.1-MILESTONE-AUDIT.md` for final accounting.

## Summary

At planning time, Phase 8 was a closure phase with clear known gaps, not an exploration phase. The repository already had targeted integration coverage for the three failing `streaming_infer` scenarios and existing runtime validation surfaces (`foundation-run`, `foundation-smoke`, TUI API polling). The work was to restore failing behavior contracts and produce fresh evidence artifacts against then-current code.

The required evidence stack is also already present: streaming tests encode expected drain and unavailable-worker behavior; observability trace propagation and SIGTERM lifecycle tests exist; and phase summaries document exactly what was deferred. The main risk is mixing code fixes and evidence capture without sequencing. A three-plan split keeps causal clarity: fix drain semantics first, stabilize unavailable-worker envelopes second, then run fidelity/manual evidence revalidation on the resulting code.

**Primary recommendation:** Execute `OBS-06`, then `OBS-07`, then `UX-05` evidence revalidation with concrete artifact capture, reusing existing scripts/tests only.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Reproduce known failing `streaming_infer` tests before patching.
- Split closure into three plans with requirement mapping:
  - `08-01` -> `OBS-06`
  - `08-02` -> `OBS-07`
  - `08-03` -> `UX-05` plus deferred manual evidence closure
- Close deferred manual observability evidence from Phase 05:
  - `/metrics` scrape
  - trace-chain inspection
  - SIGTERM drain timeline capture
- Re-run and evidence `scripts/foundation-run.sh` and live TUI/backend fidelity.
- Keep changes in existing abstractions; avoid parallel routes/tooling.

### Claude's Discretion
- Minimal patch strategy in middleware/handler/error mapping and existing tests.
- Exact evidence file layout under phase-owned artifacts.
- Verification sequencing that minimizes re-run cost while preserving confidence.

### Deferred Ideas (OUT OF SCOPE)
- New observability product features.
- Streaming transport redesign.
- Broad TUI/UI refactor work.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| OBS-06 | Drain semantics reject new requests with `503` while in-flight streams complete | Failing test already exists: `draining_rejects_new_requests_while_allowing_in_flight_stream_to_complete` in `crates/adapteros-server-api/tests/streaming_infer.rs`; drain gate is implemented in `crates/adapteros-server-api/src/middleware_security.rs`. |
| OBS-07 | Worker-unavailable streaming paths return stable `NO_COMPATIBLE_WORKER` envelopes | Failing tests exist: `streaming_infer_emits_structured_error_on_unavailable_resource` and `streaming_infer_resolves_effective_adapters_from_session_stack`; error code contract is in `crates/adapteros-server-api/src/types/error.rs` and streaming error envelope is in `crates/adapteros-server-api/src/handlers/streaming_infer.rs`. |
| UX-05 | Foundation smoke and live TUI/backend fidelity are re-run with fresh evidence | Canonical runtime scripts exist (`scripts/foundation-run.sh`, `scripts/foundation-smoke.sh`); TUI live data path is explicit in `crates/adapteros-tui/src/app/api.rs` via `/api/metrics` and `/api/adapters`. |
</phase_requirements>

## Standard Stack

| Area | Existing Surface | Why This Is the Right Reuse Point |
|------|------------------|-----------------------------------|
| Drain behavior verification | `crates/adapteros-server-api/tests/streaming_infer.rs` | Already codifies expected 503-vs-in-flight semantics; no new harness needed. |
| Worker-unavailable envelope verification | `streaming_infer.rs` integration tests + `types/error.rs` mappings | Keeps error-code stability tied to existing canonical error types. |
| Runtime smoke validation | `scripts/foundation-run.sh` -> `scripts/foundation-smoke.sh` | Existing end-to-end path and logs already used as release evidence. |
| TUI fidelity validation | `crates/adapteros-tui/src/app/api.rs` endpoints + live TUI run | Validates real operator surface rather than synthetic-only checks. |

## Architecture Patterns

### Pattern: Failing-Test-First Gap Closure
- Reproduce each known failing test before edits.
- Patch only the narrow behavior boundary under test.
- Re-run exact test(s) and then a small adjacent guard test.

### Pattern: Existing Error Contract Pipeline
- Keep `InferenceError` code mapping as source of truth.
- Keep streaming SSE error payload (`code`, `message`, `retryable`, `correlation_id`) stable.
- Validate through existing integration tests that already parse event payloads.

### Pattern: Evidence-on-Current-Code
- Collect manual/runtime evidence only after streaming fixes are green.
- Persist evidence artifacts in deterministic paths for summary linking.
- Reuse canonical scripts and avoid ad-hoc replacements.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Drain semantic validation | New custom shutdown harness | Existing `streaming_infer` drain test + `drain_timeout_test` | Already targets the exact behavior boundary and is maintainable. |
| Worker-unavailable contract checks | New bespoke SSE parser script | Existing integration tests that parse SSE error blocks | Existing assertions cover expected payload fields and codes. |
| Runtime acceptance evidence | New shell wrappers for startup/smoke | `scripts/foundation-run.sh` and `scripts/foundation-smoke.sh` | Canonical path already consumed in prior phase evidence. |
| TUI fidelity checks | Mocked-only TUI data feed | Live backend + `/api/metrics` and `/api/adapters` snapshots | Requirement explicitly calls for live fidelity revalidation. |

## Common Pitfalls

### Pitfall 1: Fixing drain behavior without preserving in-flight completion
- What goes wrong: New requests reject correctly, but active requests are interrupted.
- Why: Middleware order or state checks are changed without request-tracking symmetry.
- How to avoid: Keep request tracking + drain middleware behavior aligned and verify with exact drain test.

### Pitfall 2: Returning non-canonical unavailable-worker codes
- What goes wrong: Error envelopes vary by call path (`SERVICE_UNAVAILABLE` vs `NO_COMPATIBLE_WORKER`) and tests fail.
- Why: Error mapping drift between `InferenceError` and stream event serialization.
- How to avoid: Keep `InferenceError::error_code()` mapping canonical and route streaming error events through it.

### Pitfall 3: Marking UX-05 complete from compile checks alone
- What goes wrong: Evidence says "revalidated" but no fresh `foundation-run` or live TUI observations exist.
- Why: Over-reliance on unit checks instead of runtime scripts and live snapshots.
- How to avoid: Capture script logs and API/TUI observations as explicit phase artifacts.

## Code Examples

### Reproduce OBS-06 Failure/Fix Signal
```bash
CARGO_TARGET_DIR=target-phase08 cargo test -p adapteros-server-api --test streaming_infer draining_rejects_new_requests_while_allowing_in_flight_stream_to_complete -- --exact --test-threads=1 --nocapture
```

### Reproduce OBS-07 Failure/Fix Signals
```bash
CARGO_TARGET_DIR=target-phase08 cargo test -p adapteros-server-api --test streaming_infer streaming_infer_emits_structured_error_on_unavailable_resource -- --exact --test-threads=1 --nocapture
CARGO_TARGET_DIR=target-phase08 cargo test -p adapteros-server-api --test streaming_infer streaming_infer_resolves_effective_adapters_from_session_stack -- --exact --test-threads=1 --nocapture
```

### Revalidate UX-05 Runtime and Collect Evidence
```bash
mkdir -p var/evidence/phase08
bash scripts/foundation-run.sh --no-clean --headless | tee var/evidence/phase08/foundation-run.log
curl -fsS http://127.0.0.1:8080/api/metrics > var/evidence/phase08/api-metrics.json
curl -fsS http://127.0.0.1:8080/api/adapters > var/evidence/phase08/api-adapters.json
```

### Close Deferred Manual Observability Evidence
```bash
curl -fsS http://127.0.0.1:8080/metrics > var/evidence/phase08/metrics.prom
CARGO_TARGET_DIR=target-phase08 cargo test -p adapteros-server-api --test observability_trace_propagation -- --test-threads=1 --nocapture | tee var/evidence/phase08/trace-propagation.txt
CARGO_TARGET_DIR=target-phase08 cargo test --test server_lifecycle_tests test_graceful_shutdown_sigterm -- --exact --test-threads=1 --nocapture | tee var/evidence/phase08/sigterm-drain.txt
```

## State of the Art

| Prior State | Current Requirement | Impact for Phase 8 |
|-------------|---------------------|--------------------|
| Phase 05 closed with 3 `streaming_infer` failures and deferred manual observability checks | Must close drain/unavailable-worker semantics and collect deferred evidence | Phase 8 is a direct residual-risk closure lane. |
| Phase 07 closed with targeted checks but deferred runtime revalidation | Must re-run `foundation-run` and live TUI fidelity | UX evidence must be refreshed on current code, not inherited. |

## Current State (Verified)

- `crates/adapteros-server-api/tests/streaming_infer.rs` includes all three known failing test cases that define Phase 8 behavior.
- `crates/adapteros-server-api/src/middleware_security.rs` contains current drain gate logic returning `503`.
- `crates/adapteros-server-api/src/types/error.rs` contains canonical `NO_COMPATIBLE_WORKER` code mapping.
- `crates/adapteros-server-api/src/handlers/streaming_infer.rs` emits structured stream error payload fields including `correlation_id`.
- `scripts/foundation-run.sh` and `scripts/foundation-smoke.sh` are present as canonical runtime smoke path.
- `crates/adapteros-tui/src/app/api.rs` pulls live data from `/api/metrics` and `/api/adapters`.

---
*Phase: 08-observability-and-runtime-evidence-closure*
*Research completed: 2026-02-24*
