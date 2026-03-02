# Phase 2: FFI Safety Hardening - Research

**Researched:** 2026-02-24
**Domain:** Rust/C++ FFI memory safety hardening for MLX boundary
**Confidence:** HIGH

## Summary

Phase 2 is a correctness hardening phase for the `adapteros-lora-mlx-ffi` boundary and is explicitly scoped to `FFI-01` through `FFI-04`.

The smallest execution split that matches roadmap criteria is:
1. Plan `02-01`: finish unsafe-block SAFETY coverage and remove non-test `unwrap`/`expect` in FFI runtime code (`FFI-01`, `FFI-02`).
2. Plan `02-02`: enable ASAN in CI for the FFI test path and add/verify concurrent adapter hot-swap stress coverage (`FFI-03`, `FFI-04`).

**Primary recommendation:** keep all behavior changes limited to FFI safety/error paths and CI/test wiring. Reuse existing FFI tests and workflow structure instead of introducing parallel harnesses.

## User Constraints (from CONTEXT.md)

### Locked Decisions

- Every unsafe block in `adapteros-lora-mlx-ffi` must have concrete call-site `SAFETY` rationale.
- Non-test FFI code must remove `unwrap`/`expect` and use `Result` propagation.
- CI must run ASAN on push for the FFI path.
- Concurrent adapter hot-swap under inference load must pass without corruption/UB.
- Keep non-FFI behavior changes minimal and avoid broad refactors.

### Claude's Discretion

- How to organize safety-comment remediation (manual + script-assisted inventory).
- Exact ASAN CI wiring in existing workflow structure.
- Which existing stress/integration test gets extended vs. where a new focused test is added.

### Deferred Ideas (OUT OF SCOPE)

- Determinism receipt envelope expansion (`DET-*`, Phase 3).
- OpenAI API compatibility features (`API-*`, Phase 4).
- Cross-request MLX KV cache feature exploration (v2).

## Phase Requirements

- `FFI-01`: All 187 unsafe blocks in `adapteros-lora-mlx-ffi` audited with SAFETY comments.
- `FFI-02`: unwrap/expect calls in non-test FFI code replaced with Result propagation.
- `FFI-03`: AddressSanitizer (ASAN) enabled in CI for FFI crate tests.
- `FFI-04`: Concurrent adapter hot-swap tested under load without memory corruption.

## Standard Stack

### Core

- Rust workspace with C++ bridge via `cc` + `bindgen` in `crates/adapteros-lora-mlx-ffi`.
- Existing CI orchestration in `.github/workflows/ci.yml`.
- Existing FFI tests in `crates/adapteros-lora-mlx-ffi/tests/` (including integration and resilience paths).

### Supporting

- Existing Miri coverage in CI (`miri_safe_tests`) for pure-Rust unsafe paths.
- Existing sanitizer profile guidance in root `Cargo.toml` comments.

## Architecture Patterns

### Pattern: Call-Site Safety Documentation

Unsafe FFI invocations already use localized `// SAFETY:` comments in parts of the crate. Phase 2 should complete this pattern uniformly, keeping rationale concrete to pointer validity, ownership/lifetime, aliasing, and thread-serialization assumptions.

### Pattern: Typed Error Boundary

Runtime FFI paths already use typed error modules (`ffi_error.rs`) and `Result` in many code paths. The phase should extend this pattern to remaining non-test panic edges (`unwrap`/`expect`) instead of introducing a new error subsystem.

### Pattern: Reuse Existing Stress Surfaces

The crate already has broad integration/e2e/resilience tests. For `FFI-04`, prefer extending existing concurrent hot-swap tests first to avoid duplicate stress harnesses.

### Anti-Patterns to Avoid

- Creating a second safety-audit report format outside phase docs.
- Adding a parallel CI workflow when existing `ci.yml` can host the ASAN lane.
- Introducing retries around unsafe failures instead of explicit error propagation.

## Don't Hand-Roll

- Do not create a new FFI wrapper abstraction layer just for this phase.
- Do not build a custom concurrency harness if existing hot-swap tests can be extended.
- Do not fork CI conventions; keep ASAN in existing workflow style.

## Common Pitfalls

### Pitfall 1: Generic SAFETY comments that do not prove invariants

- **What goes wrong:** comments exist but are non-actionable boilerplate.
- **Avoidance:** each `unsafe` call documents specific preconditions and why they hold at that site.

### Pitfall 2: Panic edges remain in non-test runtime FFI code

- **What goes wrong:** `unwrap`/`expect` in runtime FFI path can panic at unsafe boundary.
- **Avoidance:** convert to typed errors and propagate via `Result`.

### Pitfall 3: Sanitizer job exists but does not run on push

- **What goes wrong:** ASAN is manually runnable but not enforced in CI criteria.
- **Avoidance:** wire a push-triggered CI lane and fail build on violations.

### Pitfall 4: Stress test runs but misses concurrent load/unload while inference is active

- **What goes wrong:** test validates only adapter operations or only inference, not both concurrently.
- **Avoidance:** ensure overlap of inference execution with adapter load/unload churn in one scenario.

## Code Examples

### Unsafe + SAFETY audit inventory

```bash
rg -n "unsafe" crates/adapteros-lora-mlx-ffi/src
rg -n "SAFETY:" crates/adapteros-lora-mlx-ffi/src
```

### Non-test panic edge inventory

```bash
rg -n "\\b(unwrap|expect)\\(" crates/adapteros-lora-mlx-ffi/src -S
```

### ASAN check lane (target pattern)

```bash
RUSTFLAGS="-Zsanitizer=address" \
  cargo +nightly test -p adapteros-lora-mlx-ffi --tests -- --nocapture
```

## Current FFI Safety State (Verified)

### What exists now

- Phase 2 context is complete and requirements are explicit (`FFI-01..04`).
- `adapteros-lora-mlx-ffi` has broad runtime + test surfaces already present.
- CI currently executes FFI tests and Miri checks.

### What is still missing for phase exit

- Verified full unsafe-site `SAFETY` coverage against the 187-block target.
- Verified zero non-test `unwrap`/`expect` in runtime FFI source.
- ASAN lane enforced in CI on push.
- Explicitly verified concurrent hot-swap-under-load stress result tied to `FFI-04`.

## Open Questions

1. Should ASAN run on a reduced focused FFI test subset for runtime budget, or full FFI test target set?
2. Which existing test file is the canonical owner for `FFI-04` evidence (extend current e2e vs. resilience)?
3. Should unsafe/SAFETY coverage be enforced by a lightweight CI script gate in this phase or only by review checklist?

## Sources

### Primary (HIGH confidence)

- `.planning/phases/02-ffi-safety-hardening/02-CONTEXT.md`
- `.planning/ROADMAP.md`
- `.planning/REQUIREMENTS.md`
- `crates/adapteros-lora-mlx-ffi/src/`
- `.github/workflows/ci.yml`

### Secondary (MEDIUM confidence)

- `.planning/research/PITFALLS.md`

## Metadata

**Confidence breakdown:**
- Scope/requirements mapping: HIGH
- Existing test/CI entry points: HIGH
- Exact future edit count in FFI sources: MEDIUM

**Research date:** 2026-02-24
**Valid until:** 2026-03-24 (stable phase scope; validate file counts at execution time)
