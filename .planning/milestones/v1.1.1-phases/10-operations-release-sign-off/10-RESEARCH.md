# Phase 10: Operations Release Sign-off - Research

**Researched:** 2026-02-24
**Domain:** Release operations sign-off, evidence packaging, and artifact attestation
**Confidence:** HIGH
**Status:** Executed and reconciled (historical planning research)

## Reconciled Execution State (2026-02-24)

This research document preserves planning-time release-gate risk framing. Phase 10 is complete and reconciled with passing control-room evidence, signed release artifacts, and finalized GO/NO-GO packaging.

## Summary

At planning time, Phase 10 closed the final release-readiness gaps left by Phase 06. The repository already had the required control surfaces (`mvp_control_room.sh`, `sbom.sh`, checklist docs), but prior evidence showed one blocking execution failure and one signing gap: control-room doctor failed because no server was reachable at `http://localhost:8080`, and SBOM/provenance were generated unsigned.

This phase should not add new operational paths. The lowest-risk strategy is to execute the existing scripts with stricter input discipline: require explicit live `BASE_URL`, require `RELEASE_SIGNING_KEY_PEM`, and package fresh evidence into checklist-backed sign-off artifacts.

**Primary recommendation:** Use a 3-plan wave model where control-room and signed artifact generation run in parallel first, then checklist reconciliation and final GO/NO-GO packaging run as a dependent closeout plan.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Use existing operational scripts and checklist surfaces only; no parallel process.
- `OPS-06` requires successful control-room rehearsal against a reachable live server.
- `OPS-07` requires signed SBOM/provenance outputs (not unsigned placeholders).
- `OPS-08` requires checklist reconciliation with fresh evidence links and final GO/NO-GO packaging.
- Planning must include three plans with explicit wave/dependency modeling and concrete verification commands.

### Claude's Discretion
- Exact preflight method to validate `BASE_URL` reachability.
- Minimal drift fixes to scripts/docs if they block evidence closure.
- Evidence package structure, as long as OPS requirement traceability is explicit.

### Deferred Ideas (OUT OF SCOPE)
- New release tooling or process redesign outside current script/checklist system.
- Product feature or UX work unrelated to operations sign-off.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Requirement | Evidence Anchor | Plan Coverage |
|----|-------------|-----------------|---------------|
| OPS-06 | Control-room rehearsal succeeds end-to-end against reachable server (`aosctl doctor` and readiness checks pass) | `scripts/release/mvp_control_room.sh`, Phase 06 summary failure at doctor step due connection refused | `10-01` |
| OPS-07 | SBOM/provenance artifacts generated with signing enabled (`RELEASE_SIGNING_KEY_PEM`) | `scripts/release/sbom.sh` signing path emits `signature.sig` + `build_provenance.sig` when key is set | `10-02` |
| OPS-08 | MVP checklist required items reconciled/closed with fresh evidence links | `MVP_PROD_CHECKLIST.md`, `MVP_PROD_CONTROL_ROOM.md`, control-room evidence logs, signed release bundle artifacts | `10-03` |
</phase_requirements>

<plan_dependency_model>
## Plan Dependency Model

| Plan | Wave | Depends On | Requirement Focus | Why This Shape |
|------|------|------------|-------------------|----------------|
| 10-01 | 1 | none | OPS-06 | Establishes live control-room evidence and clears known doctor reachability blocker. |
| 10-02 | 1 | none | OPS-07 | Produces signed release artifacts independently from control-room run timing. |
| 10-03 | 2 | 10-01, 10-02 | OPS-08 | Reconciles checklist and composes final GO/NO-GO package only after both evidence streams are complete. |
</plan_dependency_model>

## Standard Stack

| Surface | Canonical Path | Role in Phase 10 |
|---------|----------------|------------------|
| Control-room runner | `scripts/release/mvp_control_room.sh` | Ordered rehearsal with timestamped evidence logging |
| Artifact generator | `scripts/release/sbom.sh` | SBOM/provenance creation and optional signing |
| Release checklist | `MVP_PROD_CHECKLIST.md` | Required launch gate reconciliation |
| Control-room playbook | `MVP_PROD_CONTROL_ROOM.md` | Decision-card and operator run sequencing |
| Evidence storage | `var/release-control-room/<timestamp>/` and `target/release-bundle/` | Primary artifacts for sign-off traceability |

## Architecture Patterns

### Pattern: Reachability first, then doctor
- Validate `BASE_URL` health/readiness before running control-room sequence.
- This directly addresses the prior `Connection refused` doctor failure mode.

### Pattern: Signed artifacts as release contract
- Treat signature outputs as required artifacts, not optional extras.
- `signature.sig` and `build_provenance.sig` are acceptance checks for OPS-07.

