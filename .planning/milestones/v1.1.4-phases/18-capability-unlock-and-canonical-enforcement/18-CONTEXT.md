---
phase: "18"
name: "Capability Unlock and Canonical Enforcement"
created: 2026-02-25
updated: 2026-02-26T00:42:00Z
status: discussed
---

# Phase 18: Capability Unlock and Canonical Enforcement — Context

## Decisions

- Canonical enforcement target remains fixed to `rogu3bear/adapter-os` on `main` with required context `FFI AddressSanitizer (push)`.
- Capability gate remains hard-stop: when preflight reports `blocked_external`, no branch-protection PATCH/write path is allowed.
- Enforcement closure claims require a complete read/write/readback sequence plus rollback-safe artifact path.
- Evidence packaging remains deterministic and immutable under `var/evidence/governance-enforcement-<UTCSTAMP>/`.
- Auto-execution profile remains `taste` with verifier gates enabled.
- Discuss-phase decisions (delegated by user request):
  - Polling defaults are fixed to `attempts=4` and `sleep_seconds=2` for deterministic gate checks.
  - Blocked branch must emit explicit no-write receipts (`write_attempts=0`, `policy_mutations=0`, `rollback_attempts=0`).
  - Capable branch enforcement must run through a single executor command with preserve+add context union, strict verification, and rollback on verification failure.
  - Outcome routing remains explicit: `compliant->retain`, `drifted->remediate`, `blocked_external->escalate_blocker`, `approved_exception->review_exception`.

## Discretion Areas

- Specific probe retry profile beyond defaults when operators need longer polling windows.
- Evidence directory naming strategy per run as long as UTC-stamped immutability is preserved.
- Exact report presentation shape for blocked/capable outcomes (single combined report vs paired receipts).

## Deferred Ideas

- Automatic branch-protection remediation across all parity targets before canonical capable proof is completed.
- Broad governance-domain expansion beyond required status checks in this milestone.
- Event-driven unlock triggers from external systems (manual/CI polling remains sufficient for this phase).
- Scheduled automation to rerun capable executor after plan/visibility changes.
