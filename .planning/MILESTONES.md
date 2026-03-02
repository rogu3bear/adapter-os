# Milestones

## v1.0 milestone (Shipped: 2026-02-24)

**Phases completed:** 7 phases, 12 plans, 0 tasks

**Key accomplishments:**
- Clean 85-crate compile baseline with clippy lint debt resolved and foundation CI/smoke scripts established
- P0 dependency upgrades (sqlx 0.8.6, safetensors 0.7, tokio 1.44) with safetensors API migrations and regenerated SQLx cache

---


## v1.1 Stability and Release Sign-off (Shipped: 2026-02-24, reconciled: 2026-02-24)

**Phases completed:** 11 phases, 24 plans, 0 tasks

**Key accomplishments:**
- Control-room rehearsal closed OPS-06 with full doctor/readiness/smoke evidence (`var/release-control-room/20260224T103206Z/`).
- Signed release bundle (SBOM + provenance + detached signatures) closed OPS-07 with traceable artifact evidence under `target/release-bundle/`.
- Final GO sign-off package and checklist reconciliation closed OPS-08 for milestone release governance.
- Phase 11 governance closure execution completed (`11-01`..`11-03`) with consolidated audit/requirements reconciliation.

**Completion-time gaps and final reconciliation:**
- UX-05 partial closure and SBOM artifact alias noise were recorded at closeout, then resolved in a post-closeout rectification pass on 2026-02-24.
- FFI-05 strict merge-gate proof remains externally gated: GitHub branch-protection required-check API is blocked by HTTP 403 in this environment.
- External blocker is now explicitly accepted and tracked as technical debt in `.planning/milestones/v1.1-MILESTONE-AUDIT.md` (status `tech_debt`), with no remaining repo-actionable closure blockers.

---

## v1.1.1 Post-v1.1 Hardening Closure (Shipped: 2026-02-25)

**Phases completed:** 3 phases (12-14), 9 plans

**Key accomplishments:**
- Codified governance retirement playbook with capability-gated outcome classes and automated governance preflight checks with deterministic CI-ready status model (GOV-06, GOV-07, GOV-08)
- Enforced determinism diagnostics freshness contract (fresh/stale/unknown) with replay guardrails in CI merge-gate and readiness surfacing (DET-07, DET-08, DET-09)
- Wired config-driven deadlock fail-safe recovery, normalized SSE breaker transition telemetry, and aligned model-server to UDS-first production contract with zero-egress validation (OBS-08, OBS-09, SEC-06)

**Known tech debt:**
- `FFI-05` strict merge-gate proof remains externally gated (`HTTP 403`). Tracked in `v1.1-MILESTONE-AUDIT.md`.

---

## v1.1.2 Governance Retirement Enforcement (Executed: 2026-02-25, blocked branch)

**Phases completed:** 1 phase (15), 3 plans

**Key accomplishments:**
- Executed full capability-gated governance retirement workflow for the canonical target (`rogu3bear/adapter-os`, `main`, `FFI AddressSanitizer (push)`).
- Captured immutable evidence bundles across baseline, resumed retries, and plan-level enforcement attempts under `var/evidence/governance-retirement-*`.
- Preserved deterministic no-write behavior while capability remained blocked (`HTTP 403`), then reconciled planning/state/requirements/audit narratives to the same observed truth.
- Completed phase-level verification and UAT on the accepted external-blocker branch (no contradictory closure claims).

**Known tech debt:**
- Strict branch-protection read/write/read enforcement proof remains externally gated by GitHub plan/visibility capability for private repositories.

---

## v1.1.3 Governance Drift Guardrails (Shipped: 2026-02-25, audited: 2026-02-25)

**Phases completed:** 2 phases (16-17), 6 plans, 0 tasks

