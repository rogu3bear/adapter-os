# Phase 3: Determinism Verification - Context

**Gathered:** 2026-02-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Verify end-to-end determinism guarantees for inference receipts so cryptographic reproducibility claims are defensible. This phase covers canonicalization of receipt-bound values, determinism envelope documentation for unquantized paths, runtime MLX version enforcement at boot, replay-harness verification, and CI fast-math guard enforcement only. OpenAI API expansion and broader observability work remain in later phases.

</domain>

<decisions>
## Implementation Decisions

### Receipt Canonicalization Coverage
- Treat `DET-01` as a complete dataflow audit, not a spot fix: every value entering receipt hashes must be quantized/canonicalized before digest computation.
- Canonicalization checks should be enforced at the data-boundary where receipt fields are assembled, with test vectors proving no raw floating-point leakage.
- Where existing invariants already enforce Q15 formats, extend those patterns instead of introducing parallel canonicalization utilities.

### Determinism Envelope Documentation
- `DET-02` requires a concrete map of unquantized layers and residual floating-point operations across inference/runtime paths.
- For each unquantized layer, document: source of nondeterminism risk, mitigation strategy, whether it is inside/outside receipt attestation scope, and verification status.
- Keep the envelope document implementation-aligned with code paths; avoid abstract policy prose disconnected from actual runtime behavior.

### Runtime MLX Version Enforcement
- Upgrade runtime/build version mismatch handling from warning-only to fail-fast boot behavior for strict determinism compliance (`DET-03`).
- Failure messages must be operator-actionable (build version, runtime version, remediation path), and surfaced during boot invariant checks.
- Preserve development ergonomics by scoping strict fail-fast behavior to determinism-enforcing startup modes if existing config patterns require it.

### Verification and CI Gates
- `DET-04` completion requires replay harness determinism suite coverage with explicit pass evidence and no flaky bypass path.
- `DET-05` completion requires both script-level fast-math scans and CI workflow integration confirmation.
- Prefer targeted determinism/replay checks over broad workspace suites unless a targeted check reveals cross-crate breakage.

### Claude's Discretion
- Exact receipt canonicalization helper placement and naming.
- Whether to implement fail-fast as boot invariant violation, startup guard, or runtime initialization error type.
- Replay suite command selection and test shard strategy, as long as determinism guarantees are directly validated.

</decisions>

<specifics>
## Specific Ideas

- Requirements for this phase: `DET-01`, `DET-02`, `DET-03`, `DET-04`, `DET-05`.
- Success criteria anchor points:
- all receipt-hash inputs canonicalized (Q15 or equivalent deterministic representation)
- unquantized layer inventory and determinism envelope explicitly documented
- runtime MLX version mismatch blocks boot in determinism-enforcing mode
- replay harness passes determinism suite end-to-end
- CI fast-math scans pass with no forbidden flags
- Sequencing expectation: Phase 2 FFI safety hardening is the correctness baseline for this determinism phase.

</specifics>

<deferred>
## Deferred Ideas

- OpenAI structured output and tool-calling compatibility work (Phase 4).
- Broader observability/runtime hardening deliverables (Phase 5).
- Production ops release/backup/provenance hardening beyond determinism scope (Phase 6).

</deferred>

---

*Phase: 03-determinism-verification*
*Context gathered: 2026-02-24*
