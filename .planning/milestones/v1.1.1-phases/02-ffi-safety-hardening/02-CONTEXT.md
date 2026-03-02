# Phase 2: FFI Safety Hardening - Context

**Gathered:** 2026-02-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Harden the MLX C++ FFI boundary so memory safety issues and panic-based failure paths cannot escape into the Rust runtime. This phase covers unsafe-boundary correctness and reliability hardening only; API surface expansion and determinism policy upgrades remain in later phases.

</domain>

<decisions>
## Implementation Decisions

### Safety Invariants Coverage
- Every unsafe block in `adapteros-lora-mlx-ffi` must carry a concrete SAFETY rationale tied to pointer lifetime, aliasing, ownership, and thread-safety assumptions.
- Comments must be specific to the call site; no generic copy-paste SAFETY text.
- Gaps are blockers for phase completion.

### Error Handling Policy
- Non-test FFI code removes `unwrap`/`expect` and propagates typed errors through `Result`.
- FFI failures must surface actionable diagnostics (operation, failure boundary, and likely cause), not opaque strings.
- Fallible paths prioritize containment over retry loops unless the operation is explicitly idempotent and bounded.

### Runtime Hardening Scope
- Add CI-grade validation that detects memory safety regressions (ASAN requirement from roadmap criteria).
- Include a concurrent adapter hot-swap stress scenario representative of inference load to prove boundary resilience.
- Keep behavior changes minimal outside FFI safety paths; avoid broad refactors during this phase.

### Claude's Discretion
- Exact organization of safety comments and helper abstractions.
- Concrete ASAN job wiring details and test harness implementation structure.
- Stress test fixture design and load profile, as long as it exercises concurrent load/unload under inference pressure.

</decisions>

<specifics>
## Specific Ideas

- Requirements for this phase: `FFI-01`, `FFI-02`, `FFI-03`, `FFI-04`.
- Success criteria anchor points:
- unsafe coverage with SAFETY rationale
- no non-test unwrap/expect in FFI code
- ASAN on push in CI
- concurrent hot-swap stress without corruption
- Known sequencing expectation: Phase 1 is complete and acts as compile/CI baseline for this hardening work.

</specifics>

<deferred>
## Deferred Ideas

- Determinism receipt expansion and Q15 verification details (Phase 3).
- Structured output/function-calling API compatibility work (Phase 4).
- Cross-request prompt-cache behavior exploration in MLX KV cache (deferred to v2).

</deferred>

---

*Phase: 02-ffi-safety-hardening*
*Context gathered: 2026-02-24*
