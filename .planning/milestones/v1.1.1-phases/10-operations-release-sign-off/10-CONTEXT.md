# Phase 10: Operations Release Sign-off - Context

**Gathered:** 2026-02-24
**Status:** Executed and reconciled (historical planning record)

## Reconciled Execution State (2026-02-24)

This context captures planning-time sign-off scope. Phase 10 execution is complete with recorded GO decision and evidence package; any earlier rehearsal/signing blockers in this file are historical and were resolved before final closeout.

<domain>
## Phase Boundary

Historical planning objective: close v1.1 release operations sign-off using existing operational scripts and checklists only. This phase was scoped to clear then-remaining production sign-off blockers: reachable live control-room rehearsal, signed SBOM/provenance artifacts, and final checklist reconciliation with a recorded GO/NO-GO decision package.

</domain>

<decisions>
## Implementation Decisions

### Control-room rehearsal must run against a reachable server (OPS-06)
- Treat `scripts/release/mvp_control_room.sh` as the canonical execution path.
- Use explicit `--base-url` targeting a live server; do not rely on implicit localhost defaults.
- Fail fast if `Preflight: aosctl doctor` or readiness probes fail.
- Capture run evidence from `var/release-control-room/<timestamp>/evidence.log` and `summary.txt`.

### Signed SBOM and provenance are mandatory (OPS-07)
- Use `scripts/release/sbom.sh` as the canonical generator.
- `RELEASE_SIGNING_KEY_PEM` is required for Phase 10 acceptance; unsigned warnings are not acceptable.
- Verify presence of `target/release-bundle/signature.sig` and `target/release-bundle/build_provenance.sig` after generation.
- Keep artifact generation deterministic using existing script behavior (`SOURCE_DATE_EPOCH`, staged release artifacts).

### Checklist reconciliation and GO/NO-GO packaging (OPS-08)
- `MVP_PROD_CHECKLIST.md` remains the release gate source of truth.
- Every required item closed in this phase must include fresh, timestamped evidence links.
- Final sign-off package must include:
  - control-room evidence logs
  - signed SBOM/provenance artifacts
  - checklist reconciliation results
  - explicit GO/NO-GO decision with rationale and any accepted debt

### Plan decomposition and dependency model
- Split into three focused plans:
  - `10-01`: control-room rehearsal readiness and successful run (`OPS-06`)
  - `10-02`: signed SBOM/provenance generation and signature validation (`OPS-07`)
  - `10-03`: checklist reconciliation and final GO/NO-GO evidence package (`OPS-08`)
- Wave model:
  - Wave 1: `10-01` and `10-02` (parallel)
  - Wave 2: `10-03` (depends on `10-01` and `10-02`)

### Claude's Discretion
- Exact mechanism to source and validate the rehearsal `BASE_URL`.
- Minimal script-level hardening if needed to eliminate false-negative operational failures.
- Evidence packaging format (single bundle index vs structured sectioned report), as long as traceability is explicit.

</decisions>

<specifics>
## Specific Ideas

- Requirements for this phase: `OPS-06`, `OPS-07`, `OPS-08`.
- Known blocker from prior phase evidence:
  - `aosctl doctor` failed with `Failed to connect to server at http://localhost:8080` (`Connection refused`) during Phase 06 rehearsal.
- Known release artifact gap:
  - prior SBOM/provenance run was unsigned because `RELEASE_SIGNING_KEY_PEM` was not supplied.
- Known closure gap:
  - checklist reconciliation and final GO/NO-GO evidence packaging remained pending after Phase 06.

</specifics>

<deferred>
## Deferred Ideas

- New operational tooling, dashboarding, or process redesign beyond existing scripts/checklists.
- Feature-level backend/UI/CLI changes unrelated to sign-off blockers.
- Long-term release automation improvements not required for v1.1 closure.

</deferred>

---

*Phase: 10-operations-release-sign-off*
*Context gathered: 2026-02-24*