**Key accomplishments:**
- Implemented deterministic read-only governance drift detection with manifest validation and machine/human evidence outputs (`GOV-13`).
- Wired check-only CI workflow + operator runbook/checklist for reproducible governance drift response.
- Executed approved multi-repo parity proof with explicit `approved_exception` receipts and final acceptance transcript (`OPS-09`).
- Anchored milestone audit truth at `.planning/milestones/v1.1.3-MILESTONE-AUDIT.md` with transparent external-blocker posture.

**Known tech debt:**
- Strict required-check enforcement capability remains externally blocked (`HTTP 403`) across approved targets; parity remains on the approved-exception branch until external capability changes.

---

## v1.1.4 Governance Capability Unlock and Enforcement Closure (Executed: 2026-02-26, audited: 2026-02-26)

**Phases completed:** 2 phases (18-19), 6 plans

**Key accomplishments:**
- Added deterministic canonical capability polling with immutable loop/gate-state evidence and blocked-branch no-write receipts (`GOV-14`).
- Executed branch-safe enforcement posture where blocked capability produced explicit `write_attempts=0` and capable-path execution contract artifacts (`GOV-15` branch-aware closure).
- Graduated multi-repo governance outputs to deterministic target matrix + operator routing receipts (`OPS-10`).
- Anchored milestone audit at `.planning/milestones/v1.1.4-MILESTONE-AUDIT.md` with transparent external-blocker posture.

**Known tech debt:**
- Canonical capable write/readback enforcement branch remains externally blocked (`HTTP 403`) in current repository/plan context.
- Approved target set remains on explicit `approved_exception` posture from raw `blocked_external` outcomes until capability changes.

---

## v1.1.5 Governance Capability Activation and Debt Retirement (Executed: 2026-02-26, audited: 2026-02-26)

**Phases completed:** 2 phases (20-21), 6 plans

**Key accomplishments:**
- Re-ran canonical capability-aware governance flow with fresh deterministic evidence bundles for capability gating, executor branch outcome, and graduation routing receipts.
- Preserved strict blocked-branch no-write behavior while reconciling milestone/governance/checklist narratives to observed truth.
- Published closure evidence and phase verification/UAT artifacts under phase-20/21 directories and `var/evidence/governance-*-20260226T*/`.

**Accepted external debt:**
- `GOV-16` capable-path proof remains externally blocked (`HTTP 403`) and is explicitly accepted debt, tracked in `.planning/milestones/v1.1.5-MILESTONE-AUDIT.md`.

---

## v1.1.7 Adapter Git-Like Version Control and Dataset Feed Workflows (Shipped: 2026-02-28)

**Phases completed:** 3 phases (31-33), 3 plans

**Key accomplishments:**
- Shifted adapter update workflows to checkout-first semantics while preserving rollback compatibility.
- Wired dataset-feed actions from version controls into training flow with branch/version context continuity.
- Reconciled planning truth (`PROJECT`/`REQUIREMENTS`/`ROADMAP`/`STATE`) with execution truth and validated milestone closure via health/artifact/key-link checks.

---

## v1.1.8 Assistive AdapterOps Foundation and Guided Operator Flow (Shipped: 2026-02-28)

**Phases completed:** 1 phases, 1 plans, 0 tasks

**Key accomplishments:**
- Added explicit `Quick operator guide` and dynamic `Recommended Next Action` messaging in adapter detail update workflows.
- Clarified guided-flow resume language on dashboard and improved high-impact action `aria_label` coverage across core operator surfaces.
- Verified assistive UX changes with targeted compile and planning-health checks.

---


## v1.1.9 Adapter Git Command Surface and Feed Automation (Shipped: 2026-02-28)

**Phases completed:** 1 phases, 1 plans, 0 tasks

**Key accomplishments:**
- Added git-style command-map guidance for checkout/promote/feed operations in adapter detail and update center surfaces.
- Tightened command-oriented natural-language guidance across dashboard/update/detail paths for faster operator intent matching.
- Preserved branch/version-aware dataset feed continuity cues into training-entry workflows with targeted verification evidence.

