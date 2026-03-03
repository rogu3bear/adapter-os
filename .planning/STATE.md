# Session State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-02)

**Core value:** Deterministic, verifiable LoRA inference on Apple Silicon so every operator action and model response is reproducible and auditable.
**Current focus:** Prod cut execution handoff locked in `.planning/PROD_CUT.md` (post-phase-46 contract closure + release hardening).

## Position

**Milestone:** v1.1.17 Production Cut Closure (active)
**Current phase:** 47 (in progress)
**Status:** Prod cut execution receipts captured; local release gating now runs without mandatory GitHub governance dependency.

## Session Log

- 2026-02-28: Initialized and executed milestone v1.1.15 to close deferred adapter UX requirements (`UX-41-01`, `UX-41-02`) plus training contract continuity (`VC-41-01`).
- 2026-02-28: Completed phase 43 by adding repository command timeline visibility and refresh behavior in adapter detail Update Center.
- 2026-02-28: Completed phase 44 by adding command deck adapter operation parity and query-driven command-intent continuity in Update Center.
- 2026-02-28: Completed phase 45 by migrating wizard submit to typed `create_training_job` request path with dataset-version continuity.
- 2026-02-28: Reconciled PROJECT/ROADMAP/REQUIREMENTS/STATE to the same v1.1.15 completion truth and regenerated phase closure artifacts with citations.
- 2026-02-28: Bootstrapped v1.1.16 planning by creating Phase 46 roadmap/requirements entries and execution plan for training pipeline hardening.
- 2026-02-28: Began Phase 46 execution; patched training preflight for degraded-worker and active-model mismatch fail-closed behavior, then hit existing compile blocker in `services/dataset_domain.rs` during targeted check.
- 2026-02-28: Fixed compile blocker, added failure-reason fallback mapping, corrected env model defaults to `Qwen3.5-27B`, and restored server readiness on `127.0.0.1:8080`.
- 2026-02-28: Completed Phase 46 by adding deterministic fail-fast probe path, resolving KV/SQL training status divergence handling, and capturing runtime evidence for `ACTIVE_MODEL_MISMATCH`, `TRAINING_WORKER_DEGRADED`, and terminal failed-job error visibility.
- 2026-02-28: Finalized Phase 46 incomplete features with explicit dev fail-fast switch, targeted regressions, and structured completion report with citations.
- 2026-02-28: Executed targeted regression suite confirming KV/SQL terminal-status authority and API terminal-error fallback behavior.
- 2026-02-28: Added deterministic failure verification runbook and expanded KV/SQL status-authority regression coverage to both divergence directions.
- 2026-03-01: Completed post-phase-46 model/worker load-state rectification by enforcing single-ready-per-tenant at DB + projection layers, unifying active-model setter reuse across handler surfaces, tenant-scoping all control-plane status aggregation paths, and hardening workspace reconciliation to probe tenant worker sockets instead of first-worker assumptions; targeted regressions passed.
- 2026-03-01: Completed follow-up hardening to align effective model-status reads across training/infrastructure/evidence paths, tightened hot-swap gating to active-model-specific status checks, and upgraded worker reconciliation probes to require expected `active_model_id` (not just generic loaded state); targeted checks/regressions passed.
- 2026-03-02: Created `.planning/PROD_CUT.md` as canonical production-cut scope/gate contract, with explicit no-skip prod mode policy and evidence paths.
- 2026-03-02: Replaced all runbook evidence placeholders with live drill receipts for `worker_crash`, `determinism_violation`, `latency_spike`, `memory_pressure`, and `disk_full`; strict runbook evidence check now passes.
- 2026-03-02: Generated strict signed release artifacts (SBOM/provenance/signatures) and captured release verification evidence under `.planning/prod-cut/evidence/release/`.
- 2026-03-02: Captured fresh governance blocker evidence (`status=blocked_external`, exit `20`) and reran strict prod gate; gate fails at governance step by policy.
- 2026-03-02: Published final go/no-go receipt at `.planning/prod-cut/evidence/final-go-no-go.md` with explicit **NO-GO** until governance capability is restored.
- 2026-03-02: Updated local release policy to make governance preflight optional (`LOCAL_RELEASE_GOVERNANCE_MODE=off|warn|enforce`, default `off`) so local packaging is not blocked by GitHub plan capability.

## Session

**Last Date:** 2026-03-02T08:35:00Z
**Stopped At:** Governance blocker policy relaxed for local release path; governance checks are now opt-in enforcement.
**Resume File:** scripts/ci/local_release_gate.sh
