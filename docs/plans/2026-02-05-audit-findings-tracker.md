# Audit Findings Tracker

**Created**: 2026-02-05
**Last Updated**: 2026-02-16
**Purpose**: Track all issues identified in Logic, Design, and Topography audits

---

## Summary

| Category | Critical | Moderate | Minor | Total |
|----------|----------|----------|-------|-------|
| Logic    | 3        | 6        | 4     | 13    |
| Design   | 0        | 3        | 7     | 10    |
| Topo     | 3        | 3        | 3     | 9     |
| **Total**| **6**    | **12**   | **14**| **32**|

---

## Backend Rectification (Items 1-10, 2026-02-16)

The backend/E2E rectification pass for Items 1-10 was completed on 2026-02-16 and is tracked separately from the audit issue inventory above.

| Item | Status | Implementation Note |
|------|--------|---------------------|
| 1 | Resolved | Evidence ingestion now performs real Ed25519 signature verification (no format-only acceptance path). |
| 2 | Resolved | Default backend profile is embeddings-capable; default builds no longer emit 501 "not implemented" for document/dataset embedding paths. |
| 3 | Resolved | Operation tracker supports Redis-backed start/update/complete/status with TTL and conflict detection; local cancellation semantics retained. |
| 4 | Resolved | Dedicated embedding execution path is implemented in worker embedding runtime. |
| 5 | Resolved | `X-Signal-Stream` requests now emit and parse SSE `event: signal` lifecycle frames end-to-end. |
| 6 | Resolved | Secure enclave attestation is explicit (`hardware` vs `synthetic`) and fail-closed when hardware attestation is required. |
| 7 | Resolved | Rotation daemon KMS provider mode is enabled via concrete key-provider wiring. |
| 8 | Resolved | Inference spoke handlers/routes delegate to real production inference handlers (no placeholder payloads). |
| 9 | Resolved | Worker determinism guards are re-enabled with active initialization and violation accounting. |
| 10 | Resolved | Documentation and verification notes reconciled with current backend/runtime behavior. |

---

## Logic Audit Issues

### [C-1] Determinism Violation: partial_cmp in Router Sorting
- **Severity**: Critical
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-lora-router/src/router.rs`
- **Description**: Use of `partial_cmp` for floating-point comparison in router gate scoring can produce non-deterministic ordering due to NaN handling differences across platforms.
- **Fix**: Replace with total ordering comparison using `f32::total_cmp()` or wrap in `OrderedFloat`.

### [C-2] Determinism Violation: partial_cmp in Adapter Selection
- **Severity**: Critical
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-lora-router/src/selection.rs`
- **Description**: Adapter selection logic uses `partial_cmp` which can yield inconsistent results when scores contain edge-case floating-point values.
- **Fix**: Implement total ordering with explicit NaN handling per determinism rules.

### [C-3] Determinism Violation: partial_cmp in Score Aggregation
- **Severity**: Critical
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-lora-router/src/aggregation.rs`
- **Description**: Score aggregation uses partial ordering, violating determinism guarantees for replay.
- **Fix**: Use total ordering comparisons; add property tests for edge cases.

### [M-1] Missing Seed Propagation in Batch Inference
- **Severity**: Moderate
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-server-api/src/handlers/batch.rs`
- **Description**: Batch inference requests may not properly propagate deterministic seeds to all sub-requests.
- **Fix**: Ensure seed derivation (HKDF-SHA256) is applied consistently per batch item.

### [M-2] Race Condition in Adapter Hot-Reload
- **Severity**: Moderate
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-lora-worker/src/directory_adapters.rs`
- **Description**: Hot-reload of adapters has potential race between read and swap operations.
- **Fix**: Add atomic swap with generation counter or RCU pattern.

### [M-3] Incomplete Error Code Coverage
- **Severity**: Moderate
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-core/src/error_codes.rs`, various handlers
- **Description**: Some error paths return generic errors without proper error codes.
- **Fix**: Audit all error returns and assign appropriate error codes.

