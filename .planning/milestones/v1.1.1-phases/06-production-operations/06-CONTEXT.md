# Phase 6: Production Operations - Context

**Gathered:** 2026-02-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Lock production operations readiness using existing repository controls: security regression gates, backup/restore validation, deterministic release artifacts (SBOM + provenance), MVP checklist closure, and deployment runbook execution in staging-equivalent conditions. This phase is operational hardening and release validation only. New product features, API expansion, and UI polish are out of scope.

</domain>

<decisions>
## Implementation Decisions

### Security Regression Gate
- Treat `.github/workflows/security-regression-tests.yml` as the authoritative security regression gate for this phase.
- Require parity between local evidence commands and CI gate behavior (`security_regression_suite`, crypto tests, security lints).
- Security exceptions are allowed only with explicit owner + expiry, and must be documented in MVP checklist evidence.

### Backup and Restore Validation
- Use existing backup pipeline only: `scripts/backup/backup.sh`, `verify-backups.sh`, `test-restore.sh`, and `ci-smoke.sh`.
- Validate backup/restore on clean disposable paths and retain machine-readable evidence logs.
- Operational cron assets (`deploy/cron/adapteros-backup`, `scripts/backup/cron.example`) are verification targets, not redesign targets.

### Release Artifact and Provenance Integrity
- Use `scripts/release/sbom.sh` as the canonical SBOM/provenance generator.
- Release readiness requires build outputs plus `sbom.json` and `build_provenance.json` (and signatures when signing key is configured).
- Provenance generation must be deterministic/reproducible-friendly (normalized timestamps via `SOURCE_DATE_EPOCH` path already in script).

### Checklist and Runbook Closure
- `MVP_PROD_CHECKLIST.md` is the launch gate; all REQUIRED items tied to Phase 6 must have concrete evidence.
- `scripts/release/mvp_control_room.sh` is the operator fast path and evidence logger for rehearsal/execution.
- `scripts/verify-deployment.sh` and runbooks under `docs/runbooks/` are validation surfaces for deploy readiness.

### Claude's Discretion
- Exact sequencing of validation commands to minimize cycle time while preserving confidence.
- Whether to fix minor script/docs drift discovered during validation or defer with explicit TODO + rationale.
- Evidence packaging format (single ops evidence artifact vs split per requirement), as long as traceability to OPS-01..OPS-05 remains explicit.

</decisions>

<specifics>
## Specific Ideas

- Requirements for this phase: `OPS-01`, `OPS-02`, `OPS-03`, `OPS-04`, `OPS-05`.
- Success criteria anchor points:
  - security suite green
  - backup + restore proven on clean target
  - release SBOM + provenance artifacts generated
  - MVP checklist REQUIRED gates satisfied with evidence
  - deployment runbook validated in staging-equivalent path
- Existing assets already aligned to this phase:
  - `MVP_PROD_CHECKLIST.md`
  - `MVP_PROD_CONTROL_ROOM.md`
  - `.github/workflows/security-regression-tests.yml`
  - `scripts/backup/*`
  - `scripts/release/sbom.sh`
  - `scripts/release/mvp_control_room.sh`

</specifics>

<deferred>
## Deferred Ideas

- Multi-node disaster recovery and orchestration automation beyond MVP single-node assumptions.
- New security feature development (this phase validates existing controls rather than expanding security surface).
- Post-MVP operational UX improvements for dashboarding/release command ergonomics.

</deferred>

---

*Phase: 06-production-operations*
*Context gathered: 2026-02-24*
