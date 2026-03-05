# Phase 47 Context: Production Cut Contract Closure

## Problem Framing

Phase 47 closes production-cut release blockers by enforcing strict governance posture, startup/runtime readiness gates, route contract checks, and signed release evidence requirements.

## Operator Intent

1. Run one deterministic production-cut gate path and receive a definitive pass/fail outcome.
2. Fail closed on governance blocker states (`blocked_external`) unless an explicit override is chosen.
3. Keep startup readiness errors actionable and early (preflight before expensive launches).

## Constraints

1. Preserve canonical release workflow in `scripts/ci/local_release_gate_prod.sh`.
2. Avoid broad refactors; keep changes localized to scripts/docs/planning artifacts.
3. Keep historical receipts immutable; only update current policy and execution artifacts.

## Phase Citations

1. `/Users/star/Dev/adapter-os/.planning/PROD_CUT.md`
2. `/Users/star/Dev/adapter-os/scripts/ci/local_release_gate.sh`
3. `/Users/star/Dev/adapter-os/scripts/ci/local_release_gate_prod.sh`
4. `/Users/star/Dev/adapter-os/docs/governance/README.md`

