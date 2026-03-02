---
phase: "17"
name: "Multi-Repo Parity Proof"
created: 2026-02-25
---

# Phase 17: Multi-Repo Parity Proof — Context

## Decisions

- Phase 17 consumes Phase 16 drift-report outputs and does not redefine Phase 16 report semantics.
- Approved exceptions must remain explicit and reviewed; implicit exceptions are invalid.
- External blockers (`blocked_external`) must be preserved as first-class outcomes in parity closure artifacts.

## Discretion Areas

- Exact parity target expansion order (batch vs lane-by-lane) as long as evidence remains deterministic.
- Report presentation shape for parity closure package (single consolidated report vs per-target bundles).

## Deferred Ideas

- Automatic parity remediation (write path) remains deferred until capability and rollback requirements are approved.
