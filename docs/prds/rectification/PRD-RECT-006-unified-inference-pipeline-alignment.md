# PRD-RECT-006: Unified Inference Pipeline Alignment

## Status
- **Status**: Draft
- **Author**: rogu3bear
- **Date**: Dec 2025
- **Tracking**: STAB-GAP-REPORT

## 1. Overview
The current inference pipeline in `crates/adapteros-server-api/src/inference_core.rs` advertises an 11-stage process in its documentation, but the implementation had several gaps, most notably the policy hooks (Stages 3, 7, and 9) which were effectively no-op stubs. This PRD formalizes the integration of these hooks and aligns the implementation with the documented "Northstar" architecture.

## 2. Goals
- **Full Policy Enforcement**: Ensure `PolicyEngine` is invoked at all critical pipeline stages.
- **Deterministic Replay**: Capture policy decisions in a `policy_mask_digest` to ensure replayed inferences use the same constraints.
- **Pipeline Consistency**: Align the code flow with the 11-stage diagram.

## 3. Implementation Details

### 3.1. Policy Hook Integration
- **Stage 3 (OnRequestBeforeRouting)**:
  - Invoked after adapter resolution but before RAG retrieval.
  - Checks input prompt safety, egress rules, and tenant resource budgets.
  - Rejection here prevents all downstream computation.
- **Stage 7 (OnBeforeInference)**:
  - Invoked after worker selection.
  - Final validation of the resolved worker, placement constraints, and quota enforcement.
  - Captures the aggregate policy decisions into a `policy_mask_digest`.
- **Stage 9 (OnAfterInference)**:
  - Invoked after the worker call returns.
  - Validates output safety and applies any post-inference filtering.

### 3.2. Data Structure Updates
- `InferenceRequestInternal`: Added `policy_mask_digest` and `claims`.
- `WorkerInferRequest`: Added `policy_mask_digest` to propagate constraints to the worker.
- `InferenceResult`: Added expert routing and model type metadata for observability.
- `InferenceError`: Added `PolicyViolation` variant for clear error reporting.

### 3.3. Replay & Determinism
- The `policy_mask_digest` must be stored in `replay_metadata` (via `policy_mask_digest_b3`).
- Replay inferences must verify that the current policy environment produces a compatible mask.

## 4. Success Criteria
- [x] Policy hooks are wired into `route_and_infer`.
- [x] All policy decisions are logged to `all_policy_decisions`.
- [x] `policy_mask_digest` is computed and passed to the worker.
- [x] Replay metadata includes the policy mask digest.
- [x] Test suite covers policy hook execution flow.

## 5. File Boundaries
- `crates/adapteros-server-api/src/inference_core.rs` (Primary implementation)
- `crates/adapteros-server-api/src/types.rs` (Data structures)
- `crates/adapteros-db/src/replay_metadata.rs` (Storage)
- `crates/adapteros-policy/src/hooks.rs` (Policy logic)
