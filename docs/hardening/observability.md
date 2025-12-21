# Observability Hardening

- Goals: make failures diagnosable and compliance failures loud.
- Scope: structured logs, determinism metrics, dual-write alerting, audit export telemetry.

## Canonical events
- `receipt_mismatch_event`: emit `ReceiptMismatch` on any signature/receipt mismatch (strict alert).
- `dual_write_divergence_event`: emit `DualWriteDivergence` when SQL vs KV checksums disagree (alerts, include attempt count).
- `determinism_violation_event`: emits `DeterminismViolation` plus metric `determinism_violation_total` with labels `violation` and `strict_mode`.
- `strict_mode_failure_event`: alert when strict determinism guard fails closed or falls back.
- `audit_export_tamper_event`: alert when export bundle hash/signature differs (include `bundle_id`, expected/observed hash, export path).

All helpers return `ObservabilityEvent` and map severity to tracing; call `emit_observability_event(&event)` to guarantee structured logging with fields: `event_kind`, `severity`, `component`, `tenant_id`, `request_id`, `correlation_id`, `labels`, `detail`.

## Metrics and alerting
- Counter: `determinism_violation_total` (labels: `violation`, `strict_mode`). Wire alerts to any non-zero increment while in strict mode.
- Dual-write divergence and receipt/audit tamper events are always `Alert` level; pipeline them to incident channels.
- Determinism violations in non-strict mode log as `Warning` to preserve signal without paging.

## Acceptance checks
- Simulated tamper (mismatch hash or receipt) must emit `AuditExportTamper` or `ReceiptMismatch` with severity `Alert`.
- Dual-write mismatch (SQL vs KV checksum) must emit `DualWriteDivergence` with the failing key/table and attempt count.
- Strict determinism failure must emit `StrictModeFailure` and should not silently downgrade to warning.
- Determinism violations must bump `determinism_violation_total` and carry `violation` label for triage.

MLNavigator Inc Dec 11, 2025.