---

## v1.1.10 Command Deck Validation and Assistive Refinement (Shipped: 2026-02-28)

**Phases completed:** 1 phases, 1 plans, 0 tasks

**Key accomplishments:**
- Closed residual command vocabulary drift by aligning promote/checkout language across adapter detail recommendations, action labels, and confirmations.
- Validated command-map and assistive guidance consistency across dashboard/update/detail surfaces without changing backend semantics.
- Re-verified command-first flow continuity with targeted compile plus artifact/key-link/health checks.

---

## v1.1.11 Operator Command Assistive Workflow Extension (Shipped: 2026-02-28)

**Phases completed:** 1 phases, 1 plans, 0 tasks

**Key accomplishments:**
- Finalized command-first default-path guidance across dashboard, update center, and adapter detail with low-ambiguity operator phrasing.
- Standardized selected-version command labels to align with list-level command actions (`Run Promote`, `Run Checkout`).
- Re-verified assistive and continuity wording with targeted compile, artifact/key-link, and planning-health checks.

---

## v1.1.12 Operator Command Assistive Continuity Finalization (Shipped: 2026-02-28)

**Phases completed:** 1 phases, 1 plans, 0 tasks

**Key accomplishments:**
- Finalized concise command-first default-path language across adapter detail, dashboard guided flow, and update center workflows.
- Tightened recommended-action phrasing to reduce ambiguity while preserving explicit `Run Promote` / `Run Checkout` semantics.
- Re-verified command-assistive continuity with targeted compile, artifact/key-link checks, and healthy planning status.

---

## v1.1.13 Operator Command Guidance Stability Pass (Shipped: 2026-02-28)

**Phases completed:** 1 phases, 1 plans, 0 tasks

**Key accomplishments:**
- Executed the final command-guidance stability pass for phase 39 with closure artifacts (`39-01-SUMMARY.md`, `39-VERIFICATION.md`, `39-UAT.md`).
- Preserved checkout-first operator wording across adapter detail/dashboard/update-center command surfaces.
- Removed residual Update Center discoverability drift by aligning nav keywords and command palette copy with checkout/feed-dataset and run-history terminology.
- Re-verified with targeted compile plus GSD artifact/key-link/consistency/health checks.

---

## v1.1.14 AdapterOps Command Language and Assistive Continuity (Shipped: 2026-02-28)

**Phases completed:** 3 phases (40-42), 3 plans, 0 tasks

**Key accomplishments:**
- Harmonized checkout-first command vocabulary and recommended default-path guidance across dashboard, update center, and adapter detail surfaces.
- Preserved dataset-feed provenance continuity by carrying `repo_id`, `branch`, and `source_version_id` context into training-entry launches with explicit operator messaging.
- Closed assistive guidance parity with consistent promote/checkout accessible names and explicit lineage-toggle labeling, backed by citation-grounded verification/UAT artifacts.
- Reconciled milestone planning truth in `ROADMAP`, `REQUIREMENTS`, and `STATE`, with consistency/health checks passing after closure.

---

## v1.1.15 AdapterOps Timeline, Command Deck Parity, and Dataset Version Pinning (Shipped: 2026-02-28)

**Phases completed:** 3 phases (43-45), 3 plans, 0 tasks

**Key accomplishments:**
- Added repository command timeline visibility in adapter detail Update Center with refresh-on-promote/checkout behavior (`UX-41-01`).
- Added command deck adapter operation parity (`Run Promote`, `Run Checkout`, `Feed Dataset`) with selected-adapter intent continuity into Update Center (`UX-41-02`).
- Migrated training wizard submit flow to typed `CreateTrainingJobRequest` and carried `dataset_version_id` when available (`VC-41-01`).
- Reconciled all active planning artifacts and phase closure docs with code citations and best-practice references (`DOC-41-01`).

---