### [M-4] Token Cache Invalidation Gap
- **Severity**: Moderate
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-memory/src/cross_backend.rs`
- **Description**: Token cache may serve stale entries after adapter promotion.
- **Fix**: Add cache invalidation hook to promotion workflow.

### [M-5] Checkpoint Corruption on SIGTERM
- **Severity**: Moderate
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-lora-worker/src/training/checkpoint.rs`
- **Description**: Training checkpoints may be partially written if SIGTERM arrives mid-write.
- **Fix**: Implement atomic write with rename pattern.

### [M-6] Policy Evaluation Short-Circuit
- **Severity**: Moderate
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-policy/src/evaluator.rs`
- **Description**: Policy evaluation may short-circuit without logging denied policies for audit.
- **Fix**: Ensure all policy evaluations are logged regardless of outcome.

### [m-1] Unused Feature Flag Combinations
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: `Cargo.toml` (workspace)
- **Description**: Some feature flag combinations are defined but never tested in CI.
- **Fix**: Add CI matrix for all supported feature combinations.

### [m-2] Inconsistent Logging Levels
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: Various crates
- **Description**: Some crates use `info!` for debug-level messages, cluttering production logs.
- **Fix**: Audit and normalize logging levels per severity guidelines.

### [m-3] Missing Doc Comments on Public APIs
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-api-types/src/`
- **Description**: Several public types lack rustdoc comments.
- **Fix**: Add documentation for all public API types.

### [m-4] Test Cleanup in Temp Directories
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: Various test files
- **Description**: Some tests create temp directories without cleaning up on failure.
- **Fix**: Use `tempfile` crate with auto-cleanup or explicit drop guards.

---

## Design Audit Issues

### [D-1] Duplicate CSS Definitions
- **Severity**: Moderate
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-ui/dist/glass.css`, `crates/adapteros-ui/dist/components.css`
- **Description**: Multiple CSS files define overlapping styles for the same components, leading to specificity conflicts and maintenance burden.
- **Fix**: Consolidate duplicate definitions; establish single source of truth per component.

### [D-2] Hardcoded Blur Values
- **Severity**: Moderate
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-ui/dist/glass.css`
- **Description**: Blur values are hardcoded throughout instead of using CSS custom properties, making Liquid Glass tier adjustments difficult.
- **Fix**: Define blur values as CSS variables (`--glass-blur-tier-1`, etc.) per design system spec.

### [D-3] Accessibility: Missing ARIA Labels
- **Severity**: Moderate
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-ui/src/components/`
- **Description**: Several interactive components lack proper ARIA labels for screen readers.
- **Fix**: Add `aria-label`, `aria-describedby`, and role attributes to all interactive elements.

### [D-4] Accessibility: Insufficient Color Contrast
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-ui/dist/glass.css`
- **Description**: Some text-on-glass combinations may not meet WCAG AA contrast ratios.
- **Fix**: Audit contrast ratios; adjust alpha values or add text shadows for legibility.

### [D-5] Accessibility: Missing Focus Indicators
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-ui/dist/components.css`
- **Description**: Custom focus styles override browser defaults without providing visible alternatives.
- **Fix**: Ensure all focusable elements have visible focus indicators.

### [D-6] Motion Violations: Borderline Idle Animations
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-ui/src/components/spinner.rs`
- **Description**: Spinner animation may be considered an idle animation; Liquid Glass spec says "state-change only, no idle animations."
- **Fix**: Review spinner usage; ensure it only appears during active operations.

### [D-7] Component: Inconsistent Button Variants
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-ui/src/components/button.rs`
- **Description**: Button component has inconsistent variant naming between code and CSS classes.
- **Fix**: Align variant names with design system vocabulary.

### [D-8] Component: Table Missing Responsive Behavior
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-ui/src/components/table.rs`
- **Description**: Table component does not handle narrow viewports gracefully.
- **Fix**: Add horizontal scroll or responsive column hiding.

### [D-9] Component: Form Field Error State Styling
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-ui/src/components/form_field.rs`
- **Description**: Form field error states use red color alone without iconography or ARIA.
- **Fix**: Add error icon and `aria-invalid` attribute.

### [D-10] Noise Pattern Not Applied Consistently
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-ui/dist/glass.css`
- **Description**: Liquid Glass spec requires 2% opacity noise pattern; some glass surfaces lack it.
- **Fix**: Apply noise overlay consistently to all glass-tier elements.