### Pattern: Evidence-backed checklist closure
- Checklist reconciliation must point to concrete, fresh artifacts from this phase run.
- GO/NO-GO decision is invalid without artifact-backed rationale.

## Do Not Hand-Roll

| Problem | Do Not Build | Use Instead | Why |
|---------|--------------|-------------|-----|
| Rehearsal orchestration | New custom launch sequence | `scripts/release/mvp_control_room.sh` | Already encodes ordering and evidence logging |
| Provenance/signing flow | Custom SBOM signer | `scripts/release/sbom.sh` with `RELEASE_SIGNING_KEY_PEM` | Existing deterministic and signing-aware implementation |
| Sign-off gate | Separate ad-hoc checklist | `MVP_PROD_CHECKLIST.md` + evidence links | Prevents policy drift and duplicate launch truth |

## Common Pitfalls

### Pitfall 1: Implicit localhost base URL
**What goes wrong:** Control-room doctor runs against non-live localhost and fails immediately.
**How to avoid:** Require explicit `BASE_URL` and pre-probe `/healthz` and `/readyz` before rehearsal.

### Pitfall 2: Unsigned artifacts accepted as sufficient
**What goes wrong:** `sbom.sh` runs without key and emits unsigned outputs.
**How to avoid:** Treat missing `RELEASE_SIGNING_KEY_PEM` as hard failure for Phase 10.

### Pitfall 3: Checklist marked complete without fresh links
**What goes wrong:** Gates are checked based on stale or prior-phase evidence.
**How to avoid:** Use latest run directory and current `target/release-bundle` file references in reconciliation.

### Pitfall 4: GO/NO-GO recorded without blocker accounting
**What goes wrong:** Decision text exists, but blocker ownership or accepted debt is missing.
**How to avoid:** Add explicit blocker table with owner, rationale, and follow-up timeline in final package.

## Code Examples

### Reachability + control-room rehearsal
```bash
BASE_URL="${BASE_URL:?set BASE_URL to reachable server URL}"
curl -fsS "$BASE_URL/healthz"
curl -fsS "$BASE_URL/readyz"
bash scripts/release/mvp_control_room.sh --skip-deploy --base-url "$BASE_URL"
RUN_DIR="$(ls -1dt var/release-control-room/* | head -n 1)"
! rg -n "RESULT: FAIL" "$RUN_DIR/evidence.log"
```

### Signed SBOM/provenance generation
```bash
cargo build --release -p adapteros-server --bin aos-server
cargo build --release -p adapteros-lora-worker --bin aos-worker
cargo build --release -p adapteros-cli --bin aosctl
RELEASE_SIGNING_KEY_PEM="${RELEASE_SIGNING_KEY_PEM:?missing signing key}" bash scripts/release/sbom.sh
test -s target/release-bundle/sbom.json
test -s target/release-bundle/build_provenance.json
test -s target/release-bundle/signature.sig
test -s target/release-bundle/build_provenance.sig
```

### Checklist reconciliation signal checks
```bash
rg -n "var/release-control-room/.*/evidence.log" MVP_PROD_CHECKLIST.md
rg -n "target/release-bundle/(sbom.json|build_provenance.json|signature.sig|build_provenance.sig)" MVP_PROD_CHECKLIST.md
```

## Current State (Verified)

- Phase 06 summary captured a failed control-room doctor preflight because no server was reachable at `http://localhost:8080`.
- `scripts/release/mvp_control_room.sh` explicitly runs `AOS_SERVER_URL='${BASE_URL}' ./aosctl doctor` and records step logs under `var/release-control-room/<timestamp>/`.
- `scripts/release/sbom.sh` emits signature files only when `RELEASE_SIGNING_KEY_PEM` is set; otherwise it warns outputs are unsigned.
- MVP checklist and control-room docs already define GO/NO-GO decision gates and evidence expectations.

## Sources

### Primary (repository-grounded)
- `.planning/REQUIREMENTS.md` (OPS-06, OPS-07, OPS-08 definitions)
- `.planning/ROADMAP.md` (Phase 10 success criteria)
- `.planning/phases/06-production-operations/06-01-SUMMARY.md` (known blocker and unsigned artifact evidence)
- `scripts/release/mvp_control_room.sh` (rehearsal flow and evidence logging semantics)
- `scripts/release/sbom.sh` (signed artifact generation semantics)
- `MVP_PROD_CHECKLIST.md` and `MVP_PROD_CONTROL_ROOM.md` (launch gates and GO/NO-GO card)

---
*Phase: 10-operations-release-sign-off*
*Research completed: 2026-02-24*
*Ready for planning: no (already executed and reconciled)*
