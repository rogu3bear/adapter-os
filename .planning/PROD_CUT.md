# Production Cut Spec (Post-Phase 46)

## Purpose

This document is the canonical production-cut contract for AdapterOS after phase 46 closure.
It freezes scope, defines blocking gates, and sets go/no-go criteria for release.

## Frozen Scope (In Scope)

1. Route contract closure:
   - Runtime/OpenAPI closure matrix generation in `.planning/prod-cut/artifacts/`.
   - Shape-based closure policy with explicit allowlists.
2. Runtime/startup hardening:
   - Canonical startup path remains `./start` + `scripts/check-config.sh`.
   - Preflight validation before smoke execution.
3. Determinism/core correctness hardening:
   - Determinism contract remains mandatory.
   - Unseeded-randomness warnings require code fixes or explicit allowlist decisions.
4. Security/tenant isolation closure:
   - Release-safe auth posture (no dev bypass in release paths).
   - Tenant guard assertions remain blocking.
5. Reliability/incident readiness:
   - Runbook drill evidence required for worker crash, determinism violation, latency spike, memory pressure, disk full.
6. CI/gate escalation:
   - No-skip prod mode (`scripts/ci/local_release_gate_prod.sh`) is canonical for prod cut.
7. Release artifact integrity:
   - SBOM/provenance generation, signing requirements, and verification-log evidence.
8. Governance policy resolution:
   - `blocked_external` is a production release blocker.

## Explicit Deferrals (Out of Scope)

1. Net-new product feature surfaces unrelated to prod-cut gates.
2. Broad refactors that are not required for gate closure.
3. Non-blocking performance initiatives without release-gate impact.

## Required Gates and Pass Criteria

| Gate | Script / Source | Pass Criteria |
|---|---|---|
| Contract artifacts | `scripts/contracts/check_contract_artifacts.sh` | Generated artifacts are up to date and committed. |
| Route closure coverage | `scripts/ci/check_route_inventory_openapi_coverage.sh` | Runtime missing shape count = 0 after exclusions; OpenAPI-only and param mismatch counts = 0 after explicit allowlists. |
| Startup contract | `scripts/contracts/check_startup_contract.sh` | Canonical startup surfaces and preflight hooks remain intact. |
| Startup negative paths | `scripts/contracts/check_startup_negative_paths.sh` | Invalid config/occupied-port cases fail as expected. |
| Determinism contract | `scripts/contracts/check_determinism_contract.sh` | Determinism constants enforced; unseeded randomness either fixed or allowlisted with owner+expiry+rationale. |
| Security assertions | `scripts/contracts/check_release_security_assertions.sh` | Release-safe auth posture and tenant guard assertions pass; dev bypass flags not active. |
| Required checks (prod profile) | `scripts/ci/local_required_checks.sh` | Includes all-targets clippy + prod-targeted tests under `LOCAL_REQUIRED_PROFILE=prod`. |
| Release gate (prod mode) | `scripts/ci/local_release_gate_prod.sh` | Governance preflight blocks `blocked_external`; full smoke lanes run; strict inference + runbook evidence checks pass. |
| Release artifact integrity | `scripts/release/sbom.sh` | Required artifacts present; signing enforced when required; `release_verification.log` emitted and captured. |

## No-Skip Prod Mode Contract

Canonical command:

```bash
bash scripts/ci/local_release_gate_prod.sh
```

Enforced behavior:

1. `LOCAL_RELEASE_MODE=prod`
2. `LOCAL_RELEASE_RUN_INFERENCE=1`
3. `LOCAL_REQUIRED_CLIPPY_SCOPE=all-targets`
4. `SMOKE_INFERENCE_STRICT=1`
5. Full MVP smoke (no `MVP_SMOKE_SKIP_*` shortcuts)
6. Governance `blocked_external` is blocking
7. Runbook evidence strict validation
8. SBOM/provenance/signing integrity step with verification-log evidence capture

## Evidence Paths

1. Route closure artifacts:
   - `.planning/prod-cut/artifacts/runtime_routes.json`
   - `.planning/prod-cut/artifacts/openapi_routes.json`
   - `.planning/prod-cut/artifacts/route_closure_matrix.csv`
   - `.planning/prod-cut/artifacts/route_closure_summary.md`
2. Runbook drills:
   - `.planning/prod-cut/evidence/runbooks/<scenario>/`
3. Release artifact verification:
   - `.planning/prod-cut/evidence/release/release_verification.log`

## Governance Canonical Policy

Canonical policy source: `docs/governance/README.md`

Local policy and implementation must match:
1. `docs/LOCAL_CI_POLICY.md`
2. `scripts/ci/local_release_gate.sh`
3. `scripts/ci/local_release_gate_prod.sh`

Resolved rule:
`blocked_external` (`HTTP 403`) is a release blocker for production cut.

## Freeze Rule and Amendments

Scope is frozen. Any scope addition must be recorded below before implementation.

Required amendment fields:
1. `id`
2. `date`
3. `owner`
4. `change`
5. `rationale`
6. `gate_impact`
7. `approval`

### Prod Cut Amendments

| id | date | owner | change | rationale | gate_impact | approval |
|---|---|---|---|---|---|---|
| PCA-000 | 2026-03-02 | platform-release | Initial frozen production-cut contract | Transition from phase-46 closure to explicit prod release governance | Establishes required gate set and go/no-go criteria | accepted |

## Final Go/No-Go Checklist

1. Route matrix closure completed per allowlist policy.
2. `scripts/ci/local_required_checks.sh` passes in prod profile.
3. `scripts/ci/local_release_gate_prod.sh` passes end-to-end.
4. Runbook drill evidence is complete and strict-check clean.
5. SBOM/provenance/signing artifacts are present and verified.
6. Governance policy is consistent across docs + scripts (no contradiction).