---

## Topography Audit Issues

### [T-1] Layer Breach: UI Imports DB Types Directly
- **Severity**: Critical
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-ui/src/`, `crates/adapteros-db/`
- **Description**: UI crate has direct dependency on DB types, bypassing the API types layer and breaking clean architecture boundaries.
- **Fix**: Route all UI data needs through `adapteros-api-types`.

### [T-2] Layer Breach: Worker Calls Server-API Handlers
- **Severity**: Critical
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-lora-worker/src/`, `crates/adapteros-server-api/src/handlers/`
- **Description**: Worker crate imports handler functions from server-api instead of using HTTP/UDS client.
- **Fix**: Workers should communicate with control plane via defined API, not direct function calls.

### [T-3] Layer Breach: Core Depends on Config
- **Severity**: Critical
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-core/Cargo.toml`
- **Description**: Core crate has dependency on config crate, but core should be dependency-minimal.
- **Fix**: Move config-dependent code to higher layer; core should only define types and traits.

### [T-4] Coupling: Circular Feature Dependencies
- **Severity**: Moderate
- **Status**: Open
- **Owner**:
- **Files**: `Cargo.toml` (workspace)
- **Description**: Some feature flags create circular dependencies when combined.
- **Fix**: Audit feature graph; break cycles with intermediate crates if needed.

### [T-5] Coupling: Shared Mutable State in Registry
- **Severity**: Moderate
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-db/src/adapters/mod.rs`
- **Description**: Adapter registry uses shared mutable state accessible from multiple crates.
- **Fix**: Introduce registry trait with explicit boundaries.

### [T-6] Coupling: Telemetry Spread Across Crates
- **Severity**: Moderate
- **Status**: Open
- **Owner**:
- **Files**: Various crates
- **Description**: Telemetry instrumentation is inconsistently applied; some crates bypass telemetry crate.
- **Fix**: Centralize telemetry through single crate with clear API.

### [T-7] Organization: Tests in Wrong Location
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: Various `tests/` directories
- **Description**: Some integration tests are in crate `tests/` when they should be in workspace `tests/`.
- **Fix**: Move cross-crate integration tests to workspace level.

### [T-8] Organization: Duplicate Utility Functions
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: Various `util.rs` files
- **Description**: Similar utility functions are duplicated across crates.
- **Fix**: Consolidate into `adapteros-core` or dedicated utils crate.

### [T-9] Organization: Inconsistent Module Structure
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: Various crates
- **Description**: Some crates use `mod.rs` pattern, others use filename modules inconsistently.
- **Fix**: Standardize on one module organization pattern.

### [T-10] Feature Flag: Unused metal-backend Flag
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: `Cargo.toml`
- **Description**: `metal-backend` feature flag is defined but code paths are incomplete.
- **Fix**: Either complete implementation or remove flag.

### [T-11] Feature Flag: mlx Always Required
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: `crates/adapteros-lora-mlx-ffi/`
- **Description**: MLX is listed as optional but build fails without it on macOS.
- **Fix**: Make dependency explicit or fix conditional compilation.

### [T-12] Build Efficiency: Slow Incremental Builds
- **Severity**: Minor
- **Status**: Open
- **Owner**:
- **Files**: Workspace `Cargo.toml`
- **Description**: Incremental builds are slower than expected due to wide dependency graph.
- **Fix**: Audit dependency tree; consider splitting hot crates.

---

## Progress Tracking

### By Status

| Status      | Count |
|-------------|-------|
| Open        | 32    |
| In Progress | 0     |
| Fixed       | 0     |

### By Priority

| Priority | Issues |
|----------|--------|
| Critical | C-1, C-2, C-3, T-1, T-2, T-3 |
| Moderate | M-1 through M-6, D-1, D-2, D-3, T-4, T-5, T-6 |
| Minor    | m-1 through m-4, D-4 through D-10, T-7 through T-12 |

---

## Change Log

| Date | Change |
|------|--------|
| 2026-02-05 | Initial tracker created with 32 issues |
