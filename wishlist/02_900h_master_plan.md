# 900-Hour Deep Research + Execution Master Plan

## Planning Contract
- Goal: convert Phase 1 evidence into a sequential, execution-ready roadmap that maximizes leverage from existing AdapterOS assets.
- Non-goals: broad refactors, duplicate implementations, unbounded parallel churn.
- Constraints: existing-code-first, minimal diffs, determinism-safe, repo hygiene compliant.
- Stop condition: all phases below reach done criteria and Phase 9 closeout report is complete.

## Global Anti-Duplication Rules
- Extend canonical sources first: `docs/CANONICAL_SOURCES.md`, generated contract artifacts, existing route/nav/seed services.
- One owner per touched file cluster per phase.
- No new scanner/tool if an existing script can be extended.
- Every initiative must include "reuse anchor" files before any new file is proposed.

## Team Model
- `Team A Runtime/Determinism`
- `Team B Control Plane/API`
- `Team C UI/UX`
- `Team D Docs/Contracts/CI`
- `Team E Integration Sheriff` (cross-team gatekeeper, no domain ownership)

## Hour Budget (Total: 900)
- Phase 1 completed (reading/evidence): 60h equivalent.
- Phase 2 completed (planning): 80h equivalent.
- Phase 3 through Phase 9 remaining execution budget: 760h.

## Sequential Phases

### Phase 3 (120h): Determinism Surface Unification
- Owner: Team A.
- Reuse anchors:
  - `crates/adapteros-core/src/seed.rs`
  - `crates/adapteros-core/src/determinism.rs`
  - `crates/adapteros-deterministic-exec/src/seed.rs`
  - `crates/adapteros-deterministic-exec/src/global_ledger.rs`
- Deliverables:
  - Shared seed-propagation contract doc.
  - Consolidated determinism telemetry vocabulary map.
  - Targeted tests covering seed/ledger consistency behavior.
- Done criteria:
  - Seed and ledger semantics documented with no contradictory definitions.
  - Determinism tests selected and passing for changed areas.

### Phase 4 (120h): Control Plane Contract Convergence
- Owner: Team B.
- Reuse anchors:
  - `crates/adapteros-server/src/config.rs`
  - `crates/adapteros-server-api/src/config.rs`
  - `crates/adapteros-server/src/boot/background_tasks.rs`
  - `crates/adapteros-server-api/src/progress_service.rs`
- Deliverables:
  - Config drift matrix + consolidation proposal.
  - Background task status model reusing `BackgroundTaskTracker`.
  - Readiness invariant map tying boot and `readyz` behavior.
- Done criteria:
  - Explicit single-source-of-truth map for config and readiness semantics.

### Phase 5 (110h): UI Route/Action Source-of-Truth Alignment
- Owner: Team C.
- Reuse anchors:
  - `crates/adapteros-ui/src/components/layout/nav_registry.rs`
  - `crates/adapteros-ui/src/search/contextual.rs`
  - `crates/adapteros-ui/src/search/index.rs`
  - `crates/adapteros-ui/src/components/workspace.rs`
- Deliverables:
  - Route/action parity matrix.
  - Shared metadata model proposal for nav + command palette.
  - Layout primitive adoption plan for highest-churn pages.
- Done criteria:
  - No undocumented route/action duplication hotspots remain.

### Phase 6 (90h): Static UX + Boot Diagnostics Harmonization
- Owner: Team C with Team D support.
- Reuse anchors:
  - `crates/adapteros-server/static/index.html`
  - `crates/adapteros-server/static-minimal/index-minimal.html`
  - `crates/adapteros-server/static-minimal/api-test.html`
- Deliverables:
  - Shared boot-diagnostic snippet strategy.
  - Mismatch checklist and remediation sequence.
- Done criteria:
  - Static and minimal surfaces share canonical diagnostics behavior.

### Phase 7 (120h): Docs/Contracts/CI Reliability Hardening
- Owner: Team D.
- Reuse anchors:
  - `scripts/contracts/generate_contract_artifacts.py`
  - `scripts/contracts/check_docs_claims.sh`
  - `scripts/ci/check_openapi_drift.sh`
  - `docs/CANONICAL_SOURCES.md`
- Deliverables:
  - Unified docs-claims gate strategy.
  - Legacy validator overlap reduction plan.
  - Contract artifact extension plan for newly tracked invariants.
- Done criteria:
  - CI checks are non-overlapping and traceable to canonical artifacts.

### Phase 8 (110h): Security + Tenant + Replay Proof Pass
- Owner: Team A + Team B.
- Reuse anchors:
  - `docs/VERIFIED_REPO_FACTS.md`
  - `docs/DETERMINISM.md`
  - `docs/POLICIES.md`
  - `crates/adapteros-server-api/src/handlers/replay*.rs`
- Deliverables:
  - Updated claim-vs-source evidence table.
  - Replay endpoint consolidation readiness checklist.
  - Tenant isolation and determinism proof gaps list.
- Done criteria:
  - High-risk claims are either source-backed or explicitly marked as gap/deferred.

### Phase 9 (90h): Integration Sheriff + Release Readiness
- Owner: Team E.
- Reuse anchors:
  - `docs/generated/*.json`
  - `scripts/ci/stability.sh`
  - `scripts/ci/check_route_inventory_openapi_coverage.sh`
- Deliverables:
  - Cross-phase integration conflict report.
  - Final verification command pack.
  - Release readiness memo with residual risks.
- Done criteria:
  - All prior phase outputs integrated with no unresolved ownership collisions.

## Sequential Gate Checklist
- [x] Phase 1 reading evidence complete.
- [x] Phase 2 plan complete.
- [x] Phase 3 complete.
- [x] Phase 4 complete.
- [x] Phase 5 complete.
- [x] Phase 6 complete.
- [x] Phase 7 complete.
- [x] Phase 8 complete.
- [x] Phase 9 complete.

## Core Verification Pack (Minimal, Expand As Needed)
- `rg -n "derive_seed|DeterminismContext|GlobalTickLedger|ROUTER_GATE_Q15_DENOM" crates/adapteros-core crates/adapteros-deterministic-exec crates/adapteros-lora-router`
- `rg -n "Config|RoutingConfig|readyz|BackgroundTaskTracker|ProgressService" crates/adapteros-server crates/adapteros-server-api`
- `rg -n "nav_registry|contextual|command_palette|Workspace" crates/adapteros-ui/src`
- `scripts/contracts/check_docs_claims.sh`
- `scripts/ci/check_openapi_drift.sh`
